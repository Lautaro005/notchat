<script lang="ts">
  import Icon from "./Icon.svelte";
  import { cachedMedia, loadMedia } from "./media.svelte";
  import { bytes, duration, t } from "./i18n";
  import { api, type Message } from "./api";

  let { m, onopen }: { m: Message; onopen: (src: string, kind: string) => void } = $props();

  const AUTO_BYTES = 6 * 1000 * 1000;
  let failed = $state(false);
  let loading = $state(false);
  const src = $derived(cachedMedia(m.chat_jid, m.id));
  /** Stickers with sound carry the sound's length in `seconds`. */
  const sound = $derived(m.kind === "sticker" && m.seconds > 0);
  let player: HTMLAudioElement | null = null;
  let playing = $state(false);
  let soundLoading = $state(false);

  async function toggleSound(e: MouseEvent) {
    e.stopPropagation();
    if (playing && player) {
      player.pause();
      player.currentTime = 0;
      playing = false;
      return;
    }
    if (soundLoading) return;
    try {
      if (!player) {
        soundLoading = true;
        player = new Audio(await api.mediaSound(m.chat_jid, m.id));
        player.onended = () => (playing = false);
      }
      await player.play();
      playing = true;
    } catch {
      failed = true;
    } finally {
      soundLoading = false;
    }
  }

  $effect(() => () => player?.pause());

  async function load() {
    if (!m.has_media || loading) return;
    loading = true;
    failed = false;
    try {
      await loadMedia(m.chat_jid, m.id);
    } catch {
      failed = true;
    } finally {
      loading = false;
    }
  }

  // Photos and stickers download on their own when small; videos wait for a click.
  $effect(() => {
    if (!src && m.has_media && (m.kind === "image" || m.kind === "sticker") && (m.size === 0 || m.size < AUTO_BYTES)) void load();
  });
</script>

{#if src && m.kind === "video"}
  <!-- svelte-ignore a11y_media_has_caption -->
  <video class="media" {src} controls preload="metadata"></video>
{:else if src}
  <div class="wrap">
    <button class="media img" class:sticker={m.kind === "sticker"} onclick={() => onopen(src, m.kind)} aria-label={t("kind.image")}>
      <img {src} alt="" draggable="false" />
    </button>
    {#if sound}
      <button
        class="sound"
        class:on={playing}
        onclick={toggleSound}
        disabled={soundLoading}
        aria-pressed={playing}
        aria-label={playing ? t("sticker.stop") : t("sticker.play")}
        title={playing ? t("sticker.stop") : t("sticker.play")}
      >
        {#if soundLoading}<span class="spin"></span>{:else}<Icon name={playing ? "pause" : "volume"} size={14} />{/if}
      </button>
    {/if}
  </div>
{:else}
  <div class="media holder" class:sticker={m.kind === "sticker"}>
    <Icon name={m.kind === "video" ? "camera" : "image"} size={26} />
    {#if m.has_media}
      <button class="dl" onclick={load} disabled={loading}>
        {#if loading}
          <span class="spin"></span>
        {:else}
          <Icon name={failed ? "refresh" : "download"} size={15} />
        {/if}
        <span>
          {failed ? t("chat.retry") : m.size ? bytes(m.size) : t("chat.download")}{#if m.kind === "video" && m.seconds}
            · {duration(m.seconds)}{/if}
        </span>
      </button>
    {/if}
  </div>
{/if}

<style>
  .media {
    display: block;
    width: min(300px, 100%);
    border-radius: 12px;
    overflow: hidden;
    background: color-mix(in srgb, currentColor 8%, transparent);
  }
  video.media {
    max-height: 340px;
    background: #000;
  }
  .img {
    padding: 0;
    cursor: zoom-in;
  }
  img {
    display: block;
    width: 100%;
    max-height: 360px;
    object-fit: cover;
  }
  .wrap {
    position: relative;
    width: fit-content;
  }
  .sound {
    position: absolute;
    left: 6px;
    bottom: 6px;
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: rgb(0 0 0 / 0.55);
    color: #fff;
    transition: background 140ms var(--ease);
  }
  .sound:hover,
  .sound.on {
    background: var(--accent);
    color: var(--on-accent);
  }
  .sticker {
    width: 140px;
    background: none;
  }
  .sticker img {
    object-fit: contain;
  }
  .holder {
    aspect-ratio: 4 / 3;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 10px;
    color: color-mix(in srgb, currentColor 45%, transparent);
  }
  .holder.sticker {
    aspect-ratio: 1;
  }
  .dl {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    border-radius: 14px;
    background: rgb(0 0 0 / 0.55);
    color: #fff;
    font-size: 12px;
    font-weight: 500;
  }
  .spin {
    width: 13px;
    height: 13px;
    border-radius: 50%;
    border: 2px solid rgb(255 255 255 / 0.35);
    border-top-color: #fff;
    animation: spin 800ms linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
