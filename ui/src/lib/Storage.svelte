<script lang="ts">
  import { onMount } from "svelte";
  import Avatar from "./Avatar.svelte";
  import Segmented from "./Segmented.svelte";
  import Switch from "./Switch.svelte";
  import { api, type StorageInfo } from "./api";
  import { app, chatTitle, confirmAction, toast } from "./app.svelte";
  import { bytes, t, tf } from "./i18n";

  let info = $state<StorageInfo | null>(null);
  let busy = $state(false);
  let older = $state<"30" | "90" | "365">("90");

  let fullHistory = $state(true);

  async function refresh() {
    info = await api.storage();
  }
  onMount(() => {
    void refresh();
    void api.getFullHistory().then((v) => (fullHistory = v), () => {});
  });

  async function setFullHistory(v: boolean) {
    fullHistory = v;
    try {
      await api.setFullHistory(v);
    } catch {
      fullHistory = !v;
      toast(t("toast.failed"));
    }
  }

  const total = $derived(info ? info.database + info.media + info.avatars + info.session : 0);
  const parts = $derived(
    info
      ? [
          { key: "media", label: t("storage.media"), value: info.media },
          { key: "database", label: t("storage.database"), value: info.database },
          { key: "avatars", label: t("storage.avatars"), value: info.avatars },
          { key: "session", label: t("storage.session"), value: info.session },
        ]
      : [],
  );

  async function run(fn: () => Promise<unknown>, done = t("storage.done")) {
    busy = true;
    try {
      await fn();
      toast(done);
    } catch {
      toast(t("toast.failed"));
    } finally {
      busy = false;
      await refresh();
    }
  }

  async function deleteOlder() {
    const label = t(older === "30" ? "storage.d30" : older === "90" ? "storage.d90" : "storage.d365");
    const ok = await confirmAction({ title: `${t("storage.older")}: ${label.toLowerCase()}`, body: t("storage.olderHint"), action: t("storage.olderAction") });
    if (!ok) return;
    busy = true;
    try {
      const n = await api.deleteOlder(Number(older));
      toast(tf("storage.deleted", { n }));
    } catch {
      toast(t("toast.failed"));
    } finally {
      busy = false;
      await refresh();
    }
  }

  function name(jid: string, fallback: string): string {
    const chat = app.chats.find((c) => c.jid === jid);
    return chat ? chatTitle(chat) : chatTitle({ jid, name: fallback });
  }
</script>

<p class="hint">{t("storage.hint")}</p>

<section class="summary" aria-busy={!info}>
  <div class="total">
    <span>{t("storage.total")}</span>
    <strong class="num">{info ? bytes(total) : "—"}</strong>
  </div>
  <div class="meter" aria-hidden="true">
    {#each parts as p (p.key)}
      <span class={p.key} style:flex-grow={Math.max(p.value, total * 0.004)}></span>
    {/each}
  </div>
  <ul class="legend">
    {#each parts as p (p.key)}
      <li>
        <span class="sw {p.key}"></span>
        <span class="lbl">{p.label}</span>
        <span class="num val">{bytes(p.value)}</span>
        {#if p.key === "media"}
          <button class="btn quiet small" disabled={busy || !p.value} onclick={() => run(() => api.clearMedia())}>{t("storage.clearMedia")}</button>
        {:else if p.key === "avatars"}
          <button class="btn quiet small" disabled={busy || !p.value} onclick={() => run(async () => { await api.clearAvatars(); app.avatars = {}; })}>
            {t("storage.clearAvatars")}
          </button>
        {/if}
      </li>
    {/each}
  </ul>
</section>

<section>
  <div class="hist">
    <span><strong>{t("stor.fullHistory")}</strong><small>{t("stor.fullHistoryHint")}</small></span>
    <Switch checked={fullHistory} label={t("stor.fullHistory")} onchange={setFullHistory} />
  </div>
</section>

<section>
  <h3>{t("storage.older")}</h3>
  <p class="hint">{t("storage.olderHint")}</p>
  <div class="olderrow">
    <Segmented
      label={t("storage.older")}
      value={older}
      options={[
        { value: "30", label: t("storage.d30") },
        { value: "90", label: t("storage.d90") },
        { value: "365", label: t("storage.d365") },
      ]}
      onchange={(v) => (older = v)}
    />
    <button class="btn" disabled={busy} onclick={deleteOlder}>{t("storage.olderAction")}</button>
  </div>
</section>

{#if info && info.chats.length}
  <section>
    <h3>{t("storage.byChat")}</h3>
    <ul class="chats">
      {#each info.chats.slice(0, 40) as c (c.jid)}
        {@const title = name(c.jid, c.name)}
        <li>
          <Avatar {title} jid={c.jid} group={c.jid.endsWith("@g.us")} size={32} />
          <span class="cmeta">
            <span class="cname">{title}</span>
            <small class="num">
              {[
                tf("storage.messages", { n: c.messages.toLocaleString() }),
                c.media ? bytes(c.media) : "",
                c.cached ? tf("storage.cachedOf", { a: bytes(c.cached) }) : "",
              ]
                .filter(Boolean)
                .join(" · ")}
            </small>
          </span>
          {#if c.cached}
            <button class="btn quiet small" disabled={busy} onclick={() => run(() => api.clearMedia(c.jid))}>{t("storage.clearMedia")}</button>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .hint {
    margin: 0 0 14px;
    font-size: 12px;
    color: var(--text-2);
    text-wrap: pretty;
  }
  section + section {
    margin-top: 18px;
    padding-top: 20px;
    border-top: 1px solid var(--line-soft);
  }
  h3 {
    margin: 0 0 4px;
    font-size: 13px;
    font-weight: 600;
  }
  .total {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 10px;
  }
  .total span {
    font-weight: 500;
  }
  .total strong {
    font-family: var(--rounded);
    font-size: 22px;
    font-weight: 700;
  }
  .meter {
    display: flex;
    gap: 2px;
    height: 10px;
    overflow: hidden;
    border-radius: 5px;
    background: var(--bg-3);
  }
  .meter span {
    flex-basis: 0;
    transition: flex-grow 400ms var(--ease);
  }
  .media {
    background: var(--accent);
  }
  .database {
    background: var(--ink);
  }
  .avatars {
    background: color-mix(in srgb, var(--accent) 45%, var(--bg-3));
  }
  .session {
    background: var(--text-3);
  }
  .legend {
    margin: 12px 0 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 2px;
  }
  .legend li {
    min-height: 32px;
    display: flex;
    align-items: center;
    gap: 9px;
  }
  .sw {
    width: 9px;
    height: 9px;
    border-radius: 3px;
  }
  .lbl {
    flex: 1;
  }
  .val {
    color: var(--text-2);
    min-width: 64px;
    text-align: right;
  }
  .small {
    height: 26px;
    padding: 0 9px;
    font-size: 12px;
  }
  .olderrow {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .chats {
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
  }
  .chats li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 48px;
  }
  .cmeta {
    flex: 1;
    min-width: 0;
    display: grid;
  }
  .cname {
    font-weight: 500;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .cmeta small {
    color: var(--text-3);
    font-size: 11.5px;
  }
  .hist {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  .hist span {
    display: grid;
    gap: 2px;
  }
  .hist strong {
    font-size: 13px;
    font-weight: 600;
  }
  .hist small {
    font-size: 12px;
    color: var(--text-3);
  }
</style>
