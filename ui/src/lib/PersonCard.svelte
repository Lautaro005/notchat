<script lang="ts">
  import { tick } from "svelte";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import { app, selectChat, toast } from "./app.svelte";
  import { t } from "./i18n";
  import type { Person } from "./api";

  let { p, x, y }: { p: Person; x: number; y: number } = $props();

  let el: HTMLElement | undefined = $state();
  let left = $state(0);
  let top = $state(0);

  /** The phone-number chat when known: that is where a message would go. */
  const chatJid = $derived(p.phone ? `${p.phone.slice(1)}@s.whatsapp.net` : p.jid);
  const shown = $derived(p.name || p.phone || p.user);

  $effect(() => {
    void p;
    void tick().then(() => {
      if (!el) return;
      const r = el.getBoundingClientRect();
      const pad = 8;
      left = Math.max(pad, Math.min(x, window.innerWidth - r.width - pad));
      top = y + r.height > window.innerHeight - pad ? Math.max(pad, y - r.height - 30) : y;
      el.querySelector<HTMLElement>("button")?.focus();
    });
  });

  function close() {
    app.person = null;
  }

  async function message() {
    close();
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
  <div class="head">
    <Avatar title={shown.replace(/^~/, "")} jid={chatJid} size={44} />
    <div class="names">
      <strong>{shown}</strong>
      {#if !p.saved && p.name.startsWith("~")}<span class="hint">{t("person.notSaved")}</span>{/if}
    </div>
  </div>
  {#if p.phone}
    <div class="row">
      <span class="label">{t("person.phone")}</span>
      <span class="num">{p.phone}</span>
    </div>
  {/if}
  <div class="acts">
    <button class="btn primary" onclick={message}><Icon name="chat" size={15} />{t("person.message")}</button>
    {#if p.phone}
      <button class="btn quiet" onclick={copy}><Icon name="copy" size={15} />{t("person.copy")}</button>
    {/if}
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
    width: 292px;
    display: grid;
    gap: 12px;
    padding: 14px;
    border-radius: 14px;
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
  .head {
    display: flex;
    align-items: center;
    gap: 11px;
    min-width: 0;
  }
  .names {
    min-width: 0;
    display: grid;
    gap: 2px;
  }
  strong {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 15px;
    font-weight: 600;
  }
  .hint {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .row {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 10px;
    background: var(--bg-2);
    font-size: 13px;
    user-select: text;
    -webkit-user-select: text;
  }
  .label {
    color: var(--text-3);
  }
  .acts {
    display: flex;
    gap: 6px;
  }
  .acts .btn {
    flex: 1 1 auto;
    justify-content: center;
    gap: 6px;
    padding-inline: 10px;
    white-space: nowrap;
  }
  @media (prefers-reduced-motion: reduce) {
    .card {
      animation: none;
    }
  }
</style>
