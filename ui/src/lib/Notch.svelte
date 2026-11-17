<script lang="ts">
  import { onMount, tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { api, on } from "./api";
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
  let expanded = $state(false);
  let draft = $state("");
  let busy = $state(false);
  let suggesting = $state(false);
  let failed = $state(false);
  let hovered = $state(false);
  let aiReady = $state(false);
  let input: HTMLTextAreaElement | undefined = $state();
  let timer: ReturnType<typeof setTimeout> | undefined;

  const preview = $derived(msg && msg.chat_jid === "preview@notchat");
  const body = $derived.by(() => {
    if (!msg) return "";
    if (msg.text) return msg.text;
    return ["image", "video", "audio", "document", "sticker"].includes(msg.kind) ? t(`kind.${msg.kind}` as Key) : "";
  });

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

  onMount(() => {
    document.documentElement.classList.add("notch-root");
    void on<NotchMessage>("notch-message", async (m) => {
      const sameChat = msg?.chat_jid === m.chat_jid;
      msg = m;
      if (!sameChat) {
        draft = "";
        failed = false;
        if (expanded) {
          expanded = false;
          await api.notchExpand(false);
        }
      }
      aiReady = await api.aiConfig().then((c) => c.provider !== null && c.model !== "", () => false);
      arm();
    });
  });
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="notch"
  style:padding-top="{msg?.notch_height ?? 0}px"
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
    <div class="row">
      <button class="main" onclick={() => (preview ? close() : api.notchOpenChat(msg!.chat_jid))}>
        <span class="title">
          {msg.title}{#if msg.sender}<span class="sender"> · {msg.sender}</span>{/if}
        </span>
        <span class="text" class:clamp={!expanded}>{body}</span>
      </button>
      {#if !expanded && !preview}
        <button class="pill" onclick={expand}>{t("notch.reply")}</button>
      {/if}
      <button class="x" onclick={close} aria-label={t("notch.dismiss")}><Icon name="close" size={14} /></button>
    </div>
    {#if expanded}
      <div class="reply">
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
    {/if}
  {/if}
</div>

<style>
  :global(html.notch-root),
  :global(html.notch-root body) {
    background: #000;
  }
  .notch {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: #000;
    color: #f5f3ee;
    font-size: 13px;
  }
  .row {
    flex: none;
    height: 64px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px 0 18px;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 1px;
    text-align: left;
    color: inherit;
  }
  .title {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sender {
    font-weight: 400;
    color: #a9a59b;
  }
  .text {
    color: #cfcbc2;
    overflow: hidden;
  }
  .text.clamp {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    line-height: 1.3;
  }
  .row:has(.text.clamp) .text {
    font-size: 12.5px;
  }
  .pill {
    flex: none;
    height: 26px;
    padding: 0 12px;
    border-radius: 13px;
    background: #ff7a3d;
    color: #1a0d05;
    font-size: 12px;
    font-weight: 600;
    transition: transform 160ms var(--ease);
  }
  .pill:active {
    transform: scale(0.96);
  }
  .x {
    flex: none;
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: #a9a59b;
  }
  .x:hover {
    background: #26251f;
    color: #f5f3ee;
  }
  .reply {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 8px;
    align-items: flex-end;
    padding: 0 12px 12px 14px;
    animation: drop 220ms var(--ease);
  }
  @keyframes drop {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }
  }
  textarea {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 8px 12px;
    border: 0;
    border-radius: 14px;
    background: #1c1b18;
    color: #f5f3ee;
    caret-color: #ff7a3d;
    resize: none;
    outline: none;
    line-height: 1.35;
    user-select: text;
    -webkit-user-select: text;
  }
  textarea::placeholder {
    color: #8d897f;
  }
  textarea:focus-visible {
    box-shadow: inset 0 0 0 1.5px #ff7a3d;
  }
  .acts {
    flex: none;
    display: grid;
    justify-items: end;
    gap: 6px;
  }
  .ghost {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 24px;
    padding: 0 8px;
    border-radius: 12px;
    color: #ff9a68;
    font-size: 12px;
    font-weight: 500;
  }
  .ghost:hover:not(:disabled) {
    background: #26251f;
  }
  .ghost:disabled {
    opacity: 0.5;
  }
  .send {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: #ff7a3d;
    color: #1a0d05;
    transition: opacity 160ms var(--ease);
  }
  .send:disabled {
    opacity: 0.35;
  }
  .fail {
    font-size: 11px;
    color: #ff8a80;
    max-width: 110px;
    text-align: right;
  }
  :global(html.notch-root) :focus-visible {
    outline-color: #ff7a3d;
  }
</style>
