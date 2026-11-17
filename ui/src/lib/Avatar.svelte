<script lang="ts">
  import Icon from "./Icon.svelte";

  let { title, group = false, size = 38 }: { title: string; group?: boolean; size?: number } = $props();

  const initials = $derived(
    title
      .replace(/^\+/, "")
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((w) => [...w][0]?.toUpperCase() ?? "")
      .join(""),
  );
</script>

<span class="avatar" style:width="{size}px" style:height="{size}px" style:font-size="{size * 0.36}px" aria-hidden="true">
  {#if group}
    <Icon name="group" size={size * 0.5} />
  {:else}
    {initials}
  {/if}
</span>

<style>
  .avatar {
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--bg-3);
    color: var(--text-2);
    font-weight: 600;
    letter-spacing: 0.01em;
  }
</style>
