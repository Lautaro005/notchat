<script lang="ts" generics="T extends string">
  let {
    value,
    options,
    onchange,
    label,
    disabled = false,
  }: {
    value: T;
    options: { value: T; label: string }[];
    onchange: (v: T) => void;
    label: string;
    disabled?: boolean;
  } = $props();
</script>

<div class="seg" role="radiogroup" aria-label={label}>
  {#each options as o (o.value)}
    <button role="radio" aria-checked={o.value === value} {disabled} onclick={() => onchange(o.value)}>
      {o.label}
    </button>
  {/each}
</div>

<style>
  .seg {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 1fr;
    padding: 2px;
    border-radius: var(--r-m);
    background: var(--bg-3);
  }
  button {
    height: 26px;
    padding: 0 8px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
    white-space: nowrap;
    transition: background 160ms var(--ease), color 160ms var(--ease);
  }
  button[aria-checked="true"] {
    background: var(--bg);
    color: var(--text);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.14);
  }
  button:disabled {
    opacity: 0.5;
  }
</style>
