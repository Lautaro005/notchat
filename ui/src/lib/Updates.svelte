<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import Switch from "./Switch.svelte";
  import { api } from "./api";
  import { app } from "./app.svelte";
  import { t, tf } from "./i18n";

  let auto = $state(true);
  let version = $state("");
  let checking = $state(false);
  let installing = $state(false);
  let result = $state<"none" | "error" | null>(null);
  let error = $state("");

  onMount(async () => {
    auto = await api.updateAuto();
    version = await api.version();
  });

  async function check() {
    checking = true;
    result = null;
    try {
      const r = await api.checkUpdate();
      app.update = r;
      if (!r) result = "none";
    } catch (e) {
      result = "error";
      error = String(e);
    } finally {
      checking = false;
    }
  }

  async function install() {
    if (!app.update) return;
    installing = true;
    result = null;
    try {
      await api.installUpdate(app.update.version);
    } catch (e) {
      result = "error";
      error = String(e);
      installing = false;
    }
  }
</script>

<section>
  <div class="current">
    <span class="appmark"><Icon name="send" size={22} /></span>
    <div>
      <strong>Notchat</strong>
      <span class="num">{tf("upd.current", { v: version })}</span>
    </div>
    <button class="btn" onclick={check} disabled={checking || installing}>
      {checking ? t("upd.checking") : t("upd.check")}
    </button>
  </div>

  {#if app.update}
    <div class="available">
      <Icon name="download" size={18} />
      <div>
        <strong>{tf("upd.available", { v: app.update.version })}</strong>
        {#if app.update.notes}<p class="notes">{app.update.notes}</p>{/if}
      </div>
      <button class="btn primary" onclick={install} disabled={installing}>
        {installing ? t("upd.installing") : t("upd.install")}
      </button>
    </div>
  {:else if result === "none"}
    <p class="ok" role="status"><Icon name="check" size={15} />{t("upd.none")}</p>
  {/if}
  {#if result === "error"}
    <p class="err" role="alert">{tf("upd.failed", { e: error })}</p>
  {/if}
</section>

<section>
  <label class="line">
    <span><strong>{t("upd.auto")}</strong><small>{t("upd.autoHint")}</small></span>
    <Switch
      checked={auto}
      label={t("upd.auto")}
      onchange={async (v) => {
        auto = v;
        await api.setUpdateAuto(v);
      }}
    />
  </label>
</section>

<style>
  section + section {
    margin-top: 18px;
    padding-top: 16px;
    border-top: 1px solid var(--line-soft);
  }
  .current {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .appmark {
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    border-radius: 13px;
    background: var(--ink);
    color: var(--on-ink);
    transform: rotate(-6deg);
  }
  .current div {
    flex: 1;
    display: grid;
  }
  .current strong {
    font-family: var(--rounded);
    font-size: 15px;
  }
  .current span {
    color: var(--text-2);
    font-size: 12px;
  }
  .available {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin-top: 16px;
    padding: 14px;
    border-radius: 12px;
    background: var(--accent-soft);
  }
  .available > :global(svg) {
    margin-top: 1px;
    color: var(--accent);
  }
  .available div {
    flex: 1;
    min-width: 0;
  }
  .notes {
    margin: 6px 0 0;
    max-height: 120px;
    overflow: auto;
    font-size: 12px;
    color: var(--text-2);
    white-space: pre-wrap;
  }
  .ok,
  .err {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 14px 0 0;
    font-size: 12.5px;
  }
  .ok {
    color: var(--text-2);
  }
  .err {
    color: var(--danger);
  }
  .line {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
  }
  .line > span {
    display: grid;
    gap: 2px;
  }
  .line strong {
    font-weight: 500;
  }
  small {
    font-size: 12px;
    color: var(--text-2);
    text-wrap: pretty;
  }
</style>
