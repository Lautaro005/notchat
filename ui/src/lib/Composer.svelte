<script lang="ts">
  import { untrack, onDestroy, tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { api } from "./api";
  import { app, appendSent, chatTitle, kindOf, MAX_FILE, openMenu, send, sendFile } from "./app.svelte";
  import { EMOJI } from "./emoji";
  import { textWithNames } from "./messageMenu";
  import { bytes, duration, t, tf, type Key } from "./i18n";
  import type { Sticker } from "./api";

  let { files = $bindable([]) }: { files?: File[] } = $props();

  let draft = $state("");
  let sending = $state(false);
  let failed = $state<string | null>(null);
  let input: HTMLTextAreaElement | undefined = $state();
  let mediaInput: HTMLInputElement | undefined = $state();
  let docInput: HTMLInputElement | undefined = $state();
  let emojiOpen = $state(false);
  let panelTab = $state<"emoji" | "stickers">("emoji");
  let stickers = $state<Sticker[] | null>(null);
  let stickerUrls = $state<Record<string, string>>({});

  // ---- voice recording ------------------------------------------------------
  let recorder: MediaRecorder | null = null;
  let chunks: Blob[] = [];
  let recording = $state(false);
  let elapsed = $state(0);
  let timer: ReturnType<typeof setInterval> | undefined;
  let started = 0;
  let level = $state<number[]>([]);
  let analyserStop: (() => void) | null = null;

  const previews = $derived(files.map((f) => (f.type.startsWith("image/") ? URL.createObjectURL(f) : null)));
  $effect(() => {
    const urls = previews;
    return () => urls.forEach((u) => u && URL.revokeObjectURL(u));
  });

  $effect(() => {
    void app.selected;
    draft = "";
    failed = null;
    files = [];
    stopRecording(false);
    void tick().then(() => input?.focus());
  });

  // Editing puts the original text in the box; replying just focuses it.
  $effect(() => {
    const editing = app.editing;
    if (editing) draft = editing.text;
    void tick().then(() => input?.focus());
  });
  $effect(() => {
    if (app.reply) void tick().then(() => input?.focus());
  });

  function replyName(): string {
    const m = app.reply;
    if (!m) return "";
    if (m.from_me) return t("chat.you");
    if (m.sender_name) return m.sender_name;
    const c = app.chats.find((x) => x.jid === (m.sender_jid || m.chat_jid));
    return c ? chatTitle(c) : chatTitle({ jid: m.sender_jid || m.chat_jid, name: "" });
  }

  function cancelContext() {
    if (app.editing) draft = "";
    app.reply = null;
    app.editing = null;
  }

  // A sticker saved from a message shows up next time the picker opens.
  $effect(() => {
    void app.stickersVersion;
    untrack(() => {
      stickers = null;
      if (panelTab === "stickers") void openStickers();
    });
  });

  async function openStickers() {
    panelTab = "stickers";
    if (stickers === null) {
      try {
        stickers = await api.stickers();
      } catch {
        stickers = [];
      }
    }
  }

  // Plain set: the template asks for images while rendering, so starting a
  // load must not touch reactive state; only the finished load does.
  const loadingStickers = new Set<string>();
  function stickerSrc(id: string): string | undefined {
    if (!(id in stickerUrls) && !loadingStickers.has(id)) {
      loadingStickers.add(id);
      api
        .stickerImage(id)
        .then((url) => (stickerUrls[id] = url))
        .catch(() => loadingStickers.delete(id));
    }
    return stickerUrls[id];
  }

  async function sendSticker(s: Sticker) {
    const jid = app.selected;
    if (!jid) return;
    emojiOpen = false;
    try {
      appendSent(await api.sendSticker(jid, s.id));
    } catch {
      failed = t("chat.sendFailed");
    }
  }

  function stickerMenu(e: MouseEvent, s: Sticker) {
    openMenu(e, [
      {
        label: s.favorite ? t("stk.unfavorite") : t("stk.favorite"),
        icon: "star",
        action: async () => {
          await api.stickerFavorite(s.id, !s.favorite);
          stickers = await api.stickers();
        },
      },
      {
        label: t("stk.remove"),
        icon: "trash",
        danger: true,
        action: async () => {
          await api.removeSticker(s.id);
          stickers = (stickers ?? []).filter((x) => x.id !== s.id);
        },
      },
    ]);
  }

  // Typing indicator, throttled; failures do not matter.
  let typingSent = 0;
  let typingTimer: ReturnType<typeof setTimeout> | undefined;
  function onInput() {
    const jid = app.selected;
    if (!jid || !draft.trim()) return;
    const now = Date.now();
    if (now - typingSent > 4000) {
      typingSent = now;
      api.typing(jid, true).catch(() => {});
    }
    clearTimeout(typingTimer);
    typingTimer = setTimeout(() => {
      typingSent = 0;
      api.typing(jid, false).catch(() => {});
    }, 5000);
  }

  function addFiles(list: FileList | File[] | null) {
    if (!list) return;
    const ok: File[] = [];
    for (const f of list) {
      if (f.size > MAX_FILE) failed = t("chat.fileTooBig");
      else ok.push(f);
    }
    files = [...files, ...ok].slice(0, 10);
  }

  /** Files dropped on the conversation: queued like the attach button, ready to send. */
  export function add(list: FileList | File[] | null) {
    addFiles(list);
    void tick().then(() => input?.focus());
  }

  async function submit() {
    if (sending) return;
    const text = draft.trim();
    if (!text && files.length === 0) return;
    sending = true;
    failed = null;
    try {
      if (files.length) {
        const queue = [...files];
        for (const [i, file] of queue.entries()) {
          await sendFile(file, { name: file.name, kind: kindOf(file), caption: i === 0 ? text : "" });
          files = files.filter((f) => f !== file);
        }
      } else {
        await send(text);
      }
      draft = "";
      clearTimeout(typingTimer);
    } catch (e) {
      const msg = String(e);
      failed = msg.includes("too-big")
        ? t("chat.fileTooBig")
        : app.editing && msg.includes("not connected")
          ? t("msg.offlineEdit")
          : t("chat.sendFailed");
    } finally {
      sending = false;
      await tick();
      input?.focus();
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void submit();
    } else if (e.key === "Escape" && (app.reply || app.editing)) {
      e.preventDefault();
      cancelContext();
    }
  }

  function onPaste(e: ClipboardEvent) {
    const pasted = [...(e.clipboardData?.files ?? [])];
    if (pasted.length) {
      e.preventDefault();
      addFiles(pasted);
    }
  }

  function attachMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openMenu({ x: r.left, y: r.top - 86 }, [
      { label: t("attach.media"), icon: "image", action: () => mediaInput?.click() },
      { label: t("attach.doc"), icon: "doc", action: () => docInput?.click() },
    ]);
  }

  function insertEmoji(e: string) {
    const el = input;
    if (!el) {
      draft += e;
      return;
    }
    const start = el.selectionStart ?? draft.length;
    const end = el.selectionEnd ?? draft.length;
    draft = draft.slice(0, start) + e + draft.slice(end);
    void tick().then(() => {
      el.focus();
      el.selectionStart = el.selectionEnd = start + e.length;
    });
  }

  function pickMime(): string {
    for (const m of ["audio/ogg;codecs=opus", "audio/webm;codecs=opus", "audio/mp4"]) {
      if (typeof MediaRecorder !== "undefined" && MediaRecorder.isTypeSupported(m)) return m;
    }
    return "";
  }

  async function startRecording() {
    failed = null;
    let stream: MediaStream;
    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    } catch {
      failed = t("chat.micDenied");
      return;
    }
    const mime = pickMime();
    recorder = new MediaRecorder(stream, mime ? { mimeType: mime } : undefined);
    chunks = [];
    recorder.ondataavailable = (e) => e.data.size && chunks.push(e.data);
    recorder.start(250);
    recording = true;
    started = performance.now();
    elapsed = 0;
    timer = setInterval(() => (elapsed = (performance.now() - started) / 1000), 200);
    // Live level meter so the user sees the microphone is working.
    try {
      const ctx = new AudioContext();
      const analyser = ctx.createAnalyser();
      analyser.fftSize = 256;
      ctx.createMediaStreamSource(stream).connect(analyser);
      const data = new Uint8Array(analyser.frequencyBinCount);
      let raf = 0;
      const tickLevel = () => {
        analyser.getByteTimeDomainData(data);
        let peak = 0;
        for (const v of data) peak = Math.max(peak, Math.abs(v - 128));
        level = [...level.slice(-27), Math.min(1, peak / 64)];
        raf = requestAnimationFrame(tickLevel);
      };
      tickLevel();
      analyserStop = () => {
        cancelAnimationFrame(raf);
        void ctx.close();
      };
    } catch {
      analyserStop = null;
    }
  }

  function stopRecording(sendIt: boolean) {
    clearInterval(timer);
    analyserStop?.();
    analyserStop = null;
    level = [];
    const rec = recorder;
    recorder = null;
    if (!rec) {
      recording = false;
      return;
    }
    const seconds = (performance.now() - started) / 1000;
    rec.onstop = async () => {
      rec.stream.getTracks().forEach((tr) => tr.stop());
      recording = false;
      if (!sendIt || seconds < 0.8) return;
      const blob = new Blob(chunks, { type: rec.mimeType || "audio/mp4" });
      sending = true;
      try {
        await sendFile(blob, { name: "voice", kind: "voice", seconds });
      } catch {
        failed = t("chat.sendFailed");
      } finally {
        sending = false;
      }
    };
    rec.stop();
  }

  onDestroy(() => stopRecording(false));

  function icon(f: File): string {
    const k = kindOf(f);
    return k === "image" ? "image" : k === "video" ? "camera" : k === "audio" ? "mic" : "doc";
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape" && emojiOpen && !e.defaultPrevented) {
      e.preventDefault();
      emojiOpen = false;
    }
  }}
/>

<footer>
  {#if failed}<p class="fail" role="alert">{failed}</p>{/if}

  {#if files.length}
    <ul class="tray" aria-label={t("chat.attach")}>
      {#each files as f, i (f)}
        <li>
          {#if previews[i]}
            <img src={previews[i]} alt="" />
          {:else}
            <span class="fic"><Icon name={icon(f) as never} size={20} /></span>
          {/if}
          <span class="fname">{f.name}<small class="num">{bytes(f.size)}</small></span>
          <button class="x" onclick={() => (files = files.filter((x) => x !== f))} aria-label={t("ai.remove")}>
            <Icon name="close" size={13} />
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if emojiOpen}
    <div class="panel" role="dialog" aria-label={t("chat.emoji")}>
      <div class="tabs" role="tablist">
        <button role="tab" aria-selected={panelTab === "emoji"} onclick={() => (panelTab = "emoji")}>
          <Icon name="smile" size={15} />{t("chat.emojiTab")}
        </button>
        <button role="tab" aria-selected={panelTab === "stickers"} onclick={openStickers}>
          <Icon name="sticker" size={15} />{t("stk.title")}
        </button>
      </div>
      {#if panelTab === "emoji"}
        <div class="emoji">
          {#each EMOJI as e (e)}
            <button onclick={() => insertEmoji(e)} aria-label={e}>{e}</button>
          {/each}
        </div>
      {:else if stickers === null}
        <p class="pempty">…</p>
      {:else if stickers.length === 0}
        <p class="pempty">{t("stk.empty")}</p>
      {:else}
        <div class="stickers scroll">
          {#each stickers as s (s.id)}
            {@const src = stickerSrc(s.id)}
            <button
              class="stk"
              class:fav={s.favorite}
              onclick={() => sendSticker(s)}
              oncontextmenu={(e) => stickerMenu(e, s)}
              aria-label={t("stk.send")}
              title={s.favorite ? t("stk.favorites") : t("stk.send")}
            >
              {#if src}<img {src} alt="" loading="lazy" />{:else}<span class="ph"></span>{/if}
              {#if s.favorite}<span class="favmark"><Icon name="star" size={10} /></span>{/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  {#if app.reply || app.editing}
    {@const m = app.reply ?? app.editing}
    <div class="context">
      <span class="cic"><Icon name={app.editing ? "pencil" : "reply"} size={16} /></span>
      <span class="cbody">
        <b>{app.editing ? t("msg.editing") : tf("msg.replyingTo", { name: replyName() })}</b>
        <span>{(m && textWithNames(m)) || (m ? t(`kind.${m.kind}` as Key) : "")}</span>
      </span>
      <button class="icon-btn" onclick={cancelContext} aria-label={t("msg.cancelReply")} title={t("msg.cancelReply")}>
        <Icon name="close" size={16} />
      </button>
    </div>
  {/if}

  <div class="composer" class:rec={recording}>
    {#if recording}
      <button class="icon-btn" onclick={() => stopRecording(false)} aria-label={t("chat.cancelRec")} title={t("chat.cancelRec")}>
        <Icon name="trash" size={18} />
      </button>
      <span class="recdot"></span>
      <span class="rectime num">{duration(elapsed)}</span>
      <span class="meter" aria-hidden="true">
        {#each level as v, i (i)}<span style:height="{8 + v * 92}%"></span>{/each}
      </span>
      <span class="visually-hidden" role="status">{t("chat.recording")}</span>
      <button class="sendbtn" onclick={() => stopRecording(true)} aria-label={t("chat.sendVoice")}>
        <Icon name="send" size={18} />
      </button>
    {:else}
      <button class="icon-btn" onclick={attachMenu} aria-label={t("chat.attach")} title={t("chat.attach")}>
        <Icon name="plus" size={20} />
      </button>
      <button
        class="icon-btn"
        class:on={emojiOpen}
        onclick={() => (emojiOpen = !emojiOpen)}
        aria-label={t("chat.emoji")}
        aria-expanded={emojiOpen}
        title={t("chat.emoji")}
      >
        <Icon name="smile" size={19} />
      </button>
      <textarea
        bind:this={input}
        bind:value={draft}
        rows="1"
        placeholder={files.length ? t("chat.caption") : t("chat.placeholder")}
        aria-label={t("chat.placeholder")}
        onkeydown={onKey}
        oninput={onInput}
        onpaste={onPaste}
      ></textarea>
      {#if draft.trim() || files.length}
        <button class="sendbtn" onclick={submit} disabled={sending} aria-label={t("chat.send")}>
          {#if sending}<span class="spin"></span>{:else}<Icon name="send" size={18} />{/if}
        </button>
      {:else}
        <button class="icon-btn mic" onclick={startRecording} disabled={sending} aria-label={t("chat.record")} title={t("chat.record")}>
          {#if sending}<span class="spin dark"></span>{:else}<Icon name="mic" size={20} />{/if}
        </button>
      {/if}
    {/if}
  </div>

  <input
    bind:this={mediaInput}
    type="file"
    accept="image/*,video/mp4,video/quicktime"
    multiple
    hidden
    onchange={(e) => {
      addFiles(e.currentTarget.files);
      e.currentTarget.value = "";
    }}
  />
  <input
    bind:this={docInput}
    type="file"
    multiple
    hidden
    onchange={(e) => {
      addFiles(e.currentTarget.files);
      e.currentTarget.value = "";
    }}
  />
</footer>

<style>
  footer {
    position: relative;
    flex: none;
    padding: 8px 16px 14px;
  }
  .fail {
    margin: 0 0 6px 14px;
    font-size: 12px;
    color: var(--danger);
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
  .composer.rec {
    align-items: center;
  }
  .icon-btn {
    border-radius: 50%;
    flex: none;
  }
  .icon-btn.on {
    color: var(--accent);
  }
  textarea {
    flex: 1;
    min-width: 0;
    max-height: 160px;
    padding: 6px 6px;
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
  .sendbtn {
    flex: none;
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--accent);
    color: var(--on-accent);
    animation: popin 180ms var(--ease);
  }
  @keyframes popin {
    from {
      transform: scale(0.7);
      opacity: 0;
    }
  }
  .sendbtn:active:not(:disabled) {
    transform: scale(0.94);
  }
  .mic {
    color: var(--text-2);
  }
  .recdot {
    width: 9px;
    height: 9px;
    margin-left: 4px;
    border-radius: 50%;
    background: var(--danger);
    animation: blink 1s ease-in-out infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.25;
    }
  }
  .rectime {
    min-width: 40px;
    margin-left: 6px;
    font-size: 13px;
    font-weight: 500;
  }
  .meter {
    flex: 1;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 2px;
    margin-right: 8px;
    overflow: hidden;
  }
  .meter span {
    width: 3px;
    flex: none;
    border-radius: 2px;
    background: var(--accent);
  }
  .tray {
    display: flex;
    gap: 8px;
    margin: 0 0 8px;
    padding: 0;
    list-style: none;
    overflow-x: auto;
  }
  .tray li {
    position: relative;
    flex: none;
    width: 180px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 28px 6px 6px;
    border-radius: 12px;
    background: var(--panel);
    box-shadow: 0 0 0 1px var(--line-soft);
    animation: popin 180ms var(--ease);
  }
  .tray img,
  .fic {
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: 8px;
    object-fit: cover;
  }
  .fic {
    display: grid;
    place-items: center;
    background: var(--bg-3);
    color: var(--text-2);
  }
  .fname {
    min-width: 0;
    display: grid;
    font-size: 12px;
    font-weight: 500;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .fname small {
    font-weight: 400;
    color: var(--text-3);
  }
  .x {
    position: absolute;
    top: 6px;
    right: 6px;
    width: 18px;
    height: 18px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: var(--text-2);
  }
  .x:hover {
    background: var(--bg-3);
  }
  .panel {
    position: absolute;
    left: 16px;
    bottom: calc(100% - 4px);
    z-index: 20;
    width: 340px;
    padding: 6px 8px 8px;
    border-radius: 16px;
    background: var(--panel);
    box-shadow: var(--shadow), 0 0 0 0.5px var(--line);
    animation: popin 160ms var(--ease);
    transform-origin: bottom left;
  }
  .tabs {
    display: flex;
    gap: 4px;
    margin-bottom: 6px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--line-soft);
  }
  .tabs button {
    height: 28px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
    border-radius: 8px;
    font-size: 12.5px;
    font-weight: 500;
    color: var(--text-2);
  }
  .tabs button:hover {
    background: var(--bg-3);
  }
  .tabs button[aria-selected="true"] {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .emoji {
    display: grid;
    grid-template-columns: repeat(9, 1fr);
    gap: 2px;
  }
  .stickers {
    max-height: 260px;
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 4px;
  }
  .stk {
    position: relative;
    aspect-ratio: 1;
    display: grid;
    place-items: center;
    border-radius: 10px;
    transition: background 120ms var(--ease), transform 160ms var(--ease);
  }
  .stk:hover {
    background: var(--bg-3);
    transform: scale(1.04);
  }
  .stk img {
    width: 84%;
    height: 84%;
    object-fit: contain;
  }
  .stk .ph {
    width: 60%;
    height: 60%;
    border-radius: 50%;
    background: var(--bg-3);
  }
  .favmark {
    position: absolute;
    top: 4px;
    right: 4px;
    color: var(--accent);
    display: grid;
  }
  .pempty {
    margin: 0;
    padding: 18px 10px;
    color: var(--text-2);
    font-size: 12.5px;
    text-wrap: pretty;
  }
  .context {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 4px 6px;
    padding: 7px 6px 7px 8px;
    border-radius: 14px;
    background: var(--panel);
    box-shadow: 0 0 0 1px var(--line-soft);
    animation: popin 160ms var(--ease);
  }
  .cic {
    flex: none;
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border-radius: 9px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .cbody {
    flex: 1;
    min-width: 0;
    display: grid;
    font-size: 12.5px;
  }
  .cbody b {
    font-weight: 600;
    color: var(--accent);
  }
  .cbody span {
    color: var(--text-2);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .emoji button {
    height: 32px;
    border-radius: 8px;
    font-size: 19px;
    line-height: 1;
  }
  .emoji button:hover {
    background: var(--bg-3);
  }
  .spin {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid color-mix(in srgb, var(--on-accent) 35%, transparent);
    border-top-color: var(--on-accent);
    animation: spin 800ms linear infinite;
  }
  .spin.dark {
    border-color: var(--line);
    border-top-color: var(--text-2);
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
