<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { api, type AiRow, type SendAction } from "./api";
  import { aiReady, app, askAi } from "./app.svelte";
  import { t, type Key } from "./i18n";

  const MAX_FILE = 15 * 1024 * 1024;

  let draft = $state("");
  let files = $state<{ name: string; base64: string }[]>([]);
  let error = $state("");
  let scroller: HTMLElement | undefined = $state();
  let input: HTMLTextAreaElement | undefined = $state();
  let picker: HTMLInputElement | undefined = $state();

  const ai = $derived(app.ai);
  // Local models may stream their reasoning inline; never show it.
  const streaming = $derived(
    ai.streaming.replace(/<think>[\s\S]*?<\/think>/g, "").replace(/<think>[\s\S]*$/, "").trim(),
  );

  $effect(() => {
    void app.tail;
    void tick().then(() => scroller?.scrollTo({ top: scroller.scrollHeight }));
  });

  function parse<T>(json: string, fallback: T): T {
    try {
      return json ? (JSON.parse(json) as T) : fallback;
    } catch {
      return fallback;
    }
  }

  function action(row: AiRow): SendAction | null {
    return parse<SendAction | null>(row.meta, null);
  }

  function attachments(row: AiRow): { name: string; kind: string }[] {
    return parse(row.meta, []);
  }

  function toolLabel(name: string): string {
    const key = `tool.${name}` as Key;
    return ["list_chats", "read_chat", "search_messages", "send_message", "web_search", "fetch_url"].includes(name)
      ? t(key)
      : name;
  }

  function toBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result).split(",")[1] ?? "");
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(file);
    });
  }

  async function addFiles(list: FileList | null) {
    error = "";
    for (const file of list ?? []) {
      if (files.length >= 6) break;
      if (file.size > MAX_FILE) {
        error = t("ai.fileTooBig");
        continue;
      }
      files.push({ name: file.name, base64: await toBase64(file) });
    }
    if (picker) picker.value = "";
  }

  async function submit() {
    const text = draft.trim();
    if ((!text && files.length === 0) || ai.busy) return;
    error = "";
    try {
      await askAi(text, $state.snapshot(files));
      draft = "";
      files = [];
    } catch (e) {
      error = String(e);
    }
    await tick();
    input?.focus();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void submit();
    }
  }

  async function resolve(id: number, approve: boolean) {
    error = "";
    try {
      await api.aiResolve(id, approve);
    } catch (e) {
      error = String(e);
    }
  }
</script>

<section class="ai">
  <header data-tauri-drag-region>
    <span class="mark"><Icon name="spark" size={16} /></span>
    <div>
      <h1>{t("ai.name")}</h1>
      <span>{ai.config.model || t("ai.sub")}</span>
    </div>
    {#if ai.rows.length > 0}
      <button class="icon-btn" onclick={() => api.aiClear()} aria-label={t("ai.clear")} title={t("ai.clear")}>
        <Icon name="close" size={18} />
      </button>
    {/if}
  </header>

  <div class="log scroll" bind:this={scroller}>
    {#if !aiReady()}
      <div class="intro">
        <h2>{t("ai.setupTitle")}</h2>
        <p>{t("ai.setupBody")}</p>
        <button class="btn primary" onclick={() => (app.settingsOpen = true)}>{t("ai.setupAction")}</button>
      </div>
    {:else if ai.rows.length === 0 && !ai.busy}
      <div class="intro">
        <h2>{t("ai.emptyTitle")}</h2>
        <p>{t("ai.emptyBody")}</p>
        <div class="tries">
          {#each ["ai.try1", "ai.try2", "ai.try3"] as const as key (key)}
            <button
              onclick={() => {
                draft = t(key);
                input?.focus();
              }}>{t(key)}</button
            >
          {/each}
        </div>
      </div>
    {/if}

    {#each ai.rows as row (row.id)}
      {#if row.role === "user"}
        <article class="turn me">
          <div class="bubble">
            {#each attachments(row) as a (a.name)}
              <span class="file"><Icon name={a.kind === "image" ? "image" : "doc"} size={15} />{a.name}</span>
            {/each}
            {#if row.content}<span class="text">{row.content}</span>{/if}
          </div>
        </article>
      {:else if row.role === "assistant"}
        <article class="turn answer"><p>{row.content}</p></article>
      {:else if row.role === "tool"}
        <p class="step">
          <Icon name={row.content.startsWith("web") || row.content === "fetch_url" ? "globe" : "check"} size={14} />
          {toolLabel(row.content)}
        </p>
      {:else if row.role === "action"}
        {@const a = action(row)}
        {#if a}
          <article class="card" class:done={a.state !== "pending"}>
            <span class="to">{t("action.to")} <strong>{a.chat_name}</strong></span>
            <p>{a.text}</p>
            {#if a.state === "pending"}
              <div class="acts">
                <button class="btn quiet" onclick={() => resolve(row.id, false)}>{t("action.cancel")}</button>
                <button class="btn primary" onclick={() => resolve(row.id, true)}>{t("action.send")}</button>
              </div>
            {:else}
              <span class="state" class:bad={a.state === "failed"}>
                {#if a.state === "sent"}<Icon name="check" size={14} />{/if}
                {t(`action.${a.state}` as Key)}
              </span>
            {/if}
          </article>
        {/if}
      {:else}
        <p class="err" role="alert">{row.content}</p>
      {/if}
    {/each}

    {#if ai.busy}
      <article class="turn answer live" aria-live="polite">
        {#if streaming}<p>{streaming}</p>{:else}<p class="wait">{t("ai.thinking")}</p>{/if}
      </article>
    {/if}
  </div>

  <footer>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    {#if files.length > 0}
      <div class="files">
        {#each files as f, i (i)}
          <span class="file chip">
            <Icon name="doc" size={14} />{f.name}
            <button onclick={() => files.splice(i, 1)} aria-label="{t('ai.remove')} {f.name}"><Icon name="close" size={12} /></button>
          </span>
        {/each}
      </div>
    {/if}
    <div class="composer">
      <input
        bind:this={picker}
        type="file"
        multiple
        hidden
        accept=".txt,.md,.csv,.json,.pdf,.log,.html,.xml,.yaml,.yml,.rs,.py,.js,.ts,image/png,image/jpeg,image/webp,image/gif"
        onchange={(e) => addFiles(e.currentTarget.files)}
      />
      <button class="icon-btn" onclick={() => picker?.click()} disabled={!aiReady()} aria-label={t("ai.attach")} title={t("ai.attach")}>
        <Icon name="clip" size={18} />
      </button>
      <textarea
        bind:this={input}
        bind:value={draft}
        rows="1"
        placeholder={t("ai.placeholder")}
        aria-label={t("ai.placeholder")}
        disabled={!aiReady()}
        onkeydown={onKey}
      ></textarea>
      {#if ai.busy}
        <button class="round stop" onclick={() => api.aiStop()} aria-label={t("ai.stop")} title={t("ai.stop")}>
          <Icon name="stop" size={16} />
        </button>
      {:else}
        <button class="round" onclick={submit} disabled={!draft.trim() && files.length === 0} aria-label={t("chat.send")}>
          <Icon name="send" size={18} />
        </button>
      {/if}
    </div>
  </footer>
</section>

<style>
  .ai {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  header {
    flex: none;
    height: 52px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 12px 0 20px;
    border-bottom: 1px solid var(--line);
  }
  header > div {
    flex: 1;
    min-width: 0;
    pointer-events: none;
  }
  .mark {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--accent);
    color: var(--on-accent);
  }
  h1 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }
  header span:not(.mark) {
    display: block;
    font-size: 11px;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .log {
    flex: 1;
    min-height: 0;
    padding: 20px;
  }
  .log > * {
    max-width: 680px;
    margin-inline: auto;
  }
  .intro {
    padding: 12vh 0 24px;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 22px;
    font-weight: 600;
    letter-spacing: -0.01em;
    text-wrap: balance;
  }
  .intro p {
    margin: 0 0 20px;
    max-width: 52ch;
    font-size: 15px;
    color: var(--text-2);
    text-wrap: pretty;
  }
  .tries {
    display: grid;
    justify-items: start;
    gap: 6px;
  }
  .tries button {
    padding: 7px 12px;
    border-radius: var(--r-l);
    background: var(--bg-2);
    font-size: 13px;
    text-align: left;
    transition: background 160ms var(--ease);
  }
  .tries button:hover {
    background: var(--bg-3);
  }
  .turn {
    margin-top: 16px;
    user-select: text;
    -webkit-user-select: text;
    cursor: text;
  }
  .turn.me {
    display: grid;
    justify-items: end;
  }
  .bubble {
    max-width: 80%;
    display: grid;
    gap: 6px;
    padding: 8px 13px;
    border-radius: var(--r-l);
    background: var(--ink);
    color: var(--on-ink);
    font-size: 14px;
    line-height: 1.4;
  }
  .text,
  .answer p,
  .card p {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .answer p {
    margin: 0;
    max-width: 70ch;
    font-size: 14px;
    line-height: 1.5;
  }
  .wait {
    color: var(--text-3);
    animation: pulse 1.4s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.45;
    }
  }
  .step {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-block: 10px 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .card {
    margin-top: 14px;
    padding: 14px;
    border-radius: var(--r-l);
    background: var(--bg-2);
    border: 1px solid var(--line);
    max-width: 440px;
    margin-inline: auto auto;
  }
  .log > .card {
    margin-inline: max(0px, calc((100% - 680px) / 2)) auto;
  }
  .card .to {
    font-size: 12px;
    color: var(--text-2);
  }
  .card strong {
    color: var(--text);
    font-weight: 600;
  }
  .card p {
    margin: 6px 0 12px;
    font-size: 14px;
    line-height: 1.4;
    user-select: text;
    -webkit-user-select: text;
  }
  .card.done p {
    margin-bottom: 8px;
    color: var(--text-2);
  }
  .acts {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
  .acts .primary {
    background: var(--accent);
    color: var(--on-accent);
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  .state.bad,
  .err {
    color: var(--danger);
  }
  .err {
    margin-block: 12px 0;
    font-size: 12px;
    user-select: text;
    -webkit-user-select: text;
  }
  .file {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 500;
    opacity: 0.85;
  }
  footer {
    flex: none;
    padding: 8px 16px 14px;
  }
  footer > * {
    max-width: 712px;
    margin-inline: auto;
  }
  footer .err {
    margin: 0 auto 6px;
    padding-left: 14px;
  }
  .files {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 6px;
  }
  .chip {
    padding: 4px 4px 4px 10px;
    border-radius: var(--r-l);
    background: var(--bg-2);
    opacity: 1;
  }
  .chip button {
    width: 18px;
    height: 18px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: var(--text-2);
  }
  .chip button:hover {
    background: var(--bg-3);
  }
  .composer {
    display: flex;
    align-items: flex-end;
    gap: 4px;
    padding: 4px;
    border-radius: var(--r-l);
    background: var(--bg-2);
    border: 1px solid var(--line);
    transition: border-color 160ms var(--ease);
  }
  .composer:focus-within {
    border-color: var(--text-3);
  }
  .composer .icon-btn {
    width: 28px;
    height: 28px;
    border-radius: 50%;
  }
  textarea {
    flex: 1;
    min-width: 0;
    max-height: 132px;
    padding: 5px 4px;
    border: 0;
    background: none;
    resize: none;
    outline: none;
    font-size: 14px;
    line-height: 1.38;
    field-sizing: content;
    user-select: text;
    -webkit-user-select: text;
  }
  textarea::placeholder {
    color: var(--text-3);
  }
  .round {
    flex: none;
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--accent);
    color: var(--on-accent);
    transition: opacity 160ms var(--ease), transform 160ms var(--ease);
  }
  .round:disabled {
    opacity: 0;
    transform: scale(0.8);
  }
  .round.stop {
    background: var(--ink);
    color: var(--on-ink);
  }
</style>
