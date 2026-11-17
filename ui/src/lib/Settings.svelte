<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import Managed from "./Managed.svelte";
  import Segmented from "./Segmented.svelte";
  import Switch from "./Switch.svelte";
  import { api, type Mode, type NotifyMode, type Provider } from "./api";
  import { aiReady, app, saveAiConfig, saveAutoReply } from "./app.svelte";
  import { locale, t } from "./i18n";

  let providers = $state<Provider[]>([]);
  let scanning = $state(true);
  let confirmUnlink = $state(false);
  let instructions = $state(app.auto.instructions);

  const modes: { value: Mode; label: string }[] = [
    { value: "none", label: t("mode.none") },
    { value: "selected", label: t("mode.selected") },
    { value: "all", label: t("mode.all") },
  ];

  let notify = $state<NotifyMode>("notch");
  let systemUnavailable = $state(false);

  const notifyModes: { value: NotifyMode; label: string }[] = [
    { value: "notch", label: t("notify.notch") },
    { value: "system", label: t("notify.system") },
    { value: "off", label: t("notify.off") },
  ];

  async function setNotify(mode: NotifyMode) {
    notify = mode;
    systemUnavailable = false;
    await api.setNotifyMode(mode);
    if (mode === "system") systemUnavailable = !(await api.enableSystemNotifications(locale));
  }

  async function scan() {
    scanning = true;
    try {
      providers = await api.providers();
    } finally {
      scanning = false;
    }
  }

  onMount(async () => {
    notify = await api.notifyMode();
    await scan();
  });

  function pick(p: Provider, model: string) {
    void saveAiConfig({ provider: p.kind, base_url: p.base_url, model });
  }

  function status(p: Provider): string {
    if (p.running) return p.models.length ? t("set.running") : t("set.noModels");
    return p.installed ? t("set.stopped") : t("set.missing");
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") app.settingsOpen = false;
  }
</script>

<svelte:window onkeydown={onKey} />

<aside class="sheet" aria-label={t("set.title")}>
  <header data-tauri-drag-region>
    <h1>{t("set.title")}</h1>
    <button class="icon-btn" onclick={() => (app.settingsOpen = false)} aria-label={t("set.close")}>
      <Icon name="close" size={18} />
    </button>
  </header>

  <div class="body scroll">
    <section>
      <div class="head">
        <h2>{t("set.model")}</h2>
        <button class="btn quiet small" onclick={scan} disabled={scanning}>{t("set.refresh")}</button>
      </div>
      <p class="hint">{t("set.modelHint")}</p>
      <ul class="providers" aria-busy={scanning}>
        {#each providers.filter((p) => p.kind !== "managed") as p (p.kind)}
          {@const selected = app.ai.config.provider === p.kind}
          <li class:off={!p.running}>
            <div class="prow">
              <span class="dot" class:on={p.running}></span>
              <span class="pname">{p.name}</span>
              <span class="pstatus">{status(p)}</span>
            </div>
            {#if p.running && p.models.length > 0}
              <select
                aria-label="{t('set.pickModel')} · {p.name}"
                value={selected ? app.ai.config.model : ""}
                onchange={(e) => pick(p, e.currentTarget.value)}
                class:chosen={selected}
              >
                <option value="" disabled>{t("set.pickModel")}</option>
                {#each p.models as m (m)}
                  <option value={m}>{m}</option>
                {/each}
              </select>
            {/if}
          </li>
        {:else}
          {#if scanning}
            <li class="skeleton"></li>
            <li class="skeleton"></li>
          {/if}
        {/each}
      </ul>
      <Managed onchange={scan} />
    </section>

    <section>
      <h2>{t("set.assistant")}</h2>
      <label class="line">
        <span><strong>{t("set.web")}</strong><small>{t("set.webHint")}</small></span>
        <Switch checked={app.ai.config.web_access} label={t("set.web")} onchange={(v) => saveAiConfig({ web_access: v })} />
      </label>
      <label class="line">
        <span><strong>{t("set.confirm")}</strong><small>{t("set.confirmHint")}</small></span>
        <Switch checked={app.ai.config.confirm_send} label={t("set.confirm")} onchange={(v) => saveAiConfig({ confirm_send: v })} />
      </label>
    </section>

    <section>
      <h2>{t("set.auto")}</h2>
      <label class="line">
        <span>
          <strong>{t("set.autoOn")}</strong>
          {#if !aiReady()}<small>{t("set.autoNeedsModel")}</small>{/if}
        </span>
        <Switch
          checked={app.auto.enabled}
          label={t("set.autoOn")}
          disabled={!aiReady() && !app.auto.enabled}
          onchange={(v) => saveAutoReply({ enabled: v })}
        />
      </label>
      <p class="warn">{t("set.autoWarn")}</p>

      <div class="group" class:dim={!app.auto.enabled}>
        <div class="field">
          <span>{t("set.chats")}</span>
          <Segmented label={t("set.chats")} value={app.auto.chats} options={modes} onchange={(v) => saveAutoReply({ chats: v })} />
        </div>
        <div class="field">
          <span>{t("set.groups")}</span>
          <Segmented label={t("set.groups")} value={app.auto.groups} options={modes} onchange={(v) => saveAutoReply({ groups: v })} />
        </div>
        {#if app.auto.chats === "selected" || app.auto.groups === "selected"}
          <p class="hint">{t("set.selectedHint")}</p>
        {/if}
        <label class="line">
          <span><strong>{t("set.mentions")}</strong></span>
          <Switch
            checked={app.auto.groups_only_mentions}
            label={t("set.mentions")}
            onchange={(v) => saveAutoReply({ groups_only_mentions: v })}
          />
        </label>
        <label class="line">
          <span><strong>{t("set.maxHour")}</strong></span>
          <input
            class="numfield num"
            type="number"
            min="1"
            max="60"
            value={app.auto.max_per_chat_per_hour}
            onchange={(e) => {
              const n = Math.min(60, Math.max(1, Math.round(Number(e.currentTarget.value) || 1)));
              e.currentTarget.value = String(n);
              void saveAutoReply({ max_per_chat_per_hour: n });
            }}
          />
        </label>
        <label class="stack">
          <strong>{t("set.instructions")}</strong>
          <textarea
            rows="3"
            maxlength="2000"
            bind:value={instructions}
            placeholder={t("set.instructionsPh")}
            onblur={() => saveAutoReply({ instructions: instructions.trim() })}
          ></textarea>
        </label>
      </div>
    </section>

    <section>
      <div class="head">
        <h2>{t("set.notify")}</h2>
        {#if notify !== "off"}
          <button class="btn quiet small" onclick={() => api.notchTest(t("set.notifyTestTitle"), t("set.notifyTestBody"))}>
            {t("set.notifyTest")}
          </button>
        {/if}
      </div>
      <p class="hint">{t("set.notifyHint")}</p>
      <Segmented label={t("set.notify")} value={notify} options={notifyModes} onchange={setNotify} />
      {#if systemUnavailable}<p class="hint">{t("set.notifyUnavailable")}</p>{/if}
    </section>

    <section>
      <h2>{t("set.account")}</h2>
      {#if confirmUnlink}
        <p class="warn">{t("set.unlinkConfirm")}</p>
        <div class="acts">
          <button class="btn quiet" onclick={() => (confirmUnlink = false)}>{t("action.cancel")}</button>
          <button
            class="btn danger"
            onclick={async () => {
              app.settingsOpen = false;
              await api.logout();
            }}>{t("set.unlinkYes")}</button
          >
        </div>
      {:else}
        <button class="btn" onclick={() => (confirmUnlink = true)}>{t("set.unlink")}</button>
      {/if}
    </section>
  </div>
</aside>

<style>
  .sheet {
    width: 340px;
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-2);
    border-left: 1px solid var(--line);
    animation: slide 220ms var(--ease);
  }
  @keyframes slide {
    from {
      transform: translateX(24px);
      opacity: 0;
    }
  }
  header {
    flex: none;
    height: 52px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 10px 0 20px;
  }
  h1 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    pointer-events: none;
  }
  .body {
    flex: 1;
    min-height: 0;
    padding: 0 20px 24px;
  }
  section {
    padding: 20px 0 4px;
  }
  section + section {
    margin-top: 16px;
    border-top: 1px solid var(--line);
  }
  h2 {
    margin: 0 0 10px;
    font-size: 13px;
    font-weight: 600;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .head h2 {
    margin: 0;
  }
  .small {
    height: 26px;
    padding: 0 8px;
    margin-right: -8px;
    font-size: 12px;
  }
  .hint,
  .warn,
  small {
    font-size: 12px;
    color: var(--text-2);
    text-wrap: pretty;
  }
  .hint {
    margin: 4px 0 12px;
  }
  .warn {
    margin: 2px 0 14px;
    padding: 8px 10px;
    border-radius: var(--r-m);
    background: var(--accent-soft);
    color: var(--text);
  }
  .providers {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 6px;
  }
  .providers li {
    display: grid;
    gap: 8px;
    padding: 10px 12px;
    border-radius: var(--r-m);
    background: var(--bg);
  }
  .providers li.off {
    background: none;
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .skeleton {
    height: 38px;
    background: var(--bg-3) !important;
    animation: fade 1.2s ease-in-out infinite;
  }
  @keyframes fade {
    50% {
      opacity: 0.5;
    }
  }
  .prow {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-3);
    opacity: 0.5;
  }
  .dot.on {
    background: var(--accent);
    opacity: 1;
  }
  .pname {
    flex: 1;
    font-weight: 600;
  }
  .pstatus {
    font-size: 12px;
    color: var(--text-2);
  }
  select {
    width: 100%;
    height: 30px;
    padding: 0 28px 0 10px;
    border: 0;
    border-radius: 8px;
    background: var(--bg-2)
      url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12' fill='none' stroke='%23888' stroke-width='1.6' stroke-linecap='round' stroke-linejoin='round'><path d='M3 4.5l3 3 3-3'/></svg>")
      no-repeat right 9px center;
    appearance: none;
    font: inherit;
    color: var(--text-2);
    text-overflow: ellipsis;
  }
  select.chosen {
    color: var(--text);
    font-weight: 500;
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .line {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 7px 0;
  }
  .line > span {
    display: grid;
    gap: 2px;
  }
  strong {
    font-weight: 500;
  }
  .group {
    display: grid;
    gap: 6px;
    transition: opacity 180ms var(--ease);
  }
  .group.dim {
    opacity: 0.5;
  }
  .field {
    display: grid;
    grid-template-columns: 64px 1fr;
    align-items: center;
    gap: 10px;
    padding: 3px 0;
  }
  .field > span {
    font-weight: 500;
  }
  .group .hint {
    margin: 2px 0 4px;
  }
  .numfield {
    width: 56px;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: 8px;
    background: var(--bg-3);
    text-align: right;
  }
  .stack {
    display: grid;
    gap: 6px;
    padding-top: 6px;
  }
  textarea {
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: var(--r-m);
    background: var(--bg-3);
    resize: vertical;
    line-height: 1.4;
    user-select: text;
    -webkit-user-select: text;
  }
  textarea::placeholder {
    color: var(--text-3);
  }
  .acts {
    display: flex;
    gap: 6px;
  }
  .btn.danger {
    background: var(--danger);
    color: var(--bg);
  }
</style>
