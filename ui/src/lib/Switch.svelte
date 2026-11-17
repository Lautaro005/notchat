<script lang="ts">
  let {
    checked,
    onchange,
    label,
    disabled = false,
  }: { checked: boolean; onchange: (v: boolean) => void; label: string; disabled?: boolean } = $props();
</script>

<button
  class="switch"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span></span>
</button>

<style>
  .switch {
    flex: none;
    width: 36px;
    height: 22px;
    padding: 2px;
    border-radius: 11px;
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line);
    transition: background 180ms var(--ease);
  }
  .switch span {
    display: block;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.3);
    transition: transform 180ms var(--ease);
  }
  .switch[aria-checked="true"] {
    background: var(--accent);
    box-shadow: none;
  }
  .switch[aria-checked="true"] span {
    transform: translateX(14px);
  }
  .switch:disabled {
    opacity: 0.5;
  }
</style>
