<script lang="ts">
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import { api, type Chat, type GroupInfo } from "./api";
  import { app, chatTitle, isMuted, openMenu } from "./app.svelte";
  import { chatMenu, newList } from "./chatMenu";
  import { t, tf } from "./i18n";

  let { chat }: { chat: Chat } = $props();

  let info = $state<GroupInfo | null>(null);
  let loading = $state(false);
  let offline = $state(false);

  const title = $derived(chatTitle(chat));
  const phone = $derived.by(() => {
    const user = chat.jid.split("@")[0];
    return chat.jid.endsWith("@s.whatsapp.net") && /^\d+$/.test(user) ? `+${user}` : "";
  });
  const created = $derived(
    info?.created ? new Intl.DateTimeFormat(undefined, { dateStyle: "long" }).format(new Date(info.created * 1000)) : "",
  );

  $effect(() => {
    const jid = chat.jid;
    info = null;
    offline = false;
    if (!chat.is_group) return;
    loading = true;
    api
      .groupInfo(jid)
      .then((g) => {
        if (chat.jid === jid) info = g;
      })
      .catch(() => (offline = true))
      .finally(() => (loading = false));
  });

  // Lists: the same toggles as the context menu, always visible here.
  async function toggleList(id: number, member: boolean) {
    await api.setListMember(id, chat.jid, !member);
  }

  /** Destructive actions are the tail of the chat menu (clear, exit, delete). */
  const danger = $derived(chatMenu(chat).filter((i) => i.danger));
</script>

<aside class="info" aria-label={t("chat.info")}>
  <header data-tauri-drag-region>
    <button class="icon-btn" onclick={() => (app.infoOpen = false)} aria-label={t("info.close")}>
      <Icon name="close" size={17} />
    </button>
    <h2>{chat.is_group ? t("info.group") : t("info.contact")}</h2>
  </header>

  <div class="body scroll">
    <section class="hero">
      <Avatar {title} jid={chat.jid} group={chat.is_group} size={96} />
      <h1>{title}</h1>
      {#if phone && phone !== title}<p class="sub num">{phone}</p>{/if}
      {#if chat.is_group && info}<p class="sub">{tf("info.members", { n: info.participants.length })}</p>{/if}
      <div class="quick">
        <button onclick={() => api.mute(chat.jid, isMuted(chat) ? 0 : -1)}>
          <Icon name={isMuted(chat) ? "bell" : "mute"} size={18} />
          <span>{isMuted(chat) ? t("menu.unmute") : t("menu.mute")}</span>
        </button>
        <button onclick={() => api.archive(chat.jid, !chat.archived)}>
          <Icon name={chat.archived ? "unarchive" : "archive"} size={18} />
          <span>{chat.archived ? t("menu.unarchive") : t("menu.archive")}</span>
        </button>
        <button onclick={(e) => openMenu(e, chatMenu(chat, { openInfo: () => {} }))}>
          <Icon name="more" size={18} />
          <span>{t("info.more")}</span>
        </button>
      </div>
    </section>

    {#if chat.is_group}
      {#if info?.description}
        <section>
          <h3>{t("info.description")}</h3>
          <p class="desc">{info.description}</p>
          {#if created}<p class="meta">{tf("info.created", { d: created })}</p>{/if}
        </section>
      {/if}
      <section>
        <h3>{info ? tf("info.members", { n: info.participants.length }) : t("info.membersTitle")}</h3>
        {#if loading}
          <p class="meta">{t("info.loading")}</p>
        {:else if offline}
          <p class="meta">{t("info.offline")}</p>
        {:else if info}
          <ul class="people">
            {#each info.participants as p (p.jid)}
              {@const name = p.me ? t("chat.you") : p.name || p.phone || chatTitle({ jid: p.jid, name: "" })}
              <!-- Saved contacts show only their name; the rest show the number under it. -->
              {@const sub = p.me || (p.name && !p.saved) ? p.phone : ""}
              <li>
                <Avatar title={name} jid={p.jid} size={32} />
                <span class="pwho">
                  <span class="pname" class:num={!p.name && !p.me}>{name}</span>
                  {#if sub}<small class="pphone num">{sub}</small>{/if}
                </span>
                {#if p.admin}<span class="tag">{t("info.admin")}</span>{/if}
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    <section>
      <h3>{t("info.lists")}</h3>
      <div class="lists">
        {#each app.lists as l (l.id)}
          {@const member = l.jids.includes(chat.jid)}
          <button class="chip" aria-pressed={member} onclick={() => toggleList(l.id, member)}>
            {#if member}<Icon name="check" size={13} />{/if}{l.name}
          </button>
        {/each}
        <button class="chip add" onclick={() => newList(chat.jid)}>
          <Icon name="plus" size={13} />{t("filter.newList")}
        </button>
      </div>
    </section>

    <section class="danger">
      {#each danger as item (item.label)}
        <button onclick={() => item.action?.()}>
          <Icon name={item.icon as never} size={17} />{item.label}
        </button>
      {/each}
    </section>
  </div>
</aside>

<style>
  .info {
    width: 320px;
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-left: 1px solid var(--line-soft);
    background: var(--panel);
    animation: slide 220ms var(--ease);
  }
  @keyframes slide {
    from {
      opacity: 0;
      transform: translateX(16px);
    }
  }
  header {
    flex: none;
    height: 58px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 12px;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    pointer-events: none;
  }
  .body {
    flex: 1;
    min-height: 0;
    padding: 0 0 24px;
  }
  section {
    padding: 16px 20px;
    border-top: 1px solid var(--line-soft);
  }
  .hero {
    display: grid;
    justify-items: center;
    text-align: center;
    border-top: 0;
    padding-top: 4px;
  }
  h1 {
    margin: 14px 0 2px;
    font-family: var(--rounded);
    font-size: 19px;
    font-weight: 700;
    overflow-wrap: anywhere;
  }
  .sub {
    margin: 0;
    color: var(--text-2);
    user-select: text;
    -webkit-user-select: text;
  }
  .quick {
    display: flex;
    gap: 8px;
    margin-top: 16px;
  }
  .quick button {
    width: 80px;
    display: grid;
    justify-items: center;
    gap: 5px;
    padding: 10px 4px 8px;
    border-radius: 12px;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--text-2);
    background: var(--bg-2);
    transition: background 140ms var(--ease), color 140ms var(--ease);
  }
  .quick button:hover {
    background: var(--bg-3);
    color: var(--text);
  }
  .quick :global(svg) {
    color: var(--accent);
  }
  h3 {
    margin: 0 0 10px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
  }
  .desc {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
    -webkit-user-select: text;
  }
  .meta {
    margin: 8px 0 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .people {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 2px;
  }
  .people li {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 44px;
  }
  .pwho {
    flex: 1;
    min-width: 0;
    display: grid;
    line-height: 1.25;
  }
  .pname,
  .pphone {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .pname {
    font-weight: 500;
  }
  .pphone {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .tag {
    padding: 2px 7px;
    border-radius: 6px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11px;
    font-weight: 600;
  }
  .lists {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    height: 28px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0 11px;
    border-radius: 999px;
    font-size: 12.5px;
    font-weight: 500;
    color: var(--text-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .chip[aria-pressed="true"] {
    background: var(--accent);
    color: var(--on-accent);
    box-shadow: none;
  }
  .chip:hover:not([aria-pressed="true"]) {
    background: var(--bg-3);
    color: var(--text);
  }
  .danger {
    display: grid;
    gap: 2px;
    padding-top: 10px;
  }
  .danger button {
    height: 38px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 8px;
    margin: 0 -8px;
    border-radius: var(--r-m);
    color: var(--danger);
    font-weight: 500;
    text-align: left;
  }
  .danger button:hover {
    background: color-mix(in srgb, var(--danger) 10%, transparent);
  }
</style>
