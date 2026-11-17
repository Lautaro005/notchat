<script lang="ts">
  import { api } from "./api";
  import { app } from "./app.svelte";
  import { t } from "./i18n";

  let usePhone = $state(false);
  let phone = $state("");
  let busy = $state(false);

  const status = $derived(app.status);
  const qrSrc = $derived(
    status.qr_svg ? `data:image/svg+xml;utf8,${encodeURIComponent(status.qr_svg)}` : null,
  );
  const phoneValid = $derived(/^\d{7,15}$/.test(phone.replace(/\D/g, "")));

  async function requestCode(e: SubmitEvent) {
    e.preventDefault();
    if (!phoneValid || busy) return;
    busy = true;
    try {
      await api.startSession(phone.replace(/\D/g, ""));
    } finally {
      busy = false;
    }
  }

  async function retry() {
    usePhone = false;
    await api.startSession();
  }
</script>

<main class="pair" data-tauri-drag-region>
  <section class="card">
    <div class="code" aria-live="polite">
      {#if status.link === "error"}
        <p class="err">{t("pair.error")}</p>
        <button class="btn primary" onclick={retry}>{t("pair.retry")}</button>
      {:else if usePhone && status.pair_code}
        <output class="paircode num">{status.pair_code.slice(0, 4)}<span>–</span>{status.pair_code.slice(4)}</output>
      {:else if usePhone}
        <form onsubmit={requestCode}>
          <label for="phone">{t("pair.phoneLabel")}</label>
          <input id="phone" class="num" inputmode="numeric" autocomplete="off" bind:value={phone} placeholder="5491155551234" />
          <small>{t("pair.phoneHint")}</small>
          <button class="btn primary" disabled={!phoneValid || busy}>{t("pair.getCode")}</button>
        </form>
      {:else if qrSrc}
        <img class="qr" src={qrSrc} alt="QR" width="232" height="232" />
      {:else}
        <div class="qr skeleton" role="status">{t("pair.waiting")}</div>
      {/if}
    </div>

    <div class="how">
      <h1>{t("pair.title")}</h1>
      {#if usePhone && status.pair_code}
        <p>{t("pair.codeHelp")}</p>
      {:else}
        <ol>
          <li>{t("pair.step1")}</li>
          <li>{t("pair.step2")}</li>
          <li>{t("pair.step3")}</li>
        </ol>
      {/if}
      {#if status.link !== "error"}
        <button
          class="btn quiet swap"
          onclick={() => {
            if (usePhone && status.pair_code) void retry();
            else usePhone = !usePhone;
          }}
        >
          {usePhone ? t("pair.useQr") : t("pair.usePhone")}
        </button>
      {/if}
    </div>
  </section>
  <p class="notice">{t("pair.notice")}</p>
</main>

<style>
  .pair {
    height: 100%;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 28px;
    padding: 48px 32px 32px;
    background: var(--bg-2);
  }
  .card {
    display: flex;
    align-items: center;
    gap: 40px;
    padding: 28px 40px 28px 28px;
    background: var(--bg);
    border-radius: var(--r-xl);
    box-shadow: var(--shadow);
  }
  .code {
    width: 256px;
    height: 256px;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 14px;
    border-radius: var(--r-l);
    background: #fff;
    color: #17150f;
  }
  .qr {
    width: 232px;
    height: 232px;
    display: block;
  }
  .skeleton {
    display: grid;
    place-items: center;
    color: #6b665c;
    border-radius: var(--r-m);
    background: linear-gradient(100deg, #f1efeb 40%, #fbfaf8 50%, #f1efeb 60%) 0 0 / 300% 100%;
    animation: sweep 1.6s linear infinite;
  }
  @keyframes sweep {
    to {
      background-position: -150% 0;
    }
  }
  .paircode {
    font-size: 30px;
    font-weight: 600;
    letter-spacing: 0.08em;
    user-select: text;
    -webkit-user-select: text;
  }
  .paircode span {
    color: #7a756a;
    margin: 0 2px;
  }
  form {
    display: grid;
    gap: 8px;
    width: 208px;
  }
  label {
    font-weight: 500;
  }
  input {
    height: 36px;
    padding: 0 12px;
    border-radius: var(--r-m);
    border: 1px solid #dedad3;
    background: #fff;
    color: #17150f;
    font-size: 15px;
  }
  input::placeholder {
    color: #7a756a;
  }
  small {
    color: #5d594f;
    font-size: 12px;
    margin-bottom: 6px;
  }
  form .btn.primary,
  .code > .btn.primary {
    background: #111110;
    color: #fbfaf8;
  }
  .err {
    margin: 0;
    max-width: 200px;
    text-align: center;
    color: #b3261e;
  }
  .how {
    width: 300px;
  }
  h1 {
    margin: 0 0 14px;
    font-size: 22px;
    font-weight: 600;
    letter-spacing: -0.01em;
    text-wrap: balance;
  }
  ol {
    margin: 0 0 18px;
    padding: 0;
    list-style: none;
    counter-reset: step;
    display: grid;
    gap: 10px;
    font-size: 15px;
    color: var(--text-2);
  }
  li {
    counter-increment: step;
    display: grid;
    grid-template-columns: 22px 1fr;
    gap: 10px;
  }
  li::before {
    content: counter(step);
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--ink);
    color: var(--on-ink);
    font-size: 12px;
    font-weight: 600;
  }
  .how p {
    margin: 0 0 18px;
    font-size: 15px;
    color: var(--text-2);
  }
  .swap {
    margin-left: -14px;
  }
  .notice {
    margin: 0;
    max-width: 62ch;
    text-align: center;
    font-size: 12px;
    color: var(--text-3);
    text-wrap: pretty;
  }
  @media (max-width: 760px) {
    .card {
      flex-direction: column;
      padding: 28px;
      gap: 24px;
    }
  }
</style>
