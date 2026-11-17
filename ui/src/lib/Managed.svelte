<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import { api, on, type ManagedView } from "./api";
  import { app } from "./app.svelte";
  import { locale, t } from "./i18n";

  let { onchange }: { onchange: () => void } = $props();

  let view = $state<ManagedView | null>(null);
  let choice = $state("");
  let progress = $state<{ stage: string; done: number; total: number } | null>(null);
  let error = $state("");
  const off: (() => void)[] = [];

  const gb = (bytes: number) => `${(bytes / 1e9).toFixed(1)} GB`;
  const pct = $derived(progress && progress.total ? Math.round((progress.done / progress.total) * 100) : 0);
  const busy = $derived(view?.busy ?? false);
  const active = $derived(app.ai.config.provider === "managed" ? app.ai.config.model : null);

  async function refresh() {
    view = await api.managedView(locale);
    if (!choice) {
      choice =
        view.running_model ??
        view.models.find((m) => m.recommended && m.min_ram_gb <= Math.max(view!.ram_gb, 8))?.id ??
        view.models[0]?.id ??
        "";
    }
  }

  onMount(async () => {
    off.push(await on<{ stage: string; done: number; total: number }>("managed-progress", (p) => (progress = p)));
    off.push(
      await on<null>("managed-state", async () => {
        await refresh();
        if (!view?.busy) {
          progress = null;
          app.ai.config = await api.aiConfig();
          onchange();
        }
      }),
    );
    off.push(await on<string>("managed-error", (e) => (error = e)));
    await refresh();
  });

  onDestroy(() => off.forEach((f) => f()));

  async function install() {
    error = "";
    progress = { stage: "server", done: 0, total: 0 };
    await api.managedInstall(choice);
  }

  function stageLabel(stage: string): string {
    return stage === "server" ? t("man.stageServer") : stage === "model" ? t("man.stageModel") : t("man.stageStart");
  }
</script>

<div class="managed">
  <div class="mhead">
    <span class="mark"><Icon name="download" size={15} /></span>
    <div>
      <strong>{t("man.title")}</strong>
      <small>{t("man.hint")}</small>
    </div>
  </div>

  {#if view}
    <div class="models" role="radiogroup" aria-label={t("man.title")}>
      {#each view.models as m (m.id)}
        {@const tight = view.ram_gb > 0 && m.min_ram_gb > view.ram_gb}
        <button
          role="radio"
          aria-checked={choice === m.id}
          class="model"
          disabled={busy}
          onclick={() => (choice = m.id)}
        >
          <span class="mtop">
            <span class="mname">{m.name}</span>
            {#if m.recommended}<span class="tag">{t("man.recommended")}</span>{/if}
            {#if active === m.id && view.running_model === m.id}<span class="tag on">{t("man.inUse")}</span>{/if}
            <span class="msize num">{m.installed ? t("man.downloaded") : gb(m.size)}</span>
          </span>
          <span class="mnote">{m.note}{#if tight} {t("man.tight")}{/if}</span>
        </button>
      {/each}
    </div>

    {#if progress}
      <div class="progress" role="status" aria-live="polite">
        <div class="bar"><span style:width="{progress.stage === 'starting' ? 100 : pct}%" class:indet={progress.stage === "starting" || !progress.total}></span></div>
        <small>
          {stageLabel(progress.stage)}
          {#if progress.total && progress.stage !== "starting"}<span class="num"> · {gb(progress.done)} / {gb(progress.total)}</span>{/if}
        </small>
      </div>
    {:else}
      <button
        class="btn primary go"
        disabled={busy || !choice || (active === choice && view.running_model === choice)}
        onclick={install}
      >
        {view.models.find((m) => m.id === choice)?.installed && view.server_installed ? t("man.use") : t("man.install")}
      </button>
    {/if}
    {#if error}<p class="err" role="alert">{error}</p>{/if}
  {/if}
</div>

<style>
  .managed {
    margin-top: 10px;
    padding: 12px;
    border-radius: var(--r-m);
    background: var(--bg);
    display: grid;
    gap: 10px;
  }
  .mhead {
    display: flex;
    gap: 10px;
  }
  .mhead div {
    display: grid;
    gap: 2px;
  }
  .mark {
    flex: none;
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--ink);
    color: var(--on-ink);
  }
  strong {
    font-weight: 600;
  }
  small {
    font-size: 12px;
    color: var(--text-2);
    text-wrap: pretty;
  }
  .models {
    display: grid;
    gap: 4px;
  }
  .model {
    display: grid;
    gap: 2px;
    padding: 8px 10px;
    border-radius: 8px;
    text-align: left;
    box-shadow: inset 0 0 0 1px var(--line);
    transition: box-shadow 160ms var(--ease), background 160ms var(--ease);
  }
  .model:hover:not(:disabled) {
    background: var(--bg-2);
  }
  .model[aria-checked="true"] {
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .model:disabled {
    opacity: 0.6;
  }
  .mtop {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .mname {
    font-weight: 600;
  }
  .tag {
    padding: 1px 6px;
    border-radius: 6px;
    background: var(--bg-3);
    font-size: 10.5px;
    font-weight: 600;
    color: var(--text-2);
  }
  .tag.on {
    background: var(--accent);
    color: var(--on-accent);
  }
  .msize {
    margin-left: auto;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .mnote {
    font-size: 12px;
    color: var(--text-2);
  }
  .go {
    justify-self: start;
  }
  .progress {
    display: grid;
    gap: 6px;
  }
  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--bg-3);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    border-radius: 3px;
    background: var(--accent);
    transition: width 240ms var(--ease);
  }
  .bar span.indet {
    width: 35% !important;
    animation: slide 1.2s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(300%);
    }
  }
  .err {
    margin: 0;
    font-size: 12px;
    color: var(--danger);
    user-select: text;
    -webkit-user-select: text;
  }
</style>
