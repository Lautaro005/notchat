<script lang="ts">
  import { onMount, tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { api, native, on } from "./api";
  import { t, type Key } from "./i18n";

  interface NotchMessage {
    chat_jid: string;
    title: string;
    sender: string;
    text: string;
    kind: string;
    is_group: boolean;
    notch_height: number;
  }

  const LINGER_MS = 6500;

  let msg = $state<NotchMessage | null>(null);
  let seq = $state(0);
  let expanded = $state(false);
  let draft = $state("");
  let busy = $state(false);
  let suggesting = $state(false);
  let failed = $state(false);
  let hovered = $state(false);
  let aiReady = $state(false);
  let avatar = $state<string | null>(null);
  let input: HTMLTextAreaElement | undefined = $state();
  let timer: ReturnType<typeof setTimeout> | undefined;

  const preview = $derived(msg && msg.chat_jid === "preview@notchat");
  const body = $derived.by(() => {
    if (!msg) return "";
    if (msg.text) return msg.kind === "poll" ? `📊 ${msg.text}` : msg.text;
    return ["image", "video", "audio", "voice", "document", "sticker"].includes(msg.kind) ? t(`kind.${msg.kind}` as Key) : "";
  });
  const initials = $derived(
    (msg?.title ?? "")
      .replace(/[^\p{L}\p{N}\s]/gu, "")
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase())
      .join(""),
  );

  function arm() {
    clearTimeout(timer);
    if (!expanded && !hovered) timer = setTimeout(close, LINGER_MS);
  }

  async function close() {
    clearTimeout(timer);
    expanded = false;
    draft = "";
    failed = false;
    msg = null;
    await api.notchHide();
  }

  async function expand() {
    if (!msg || preview) return;
    expanded = true;
    clearTimeout(timer);
    await api.notchExpand(true);
    await tick();
    input?.focus();
  }

  async function submit() {
    const text = draft.trim();
    if (!msg || !text || busy) return;
    busy = true;
    failed = false;
    try {
      await api.sendText(msg.chat_jid, text);
      await api.markRead(msg.chat_jid);
      await close();
    } catch {
      failed = true;
    } finally {
      busy = false;
    }
  }

  async function suggest() {
    if (!msg || suggesting) return;
    suggesting = true;
    try {
      const text = await api.suggestReply(msg.chat_jid);
      if (text) draft = text;
    } catch {
      /* the user can still type their own reply */
    } finally {
      suggesting = false;
      input?.focus();
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") void close();
    else if (e.key === "Enter" && !e.shiftKey && !e.isComposing && expanded) {
      e.preventDefault();
      void submit();
    }
  }

  async function show(m: NotchMessage) {
    const sameChat = msg?.chat_jid === m.chat_jid;
    msg = m;
    seq++;
    if (!sameChat) {
      draft = "";
      failed = false;
      avatar = null;
      if (expanded) {
        expanded = false;
        await api.notchExpand(false);
      }
      if (m.chat_jid.includes("@") && !m.chat_jid.endsWith("@notchat")) {
        api.avatar(m.chat_jid).then(
          (url) => msg?.chat_jid === m.chat_jid && (avatar = url),
          () => {},
        );
      }
    }
    aiReady = await api.aiConfig().then((c) => c.provider !== null && c.model !== "", () => false);
    arm();
  }

  onMount(() => {
    document.documentElement.classList.add("notch-root");
    void on<NotchMessage>("notch-message", (m) => void show(m));
    // Browser preview of the panel: `?view=notch` (development only).
    if (!native && import.meta.env.DEV && new URLSearchParams(location.search).get("view") === "notch") {
      msg = {
        chat_jid: "120363000000000001@g.us",
        title: "Fútbol de los jueves",
        sender: "Lucía R.",
        text: "Falta uno para completar, ¿alguien se suma a las 21?",
        kind: "text",
        is_group: true,
        notch_height: 32,
      };
      if (location.hash.includes("open")) expanded = true;
    }
  });
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="notch"
  class:open={expanded}
  style:--top="{msg?.notch_height ?? 0}px"
  onmouseenter={() => {
    hovered = true;
    clearTimeout(timer);
  }}
  onmouseleave={() => {
    hovered = false;
    arm();
  }}
>
  {#if msg}
    {#key seq}
      <div class="row">
        <button class="who" onclick={() => (preview ? close() : api.notchOpenChat(msg!.chat_jid))} aria-label={msg.title}>
          <span class="av" class:group={msg.is_group}>
            {#if avatar}<img src={avatar} alt="" />{:else if msg.is_group}<Icon name="group" size={17} />{:else}{initials || "·"}{/if}
          </span>
          <span class="lines">
            <span class="title">
              <b>{msg.title}</b>{#if msg.sender}<span class="sender">{msg.sender}</span>{/if}
            </span>
            <span class="text" class:clamp={!expanded}>{body}</span>
          </span>
        </button>
        {#if !expanded && !preview}
          <button class="reply" onclick={expand}><Icon name="reply" size={14} />{t("notch.reply")}</button>
        {/if}
        <button class="x" onclick={close} aria-label={t("notch.dismiss")}><Icon name="close" size={13} /></button>
      </div>
    {/key}
    {#if expanded}
      <div class="compose">
        <div class="field" class:busy={suggesting}>
          <textarea
            bind:this={input}
            bind:value={draft}
            rows="2"
            placeholder={suggesting ? t("ai.thinking") : t("chat.placeholder")}
            aria-label={t("chat.placeholder")}
            disabled={busy}
          ></textarea>
          <div class="acts">
            {#if failed}<span class="fail" role="alert">{t("notch.failed")}</span>{/if}
            {#if aiReady}
              <button class="ghost" onclick={suggest} disabled={suggesting}>
                <Icon name="spark" size={13} />{t("notch.suggest")}
              </button>
            {/if}
            <button class="send" onclick={submit} disabled={!draft.trim() || busy} aria-label={t("chat.send")}>
              <Icon name="send" size={15} />
            </button>
          </div>
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  :global(html.notch-root),
  :global(html.notch-root body) {
    background: #000;
  }
  :global(html.notch-root) :focus-visible {
    outline-color: #a598ff;
  }
  /* The panel is always the colour of the hardware notch, in both themes. */
  .notch {
    --n-text: #f4f2fb;
    --n-2: #a29db4;
    --n-3: #6f6a82;
    --n-field: #15141b;
    --n-hover: #1f1d27;
    --n-accent: #a598ff;
    --n-on-accent: #120d33;
    height: 100%;
    display: flex;
    flex-direction: column;
    padding-top: var(--top);
    background: #000;
    color: var(--n-text);
    font-size: 13px;
  }
  .row {
    flex: none;
    height: 70px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px 0 14px;
    animation: arrive 320ms var(--ease);
  }
  @keyframes arrive {
    from {
      opacity: 0;
      transform: translateY(-8px);
      filter: blur(6px);
    }
  }
  .who {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 11px;
    text-align: left;
    color: inherit;
  }
  .av {
    flex: none;
    width: 40px;
    height: 40px;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 50%;
    background: #2a2638;
    color: var(--n-text);
    font-family: var(--rounded);
    font-size: 14px;
    font-weight: 700;
  }
  .av.group {
    border-radius: 13px;
  }
  .av img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .lines {
    min-width: 0;
    display: grid;
    gap: 2px;
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
    white-space: nowrap;
  }
  .title b {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 600;
  }
  .sender {
    flex: none;
    max-width: 40%;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 12px;
    color: var(--n-2);
  }
  .text {
    color: #d3cfdf;
    font-size: 12.5px;
    line-height: 1.3;
    overflow: hidden;
  }
  .text.clamp {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .open .text {
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .reply {
    flex: none;
    height: 28px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0 12px 0 10px;
    border-radius: 14px;
    background: rgb(165 152 255 / 0.16);
    color: #cdc5ff;
    font-size: 12px;
    font-weight: 600;
    transition: background 160ms var(--ease), transform 160ms var(--ease);
  }
  .reply:hover {
    background: rgb(165 152 255 / 0.26);
  }
  .reply:active {
    transform: scale(0.96);
  }
  .x {
    flex: none;
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: var(--n-3);
    transition: background 140ms var(--ease), color 140ms var(--ease);
  }
  .x:hover {
    background: var(--n-hover);
    color: var(--n-text);
  }
  .compose {
    flex: 1;
    min-height: 0;
    padding: 0 12px 12px;
    animation: unfold 240ms var(--ease);
  }
  @keyframes unfold {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }
  }
  .field {
    height: 100%;
    display: flex;
    align-items: flex-end;
    gap: 8px;
    padding: 8px 8px 8px 12px;
    border-radius: 16px;
    background: var(--n-field);
    box-shadow: inset 0 0 0 1px #24222d;
    transition: box-shadow 160ms var(--ease);
  }
  .field:focus-within {
    box-shadow: inset 0 0 0 1.5px var(--n-accent);
  }
  .field.busy {
    animation: breathe 1.4s ease-in-out infinite;
  }
  @keyframes breathe {
    50% {
      box-shadow: inset 0 0 0 1.5px rgb(165 152 255 / 0.4);
    }
  }
  textarea {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: var(--n-text);
    caret-color: var(--n-accent);
    resize: none;
    outline: none;
    line-height: 1.38;
    user-select: text;
    -webkit-user-select: text;
  }
  textarea::placeholder {
    color: var(--n-3);
  }
  .acts {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .ghost {
    height: 28px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0 10px;
    border-radius: 14px;
    color: #cdc5ff;
    font-size: 12px;
    font-weight: 500;
  }
  .ghost:hover:not(:disabled) {
    background: var(--n-hover);
  }
  .ghost:disabled {
    opacity: 0.5;
  }
  .send {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--n-accent);
    color: var(--n-on-accent);
    transition: opacity 160ms var(--ease), transform 160ms var(--ease);
  }
  .send:active:not(:disabled) {
    transform: scale(0.94);
  }
  .send:disabled {
    opacity: 0.3;
  }
  .fail {
    max-width: 120px;
    font-size: 11px;
    color: #ff8a80;
    text-align: right;
  }
</style>
