<script lang="ts">
  import { tick } from "svelte";
  import Avatar from "./Avatar.svelte";
  import ChatInfo from "./ChatInfo.svelte";
  import Composer from "./Composer.svelte";
  import Icon from "./Icon.svelte";
  import MediaView from "./MediaView.svelte";
  import VoicePlayer from "./VoicePlayer.svelte";
  import { api } from "./api";
  import { app, chatTitle, isMuted, loadOlder, openMenu, toast, type MenuItem } from "./app.svelte";
  import { chatMenu } from "./chatMenu";
  import { bytes, dayKey, dayLabel, t, time, type Key } from "./i18n";
  import type { Message } from "./api";

  let scroller: HTMLElement | undefined = $state();
  let composer: Composer | undefined = $state();
  let files = $state<File[]>([]);
  let dragging = $state(0);
  let lightbox = $state<string | null>(null);

  const chat = $derived(app.chats.find((c) => c.jid === app.selected) ?? null);
  const title = $derived(chat ? chatTitle(chat) : app.selected ? chatTitle({ jid: app.selected, name: "" }) : "");
  const isGroup = $derived(chat?.is_group ?? app.selected?.endsWith("@g.us") ?? false);

  const visual = new Set(["image", "video", "sticker"]);

  // Follow the tail when a message is appended or a chat is opened.
  $effect(() => {
    void app.tail;
    void tick().then(() => scroller?.scrollTo({ top: scroller.scrollHeight }));
  });

  async function older() {
    if (!scroller) return;
    const before = scroller.scrollHeight;
    await loadOlder();
    await tick();
    scroller.scrollTop += scroller.scrollHeight - before;
  }

  function onScroll() {
    if (scroller && scroller.scrollTop < 120 && app.hasOlder && !app.loadingOlder) void older();
  }

  function sender(m: Message): string {
    if (m.sender_name) return m.sender_name;
    const known = app.chats.find((c) => c.jid === m.sender_jid);
    return known ? chatTitle(known) : chatTitle({ jid: m.sender_jid, name: "" });
  }

  // A stable, low-chroma colour per group member so names are easy to scan.
  function hue(jid: string): number {
    return [...jid].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 11);
  }

  function messageMenu(e: MouseEvent, m: Message) {
    const items: MenuItem[] = [];
    if (m.text) {
      items.push({ label: t("menu.copy"), icon: "doc", action: () => void navigator.clipboard.writeText(m.text) });
    }
    if (m.has_media) {
      items.push({
        label: t("menu.save"),
        icon: "download",
        action: () =>
          api
            .saveMedia(m.chat_jid, m.id)
            .then(() => toast(t("toast.saved")))
            .catch(() => toast(t("toast.failed"))),
      });
    }
    if (items.length) openMenu(e, items);
  }

  async function saveDoc(m: Message) {
    try {
      await api.saveMedia(m.chat_jid, m.id);
      toast(t("toast.saved"));
    } catch {
      toast(t("toast.failed"));
    }
  }

  function extOf(m: Message): string {
    const fromName = /\.([a-z0-9]{1,5})$/i.exec(m.file_name)?.[1];
    if (fromName) return fromName.toUpperCase();
    return (m.mime.split("/")[1] ?? "").split(/[;+.-]/)[0].toUpperCase().slice(0, 5);
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    dragging = 0;
    composer?.add([...(e.dataTransfer?.files ?? [])]);
  }

  function headerMore(e: MouseEvent) {
    if (!chat) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openMenu({ x: r.right - 220, y: r.bottom + 6 }, chatMenu(chat, { openInfo: () => (app.infoOpen = true) }));
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && lightbox && (lightbox = null)} />

{#if !app.selected}
  <section class="blank" data-tauri-drag-region>
    <div class="mark"><Icon name="send" size={30} /></div>
    <strong>{t("chat.pick")}</strong>
    <span>{t("chat.pickHint")}</span>
  </section>
{:else}
  <div class="pane">
    <section
      class="conv"
      aria-label={title}
      ondragenter={(e) => {
        if (e.dataTransfer?.types.includes("Files")) dragging++;
      }}
      ondragleave={() => (dragging = Math.max(0, dragging - 1))}
      ondragover={(e) => e.preventDefault()}
      ondrop={onDrop}
    >
      <header data-tauri-drag-region>
        <button class="who-btn" onclick={() => (app.infoOpen = !app.infoOpen)} disabled={!chat} title={t("chat.tapInfo")}>
          <Avatar {title} jid={app.selected} group={isGroup} size={36} />
          <span class="titles">
            <h1>{title}</h1>
            <span class="subtitle">
              {#if chat && isMuted(chat)}<Icon name="mute" size={12} />{/if}
              {isGroup ? t("chat.group") : t("chat.tapInfo")}
            </span>
          </span>
        </button>
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
        {#if chat}
          <button class="icon-btn" class:on={app.infoOpen} onclick={() => (app.infoOpen = !app.infoOpen)} aria-label={t("chat.info")} title={t("chat.info")}>
            <Icon name="info" size={18} />
          </button>
          <button class="icon-btn" onclick={headerMore} aria-label={t("info.more")} title={t("info.more")}>
            <Icon name="more" size={18} />
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
            <div class="day"><span>{dayLabel(m.ts)}</span></div>
          {/if}
          {@const joined = prev && dayKey(prev.ts) === dayKey(m.ts) && prev.from_me === m.from_me && prev.sender_jid === m.sender_jid && m.ts - prev.ts < 300}
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <article
            class="msg"
            class:me={m.from_me}
            class:joined
            class:fresh={m.id === app.fresh}
            oncontextmenu={(e) => messageMenu(e, m)}
          >
            <div
              class="bubble"
              class:visual={visual.has(m.kind) && !m.text}
              class:sticker={m.kind === "sticker"}
              class:tail={!joined}
            >
              {#if isGroup && !m.from_me && !joined}
                <span class="who" style:--h={hue(m.sender_jid)}>{sender(m)}</span>
              {/if}
              {#if visual.has(m.kind)}
                <MediaView {m} onopen={(src) => (lightbox = src)} />
              {:else if m.kind === "voice" || m.kind === "audio"}
                <VoicePlayer {m} />
              {:else if m.kind === "document"}
                <button class="doc" onclick={() => saveDoc(m)} disabled={!m.has_media} title={t("menu.save")}>
                  <span class="doc-ic"><Icon name="doc" size={20} /><b>{extOf(m)}</b></span>
                  <span class="doc-meta">
                    <span class="doc-name">{m.file_name || t("kind.document")}</span>
                    <small class="num">{[m.size ? bytes(m.size) : "", extOf(m)].filter(Boolean).join(" · ")}</small>
                  </span>
                  {#if m.has_media}<Icon name="download" size={17} />{/if}
                </button>
              {:else if m.kind !== "text"}
                <span class="kind">{t(`kind.${m.kind}` as Key)}</span>
              {/if}
              {#if m.text}<span class="text" class:aftermedia={visual.has(m.kind)}>{m.text}</span>{/if}
              <time class="num">
                {#if m.by_ai}<Icon name="spark" size={11} />{/if}{time(m.ts)}
              </time>
            </div>
          </article>
        {/each}
      </div>

      <Composer bind:this={composer} bind:files />

      {#if dragging > 0}
        <div class="drop" aria-hidden="true">
          <div><Icon name="download" size={28} /><span>{t("chat.drop")}</span></div>
        </div>
      {/if}
    </section>

    {#if app.infoOpen && chat}
      <ChatInfo {chat} />
    {/if}
  </div>
{/if}

{#if lightbox}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="lightbox" onclick={() => (lightbox = null)}>
    <img src={lightbox} alt="" />
    <button class="icon-btn close" aria-label={t("chat.close")}><Icon name="close" size={20} /></button>
  </div>
{/if}

<style>
  .blank,
  .pane {
    flex: 1;
    min-width: 0;
    margin: 8px 8px 8px 0;
    border-radius: 14px;
    background: var(--bg);
    box-shadow: var(--shadow-s), 0 0 0 0.5px var(--line);
    overflow: hidden;
  }
  .blank {
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 4px;
    color: var(--text-2);
    background-image: radial-gradient(var(--wall-dot) 1px, transparent 1.2px);
    background-size: 18px 18px;
  }
  .blank .mark {
    width: 64px;
    height: 64px;
    margin-bottom: 14px;
    display: grid;
    place-items: center;
    border-radius: 20px;
    background: var(--ink);
    color: var(--on-ink);
    transform: rotate(-6deg);
    box-shadow: var(--shadow);
  }
  .blank strong {
    font-family: var(--rounded);
    font-size: 17px;
    font-weight: 700;
    color: var(--text);
  }
  .pane {
    position: relative;
    display: flex;
    min-height: 0;
  }
  /* On narrow windows the info panel floats over the conversation. */
  @media (max-width: 1040px) {
    .pane > :global(.info) {
      position: absolute;
      top: 0;
      right: 0;
      bottom: 0;
      z-index: 5;
      box-shadow: var(--shadow);
    }
  }
  .conv {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background-image: radial-gradient(var(--wall-dot) 1px, transparent 1.2px);
    background-size: 18px 18px;
  }
  header {
    flex: none;
    height: 58px;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 10px 0 8px;
    background: color-mix(in srgb, var(--bg) 88%, transparent);
    backdrop-filter: blur(16px) saturate(1.4);
    border-bottom: 1px solid var(--line-soft);
    z-index: 2;
  }
  .who-btn {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 8px;
    border-radius: 12px;
    text-align: left;
  }
  .who-btn:hover:not(:disabled) {
    background: var(--bg-2);
  }
  .titles {
    min-width: 0;
    display: grid;
  }
  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .subtitle {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .icon-btn.on {
    background: var(--bg-3);
    color: var(--text);
  }
  .autobtn {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    margin-right: 4px;
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
  .log {
    flex: 1;
    min-height: 0;
    padding: 12px 6% 10px;
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
    position: sticky;
    top: 4px;
    z-index: 1;
    align-self: center;
    margin: 14px 0 8px;
  }
  .day span {
    display: block;
    padding: 4px 11px;
    border-radius: 999px;
    background: var(--panel);
    box-shadow: var(--shadow-s);
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }
  .msg {
    display: flex;
    justify-content: flex-start;
    margin-top: 8px;
  }
  .msg.me {
    justify-content: flex-end;
  }
  .msg.joined {
    margin-top: 2px;
  }
  .msg.fresh {
    animation: rise 240ms var(--ease);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
  }
  .bubble {
    position: relative;
    max-width: min(70%, 560px);
    padding: 7px 11px;
    border-radius: 18px;
    background: var(--bubble);
    box-shadow: 0 1px 1px rgb(0 0 0 / 0.04), 0 0 0 1px var(--bubble-line);
    font-size: 14px;
    line-height: 1.38;
    user-select: text;
    -webkit-user-select: text;
    cursor: text;
  }
  .bubble.tail {
    border-top-left-radius: 6px;
  }
  .me .bubble {
    background: var(--ink);
    color: var(--on-ink);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.12);
  }
  .me .bubble.tail {
    border-top-left-radius: 18px;
    border-top-right-radius: 6px;
  }
  .bubble.visual {
    padding: 4px;
  }
  .bubble.visual time {
    position: absolute;
    right: 10px;
    bottom: 9px;
    margin: 0;
    padding: 2px 7px;
    border-radius: 9px;
    background: rgb(0 0 0 / 0.45);
    color: #fff;
    opacity: 1;
  }
  .bubble.sticker {
    background: none;
    box-shadow: none;
  }
  .bubble.visual .who {
    margin: 3px 7px 5px;
  }
  .who {
    display: block;
    margin-bottom: 2px;
    font-size: 12px;
    font-weight: 600;
    color: oklch(0.55 0.13 var(--h));
  }
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .text.aftermedia {
    display: block;
    padding: 6px 7px 0;
  }
  .kind {
    font-style: italic;
    opacity: 0.75;
  }
  time {
    float: right;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    margin: 7px 0 -3px 10px;
    font-size: 10.5px;
    opacity: 0.6;
    user-select: none;
    -webkit-user-select: none;
  }
  time :global(svg) {
    color: var(--accent);
  }
  .doc {
    width: 270px;
    max-width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 2px 0;
    padding: 8px 10px 8px 8px;
    border-radius: 12px;
    background: color-mix(in srgb, currentColor 7%, transparent);
    text-align: left;
  }
  .doc:hover:not(:disabled) {
    background: color-mix(in srgb, currentColor 12%, transparent);
  }
  .doc-ic {
    position: relative;
    flex: none;
    width: 38px;
    height: 44px;
    display: grid;
    place-items: center;
    border-radius: 8px;
    background: var(--accent);
    color: var(--on-accent);
  }
  .doc-ic b {
    position: absolute;
    bottom: 3px;
    font-size: 8px;
    letter-spacing: 0.04em;
  }
  .doc-ic :global(svg) {
    margin-top: -8px;
  }
  .doc-meta {
    flex: 1;
    min-width: 0;
    display: grid;
  }
  .doc-name {
    font-weight: 600;
    font-size: 13px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .doc-meta small {
    font-size: 11px;
    opacity: 0.65;
  }
  .drop {
    position: absolute;
    inset: 8px;
    z-index: 10;
    display: grid;
    place-items: center;
    border-radius: 12px;
    border: 2px dashed var(--accent);
    background: color-mix(in srgb, var(--bg) 82%, transparent);
    backdrop-filter: blur(4px);
    pointer-events: none;
    animation: fadein 140ms var(--ease);
  }
  .drop div {
    display: grid;
    justify-items: center;
    gap: 8px;
    color: var(--accent);
    font-weight: 600;
  }
  @keyframes fadein {
    from {
      opacity: 0;
    }
  }
  .lightbox {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: grid;
    place-items: center;
    padding: 48px;
    background: rgb(10 9 8 / 0.86);
    animation: fadein 160ms var(--ease);
    cursor: zoom-out;
  }
  .lightbox img {
    max-width: 100%;
    max-height: 100%;
    border-radius: 8px;
    box-shadow: 0 30px 80px rgb(0 0 0 / 0.5);
  }
  .lightbox .close {
    position: absolute;
    top: 16px;
    right: 16px;
    color: #fff;
  }
</style>
