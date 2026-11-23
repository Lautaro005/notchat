<script lang="ts">
  import { tick } from "svelte";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import { api, type AiRow, type Chat, type SendAction } from "./api";
  import { aiReady, app, askAi, chatTitle, selectChat } from "./app.svelte";
  import { t, tf, type Key } from "./i18n";
  import { hostOf, renderMarkdown } from "./md";

  interface Source {
    n: number;
    title: string;
    url: string;
  }
  interface ToolMeta {
    chats?: { jid: string; name: string; unread: number; group: boolean; preview: string }[];
    chat?: { jid: string; name: string; group: boolean };
    count?: number;
    query?: string;
    sources?: Source[];
    cached?: boolean;
    error?: boolean;
  }

  const MAX_FILE = 15 * 1024 * 1024;

  let draft = $state("");
  let files = $state<{ name: string; base64: string }[]>([]);
  let error = $state("");
  let scroller: HTMLElement | undefined = $state();
  let input: HTMLTextAreaElement | undefined = $state();
  let picker: HTMLInputElement | undefined = $state();

  const ai = $derived(app.ai);
  // Local models may stream their reasoning inline; never show it.
  const streaming = $derived(
    ai.streaming.replace(/<think>[\s\S]*?<\/think>/g, "").replace(/<think>[\s\S]*$/, "").trim(),
  );

  $effect(() => {
    void app.tail;
    void tick().then(() => scroller?.scrollTo({ top: scroller.scrollHeight }));
  });

  function parse<T>(json: string, fallback: T): T {
    try {
      return json ? (JSON.parse(json) as T) : fallback;
    } catch {
      return fallback;
    }
  }

  function action(row: AiRow): SendAction | null {
    return parse<SendAction | null>(row.meta, null);
  }

  function attachments(row: AiRow): { name: string; kind: string }[] {
    return parse(row.meta, []);
  }

  function toolLabel(name: string): string {
    const key = `tool.${name}` as Key;
    return ["list_chats", "list_unread", "read_chat", "search_messages", "send_message", "web_search", "fetch_url"].includes(name)
      ? t(key)
      : name;
  }

  function toolMeta(row: AiRow): ToolMeta {
    return parse<ToolMeta>(row.meta, {});
  }

  /** Web sources the tools found for the answer at `index`. */
  function sourcesFor(index: number): Source[] {
    const out = new Map<number, Source>();
    for (let i = index - 1; i >= 0 && ai.rows[i].role !== "user"; i--) {
      const row = ai.rows[i];
      if (row.role !== "tool") continue;
      for (const src of toolMeta(row).sources ?? []) if (!out.has(src.n)) out.set(src.n, src);
    }
    return [...out.values()].sort((a, b) => a.n - b.n);
  }

  /** The answer that is still waiting for the user (its choices are live). */
  const lastAnswer = $derived.by(() => {
    for (let i = ai.rows.length - 1; i >= 0; i--) {
      if (ai.rows[i].role === "user") return -1;
      if (ai.rows[i].role === "assistant") return ai.rows[i].id;
    }
    return -1;
  });

  function choose(text: string) {
    if (ai.busy) return;
    void askAi(text, []).catch((e) => (error = String(e)));
  }

  function openChat(jid: string) {
    void selectChat(jid);
  }

  /** Clicking a citation number scrolls to (and flashes) its source pill. */
  function onAnswerClick(e: MouseEvent) {
    const sup = (e.target as HTMLElement).closest?.("sup.cite") as HTMLElement | null;
    if (!sup) return;
    const pill = (e.currentTarget as HTMLElement).parentElement?.querySelector<HTMLElement>(`[data-src="${sup.dataset.n}"]`);
    if (!pill) return;
    pill.classList.remove("flash");
    void pill.offsetWidth;
    pill.classList.add("flash");
  }

  // ---- @mentions in the composer --------------------------------------------

  let mention = $state<{ start: number; query: string } | null>(null);
  let mentionIndex = $state(0);

  function fold(s: string): string {
    return s.normalize("NFD").replace(/[\u0300-\u036f]/g, "").replace(/^~/, "").toLowerCase();
  }

  const mentionHits = $derived.by((): Chat[] => {
    if (!mention) return [];
    const q = fold(mention.query);
    return app.chats
      .filter((c) => !c.archived && chatTitle(c) && fold(chatTitle(c)).includes(q))
      .slice(0, 6);
  });

  function trackMention() {
    if (!input) return;
    const upto = draft.slice(0, input.selectionStart ?? draft.length);
    const m = /(^|\s)@([^\s@]{0,30})$/.exec(upto);
    mention = m ? { start: upto.length - m[2].length - 1, query: m[2] } : null;
    mentionIndex = 0;
  }

  async function pickMention(c: Chat) {
    if (!mention || !input) return;
    const name = chatTitle(c).replace(/^~/, "");
    const end = mention.start + 1 + mention.query.length;
    draft = `${draft.slice(0, mention.start)}@${name} ${draft.slice(end)}`;
    const caret = mention.start + name.length + 2;
    mention = null;
    await tick();
    input.focus();
    input.setSelectionRange(caret, caret);
  }

  function toBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result).split(",")[1] ?? "");
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(file);
    });
  }

  async function addFiles(list: FileList | null) {
    error = "";
    for (const file of list ?? []) {
      if (files.length >= 6) break;
      if (file.size > MAX_FILE) {
        error = t("ai.fileTooBig");
        continue;
      }
      files.push({ name: file.name, base64: await toBase64(file) });
    }
    if (picker) picker.value = "";
  }

  async function submit() {
    const text = draft.trim();
    if ((!text && files.length === 0) || ai.busy) return;
    error = "";
    try {
      await askAi(text, $state.snapshot(files));
      draft = "";
      files = [];
    } catch (e) {
      error = String(e);
    }
    await tick();
    input?.focus();
  }

  function onKey(e: KeyboardEvent) {
    if (mention && mentionHits.length) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const n = mentionHits.length;
        mentionIndex = (mentionIndex + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        void pickMention(mentionHits[mentionIndex]);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        mention = null;
        return;
      }
    }
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void submit();
    }
  }

  async function resolve(id: number, approve: boolean) {
    error = "";
    try {
      await api.aiResolve(id, approve);
    } catch (e) {
      error = String(e);
    }
  }
</script>

<section class="ai">
  <header data-tauri-drag-region>
    <span class="mark"><Icon name="spark" size={16} /></span>
    <div>
      <h1>{t("ai.name")}</h1>
      <span>{ai.config.model || t("ai.sub")}</span>
    </div>
    {#if ai.rows.length > 0}
      <button class="icon-btn" onclick={() => api.aiClear()} aria-label={t("ai.clear")} title={t("ai.clear")}>
        <Icon name="close" size={18} />
      </button>
    {/if}
  </header>

  <div class="log scroll" bind:this={scroller}>
    {#if !aiReady()}
      <div class="intro">
        <h2>{t("ai.setupTitle")}</h2>
        <p>{t("ai.setupBody")}</p>
        <button class="btn primary" onclick={() => (app.settingsOpen = true)}>{t("ai.setupAction")}</button>
      </div>
    {:else if ai.rows.length === 0 && !ai.busy}
      <div class="intro">
        <h2>{t("ai.emptyTitle")}</h2>
        <p>{t("ai.emptyBody")}</p>
        <div class="tries">
          {#each ["ai.try1", "ai.try2", "ai.try3"] as const as key (key)}
            <button
              onclick={() => {
                draft = t(key);
                input?.focus();
              }}>{t(key)}</button
            >
          {/each}
        </div>
      </div>
    {/if}

    {#each ai.rows as row, i (row.id)}
      {#if row.role === "user"}
        <article class="turn me">
          <div class="bubble">
            {#each attachments(row) as a (a.name)}
              <span class="file"><Icon name={a.kind === "image" ? "image" : "doc"} size={15} />{a.name}</span>
            {/each}
            {#if row.content}<span class="text">{row.content}</span>{/if}
          </div>
        </article>
      {:else if row.role === "assistant"}
        {@const r = renderMarkdown(row.content)}
        {@const sources = sourcesFor(i)}
        <article class="turn answer">
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="md" onclick={onAnswerClick}>{@html r.html}</div>
          {#if sources.length}
            <div class="sources" aria-label={t("ai.sources")}>
              {#each sources as src (src.n)}
                <a class="src" class:cited={r.cited.includes(src.n)} data-src={src.n} href={src.url} title={src.title} rel="noreferrer">
                  <span class="n" data-n={src.n} aria-hidden="true"></span>
                  <span class="host">{hostOf(src.url)}</span>
                  <span class="title">{src.title}</span>
                </a>
              {/each}
            </div>
          {/if}
          {#if r.choices.length && row.id === lastAnswer && !ai.busy}
            <div class="choices">
              {#each r.choices as c (c)}
                <button class="pill" onclick={() => choose(c)}>{c}</button>
              {/each}
            </div>
          {/if}
        </article>
      {:else if row.role === "tool"}
        {@const meta = toolMeta(row)}
        <div class="step" class:fail={meta.error}>
          <p class="step-head">
            <Icon name={row.content.startsWith("web") || row.content === "fetch_url" ? "globe" : row.content === "read_chat" ? "chat" : row.content === "search_messages" ? "search" : "check"} size={14} />
            {#if meta.cached}
              {t("ai.cached")}
            {:else if row.content === "read_chat" && meta.chat}
              {tf("ai.readCount", { n: meta.count ?? 0 })}
              <button class="who" onclick={() => openChat(meta.chat!.jid)} title={t("ai.openChat")}>{meta.chat.name}</button>
            {:else if row.content === "search_messages" && meta.query}
              {toolLabel(row.content)} · {tf("ai.searchCount", { q: meta.query, n: meta.count ?? 0 })}
            {:else if row.content === "web_search" && meta.query}
              {toolLabel(row.content)} · “{meta.query}”
            {:else}
              {toolLabel(row.content)}
            {/if}
          </p>
          {#if meta.chats?.length}
            <div class="chatcards">
              {#each meta.chats as c (c.jid)}
                <button class="chatcard" onclick={() => openChat(c.jid)} title={t("ai.openChat")}>
                  <Avatar title={c.name.replace(/^~/, "")} jid={c.jid} group={c.group} size={26} />
                  <span class="cc-name">{c.name}</span>
                  {#if c.unread > 0}<span class="cc-count num">{c.unread}</span>{/if}
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {:else if row.role === "action"}
        {@const a = action(row)}
        {#if a}
          <article class="card" class:done={a.state !== "pending"}>
            <span class="to">{t("action.to")} <strong>{a.chat_name}</strong></span>
            <p>{a.text}</p>
            {#if a.state === "pending"}
              <div class="acts">
                <button class="btn quiet" onclick={() => resolve(row.id, false)}>{t("action.cancel")}</button>
                <button class="btn primary" onclick={() => resolve(row.id, true)}>{t("action.send")}</button>
              </div>
            {:else}
              <span class="state" class:bad={a.state === "failed"}>
                {#if a.state === "sent"}<Icon name="check" size={14} />{/if}
                {t(`action.${a.state}` as Key)}
              </span>
            {/if}
          </article>
        {/if}
      {:else}
        <p class="err" role="alert">{row.content}</p>
      {/if}
    {/each}

    {#if ai.busy}
      <article class="turn answer live" aria-live="polite">
        {#if streaming}<div class="md">{@html renderMarkdown(streaming).html}</div>{:else}<p class="wait">{t("ai.thinking")}</p>{/if}
      </article>
    {/if}
  </div>

  <footer>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    {#if files.length > 0}
      <div class="files">
        {#each files as f, i (i)}
          <span class="file chip">
            <Icon name="doc" size={14} />{f.name}
            <button onclick={() => files.splice(i, 1)} aria-label="{t('ai.remove')} {f.name}"><Icon name="close" size={12} /></button>
          </span>
        {/each}
      </div>
    {/if}
    {#if mention && mentionHits.length}
      <div class="mentions" role="listbox" aria-label={t("ai.mentionHint")}>
        {#each mentionHits as c, k (c.jid)}
          <button
            role="option"
            aria-selected={k === mentionIndex}
            class:on={k === mentionIndex}
            onmousedown={(e) => e.preventDefault()}
            onclick={() => pickMention(c)}
            onmouseenter={() => (mentionIndex = k)}
          >
            <Avatar title={chatTitle(c).replace(/^~/, "")} jid={c.jid} group={c.is_group} size={24} />
            <span>{chatTitle(c)}</span>
            {#if c.is_group}<small>{t("chat.group")}</small>{/if}
          </button>
        {/each}
      </div>
    {/if}
    <div class="composer">
      <input
        bind:this={picker}
        type="file"
        multiple
        hidden
        accept=".txt,.md,.csv,.json,.pdf,.log,.html,.xml,.yaml,.yml,.rs,.py,.js,.ts,image/png,image/jpeg,image/webp,image/gif"
        onchange={(e) => addFiles(e.currentTarget.files)}
      />
      <button class="icon-btn" onclick={() => picker?.click()} disabled={!aiReady()} aria-label={t("ai.attach")} title={t("ai.attach")}>
        <Icon name="clip" size={18} />
      </button>
      <textarea
        bind:this={input}
        bind:value={draft}
        rows="1"
        placeholder={t("ai.placeholder")}
        aria-label={t("ai.placeholder")}
        disabled={!aiReady()}
        onkeydown={onKey}
        oninput={trackMention}
        onclick={trackMention}
        onblur={() => setTimeout(() => (mention = null), 120)}
      ></textarea>
      {#if ai.busy}
        <button class="round stop" onclick={() => api.aiStop()} aria-label={t("ai.stop")} title={t("ai.stop")}>
          <Icon name="stop" size={16} />
        </button>
      {:else}
        <button class="round" onclick={submit} disabled={!draft.trim() && files.length === 0} aria-label={t("chat.send")}>
          <Icon name="send" size={18} />
        </button>
      {/if}
    </div>
  </footer>
</section>

<style>
  .ai {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  header {
    flex: none;
    height: 58px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 12px 0 16px;
    border-bottom: 1px solid var(--line-soft);
  }
  header > div {
    flex: 1;
    min-width: 0;
    pointer-events: none;
  }
  .mark {
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--accent);
    color: var(--on-accent);
  }
  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  header span:not(.mark) {
    display: block;
    font-size: 11px;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .log {
    flex: 1;
    min-height: 0;
    padding: 20px;
  }
  .log > * {
    max-width: 760px;
    margin-inline: auto;
  }
  .intro {
    padding: 12vh 0 24px;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 22px;
    font-weight: 600;
    letter-spacing: -0.01em;
    text-wrap: balance;
  }
  .intro p {
    margin: 0 0 20px;
    max-width: 52ch;
    font-size: 15px;
    color: var(--text-2);
    text-wrap: pretty;
  }
  .tries {
    display: grid;
    justify-items: start;
    gap: 6px;
  }
  .tries button {
    padding: 7px 12px;
    border-radius: var(--r-l);
    background: var(--bg-2);
    font-size: 13px;
    text-align: left;
    transition: background 160ms var(--ease);
  }
  .tries button:hover {
    background: var(--bg-3);
  }
  .turn {
    margin-top: 16px;
    user-select: text;
    -webkit-user-select: text;
    cursor: text;
  }
  .turn.me {
    display: grid;
    justify-items: end;
  }
  .bubble {
    max-width: 80%;
    display: grid;
    gap: 6px;
    padding: 8px 13px;
    border-radius: 18px 18px 6px 18px;
    background: var(--mine);
    color: var(--on-mine);
    font-size: 14px;
    line-height: 1.4;
  }
  .text,
  .card p {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .answer .md {
    width: fit-content;
    max-width: min(100%, 680px);
    padding: 11px 15px;
    border-radius: 18px 18px 18px 6px;
    background: var(--bubble);
    box-shadow: 0 1px 1.5px rgb(20 10 60 / 0.05), 0 0 0 1px var(--bubble-line);
    font-size: 14px;
    line-height: 1.55;
    overflow-wrap: anywhere;
  }
  .md :global(p) {
    margin: 0;
  }
  .md :global(p + p),
  .md :global(p + ul),
  .md :global(p + ol),
  .md :global(ul + p),
  .md :global(ol + p),
  .md :global(blockquote),
  .md :global(pre) {
    margin-top: 8px;
  }
  .md :global(h4),
  .md :global(h5),
  .md :global(h6) {
    margin: 12px 0 4px;
    font-weight: 650;
    line-height: 1.3;
  }
  .md :global(h4) {
    font-size: 15px;
  }
  .md :global(h5) {
    font-size: 14px;
  }
  .md :global(h6) {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }
  .md :global(:first-child) {
    margin-top: 0;
  }
  .md :global(ul),
  .md :global(ol) {
    margin: 4px 0 0;
    padding-left: 20px;
  }
  .md :global(li) {
    margin: 3px 0;
  }
  .md :global(li::marker) {
    color: var(--accent);
  }
  .md :global(strong) {
    font-weight: 650;
  }
  .md :global(a) {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .md :global(code) {
    padding: 1px 5px;
    border-radius: 5px;
    background: var(--bg-2);
    font: 12.5px ui-monospace, SFMono-Regular, Menlo, monospace;
  }
  .md :global(pre) {
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--bg-2);
    overflow-x: auto;
  }
  .md :global(pre code) {
    padding: 0;
    background: none;
  }
  .md :global(blockquote) {
    margin-inline: 0;
    padding: 2px 0 2px 10px;
    border-left: 3px solid color-mix(in srgb, var(--accent) 40%, transparent);
    color: var(--text-2);
  }
  .md :global(hr) {
    margin: 10px 0;
    border: 0;
    border-top: 1px solid var(--line-soft);
  }
  /* Citation numbers are drawn from data-n, so they never end up in copied text. */
  .md :global(sup.cite),
  .src .n {
    user-select: none;
    -webkit-user-select: none;
  }
  .md :global(sup.cite) {
    display: inline-grid;
    place-items: center;
    min-width: 15px;
    height: 15px;
    margin: 0 1px 0 2px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
    vertical-align: 2px;
    cursor: pointer;
  }
  .md :global(sup.cite)::after,
  .src .n::after {
    content: attr(data-n);
  }
  .sources {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 8px;
  }
  .src {
    max-width: 260px;
    height: 28px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 10px 0 4px;
    border-radius: 14px;
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line-soft);
    color: var(--text-2);
    font-size: 12px;
    text-decoration: none;
    transition: background 140ms var(--ease);
  }
  .src:hover {
    background: var(--bg-3);
  }
  .src .n {
    flex: none;
    width: 20px;
    height: 20px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--panel);
    color: var(--text-2);
    font-size: 10.5px;
    font-weight: 700;
  }
  .src.cited .n {
    background: var(--accent);
    color: var(--on-accent);
  }
  .src .host {
    flex: none;
    font-weight: 600;
    color: var(--text);
  }
  .src .title {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .src:global(.flash) {
    animation: srcflash 1.2s var(--ease);
  }
  @keyframes srcflash {
    0%,
    40% {
      box-shadow: inset 0 0 0 2px var(--accent);
    }
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }
  .pill {
    height: 32px;
    padding: 0 14px;
    border-radius: 16px;
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 25%, transparent);
    color: var(--accent);
    font-size: 13px;
    font-weight: 600;
    transition: background 140ms var(--ease), transform 140ms var(--ease);
  }
  .pill:hover {
    background: color-mix(in srgb, var(--accent) 18%, var(--panel));
  }
  .pill:active {
    transform: scale(0.97);
  }
  .step-head {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
  }
  .step.fail {
    color: var(--danger);
  }
  .step .who {
    color: var(--accent);
    font-weight: 600;
  }
  .step .who:hover {
    text-decoration: underline;
  }
  .chatcards {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 8px;
  }
  .chatcard {
    max-width: 220px;
    height: 34px;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 0 10px 0 4px;
    border-radius: 17px;
    background: var(--panel);
    box-shadow: inset 0 0 0 1px var(--line-soft);
    color: var(--text);
    font-size: 12.5px;
    transition: background 140ms var(--ease);
  }
  .chatcard:hover {
    background: var(--bg-2);
  }
  .cc-name {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-weight: 600;
  }
  .cc-count {
    flex: none;
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    display: grid;
    place-items: center;
    border-radius: 9px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 10.5px;
    font-weight: 700;
  }
  .mentions {
    display: grid;
    gap: 1px;
    margin-bottom: 6px;
    padding: 5px;
    border-radius: 14px;
    background: var(--panel);
    box-shadow: var(--shadow-s), 0 0 0 1px var(--line);
  }
  .mentions button {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 5px 8px;
    border-radius: 9px;
    text-align: left;
    font-size: 13px;
  }
  .mentions button.on {
    background: var(--bg-2);
  }
  .mentions span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .mentions small {
    color: var(--text-3);
    font-size: 11px;
  }
  .wait {
    color: var(--text-3);
    animation: pulse 1.4s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.45;
    }
  }
  .step {
    margin-block: 10px 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .card {
    margin-top: 14px;
    padding: 14px;
    border-radius: var(--r-l);
    background: var(--bg-2);
    border: 1px solid var(--line);
    max-width: 440px;
    margin-inline: auto auto;
  }
  .log > .card {
    margin-inline: max(0px, calc((100% - 680px) / 2)) auto;
  }
  .card .to {
    font-size: 12px;
    color: var(--text-2);
  }
  .card strong {
    color: var(--text);
    font-weight: 600;
  }
  .card p {
    margin: 6px 0 12px;
    font-size: 14px;
    line-height: 1.4;
    user-select: text;
    -webkit-user-select: text;
  }
  .card.done p {
    margin-bottom: 8px;
    color: var(--text-2);
  }
  .acts {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
  .acts .primary {
    background: var(--accent);
    color: var(--on-accent);
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  .state.bad,
  .err {
    color: var(--danger);
  }
  .err {
    margin-block: 12px 0;
    font-size: 12px;
    user-select: text;
    -webkit-user-select: text;
  }
  .file {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 500;
    opacity: 0.85;
  }
  footer {
    flex: none;
    padding: 8px 16px 14px;
  }
  footer .err {
    margin: 0 0 6px;
    padding-left: 14px;
  }
  .files {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 6px;
  }
  .chip {
    padding: 4px 4px 4px 10px;
    border-radius: var(--r-l);
    background: var(--bg-2);
    opacity: 1;
  }
  .chip button {
    width: 18px;
    height: 18px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: var(--text-2);
  }
  .chip button:hover {
    background: var(--bg-3);
  }
  .composer {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    padding: 5px 5px 5px 6px;
    border-radius: 22px;
    background: var(--panel);
    box-shadow: var(--shadow-s), 0 0 0 1px var(--line-soft);
    transition: box-shadow 160ms var(--ease);
  }
  .composer:focus-within {
    box-shadow: var(--shadow-s), 0 0 0 1px var(--text-3);
  }
  .composer .icon-btn {
    border-radius: 50%;
    flex: none;
  }
  textarea {
    flex: 1;
    min-width: 0;
    max-height: 160px;
    padding: 6px;
    border: 0;
    background: none;
    resize: none;
    outline: none;
    font-size: 14px;
    line-height: 1.38;
    field-sizing: content;
    user-select: text;
    -webkit-user-select: text;
  }
  textarea::placeholder {
    color: var(--text-3);
  }
  .round {
    flex: none;
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--accent);
    color: var(--on-accent);
    transition: opacity 160ms var(--ease), transform 160ms var(--ease);
  }
  .round:disabled {
    opacity: 0;
    transform: scale(0.8);
  }
  .round.stop {
    background: var(--ink);
    color: var(--on-ink);
  }
</style>
