<script lang="ts">
  import Icon from "./Icon.svelte";
  import { t } from "./i18n";

  /** Delivery state of my message: 1 sent, 2 delivered, 3 read. 0 draws nothing. */
  /** `mine`: drawn on my own (accent-coloured) bubble. */
  let { status, size = 15, mine = false }: { status: number; size?: number; mine?: boolean } = $props();

  const label = $derived(status >= 3 ? t("msg.statusRead") : status === 2 ? t("msg.statusDelivered") : t("msg.statusSent"));
</script>

{#if status > 0}
  <span class="ticks" class:read={status >= 3} class:mine role="img" aria-label={label} title={label}>
    <Icon name={status >= 2 ? "checks" : "check"} {size} />
  </span>
{/if}

<style>
  .ticks {
    flex: none;
    display: inline-flex;
    align-items: center;
    color: inherit;
    opacity: 0.75;
  }
  .ticks.read {
    color: var(--read);
    opacity: 1;
  }
  .ticks.read.mine {
    color: var(--read-on-mine);
  }
</style>
