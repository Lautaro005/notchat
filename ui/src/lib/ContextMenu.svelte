<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  import type { MenuItem } from "./app.svelte";
  import Self from "./ContextMenu.svelte";

  let {
    items,
    x,
    y,
    onclose,
    sub = false,
  }: { items: MenuItem[]; x: number; y: number; onclose: () => void; sub?: boolean } = $props();

  let el: HTMLElement | undefined = $state();
  let left = $state(0);
  let top = $state(0);
  let openIndex = $state<number | null>(null);
  let subPos = $state({ x: 0, y: 0 });

  // Keep the menu inside the window.
  $effect(() => {
    void items;
    void tick().then(() => {
      if (!el) return;
      const r = el.getBoundingClientRect();
      const pad = 8;
      left = Math.max(pad, Math.min(x, window.innerWidth - r.width - pad));
      top = Math.max(pad, Math.min(y, window.innerHeight - r.height - pad));
      if (sub && x + r.width > window.innerWidth - pad) left = Math.max(pad, x - r.width * 2 - 4);
      if (!sub) el.querySelector<HTMLElement>("[role^=menuitem]:not([disabled])")?.focus();
    });
  });

  function enabled(): HTMLElement[] {
    return [...(el?.querySelectorAll<HTMLElement>(":scope > [role^=menuitem]:not([disabled])") ?? [])];
  }

  function openSub(i: number, target: HTMLElement) {
    const r = target.getBoundingClientRect();
    subPos = { x: r.right + 2, y: r.top - 5 };
    openIndex = i;
  }

  function run(item: MenuItem, i: number, target: HTMLElement) {
    if (item.disabled) return;
    if (item.children) {
      openSub(i, target);
      return;
    }
    onclose();
    item.action?.();
  }

  function onKey(e: KeyboardEvent) {
    const list = enabled();
    const i = list.indexOf(document.activeElement as HTMLElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      const next = e.key === "ArrowDown" ? (i + 1) % list.length : (i - 1 + list.length) % list.length;
      list[next]?.focus();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (e.key === "ArrowRight" && i >= 0) {
      e.stopPropagation();
      list[i].click();
      void tick().then(() => document.querySelector<HTMLElement>(".menu.sub [role^=menuitem]")?.focus());
    } else if (e.key === "ArrowLeft" && sub) {
      e.stopPropagation();
      onclose();
    }
  }
</script>

<div
  class="menu"
  class:sub
  role="menu"
  tabindex="-1"
  bind:this={el}
  style:left="{left}px"
  style:top="{top}px"
  onkeydown={onKey}
>
  {#each items as item, i (i)}
    {#if item.separator}
      <div class="sep" role="separator"></div>
    {:else}
      <button
        role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}
        class:danger={item.danger}
        class:open={openIndex === i}
        disabled={item.disabled}
        aria-haspopup={item.children ? "menu" : undefined}
        aria-checked={item.checked}
        onclick={(e) => run(item, i, e.currentTarget)}
        onmouseenter={(e) => (item.children ? openSub(i, e.currentTarget) : (openIndex = null))}
      >
        <span class="ic">
          {#if item.checked}<Icon name="check" size={15} />{:else if item.icon}<Icon name={item.icon as never} size={15} />{/if}
        </span>
        <span class="label">{item.label}</span>
        {#if item.children}<span class="chev"><Icon name="chevron" size={13} /></span>{/if}
      </button>
    {/if}
  {/each}
</div>

{#if openIndex !== null && items[openIndex]?.children}
  <Self items={items[openIndex].children ?? []} x={subPos.x} y={subPos.y} sub onclose={() => (sub ? onclose() : (openIndex = null))} />
{/if}

<style>
  .menu {
    position: fixed;
    z-index: 60;
    min-width: 210px;
    max-width: 280px;
    padding: 5px;
    border-radius: 12px;
    background: var(--panel);
    box-shadow: var(--shadow), 0 0 0 0.5px var(--line);
    animation: pop 140ms var(--ease);
    transform-origin: top left;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: scale(0.97);
    }
  }
  button {
    width: 100%;
    height: 30px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 8px 0 6px;
    border-radius: 7px;
    text-align: left;
    font-size: 13px;
  }
  button:hover:not(:disabled),
  button:focus-visible,
  button.open {
    background: var(--bg-3);
    outline: none;
  }
  button:disabled {
    opacity: 0.45;
  }
  .danger {
    color: var(--danger);
  }
  .ic {
    width: 18px;
    display: grid;
    place-items: center;
    color: var(--text-2);
    flex: none;
  }
  .danger .ic {
    color: inherit;
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chev {
    color: var(--text-3);
    display: grid;
  }
  .sep {
    height: 1px;
    margin: 5px 8px;
    background: var(--line-soft);
  }
</style>
