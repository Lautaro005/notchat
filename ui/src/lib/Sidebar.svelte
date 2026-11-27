<script lang="ts">
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import Ticks from "./Ticks.svelte";
  import {
    AI,
    NOTES,
    app,
    applyTheme,
    chatTitle,
    isMuted,
    isUnread,
    openMenu,
    openSettings,
    remember,
    saveAutoReply,
    selectChat,
    type Theme,
  } from "./app.svelte";
  import { chatMenu, filterMenu, listMenu, newList } from "./chatMenu";
  import { stamp, t, tf, type Key } from "./i18n";
  import type { Chat } from "./api";

  let query = $state("");

  const MIN_W = 260;
  const MAX_W = 520;

  const archivedChats = $derived(app.chats.filter((c) => c.archived));
  const archivedUnread = $derived(archivedChats.filter(isUnread).length);
  const unreadCount = $derived(app.chats.filter((c) => !c.archived && isUnread(c)).length);

  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    let list = app.chats.filter((c) => c.archived === app.archived);
    if (!app.archived) {
      if (app.filter === "unread") list = list.filter(isUnread);
      else if (app.filter === "groups") list = list.filter((c) => c.is_group);
      else if (app.filter.startsWith("list:")) {
        const members = new Set(app.lists.find((l) => `list:${l.id}` === app.filter)?.jids ?? []);
        list = list.filter((c) => members.has(c.jid));
      }
    }
    if (q) list = list.filter((c) => chatTitle(c).toLowerCase().includes(q) || c.last_preview.toLowerCase().includes(q));
    return list;
  });

  const showAi = $derived(!app.archived && app.filter === "all" && !query.trim());

  /** Media-only messages are stored as "[kind]"; show them in the user's language. */
  function preview(text: string): { icon: string | null; text: string; italic: boolean } {
    if (text === "[deleted]") return { icon: null, text: t("msg.deleted"), italic: true };
    const kind = /^\[(image|video|audio|voice|document|sticker|viewonce)\]$/.exec(text)?.[1];
    if (!kind) return { icon: null, text, italic: false };
    const icons: Record<string, string> = { image: "image", video: "camera", audio: "mic", voice: "mic", document: "doc", sticker: "sticker", viewonce: "image" };
    const icon = icons[kind] ?? null;
    return { icon, text: t(`kind.${kind}` as Key), italic: false };
  }

  /** In groups the preview says who wrote the last message. */
  function author(chat: Chat): string {
    if (!chat.is_group || !chat.last_preview) return "";
    if (chat.last_from_me) return t("chat.you");
    const name = chat.last_sender;
    if (!name) return "";
    if (name.includes("@")) return chatTitle({ jid: name, name: "" });
    return name;
  }

  function emptyText(): string {
    if (query.trim()) return t("chats.noMatch");
    if (app.archived) return t("side.archivedEmpty");
    if (app.filter === "unread") return t("side.unreadEmpty");
    if (app.filter === "groups") return t("side.groupsEmpty");
    if (app.filter.startsWith("list:")) return t("side.listEmpty");
    return "";
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const rows = [...document.querySelectorAll<HTMLElement>(".chatlist .row")];
    const i = rows.indexOf(document.activeElement as HTMLElement);
    const next = rows[e.key === "ArrowDown" ? Math.min(i + 1, rows.length - 1) : Math.max(i - 1, 0)];
    if (next) {
      e.preventDefault();
      next.focus();
    }
  }

  function rowMenu(e: MouseEvent, chat: Chat) {
    openMenu(e, chatMenu(chat));
  }

  function moreButton(e: MouseEvent, chat: Chat) {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openMenu({ x: r.left, y: r.bottom + 4 }, chatMenu(chat));
  }

  const themes: Theme[] = ["system", "light", "dark"];
  const themeIcon = { system: "monitor", light: "sun", dark: "moon" } as const;
  function cycleTheme() {
    applyTheme(themes[(themes.indexOf(app.theme) + 1) % themes.length]);
  }

  // ---- resize -------------------------------------------------------------
  function startResize(e: PointerEvent) {
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const startW = app.sidebarWidth;
    const move = (ev: PointerEvent) => {
      app.sidebarWidth = Math.round(Math.min(MAX_W, Math.max(MIN_W, startW + ev.clientX - startX)));
    };
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      remember("sidebarWidth", app.sidebarWidth);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
  }

  function resizeKey(e: KeyboardEvent) {
    const step = e.shiftKey ? 40 : 12;
    if (e.key === "ArrowLeft") app.sidebarWidth = Math.max(MIN_W, app.sidebarWidth - step);
    else if (e.key === "ArrowRight") app.sidebarWidth = Math.min(MAX_W, app.sidebarWidth + step);
    else return;
    e.preventDefault();
    remember("sidebarWidth", app.sidebarWidth);
  }
</script>

<aside class="sidebar" style:width="{app.sidebarWidth}px">
  <header data-tauri-drag-region>
    {#if app.archived}
      <button class="icon-btn" onclick={() => (app.archived = false)} aria-label={t("side.back")} title={t("side.back")}>
        <Icon name="back" size={18} />
      </button>
      <h1>{t("side.archived")}</h1>
    {:else}
      <h1>{t("side.title")}</h1>
    {/if}
  </header>

  <div class="tools">
    <label class="search">
      <Icon name="search" size={15} />
      <input type="search" bind:value={query} placeholder={t("chats.search")} aria-label={t("chats.search")} />
    </label>

    {#if !app.archived}
      <div class="filters" role="tablist" aria-label={t("filters")}>
        {#each [["all", t("filter.all")], ["unread", t("filter.unread")], ["groups", t("filter.groups")]] as [id, label] (id)}
          <button
            role="tab"
            class="chip"
            aria-selected={app.filter === id}
            onclick={() => (app.filter = id)}
            oncontextmenu={(e) => openMenu(e, filterMenu(id))}
          >
            {label}
            {#if id === "unread" && unreadCount > 0}<span class="count num">{unreadCount}</span>{/if}
          </button>
        {/each}
        {#each app.lists as list (list.id)}
          <button
            role="tab"
            class="chip"
            aria-selected={app.filter === `list:${list.id}`}
            onclick={() => (app.filter = `list:${list.id}`)}
            oncontextmenu={(e) => openMenu(e, [...filterMenu(`list:${list.id}`), { separator: true }, ...listMenu(list.id)])}
          >
            {list.name}
          </button>
        {/each}
        <button class="chip add" onclick={() => newList()} aria-label={t("filter.newList")} title={t("filter.newList")}>
          <Icon name="plus" size={14} />
        </button>
      </div>
    {/if}
  </div>

  {#if app.status.link !== "connected"}
    <p class="banner" role="status">
      <span class="pulse"></span>
      {app.status.link === "disconnected" ? t("status.disconnected") : t("status.starting")}
    </p>
  {:else if app.history !== null}
    <p class="banner" role="status">
      <span class="bar" style:--p="{app.history}%"></span>
      {tf("side.history", { n: app.history })}
    </p>
  {/if}

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <nav class="chatlist scroll" aria-label={t("chats.title")} onkeydown={onKey}>
    {#if showAi}
      <button class="row" class:active={app.selected === AI} onclick={() => selectChat(AI)}>
        <span class="aimark"><Icon name="spark" size={19} /></span>
        <span class="body">
          <span class="top"><span class="name">{t("ai.name")}</span></span>
          <span class="bottom"><span class="preview"><span class="ptext">{app.ai.config.model || t("ai.sub")}</span></span></span>
        </span>
      </button>
    {/if}
    {#if showAi}
      <button class="row" class:active={app.selected === NOTES} onclick={() => selectChat(NOTES)}>
        <span class="notesmark"><Icon name="note" size={19} /></span>
        <span class="body">
          <span class="top"><span class="name">{t("notes.name")}</span></span>
          <span class="bottom"><span class="preview"><span class="ptext">{t("notes.sub")}</span></span></span>
        </span>
      </button>
    {/if}
    {#if showAi && archivedChats.length > 0}
      <button class="row archived-row" onclick={() => (app.archived = true)}>
        <span class="archmark"><Icon name="archive" size={18} /></span>
        <span class="name">{t("side.archived")}</span>
        {#if archivedUnread > 0}<span class="count num">{archivedUnread}</span>{/if}
      </button>
    {/if}

    {#each visible as chat (chat.jid)}
      {@const p = preview(chat.last_preview)}
      {@const by = author(chat)}
      {@const unread = isUnread(chat)}
      <div class="rowwrap">
        <button
          class="row"
          class:active={chat.jid === app.selected}
          class:unread
          onclick={() => selectChat(chat.jid)}
          oncontextmenu={(e) => rowMenu(e, chat)}
        >
          <Avatar title={chatTitle(chat)} jid={chat.jid} group={chat.is_group} size={44} />
          <span class="body">
            <span class="top">
              <span class="name">{chatTitle(chat)}</span>
              <time class="num">{stamp(chat.last_ts)}</time>
            </span>
            <span class="bottom">
              <span class="preview" class:italic={p.italic}>
                {#if !chat.is_group && chat.last_from_me && chat.last_status > 0}<Ticks status={chat.last_status} size={15} />{/if}
                {#if by}<span class="by">{by}:</span>{/if}
                {#if p.icon}<Icon name={p.icon as never} size={13} />{/if}<span class="ptext">{p.text}</span>
              </span>
              <span class="marks">
                {#if isMuted(chat)}<span class="mark" title={t("chat.muted")}><Icon name="mute" size={14} /></span>{/if}
                {#if chat.pinned > 0}<span class="mark"><Icon name="pin" size={14} /></span>{/if}
                {#if chat.unread > 0}
                  <span class="badge num" class:quiet={isMuted(chat)}>{chat.unread > 99 ? "99+" : chat.unread}</span>
                {:else if chat.marked_unread}
                  <span class="badge dot"></span>
                {/if}
              </span>
            </span>
          </span>
        </button>
        <button class="more" tabindex="-1" aria-hidden="true" onclick={(e) => moreButton(e, chat)}>
          <Icon name="chevron" size={14} />
        </button>
      </div>
    {:else}
      <div class="empty">
        {#if app.chats.length === 0 && !query.trim()}
          <strong>{t("chats.empty")}</strong>
          <span>{t("chats.emptyHint")}</span>
        {:else}
          <span>{emptyText()}</span>
        {/if}
      </div>
    {/each}
  </nav>

  {#if app.update}
    <button class="update" onclick={() => openSettings("updates")}>
      <Icon name="download" size={15} />
      <span>{tf("side.update", { v: app.update.version })}</span>
      <span class="go">{t("side.updateAction")}</span>
    </button>
  {/if}

  <footer>
    {#if app.auto.enabled}
      <span class="auto"><Icon name="spark" size={14} />{t("side.autoOn")}</span>
      <button class="btn quiet pause" onclick={() => saveAutoReply({ enabled: false })}>{t("side.pause")}</button>
    {:else if app.backfill && app.status.link === "connected"}
      <span
        class="conn sync"
        role="status"
        title={t("side.syncHint")}
        style:--p="{Math.round((app.backfill.done / app.backfill.total) * 100)}%"
      >
        <span class="ring" aria-hidden="true"></span>{tf("side.syncing", { n: Math.round((app.backfill.done / app.backfill.total) * 100) })}
      </span>
    {:else}
      <span class="conn" class:on={app.status.link === "connected"}>
        <span class="dot"></span>{app.status.link === "connected" ? "WhatsApp" : t("status.starting")}
      </span>
    {/if}
    <button
      class="icon-btn"
      onclick={cycleTheme}
      aria-label="{t('theme.label')}: {t(`theme.${app.theme}` as Key)}"
      title="{t('theme.label')}: {t(`theme.${app.theme}` as Key)}"
    >
      <Icon name={themeIcon[app.theme]} size={17} />
    </button>
    <button
      class="icon-btn"
      class:on={app.settingsOpen}
      onclick={() => openSettings()}
      aria-label={t("set.open")}
      title={t("set.open")}
    >
      <Icon name="gear" size={17} />
    </button>
  </footer>

  <div
    class="resizer"
    role="slider"
    tabindex="0"
    aria-orientation="vertical"
    aria-label={t("side.resize")}
    aria-valuemin={MIN_W}
    aria-valuemax={MAX_W}
    aria-valuenow={app.sidebarWidth}
    onpointerdown={startResize}
    onkeydown={resizeKey}
    ondblclick={() => {
      app.sidebarWidth = 320;
      remember("sidebarWidth", 320);
    }}
  ></div>
</aside>

<style>
  .sidebar {
    position: relative;
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  header {
    flex: none;
    height: 84px;
    display: flex;
    align-items: flex-end;
    gap: 4px;
    padding: 0 16px 8px;
  }
  header .icon-btn {
    margin-left: -8px;
  }
  h1 {
    margin: 0;
    font-family: var(--rounded);
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
    pointer-events: none;
  }
  .tools {
    flex: none;
    display: grid;
    gap: 10px;
    padding: 0 12px 10px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 11px;
    border-radius: var(--r-m);
    background: var(--bg-3);
    color: var(--text-3);
  }
  .search:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: none;
    color: var(--text);
    outline: none;
  }
  .search input::placeholder {
    color: var(--text-3);
  }
  .search input::-webkit-search-cancel-button {
    display: none;
  }
  .filters {
    display: flex;
    gap: 6px;
    overflow-x: auto;
    scrollbar-width: none;
    mask-image: linear-gradient(90deg, #000 calc(100% - 16px), transparent);
    padding-right: 12px;
  }
  .filters::-webkit-scrollbar {
    display: none;
  }
  .chip {
    flex: none;
    height: 28px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 12px;
    border-radius: 999px;
    font-size: 12.5px;
    font-weight: 500;
    color: var(--text-2);
    box-shadow: inset 0 0 0 1px var(--line);
    transition: background 160ms var(--ease), color 160ms var(--ease), box-shadow 160ms var(--ease);
  }
  .chip:hover {
    color: var(--text);
    background: var(--bg-3);
  }
  .chip[aria-selected="true"] {
    background: var(--accent);
    color: var(--on-accent);
    box-shadow: none;
  }
  .chip.add {
    width: 28px;
    padding: 0;
    justify-content: center;
  }
  .count {
    min-width: 17px;
    height: 17px;
    padding: 0 5px;
    display: grid;
    place-items: center;
    border-radius: 9px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 10.5px;
    font-weight: 700;
  }
  .banner {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 12px 8px;
    padding: 7px 11px;
    overflow: hidden;
    border-radius: var(--r-m);
    background: var(--accent-soft);
    font-size: 12px;
    font-weight: 500;
  }
  .pulse {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
    animation: pulse 1.4s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }
  .bar {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 2px;
    width: var(--p);
    background: var(--accent);
    transition: width 400ms var(--ease);
  }
  .chatlist {
    flex: 1;
    min-height: 0;
    padding: 0 8px 8px;
  }
  .rowwrap {
    position: relative;
    content-visibility: auto;
    contain-intrinsic-size: auto 68px;
  }
  .row {
    width: 100%;
    min-width: 0;
    height: 68px;
    overflow: hidden;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 10px;
    border-radius: 12px;
    text-align: left;
    transition: background 140ms var(--ease);
  }
  .row:hover {
    background: color-mix(in srgb, var(--bg-3) 70%, transparent);
  }
  .row.active {
    background: var(--panel);
    box-shadow: var(--shadow-s);
  }
  .body {
    flex: 1;
    min-width: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 3px;
  }
  .top,
  .bottom {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .name {
    flex: 1;
    min-width: 0;
    font-size: 13.5px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  time {
    font-size: 11px;
    color: var(--text-3);
  }
  .row.unread time {
    color: var(--accent);
    font-weight: 600;
  }
  .preview {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-2);
    font-size: 12.5px;
    white-space: nowrap;
  }
  .preview.italic .ptext {
    font-style: italic;
  }
  .ptext {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .by {
    flex: none;
    max-width: 45%;
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 500;
    color: var(--text);
  }
  .preview :global(svg) {
    flex: none;
    color: var(--text-3);
  }
  .row.unread .preview {
    color: var(--text);
  }
  .marks {
    flex: none;
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-3);
  }
  .mark {
    display: grid;
  }
  .badge {
    min-width: 19px;
    height: 19px;
    padding: 0 6px;
    display: grid;
    place-items: center;
    border-radius: 10px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 11px;
    font-weight: 700;
  }
  .badge.quiet {
    background: var(--text-3);
    color: var(--bg);
  }
  .badge.dot {
    min-width: 11px;
    width: 11px;
    height: 11px;
    padding: 0;
  }
  .more {
    position: absolute;
    right: 12px;
    top: 10px;
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 6px;
    color: var(--text-3);
    opacity: 0;
    transform: rotate(90deg);
    transition: opacity 120ms var(--ease);
  }
  .rowwrap:hover .more {
    opacity: 1;
  }
  .rowwrap:hover time {
    visibility: hidden;
  }
  .more:hover {
    background: var(--bg-3);
    color: var(--text);
  }
  .aimark,
  .notesmark,
  .archmark {
    flex: none;
    width: 44px;
    height: 44px;
    display: grid;
    place-items: center;
    border-radius: 50%;
  }
  .aimark {
    background: var(--accent);
    color: var(--on-accent);
  }
  .notesmark {
    background: var(--note-yellow);
    color: var(--note-ink);
    border-radius: 14px;
  }
  .archived-row {
    height: 48px;
  }
  .archmark {
    height: 32px;
    color: var(--text-2);
  }
  .archived-row .name {
    font-weight: 500;
  }
  .update {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 12px 6px;
    padding: 9px 12px;
    border-radius: var(--r-m);
    background: var(--ink);
    color: var(--on-ink);
    font-size: 12px;
    font-weight: 500;
    text-align: left;
    animation: in 260ms var(--ease);
  }
  @keyframes in {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
  .update span:nth-child(2) {
    flex: 1;
  }
  .go {
    color: var(--accent);
    font-weight: 600;
  }
  footer {
    flex: none;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 8px 10px 12px 16px;
  }
  footer > :first-child {
    flex: 1;
    min-width: 0;
  }
  .conn {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: var(--text-3);
  }
  .conn .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .conn.on .dot {
    background: var(--accent);
  }
  /* Background history fetch: a small progress ring where the status dot is. */
  .ring {
    flex: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: conic-gradient(var(--accent) var(--p), var(--line) 0);
    mask: radial-gradient(circle, transparent 3.5px, #000 4px);
    -webkit-mask: radial-gradient(circle, transparent 3.5px, #000 4px);
    transition: background 400ms var(--ease);
  }
  .conn.sync {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .auto {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 500;
    color: var(--accent);
  }
  .pause {
    height: 26px;
    padding: 0 8px;
    font-size: 12px;
  }
  .icon-btn.on {
    background: var(--bg-3);
    color: var(--text);
  }
  .empty {
    display: grid;
    gap: 6px;
    padding: 28px 14px;
    color: var(--text-2);
    font-size: 12.5px;
    text-wrap: pretty;
  }
  .empty strong {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }
  .resizer {
    position: absolute;
    top: 0;
    right: -4px;
    bottom: 0;
    width: 8px;
    z-index: 5;
    cursor: col-resize;
  }
  .resizer::after {
    content: "";
    position: absolute;
    top: 50%;
    left: 3px;
    width: 2px;
    height: 36px;
    transform: translateY(-50%);
    border-radius: 1px;
    background: var(--line);
    opacity: 0;
    transition: opacity 160ms var(--ease);
  }
  .resizer:hover::after,
  .resizer:focus-visible::after,
  .resizer:active::after {
    opacity: 1;
  }
  .resizer:focus-visible {
    outline: none;
  }
</style>
