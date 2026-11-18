<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { EMOJI } from "./emoji";
  import { QUICK_REACTIONS } from "./messageMenu";
  import { t } from "./i18n";

  let {
    x,
    y,
    current,
    onpick,
    onclose,
  }: { x: number; y: number; current: string; onpick: (emoji: string) => void; onclose: () => void } = $props();

  let el: HTMLElement | undefined = $state();
  let full = $state(false);
  let left = $state(0);
  let top = $state(0);

  // Keep the bar inside the window, preferring above the pointer.
  $effect(() => {
    void full;
    void tick().then(() => {
      if (!el) return;
      const r = el.getBoundingClientRect();
      left = Math.max(8, Math.min(x - r.width / 2, window.innerWidth - r.width - 8));
      top = y - r.height - 8 < 8 ? y + 12 : y - r.height - 8;
      el.querySelector<HTMLElement>("button")?.focus();
    });
  });

  function pick(e: string) {
    onpick(e);
    onclose();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div class="catch" onclick={onclose}></div>
<div
  class="bar"
  class:full
  bind:this={el}
  style:left="{left}px"
  style:top="{top}px"
  role="dialog"
  aria-label={t("msg.react")}
  tabindex="-1"
  onkeydown={(e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    }
  }}
>
  {#if full}
    <div class="grid">
      {#each EMOJI as e (e)}
        <button class:on={current === e} onclick={() => pick(e)} aria-label={e}>{e}</button>
      {/each}
    </div>
  {:else}
    {#each QUICK_REACTIONS as e (e)}
      <button class="q" class:on={current === e} onclick={() => pick(e)} aria-label={e} aria-pressed={current === e}>{e}</button>
    {/each}
    <button class="more" onclick={() => (full = true)} aria-label={t("chat.emoji")}><Icon name="plus" size={16} /></button>
  {/if}
</div>

<style>
  .catch {
    position: fixed;
    inset: 0;
    z-index: 55;
  }
  .bar {
    position: fixed;
    z-index: 56;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 5px;
    border-radius: 999px;
    background: var(--panel);
    box-shadow: var(--shadow), 0 0 0 0.5px var(--line);
    animation: pop 180ms var(--ease);
  }
  .bar.full {
    border-radius: 16px;
    padding: 8px;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.94);
    }
  }
  button {
    display: grid;
    place-items: center;
    border-radius: 50%;
    line-height: 1;
  }
  .q {
    width: 36px;
    height: 36px;
    font-size: 22px;
    transition: transform 160ms var(--ease), background 140ms var(--ease);
  }
  .q:hover,
  .q:focus-visible {
    transform: scale(1.18) translateY(-2px);
    outline: none;
  }
  .on {
    background: var(--accent-soft);
  }
  .more {
    width: 32px;
    height: 32px;
    margin-left: 2px;
    color: var(--text-2);
    background: var(--bg-3);
  }
  .more:hover {
    color: var(--text);
  }
  .grid {
    width: 300px;
    display: grid;
    grid-template-columns: repeat(9, 1fr);
    gap: 2px;
  }
  .grid button {
    height: 31px;
    border-radius: 8px;
    font-size: 19px;
  }
  .grid button:hover {
    background: var(--bg-3);
  }
</style>
