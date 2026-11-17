<script lang="ts">
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import { AI, app, chatTitle, saveAutoReply, selectChat } from "./app.svelte";
  import { stamp, t, type Key } from "./i18n";

  let query = $state("");

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return app.chats;
    return app.chats.filter((c) => chatTitle(c).toLowerCase().includes(q));
  });

  /** Media-only messages are stored as "[kind]"; show them in the user's language. */
  function preview(text: string): string {
    const kind = /^\[(image|video|audio|document|sticker)\]$/.exec(text)?.[1];
    return kind ? t(`kind.${kind}` as Key) : text;
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
</script>

<aside class="sidebar">
  <header data-tauri-drag-region>
    <label class="search">
      <Icon name="search" size={16} />
      <input type="search" bind:value={query} placeholder={t("chats.search")} aria-label={t("chats.search")} />
    </label>
  </header>

  {#if app.status.link !== "connected"}
    <p class="offline" role="status">
      {app.status.link === "disconnected" ? t("status.disconnected") : t("status.starting")}
    </p>
  {/if}

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <nav class="chatlist scroll" aria-label={t("chats.title")} onkeydown={onKey}>
    {#if !query.trim()}
      <button class="row" class:active={app.selected === AI} onclick={() => selectChat(AI)}>
        <span class="aimark"><Icon name="spark" size={19} /></span>
        <span class="body">
          <span class="top"><span class="name">{t("ai.name")}</span></span>
          <span class="bottom"><span class="preview">{app.ai.config.model || t("ai.sub")}</span></span>
        </span>
      </button>
    {/if}
    {#each filtered as chat (chat.jid)}
      <button class="row" class:active={chat.jid === app.selected} onclick={() => selectChat(chat.jid)}>
        <Avatar title={chatTitle(chat)} group={chat.is_group} />
        <span class="body">
          <span class="top">
            <span class="name">{chatTitle(chat)}</span>
            <time class="num">{stamp(chat.last_ts)}</time>
          </span>
          <span class="bottom">
            <span class="preview">{preview(chat.last_preview)}</span>
            {#if chat.unread > 0}
              <span class="unread num">{chat.unread > 99 ? "99+" : chat.unread}</span>
            {/if}
          </span>
        </span>
      </button>
    {:else}
      <div class="empty">
        {#if app.chats.length === 0}
          <strong>{t("chats.empty")}</strong>
          <span>{t("chats.emptyHint")}</span>
        {:else}
          <span>{t("chats.noMatch")}</span>
        {/if}
      </div>
    {/each}
  </nav>

  <footer>
    {#if app.auto.enabled}
      <span class="auto"><Icon name="spark" size={14} />{t("side.autoOn")}</span>
      <button class="btn quiet pause" onclick={() => saveAutoReply({ enabled: false })}>{t("side.pause")}</button>
    {:else}
      <span></span>
    {/if}
    <button
      class="icon-btn"
      class:on={app.settingsOpen}
      onclick={() => (app.settingsOpen = !app.settingsOpen)}
      aria-label={t("set.open")}
      title={t("set.open")}
    >
      <Icon name="gear" size={18} />
    </button>
  </footer>
</aside>

<style>
  .sidebar {
    width: 300px;
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--bg-2);
    min-height: 0;
  }
  header {
    padding: 44px 12px 10px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 10px;
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
  .offline {
    margin: 0 12px 8px;
    padding: 6px 10px;
    border-radius: var(--r-m);
    background: var(--accent-soft);
    color: var(--text);
    font-size: 12px;
  }
  .chatlist {
    flex: 1;
    min-height: 0;
    padding: 0 8px 8px;
  }
  .row {
    width: 100%;
    height: 60px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 8px;
    border-radius: var(--r-m);
    text-align: left;
    content-visibility: auto;
    contain-intrinsic-size: auto 60px;
    transition: background 140ms var(--ease);
  }
  .row:hover {
    background: var(--bg-3);
  }
  .row.active {
    background: var(--ink);
    color: var(--on-ink);
  }
  .row.active :global(.avatar) {
    background: color-mix(in srgb, var(--on-ink) 18%, transparent);
    color: var(--on-ink);
  }
  .body {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 2px;
  }
  .top,
  .bottom {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .name {
    flex: 1;
    min-width: 0;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  time {
    font-size: 11px;
    color: var(--text-3);
  }
  .preview {
    flex: 1;
    min-width: 0;
    color: var(--text-2);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row.active time,
  .row.active .preview {
    color: color-mix(in srgb, var(--on-ink) 72%, transparent);
  }
  .unread {
    flex: none;
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    display: grid;
    place-items: center;
    border-radius: 9px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 11px;
    font-weight: 600;
    align-self: center;
  }
  .aimark {
    flex: none;
    width: 38px;
    height: 38px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--accent);
    color: var(--on-accent);
  }
  footer {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px 10px 14px;
  }
  footer > :first-child {
    flex: 1;
    min-width: 0;
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
    font-size: 12px;
    text-wrap: pretty;
  }
  .empty strong {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }
</style>
