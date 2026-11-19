<script lang="ts">
  import { tick } from "svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import ForwardDialog from "./ForwardDialog.svelte";
  import PersonCard from "./PersonCard.svelte";
  import { app, chatTitle } from "./app.svelte";
  import { t, time, dayLabel } from "./i18n";

  let promptInput: HTMLInputElement | undefined = $state();
  let confirmBtn: HTMLButtonElement | undefined = $state();

  $effect(() => {
    if (app.prompt) void tick().then(() => promptInput?.select());
  });
  $effect(() => {
    if (app.confirm) void tick().then(() => confirmBtn?.focus());
  });

  function closeConfirm(ok: boolean) {
    app.confirm?.resolve(ok);
    app.confirm = null;
  }

  function closePrompt(value: string | null) {
    app.prompt?.resolve(value);
    app.prompt = null;
  }

  function closeChoice(id: string | null) {
    app.choice?.resolve(id);
    app.choice = null;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    if (app.confirm) closeConfirm(false);
    else if (app.prompt) closePrompt(null);
    else if (app.choice) closeChoice(null);
    else if (app.info) app.info = null;
    else if (app.forward) app.forward = null;
    else if (app.person) app.person = null;
    else return;
    e.preventDefault();
  }

  function who(jid: string): string {
    if (jid === "me") return t("chat.you");
    const c = app.chats.find((x) => x.jid === jid);
    return c ? chatTitle(c) : chatTitle({ jid, name: "" });
  }
</script>

<svelte:window onkeydown={onKey} />

{#if app.menu}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="catcher" onclick={() => (app.menu = null)} oncontextmenu={(e) => { e.preventDefault(); app.menu = null; }}></div>
  <ContextMenu items={app.menu.items} x={app.menu.x} y={app.menu.y} onclose={() => (app.menu = null)} />
{/if}

{#if app.person}
  <PersonCard p={app.person.p} x={app.person.x} y={app.person.y} />
{/if}

{#if app.confirm}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && closeConfirm(false)}>
    <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="cf-title" aria-describedby="cf-body">
      <h2 id="cf-title">{app.confirm.title}</h2>
      <p id="cf-body">{app.confirm.body}</p>
      <div class="acts">
        <button class="btn quiet" onclick={() => closeConfirm(false)}>{t("action.cancel")}</button>
        <button bind:this={confirmBtn} class="btn" class:danger={app.confirm.danger} class:primary={!app.confirm.danger} onclick={() => closeConfirm(true)}>
          {app.confirm.action}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if app.prompt}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && closePrompt(null)}>
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="pr-title">
    <form
      onsubmit={(e) => {
        e.preventDefault();
        const v = app.prompt?.value.trim() ?? "";
        if (v) closePrompt(v);
      }}
    >
      <h2 id="pr-title">{app.prompt.title}</h2>
      <input bind:this={promptInput} bind:value={app.prompt.value} maxlength="40" placeholder={app.prompt.placeholder} aria-label={app.prompt.title} />
      <div class="acts">
        <button type="button" class="btn quiet" onclick={() => closePrompt(null)}>{t("action.cancel")}</button>
        <button type="submit" class="btn primary" disabled={!app.prompt.value.trim()}>{app.prompt.action}</button>
      </div>
    </form>
    </div>
  </div>
{/if}

{#if app.choice}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && closeChoice(null)}>
    <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="ch-title" aria-describedby="ch-body">
      <h2 id="ch-title">{app.choice.title}</h2>
      <p id="ch-body">{app.choice.body}</p>
      <div class="stacked">
        {#each app.choice.options as o (o.id)}
          <button class="btn" class:danger={o.danger} onclick={() => closeChoice(o.id)}>{o.label}</button>
        {/each}
        <button class="btn quiet" onclick={() => closeChoice(null)}>{t("action.cancel")}</button>
      </div>
    </div>
  </div>
{/if}

{#if app.info}
  {@const m = app.info}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (app.info = null)}>
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="in-title">
      <h2 id="in-title">{t("msg.info")}</h2>
      <dl>
        <dt>{m.from_me ? t("msg.sent") : t("msg.received")}</dt>
        <dd>{dayLabel(m.ts)}, {time(m.ts)}</dd>
        {#if !m.from_me && m.sender_jid}
          <dt>{t("info.contact")}</dt>
          <dd>{m.sender_name || who(m.sender_jid)}</dd>
        {/if}
        {#if m.edited || m.forwarded || m.starred}
          <dt>&nbsp;</dt>
          <dd>{[m.edited ? t("msg.edited") : "", m.forwarded ? t("msg.forwarded") : "", m.starred ? t("msg.star") : ""].filter(Boolean).join(" · ")}</dd>
        {/if}
        {#if m.reactions.length}
          <dt>{t("msg.reactions")}</dt>
          <dd>
            {#each m.reactions as r (r.sender_jid)}
              <span class="rxline"><span>{r.emoji}</span>{r.from_me ? t("chat.you") : who(r.sender_jid)}</span>
            {/each}
          </dd>
        {/if}
      </dl>
      <div class="acts">
        <button class="btn primary" onclick={() => (app.info = null)}>{t("storage.done")}</button>
      </div>
    </div>
  </div>
{/if}

{#if app.forward}
  <ForwardDialog />
{/if}

{#if app.toast}
  {#key app.toast.id}
    <div class="toast" role="status">{app.toast.text}</div>
  {/key}
{/if}

<style>
  .catcher {
    position: fixed;
    inset: 0;
    z-index: 59;
  }
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: grid;
    place-items: center;
    background: var(--scrim);
    animation: fade 160ms var(--ease);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  .dialog {
    width: min(360px, calc(100vw - 32px));
    padding: 20px 20px 16px;
    border-radius: 16px;
    background: var(--panel);
    box-shadow: var(--shadow);
    animation: rise 200ms var(--ease);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
  }
  h2 {
    margin: 0 0 6px;
    font-size: 15px;
    font-weight: 600;
  }
  p {
    margin: 0 0 18px;
    color: var(--text-2);
    text-wrap: pretty;
  }
  input {
    width: 100%;
    height: 34px;
    margin: 8px 0 18px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: var(--r-m);
    background: var(--bg);
    outline: none;
    user-select: text;
    -webkit-user-select: text;
  }
  input:focus {
    border-color: var(--accent);
  }
  input::placeholder {
    color: var(--text-3);
  }
  .acts {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
  .stacked {
    display: grid;
    gap: 6px;
  }
  .stacked .btn {
    height: 36px;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px 14px;
    margin: 12px 0 18px;
    font-size: 13px;
  }
  dt {
    color: var(--text-3);
  }
  dd {
    margin: 0;
    display: grid;
    gap: 4px;
  }
  .rxline {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .toast {
    position: fixed;
    left: 50%;
    bottom: 22px;
    z-index: 80;
    transform: translateX(-50%);
    max-width: min(440px, calc(100vw - 32px));
    padding: 9px 16px;
    border-radius: 999px;
    background: var(--ink);
    color: var(--on-ink);
    font-size: 12.5px;
    font-weight: 500;
    box-shadow: var(--shadow);
    animation: toast 220ms var(--ease);
  }
  @keyframes toast {
    from {
      opacity: 0;
      transform: translate(-50%, 8px);
    }
  }
</style>
