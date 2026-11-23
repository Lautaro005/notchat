// A small, safe Markdown renderer for the assistant's answers. Everything is
// HTML-escaped first; only a fixed set of tags is produced, and links are
// limited to http(s) and mailto. No third-party code.

export interface Rendered {
  html: string;
  /** `[[option]]` choices, shown as buttons. */
  choices: string[];
  /** Source numbers cited as [n]. */
  cited: number[];
}

const URL_RE = /\bhttps?:\/\/[^\s<>"'`]+[^\s<>"'`.,;:!?)\]}]/g;

function escape(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

export function safeUrl(raw: string): string | null {
  try {
    const u = new URL(raw);
    return u.protocol === "http:" || u.protocol === "https:" || u.protocol === "mailto:" ? u.href : null;
  } catch {
    return null;
  }
}

/** Inline formatting on already-escaped text. */
function inline(text: string, cited: Set<number>): string {
  const codes: string[] = [];
  let out = text.replace(/`([^`\n]+)`/g, (_, c: string) => {
    codes.push(`<code>${c}</code>`);
    return `\u0000${codes.length - 1}\u0000`;
  });
  // [label](url)
  out = out.replace(/\[([^\]\n]+)\]\(([^)\s]+)\)/g, (m, label: string, url: string) => {
    const href = safeUrl(url.replace(/&amp;/g, "&"));
    return href ? `<a href="${escape(href)}" rel="noreferrer">${label}</a>` : m;
  });
  // Bare URLs (not already inside an anchor).
  out = out.replace(/(^|[^"=>])(https?:\/\/[^\s<]+[^\s<.,;:!?)\]])/g, (m, pre: string, url: string) => {
    const href = safeUrl(url.replace(/&amp;/g, "&"));
    return href ? `${pre}<a href="${escape(href)}" rel="noreferrer">${url}</a>` : m;
  });
  out = out
    .replace(/\*\*([^*\n]+)\*\*/g, "<strong>$1</strong>")
    .replace(/__([^_\n]+)__/g, "<strong>$1</strong>")
    .replace(/(^|[^*\w])\*([^*\n]+)\*(?!\*)/g, "$1<em>$2</em>")
    .replace(/(^|[^_\w])_([^_\n]+)_(?!\w)/g, "$1<em>$2</em>")
    .replace(/~~([^~\n]+)~~/g, "<del>$1</del>");
  // Citations: [1], [2][3], [1, 2]. The number is drawn by CSS so copying the
  // text never picks it up.
  out = out.replace(/\[(\d{1,2}(?:\s*,\s*\d{1,2})*)\]/g, (_, list: string) =>
    list
      .split(",")
      .map((n) => {
        const k = Number(n.trim());
        cited.add(k);
        return `<sup class="cite" data-n="${k}" aria-label="source ${k}"></sup>`;
      })
      .join(""),
  );
  return out.replace(/\u0000(\d+)\u0000/g, (_, i: string) => codes[Number(i)]);
}

export function renderMarkdown(src: string): Rendered {
  const choices: string[] = [];
  const cited = new Set<number>();
  // Pull out [[choices]] wherever they are.
  let text = src.replace(/\[\[([^\]\n]{1,60})\]\]/g, (_, c: string) => {
    const v = c.trim();
    if (v && !choices.includes(v)) choices.push(v);
    return "";
  });
  text = text.replace(/\r\n?/g, "\n").replace(/[ \t]+$/gm, "");

  const lines = text.split("\n");
  const html: string[] = [];
  let para: string[] = [];
  let list: { tag: "ul" | "ol"; items: string[] } | null = null;
  let quote: string[] = [];

  const flushPara = () => {
    if (para.length) html.push(`<p>${inline(escape(para.join("\n")), cited).replace(/\n/g, "<br>")}</p>`);
    para = [];
  };
  const flushList = () => {
    if (list) html.push(`<${list.tag}>${list.items.map((i) => `<li>${i}</li>`).join("")}</${list.tag}>`);
    list = null;
  };
  const flushQuote = () => {
    if (quote.length) html.push(`<blockquote>${inline(escape(quote.join("\n")), cited).replace(/\n/g, "<br>")}</blockquote>`);
    quote = [];
  };
  const flushAll = () => {
    flushPara();
    flushList();
    flushQuote();
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const fence = /^```/.exec(line);
    if (fence) {
      flushAll();
      const body: string[] = [];
      i++;
      while (i < lines.length && !/^```/.test(lines[i])) body.push(lines[i++]);
      html.push(`<pre><code>${escape(body.join("\n"))}</code></pre>`);
      continue;
    }
    if (!line.trim()) {
      flushAll();
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      flushAll();
      // Headings stay small: the answer is a chat reply, not a document.
      const level = heading[1].length === 1 ? 4 : heading[1].length <= 3 ? 5 : 6;
      html.push(`<h${level}>${inline(escape(heading[2]), cited)}</h${level}>`);
      continue;
    }
    if (/^(\*\s*){3,}$|^(-\s*){3,}$|^(_\s*){3,}$/.test(line.trim())) {
      flushAll();
      html.push("<hr>");
      continue;
    }
    const q = /^>\s?(.*)$/.exec(line);
    if (q) {
      flushPara();
      flushList();
      quote.push(q[1]);
      continue;
    }
    const ul = /^\s*[-*•]\s+(.*)$/.exec(line);
    const ol = /^\s*\d{1,3}[.)]\s+(.*)$/.exec(line);
    if (ul || ol) {
      flushPara();
      flushQuote();
      const tag = ul ? "ul" : "ol";
      if (!list || list.tag !== tag) {
        flushList();
        list = { tag, items: [] };
      }
      list.items.push(inline(escape((ul ?? ol)![1]), cited));
      continue;
    }
    if (list && /^\s{2,}\S/.test(line)) {
      // Continuation of the last list item.
      list.items[list.items.length - 1] += `<br>${inline(escape(line.trim()), cited)}`;
      continue;
    }
    flushList();
    flushQuote();
    para.push(line);
  }
  flushAll();

  let out = html.join("");
  // No explicit [[choices]]: a closing question followed by a short list of
  // short items is a choice too.
  if (!choices.length) {
    const m = /<p>([^<]*\?)<\/p><(ul|ol)>((?:<li>[^<]{1,48}<\/li>){2,5})<\/\2>$/.exec(out);
    if (m) {
      for (const item of m[3].matchAll(/<li>([^<]+)<\/li>/g)) choices.push(unescape(item[1]));
      out = out.slice(0, out.length - m[0].length) + `<p>${m[1]}</p>`;
    }
  }
  return { html: out, choices, cited: [...cited] };
}

function unescape(s: string): string {
  return s.replace(/&quot;/g, '"').replace(/&gt;/g, ">").replace(/&lt;/g, "<").replace(/&amp;/g, "&");
}

/** Plain message text cut into runs of text and links. */
export function linkParts(text: string): ({ text: string } | { url: string; label: string })[] {
  const out: ({ text: string } | { url: string; label: string })[] = [];
  let last = 0;
  for (const m of text.matchAll(URL_RE)) {
    const href = safeUrl(m[0]);
    if (!href || m.index === undefined) continue;
    if (m.index > last) out.push({ text: text.slice(last, m.index) });
    out.push({ url: href, label: m[0] });
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push({ text: text.slice(last) });
  return out.length ? out : [{ text }];
}

export function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}
