<script lang="ts">
  import { tick } from "svelte";
  import Avatar from "./Avatar.svelte";
  import { t, tf } from "./i18n";
  import type { Reaction } from "./api";

  let {
    reactions,
    x,
    top: anchorTop,
    bottom: anchorBottom,
    onremove,
    onclose,
  }: {
    reactions: Reaction[];
    /** Horizontal centre of the pill that opened the list. */
    x: number;
    /** The pill's top and bottom edges: the list opens above, or below if it doesn't fit. */
    top: number;
    bottom: number;
    onremove: () => void;
    onclose: () => void;
  } = $props();

  let el: HTMLElement | undefined = $state();
  let left = $state(0);
  let top = $state(0);
  /** Emoji tab; "" shows everyone. */
  let tab = $state("");

  /** Emoji tabs, most used first. */
  const tabs = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const r of reactions) counts.set(r.emoji, (counts.get(r.emoji) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => b[1] - a[1]);
  });
  /** Mine first, then in the order they arrived. */
  const rows = $derived(
    reactions.filter((r) => !tab || r.emoji === tab).sort((a, b) => Number(b.from_me) - Number(a.from_me)),
  );

  $effect(() => {
    // Nothing left to show (the last reaction was removed): close.
    if (!reactions.length) onclose();
  });

  $effect(() => {
    void rows.length;
    void tick().then(() => {
      if (!el) return;
      const r = el.getBoundingClientRect();
      const pad = 8;
      left = Math.max(pad, Math.min(x - r.width / 2, window.innerWidth - r.width - pad));
      top = anchorTop - r.height - 6 >= pad ? anchorTop - r.height - 6 : Math.min(anchorBottom + 6, window.innerHeight - r.height - pad);
      el.querySelector<HTMLElement>("button")?.focus();
    });
  });

  function name(r: Reaction): string {
    return r.from_me ? t("chat.you") : r.name || r.phone || r.sender_jid.split("@")[0];
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div class="catch" onclick={onclose} oncontextmenu={(e) => { e.preventDefault(); onclose(); }}></div>
<div
  class="list"
  bind:this={el}
  style:left="{left}px"
  style:top="{top}px"
  role="dialog"
  aria-label={t("rx.title")}
  tabindex="-1"
  onkeydown={(e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    }
  }}
>
  {#if tabs.length > 1}
    <div class="tabs" role="tablist" aria-label={t("rx.title")}>
      <button role="tab" aria-selected={tab === ""} class:on={tab === ""} onclick={() => (tab = "")}>
        {tf("rx.all", { n: reactions.length })}
      </button>
      {#each tabs as [emoji, n] (emoji)}
        <button role="tab" aria-selected={tab === emoji} class:on={tab === emoji} onclick={() => (tab = emoji)}>
          <span class="e">{emoji}</span><span class="num">{n}</span>
        </button>
      {/each}
    </div>
  {/if}
  <ul>
    {#each rows as r (r.sender_jid + r.emoji)}
      <li>
        {#if r.from_me}
          <button class="row mine" onclick={onremove} title={t("rx.remove")}>
            <Avatar title={name(r)} size={30} />
            <span class="who">
              <span class="name">{name(r)}</span>
              <small>{t("rx.remove")}</small>
            </span>
            <span class="emoji">{r.emoji}</span>
          </button>
        {:else}
          <div class="row">
            <Avatar title={name(r)} jid={r.sender_jid} size={30} />
            <span class="who">
              <span class="name" class:num={!r.name || r.name === r.phone}>{name(r)}</span>
              {#if r.phone && r.name && r.name !== r.phone}<small class="num">{r.phone}</small>{/if}
            </span>
            <span class="emoji">{r.emoji}</span>
          </div>
        {/if}
      </li>
    {/each}
  </ul>
</div>

<style>
  .catch {
    position: fixed;
    inset: 0;
    z-index: 55;
  }
  .list {
    position: fixed;
    z-index: 56;
    width: 260px;
    max-height: min(340px, calc(100vh - 16px));
    display: flex;
    flex-direction: column;
    border-radius: 14px;
    background: var(--panel);
    box-shadow: var(--shadow), 0 0 0 0.5px var(--line);
    overflow: hidden;
    animation: pop 160ms var(--ease);
  }
  .list:focus {
    outline: none;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(4px) scale(0.97);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .list {
      animation: none;
    }
  }
  .tabs {
    flex: none;
    display: flex;
    gap: 2px;
    padding: 6px 6px 0;
    overflow-x: auto;
    border-bottom: 1px solid var(--line-soft);
  }
  .tabs button {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 30px;
    padding: 0 10px;
    border-bottom: 2px solid transparent;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
    white-space: nowrap;
  }
  .tabs button.on {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }
  .tabs .e {
    font-size: 15px;
  }
  ul {
    margin: 0;
    padding: 4px;
    list-style: none;
    overflow-y: auto;
  }
  .row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border-radius: 10px;
    text-align: left;
  }
  .row.mine:hover,
  .row.mine:focus-visible {
    background: var(--bg-2);
    outline: none;
  }
  .who {
    flex: 1;
    min-width: 0;
    display: grid;
    line-height: 1.25;
  }
  .name,
  .who small {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .name {
    font-size: 13px;
    font-weight: 500;
  }
  .who small {
    font-size: 11px;
    color: var(--text-3);
  }
  .emoji {
    flex: none;
    font-size: 18px;
    line-height: 1;
  }
</style>
