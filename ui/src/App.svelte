<script lang="ts">
  import { onMount } from "svelte";
  import Conversation from "./lib/Conversation.svelte";
  import Pairing from "./lib/Pairing.svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import Assistant from "./lib/Assistant.svelte";
  import Settings from "./lib/Settings.svelte";
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
        <Assistant />
      {:else}
        <Conversation />
      {/if}
      {#if app.settingsOpen}
        <Settings />
      {/if}
    </div>
  {:else}
    <Pairing />
  {/if}
{/if}

<style>
  .shell {
    height: 100%;
    display: flex;
  }
</style>
