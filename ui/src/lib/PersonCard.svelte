<script lang="ts">
  import { tick } from "svelte";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import { app, selectChat, toast } from "./app.svelte";
  import { t, tf } from "./i18n";
  import type { Person } from "./api";

  let {
    p,
    x,
    y,
    admin = false,
    group = "",
  }: {
    p: Person;
    x: number;
    y: number;
    /** Opened from a group's member list: whether they administer it. */
    admin?: boolean;
    group?: string;
  } = $props();

  let el: HTMLElement | undefined = $state();
  let left = $state(0);
  let top = $state(0);

  /** The phone-number chat when known: that is where a message would go. */
  const chatJid = $derived(p.phone ? `${p.phone.slice(1)}@s.whatsapp.net` : p.jid);
  const shown = $derived(p.name || p.phone || p.user);
  /** Their own WhatsApp name, shown when the contact book calls them something else. */
  const waName = $derived(p.saved && p.push_name && p.push_name !== p.name ? p.push_name : "");

  $effect(() => {
    void p;
    void tick().then(() => {
      if (!el) return;
      const r = el.getBoundingClientRect();
      const pad = 8;
      left = Math.max(pad, Math.min(x, window.innerWidth - r.width - pad));
      top = y + r.height > window.innerHeight - pad ? Math.max(pad, Math.min(y - r.height - 30, window.innerHeight - r.height - pad)) : y;
      el.querySelector<HTMLElement>(".acts button")?.focus();
    });
  });

  function close() {
    app.person = null;
  }

  async function message() {
    close();
    app.infoOpen = false;
    await selectChat(chatJid);
  }

  function copy() {
    void navigator.clipboard.writeText(p.phone).then(
      () => toast(t("person.copied")),
      () => toast(t("toast.failed")),
    );
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div class="catcher" onclick={close} oncontextmenu={(e) => { e.preventDefault(); close(); }}></div>
<div class="card" role="dialog" aria-label={shown} bind:this={el} style:left="{left}px" style:top="{top}px">
  <button class="icon-btn x" onclick={close} aria-label={t("chat.close")}><Icon name="close" size={15} /></button>
  <div class="hero">
    <Avatar title={shown.replace(/^~/, "")} jid={chatJid} size={76} />
    <strong title={shown}>{shown}</strong>
    {#if waName}
      <span class="sub" title={t("person.waName")}>~{waName}</span>
    {:else if !p.saved && p.name.startsWith("~")}
      <span class="sub">{t("person.notSaved")}</span>
    {/if}
    {#if admin}
      <span class="badge">{group ? tf("person.admin", { group }) : t("person.adminShort")}</span>
    {/if}
  </div>
  <dl>
    <div class="row">
      <dt>{t("person.phone")}</dt>
      {#if p.phone}
        <dd class="num">{p.phone}</dd>
        <button class="icon-btn copy" onclick={copy} aria-label={t("person.copy")} title={t("person.copy")}><Icon name="copy" size={14} /></button>
      {:else}
        <dd class="none">{t("person.noPhone")}</dd>
      {/if}
    </div>
  </dl>
  <div class="acts">
    <button class="btn primary" onclick={message}><Icon name="chat" size={15} />{t("person.message")}</button>
  </div>
</div>

<style>
  .catcher {
    position: fixed;
    inset: 0;
    z-index: 59;
  }
  .card {
    position: fixed;
    z-index: 60;
    width: 280px;
    display: grid;
    gap: 14px;
    padding: 20px 16px 16px;
    border-radius: 16px;
    background: var(--panel);
    box-shadow: var(--shadow), 0 0 0 1px var(--line);
    animation: pop 160ms var(--ease);
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
  }
  .x {
    position: absolute;
    top: 8px;
    right: 8px;
    width: 26px;
    height: 26px;
    color: var(--text-3);
  }
  .hero {
    min-width: 0;
    display: grid;
    justify-items: center;
    gap: 3px;
    text-align: center;
  }
  .hero :global(.avatar) {
    margin-bottom: 6px;
  }
  strong {
    max-width: 100%;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 16px;
    font-weight: 600;
  }
  .sub {
    max-width: 100%;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 12.5px;
    color: var(--text-3);
  }
  .badge {
    max-width: 100%;
    margin-top: 4px;
    padding: 2px 8px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    border-radius: 6px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11px;
    font-weight: 600;
  }
  dl {
    margin: 0;
    display: grid;
    border-radius: 11px;
    background: var(--bg-2);
    overflow: hidden;
  }
  .row {
    min-height: 40px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 6px 0 12px;
    font-size: 13px;
  }
  dt {
    flex: none;
    color: var(--text-3);
  }
  dd {
    flex: 1;
    min-width: 0;
    margin: 0;
    padding-right: 6px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    text-align: right;
    user-select: text;
    -webkit-user-select: text;
  }
  dd.none {
    color: var(--text-3);
    font-size: 12px;
    white-space: normal;
  }
  .copy {
    flex: none;
    width: 28px;
    height: 28px;
    color: var(--text-2);
  }
  .acts {
    display: flex;
  }
  .acts .btn {
    flex: 1;
    justify-content: center;
    gap: 6px;
  }
  @media (prefers-reduced-motion: reduce) {
    .card {
      animation: none;
    }
  }
</style>
