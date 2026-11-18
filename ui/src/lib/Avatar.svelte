<script lang="ts">
  import Icon from "./Icon.svelte";
  import { app, avatarFor } from "./app.svelte";

  let {
    title,
    jid = "",
    group = false,
    size = 38,
  }: { title: string; jid?: string; group?: boolean; size?: number } = $props();

  const initials = $derived(
    title
      .replace(/[^\p{L}\p{N}\s]/gu, "")
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((w) => [...w][0]?.toUpperCase() ?? "")
      .join(""),
  );

  // A stable hue per chat so initials are easy to tell apart, kept low-chroma.
  const hue = $derived([...(jid || title)].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7));
  // Pictures are fetched only once the avatar scrolls into view.
  let seen = $state(false);
  const photo = $derived(jid && seen ? (void app.avatars[jid], avatarFor(jid)) : null);
  let broken = $state(false);

  function onVisible(node: HTMLElement) {
    if (typeof IntersectionObserver === "undefined") {
      seen = true;
      return;
    }
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        seen = true;
        io.disconnect();
      }
    });
    io.observe(node);
    return { destroy: () => io.disconnect() };
  }
</script>

<span
  use:onVisible
  class="avatar"
  style:width="{size}px"
  style:height="{size}px"
  style:font-size="{size * 0.36}px"
  style:--h={hue}
  aria-hidden="true"
>
  {#if photo && !broken}
    <img src={photo} alt="" draggable="false" onerror={() => (broken = true)} />
  {:else if group}
    <Icon name="group" size={size * 0.5} />
  {:else if initials}
    {initials}
  {:else}
    <Icon name="unread" size={size * 0.4} />
  {/if}
</span>

<style>
  .avatar {
    flex: none;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 50%;
    background: oklch(0.9 0.035 var(--h));
    color: oklch(0.38 0.07 var(--h));
    font-family: var(--rounded);
    font-weight: 600;
    letter-spacing: 0.01em;
  }
  :global(:root[data-theme="dark"]) .avatar {
    background: oklch(0.32 0.04 var(--h));
    color: oklch(0.86 0.06 var(--h));
  }
  @media (prefers-color-scheme: dark) {
    :global(:root:not([data-theme="light"])) .avatar {
      background: oklch(0.32 0.04 var(--h));
      color: oklch(0.86 0.06 var(--h));
    }
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
