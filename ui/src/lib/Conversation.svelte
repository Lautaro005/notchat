<script lang="ts">
  import { tick } from "svelte";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import { api } from "./api";
  import { app, chatTitle, loadOlder, send } from "./app.svelte";
  import { dayKey, dayLabel, t, time, type Key } from "./i18n";
  import type { Message } from "./api";

  let draft = $state("");
  let sending = $state(false);
  let failed = $state(false);
  let scroller: HTMLElement | undefined = $state();
  let input: HTMLTextAreaElement | undefined = $state();

  const chat = $derived(app.chats.find((c) => c.jid === app.selected) ?? null);
  const title = $derived(chat ? chatTitle(chat) : app.selected ? chatTitle({ jid: app.selected, name: "" }) : "");
  const isGroup = $derived(chat?.is_group ?? app.selected?.endsWith("@g.us") ?? false);

  const kindIcon = { image: "image", video: "image", audio: "mic", document: "doc", sticker: "image" } as const;

  function kindLabel(m: Message): string | null {
    return m.kind in kindIcon ? t(`kind.${m.kind}` as Key) : null;
  }

  // Follow the tail when a message is appended or a chat is opened.
  $effect(() => {
    void app.tail;
    void tick().then(() => scroller?.scrollTo({ top: scroller.scrollHeight }));
  });

  $effect(() => {
    void app.selected;
    draft = "";
    failed = false;
    void tick().then(() => input?.focus());
  });

  async function older() {
    if (!scroller) return;
    const before = scroller.scrollHeight;
    await loadOlder();
    await tick();
    scroller.scrollTop += scroller.scrollHeight - before;
  }

  function onScroll() {
    if (scroller && scroller.scrollTop < 80 && app.hasOlder && !app.loadingOlder) void older();
  }

  async function submit() {
    const text = draft.trim();
    if (!text || sending) return;
    sending = true;
    failed = false;
    try {
      await send(text);
      draft = "";
    } catch {
      failed = true;
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
    }
  }
</script>

{#if !app.selected}
  <section class="blank" data-tauri-drag-region>
    <strong>{t("chat.pick")}</strong>
    <span>{t("chat.pickHint")}</span>
  </section>
{:else}
  <section class="conv">
    <header data-tauri-drag-region>
      <Avatar {title} group={isGroup} size={30} />
      <div>
        <h1>{title}</h1>
        {#if isGroup}<span>{t("chat.group")}</span>{/if}
      </div>
      {#if chat && (isGroup ? app.auto.groups : app.auto.chats) === "selected"}
        <button
          class="autobtn"
          class:on={chat.auto_reply}
          aria-pressed={chat.auto_reply}
          title={app.auto.enabled ? t("chat.auto") : t("chat.autoOff")}
          onclick={() => api.setChatAutoReply(chat.jid, !chat.auto_reply)}
        >
          <Icon name="spark" size={14} />Auto
        </button>
      {/if}
    </header>

    <div class="log scroll" bind:this={scroller} onscroll={onScroll}>
      {#if app.hasOlder}
        <button class="btn quiet older" onclick={older} disabled={app.loadingOlder}>{t("chat.older")}</button>
      {/if}
      {#each app.messages as m, i (m.id)}
        {@const prev = app.messages[i - 1]}
        {#if !prev || dayKey(prev.ts) !== dayKey(m.ts)}
          <div class="day">{dayLabel(m.ts)}</div>
        {/if}
        {@const joined = prev && prev.from_me === m.from_me && prev.sender_jid === m.sender_jid && m.ts - prev.ts < 300}
        <article class="msg" class:me={m.from_me} class:joined class:fresh={m.id === app.fresh}>
          {#if isGroup && !m.from_me && !joined}
            <span class="who">{m.sender_name || chatTitle({ jid: m.sender_jid, name: "" })}</span>
          {/if}
          <div class="bubble">
            {#if kindLabel(m)}
              <span class="kind"><Icon name={kindIcon[m.kind as keyof typeof kindIcon]} size={16} />{kindLabel(m)}</span>
            {/if}
            {#if m.text}<span class="text">{m.text}</span>{/if}
            <time class="num">
              {#if m.by_ai}<Icon name="spark" size={11} />{/if}{time(m.ts)}
            </time>
          </div>
        </article>
      {/each}
    </div>

    <footer>
      {#if failed}<p class="fail" role="alert">{t("chat.sendFailed")}</p>{/if}
      <div class="composer">
        <textarea
          bind:this={input}
          bind:value={draft}
          rows="1"
          placeholder={t("chat.placeholder")}
          aria-label={t("chat.placeholder")}
          onkeydown={onKey}
        ></textarea>
        <button class="sendbtn" onclick={submit} disabled={!draft.trim() || sending} aria-label={t("chat.send")}>
          <Icon name="send" size={18} />
        </button>
      </div>
    </footer>
  </section>
{/if}

<style>
  .blank,
  .conv {
    flex: 1;
    min-width: 0;
    background: var(--bg);
  }
  .blank {
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 4px;
    color: var(--text-2);
  }
  .blank strong {
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
  }
  .conv {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  header {
    flex: none;
    height: 52px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 20px;
    border-bottom: 1px solid var(--line);
  }
  header > :not(button) {
    pointer-events: none;
  }
  header > div {
    flex: 1;
    min-width: 0;
  }
  .autobtn {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 10px;
    border-radius: 13px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
    box-shadow: inset 0 0 0 1px var(--line);
    transition: background 160ms var(--ease), color 160ms var(--ease);
  }
  .autobtn.on {
    background: var(--accent);
    color: var(--on-accent);
    box-shadow: none;
  }
  h1 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }
  header span {
    font-size: 11px;
    color: var(--text-3);
  }
  .log {
    flex: 1;
    min-height: 0;
    padding: 16px 20px 8px;
    display: flex;
    flex-direction: column;
  }
  .log > :first-child {
    margin-top: auto;
  }
  .older {
    align-self: center;
    margin-bottom: 8px;
  }
  .day {
    align-self: center;
    margin: 16px 0 10px;
    font-size: 11px;
    font-weight: 500;
    color: var(--text-3);
  }
  .msg {
    display: grid;
    justify-items: start;
    margin-top: 8px;
  }
  .msg.fresh {
    animation: rise 220ms var(--ease);
  }
  .msg.joined {
    margin-top: 2px;
  }
  .msg.me {
    justify-items: end;
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
  .who {
    margin: 0 0 3px 12px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }
  .bubble {
    max-width: min(68%, 560px);
    padding: 7px 12px;
    border-radius: var(--r-l);
    background: var(--bubble);
    font-size: 14px;
    line-height: 1.38;
    user-select: text;
    -webkit-user-select: text;
    cursor: text;
  }
  .me .bubble {
    background: var(--ink);
    color: var(--on-ink);
  }
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .kind {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 500;
    opacity: 0.8;
  }
  time {
    float: right;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    margin: 6px 0 -2px 10px;
    font-size: 10.5px;
    opacity: 0.62;
    user-select: none;
    -webkit-user-select: none;
  }
  time :global(svg) {
    color: var(--accent);
    opacity: 1;
  }
  footer {
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
    gap: 6px;
    padding: 4px 4px 4px 14px;
    border-radius: var(--r-l);
    background: var(--bg-2);
    border: 1px solid var(--line);
    transition: border-color 160ms var(--ease);
  }
  .composer:focus-within {
    border-color: var(--text-3);
  }
  textarea {
    flex: 1;
    min-width: 0;
    max-height: 132px;
    padding: 5px 0;
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
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--accent);
    color: var(--on-accent);
    transition: opacity 160ms var(--ease), transform 160ms var(--ease);
  }
  .sendbtn:disabled {
    opacity: 0;
    transform: scale(0.8);
  }
  .sendbtn:active:not(:disabled) {
    transform: scale(0.94);
  }
</style>
