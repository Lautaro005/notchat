<script lang="ts">
  import { tick } from "svelte";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import { api } from "./api";
  import { app, chatTitle, toast } from "./app.svelte";
  import { t, tf } from "./i18n";

  const MAX = 5;
  let query = $state("");
  let picked = $state<string[]>([]);
  let busy = $state(false);
  let search: HTMLInputElement | undefined = $state();

  const chats = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return app.chats.filter((c) => !q || chatTitle(c).toLowerCase().includes(q)).slice(0, 80);
  });

  $effect(() => {
    void tick().then(() => search?.focus());
  });

  function close() {
    app.forward = null;
  }

  function toggle(jid: string) {
    if (picked.includes(jid)) picked = picked.filter((j) => j !== jid);
    else if (picked.length < MAX) picked = [...picked, jid];
    else toast(t("fwd.limit"));
  }

  async function send() {
    const what = app.forward;
    if (!what || !picked.length || busy) return;
    busy = true;
    let reached = 0;
    try {
      if ("note" in what) {
        reached = await api.sendNote(what.note, picked);
      } else {
        for (const id of what.ids) reached = Math.max(reached, await api.forward(what.jid, id, picked));
      }
      toast(reached > 1 ? tf("fwd.done", { n: reached }) : t("fwd.done1"));
      app.selecting = false;
      app.picked = [];
      close();
    } catch (e) {
      toast(String(e).includes("not connected") ? t("toast.offline") : t("toast.failed"));
    } finally {
      busy = false;
    }
  }
</script>

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="fw-title">
    <header>
      <h2 id="fw-title">{t("fwd.title")}</h2>
      <button class="icon-btn" onclick={close} aria-label={t("action.cancel")}><Icon name="close" size={16} /></button>
    </header>
    <label class="search">
      <Icon name="search" size={15} />
      <input bind:this={search} type="search" bind:value={query} placeholder={t("fwd.search")} aria-label={t("fwd.search")} />
    </label>
    <ul class="list scroll">
      {#each chats as c (c.jid)}
        {@const on = picked.includes(c.jid)}
        <li>
          <button class="row" class:on aria-pressed={on} onclick={() => toggle(c.jid)}>
            <Avatar title={chatTitle(c)} jid={c.jid} group={c.is_group} size={34} />
            <span class="name">{chatTitle(c)}</span>
            <span class="box" aria-hidden="true"><Icon name="check" size={12} /></span>
          </button>
        </li>
      {/each}
    </ul>
    <footer>
      <span class="hint">{picked.length ? picked.map((j) => chatTitle(app.chats.find((c) => c.jid === j) ?? { jid: j, name: "" })).join(", ") : t("fwd.limit")}</span>
      <button class="btn primary" onclick={send} disabled={!picked.length || busy}>
        <Icon name="send" size={14} />{t("fwd.send")}
      </button>
    </footer>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: grid;
    place-items: center;
    background: var(--scrim);
    animation: fade 160ms var(--ease);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  .dialog {
    width: min(400px, calc(100vw - 32px));
    max-height: min(560px, calc(100vh - 48px));
    display: flex;
    flex-direction: column;
    border-radius: 18px;
    background: var(--panel);
    box-shadow: var(--shadow);
    animation: rise 200ms var(--ease);
    overflow: hidden;
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 12px 6px 18px;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 34px;
    margin: 4px 14px 8px;
    padding: 0 11px;
    border-radius: var(--r-m);
    background: var(--bg-2);
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
    outline: none;
    user-select: text;
    -webkit-user-select: text;
  }
  .search input::-webkit-search-cancel-button {
    display: none;
  }
  .list {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 0 8px 8px;
    list-style: none;
  }
  .row {
    width: 100%;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border-radius: 10px;
    text-align: left;
  }
  .row:hover {
    background: var(--bg-2);
  }
  .name {
    flex: 1;
    min-width: 0;
    font-weight: 500;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .box {
    flex: none;
    width: 20px;
    height: 20px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: transparent;
    box-shadow: inset 0 0 0 1.5px var(--line);
  }
  .row.on .box {
    background: var(--accent);
    color: var(--on-accent);
    box-shadow: none;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px 14px 18px;
    border-top: 1px solid var(--line-soft);
  }
  .hint {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    color: var(--text-2);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
</style>
