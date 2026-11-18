<script lang="ts">
  import Icon from "./Icon.svelte";
  import { cachedMedia, loadMedia } from "./media.svelte";
  import { duration, t } from "./i18n";
  import type { Message } from "./api";

  let { m }: { m: Message } = $props();

  let audio: HTMLAudioElement | undefined = $state();
  let playing = $state(false);
  let loading = $state(false);
  let failed = $state(false);
  let current = $state(0);
  let total = $state(0);
  const src = $derived(cachedMedia(m.chat_jid, m.id));
  const length = $derived(total || m.seconds);

  // A deterministic pseudo-waveform: real peaks would need decoding the audio.
  const bars = $derived.by(() => {
    let seed = [...m.id].reduce((h, c) => (h * 33 + c.charCodeAt(0)) >>> 0, 5381);
    return Array.from({ length: 32 }, (_, i) => {
      seed = (seed * 1103515245 + 12345) >>> 0;
      const env = Math.sin((i / 31) * Math.PI) * 0.5 + 0.5;
      return 0.22 + ((seed % 1000) / 1000) * 0.78 * env;
    });
  });

  async function toggle() {
    if (!m.has_media && !src) return;
    if (!src) {
      loading = true;
      failed = false;
      try {
        await loadMedia(m.chat_jid, m.id);
      } catch {
        failed = true;
        return;
      } finally {
        loading = false;
      }
      await new Promise((r) => setTimeout(r, 0));
    }
    if (!audio) return;
    if (audio.paused) {
      // Only one voice note plays at a time.
      document.querySelectorAll("audio").forEach((a) => a !== audio && a.pause());
      await audio.play().catch(() => (failed = true));
    } else audio.pause();
  }

  function seek(e: MouseEvent) {
    if (!audio || !length) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    audio.currentTime = ((e.clientX - r.left) / r.width) * (audio.duration || length);
  }
</script>

<div class="voice">
  <button class="play" onclick={toggle} aria-label={playing ? t("chat.pause") : t("chat.play")} disabled={loading}>
    {#if loading}
      <span class="spin"></span>
    {:else if failed}
      <Icon name="refresh" size={16} />
    {:else}
      <Icon name={playing ? "pause" : "play"} size={16} />
    {/if}
  </button>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="wave" onclick={seek}>
    {#each bars as h, i (i)}
      <span style:height="{h * 100}%" class:done={length > 0 && i / bars.length < current / length}></span>
    {/each}
  </div>
  <span class="len num">{duration(playing || current ? current : length)}</span>
  {#if src}
    <audio
      bind:this={audio}
      {src}
      preload="auto"
      onplay={() => (playing = true)}
      onpause={() => (playing = false)}
      onended={() => {
        playing = false;
        current = 0;
      }}
      ontimeupdate={() => (current = audio?.currentTime ?? 0)}
      onloadedmetadata={() => (total = Number.isFinite(audio?.duration) ? (audio?.duration ?? 0) : 0)}
    ></audio>
  {/if}
</div>

<style>
  .voice {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 250px;
    max-width: 100%;
    padding: 2px 0;
  }
  .play {
    flex: none;
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--accent);
    color: var(--on-accent);
  }
  .play :global(svg) {
    fill: currentColor;
  }
  .wave {
    flex: 1;
    height: 26px;
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .wave span {
    flex: 1;
    min-height: 3px;
    border-radius: 2px;
    background: currentColor;
    opacity: 0.3;
    transition: opacity 120ms linear;
  }
  .wave span.done {
    opacity: 0.95;
  }
  .len {
    flex: none;
    font-size: 11px;
    opacity: 0.7;
    min-width: 30px;
    text-align: right;
  }
  .spin {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid color-mix(in srgb, var(--on-accent) 35%, transparent);
    border-top-color: var(--on-accent);
    animation: spin 800ms linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
