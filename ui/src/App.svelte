<script lang="ts">
  import { onMount } from "svelte";
  import Conversation from "./lib/Conversation.svelte";
  import Pairing from "./lib/Pairing.svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import Assistant from "./lib/Assistant.svelte";
  import Settings from "./lib/Settings.svelte";
  import Overlays from "./lib/Overlays.svelte";
  import { AI, app, init } from "./lib/app.svelte";

  let ready = $state(false);

  onMount(async () => {
    await init();
    ready = true;
  });

  // Until the device is linked there is nothing else to show: pairing owns the window.
  const linked = $derived(
    app.status.link === "connected" ||
      app.status.link === "disconnected" ||
      (app.status.link === "starting" && app.chats.length > 0),
  );
</script>

{#if ready}
  {#if linked}
    <div class="shell">
      <Sidebar />
      {#if app.selected === AI}
        <div class="card"><Assistant /></div>
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
    border-radius: 14px;
    overflow: hidden;
    box-shadow: var(--shadow-s), 0 0 0 0.5px var(--line);
  }
</style>
