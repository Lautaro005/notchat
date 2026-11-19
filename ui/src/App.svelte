<script lang="ts">
  import { onMount } from "svelte";
  import Conversation from "./lib/Conversation.svelte";
  import Pairing from "./lib/Pairing.svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import Assistant from "./lib/Assistant.svelte";
  import Settings from "./lib/Settings.svelte";
  import Overlays from "./lib/Overlays.svelte";
  import Notes from "./lib/Notes.svelte";
  import { AI, NOTES, app, closeChat, init } from "./lib/app.svelte";

  let ready = $state(false);

  onMount(async () => {
    await init();
    ready = true;
  });

  /** Esc closes whatever is on top: menus and dialogs handle it first (and
   *  mark the event), then reply/edit/selection, then the open chat. */
  function onKey(e: KeyboardEvent) {
    if (e.key !== "Escape" || e.defaultPrevented || e.isComposing) return;
    if (app.menu || app.confirm || app.prompt || app.choice || app.info || app.forward || app.lightbox || app.person) return;
    if (app.settingsOpen) return;
    if (app.reply || app.editing) {
      app.reply = null;
      app.editing = null;
    } else if (app.selecting) {
      app.selecting = false;
      app.picked = [];
    } else if (app.infoOpen) {
      app.infoOpen = false;
    } else if (app.selected) {
      closeChat();
    } else {
      return;
    }
    e.preventDefault();
  }

  // Until the device is linked there is nothing else to show: pairing owns the window.
  const linked = $derived(
    app.status.link === "connected" ||
      app.status.link === "disconnected" ||
      (app.status.link === "starting" && app.chats.length > 0),
  );
</script>

<svelte:window onkeydown={onKey} />

{#if ready}
  {#if linked}
    <div class="shell">
      <Sidebar />
      {#if app.selected === AI}
        <div class="card"><Assistant /></div>
      {:else if app.selected === NOTES}
        <div class="card"><Notes /></div>
      {:else}
        <Conversation />
      {/if}
    </div>
    {#if app.settingsOpen}
      <Settings />
    {/if}
  {:else}
    <Pairing />
  {/if}
  <Overlays />
{/if}

<style>
  .shell {
    height: 100%;
    display: flex;
    background: var(--bg-2);
  }
  .card {
    flex: 1;
    min-width: 0;
    display: flex;
    margin: 8px 8px 8px 0;
    border-radius: 16px;
    overflow: hidden;
    box-shadow: var(--shadow-s), 0 0 0 0.5px var(--line);
  }
</style>
