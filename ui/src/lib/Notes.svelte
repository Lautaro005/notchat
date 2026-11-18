<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { api, type Note, type NoteColor } from "./api";
  import { app, confirmAction, MAX_FILE, openMenu, refreshNotes, remember, toast, type MenuItem, type NotesView } from "./app.svelte";
  import { bytes, dayKey, dayLabel, stamp, t, time, type Key } from "./i18n";

  const COLORS: NoteColor[] = ["", "yellow", "pink", "blue", "green", "violet"];
  const VIEWS: { id: NotesView; icon: string; label: Key }[] = [
    { id: "chat", icon: "chat", label: "notes.viewChat" },
    { id: "board", icon: "board", label: "notes.viewBoard" },
    { id: "postits", icon: "grid", label: "notes.viewPostits" },
  ];

  let draft = $state("");
  let color = $state<NoteColor>("");
  let busy = $state(false);
  let editing = $state<number | null>(null);
  let editText = $state("");
  let fileInput: HTMLInputElement | undefined = $state();
  let scroller: HTMLElement | undefined = $state();
  let input: HTMLTextAreaElement | undefined = $state();
  let media = $state<Record<number, string>>({});
  let dragId = $state<number | null>(null);
  let dropCol = $state<string | null>(null);

  const ordered = $derived([...app.notes].sort((a, b) => a.ts - b.ts || a.id - b.id));
  const wall = $derived([...app.notes].sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.ts - a.ts));
  const columns = $derived([
    { id: "pinned", label: t("notes.pinned"), notes: wall.filter((n) => n.pinned && !n.done) },
    { id: "todo", label: t("notes.todo"), notes: wall.filter((n) => !n.pinned && !n.done) },
    { id: "done", label: t("notes.doneCol"), notes: wall.filter((n) => n.done) },
  ]);

  $effect(() => {
    void app.tail;
    if (app.notesView === "chat") void tick().then(() => scroller?.scrollTo({ top: scroller.scrollHeight }));
  });

  function setView(v: NotesView) {
    app.notesView = v;
    remember("notesView", v);
    if (v === "postits" && color === "") color = "yellow";
  }

  // Plain set so rendering never writes reactive state; the load result does.
  const loadingMedia = new Set<number>();
  function mediaFor(n: Note): string | undefined {
    if (n.kind !== "image" && n.kind !== "video") return undefined;
    if (!(n.id in media) && !loadingMedia.has(n.id)) {
      loadingMedia.add(n.id);
      api
        .noteMedia(n.id)
        .then((url) => (media[n.id] = url))
        .catch(() => loadingMedia.delete(n.id));
    }
    return media[n.id];
  }

  async function attempt(fn: () => Promise<unknown>, done?: string) {
    try {
      await fn();
      await refreshNotes();
      if (done) toast(done);
    } catch {
      toast(t("toast.failed"));
    }
  }

  async function add() {
    const text = draft.trim();
    if (!text || busy) return;
    busy = true;
    try {
      const note = await api.addNote(text, color);
      app.notes = [...app.notes, note];
      draft = "";
      app.tail++;
    } catch {
      toast(t("toast.failed"));
    } finally {
      busy = false;
      input?.focus();
    }
  }

  function readBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const r = new FileReader();
      r.onload = () => resolve(String(r.result).split(",", 2)[1] ?? "");
      r.onerror = () => reject(r.error);
      r.readAsDataURL(file);
    });
  }

  async function addFiles(list: FileList | File[] | null) {
    if (!list) return;
    for (const file of list) {
      if (file.size > MAX_FILE) {
        toast(t("chat.fileTooBig"));
        continue;
      }
      try {
        const note = await api.addNoteFile({
          text: draft.trim(),
          name: file.name,
          mime: file.type || "application/octet-stream",
          base64: await readBase64(file),
        });
        draft = "";
        app.notes = [...app.notes, note];
        app.tail++;
      } catch {
        toast(t("toast.failed"));
      }
    }
  }

  function save(n: Note, patch: Partial<Note>) {
    const next = { ...n, ...patch };
    const i = app.notes.findIndex((x) => x.id === n.id);
    if (i >= 0) app.notes[i] = next;
    api.updateNote($state.snapshot(next) as Note).catch(() => {
      toast(t("toast.failed"));
      void refreshNotes();
    });
  }

  function startEdit(n: Note) {
    editing = n.id;
    editText = n.text;
    void tick().then(() => document.querySelector<HTMLTextAreaElement>(".editbox")?.focus());
  }

  function finishEdit(n: Note, keep: boolean) {
    if (keep && editText.trim() !== n.text) save(n, { text: editText.trim() });
    editing = null;
  }

  function editKey(e: KeyboardEvent, n: Note) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      finishEdit(n, true);
    } else if (e.key === "Escape") {
      e.preventDefault();
      finishEdit(n, false);
    }
  }

  async function remove(n: Note) {
    const ok = await confirmAction({ title: t("notes.deleteTitle"), body: t("notes.deleteBody"), action: t("confirm.delete") });
    if (!ok) return;
    app.notes = app.notes.filter((x) => x.id !== n.id);
    await attempt(() => api.deleteNote(n.id));
  }

  function noteMenu(n: Note): MenuItem[] {
    const items: MenuItem[] = [];
    if (n.text) {
      items.push({
        label: t("msg.copy"),
        icon: "copy",
        action: () => void navigator.clipboard.writeText(n.text).then(() => toast(t("msg.copied"))),
      });
      items.push({ label: t("notes.edit"), icon: "pencil", action: () => startEdit(n) });
    }
    items.push(
      { label: t("msg.forward"), icon: "forward", action: () => (app.forward = { note: n.id }) },
      {
        label: t("notes.color"),
        icon: "palette",
        children: COLORS.map((c) => ({
          label: t(`color.${c || "none"}` as Key),
          checked: n.color === c,
          action: () => save(n, { color: c }),
        })),
      },
      n.pinned
        ? { label: t("notes.unpin"), icon: "pin", action: () => save(n, { pinned: false }) }
        : { label: t("notes.pin"), icon: "pin", action: () => save(n, { pinned: true, done: false }) },
      n.done
        ? { label: t("notes.undone"), icon: "check", action: () => save(n, { done: false }) }
        : { label: t("notes.done"), icon: "check", action: () => save(n, { done: true, pinned: false }) },
    );
    if (n.kind !== "text") {
      items.push({ label: t("menu.save"), icon: "download", action: () => void attempt(() => api.saveNoteFile(n.id), t("toast.saved")) });
    }
    items.push({ separator: true }, { label: t("notes.delete"), icon: "trash", danger: true, action: () => void remove(n) });
    return items;
  }

  function menuAt(e: MouseEvent, n: Note) {
    e.stopPropagation();
    if (e.type === "contextmenu") {
      openMenu(e, noteMenu(n));
      return;
    }
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openMenu({ x: r.right - 210, y: r.bottom + 4 }, noteMenu(n));
  }

  function dropOn(col: string) {
    const n = app.notes.find((x) => x.id === dragId);
    dragId = null;
    dropCol = null;
    if (!n) return;
    if (col === "pinned") save(n, { pinned: true, done: false });
    else if (col === "todo") save(n, { pinned: false, done: false });
    else save(n, { done: true, pinned: false });
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void add();
    }
  }

  function cycleColor() {
    color = COLORS[(COLORS.indexOf(color) + 1) % COLORS.length];
  }

  function ext(n: Note): string {
    return (/\.([a-z0-9]{1,5})$/i.exec(n.file_name)?.[1] ?? n.mime.split("/")[1] ?? "").toUpperCase().slice(0, 5);
  }
</script>

{#snippet body(n: Note)}
  {#if editing === n.id}
    <textarea
      class="editbox"
      bind:value={editText}
      onkeydown={(e) => editKey(e, n)}
      onblur={() => finishEdit(n, true)}
      onclick={(e) => e.stopPropagation()}
      aria-label={t("notes.edit")}
    ></textarea>
  {:else}
    {#if n.kind === "image" || n.kind === "video"}
      {@const src = mediaFor(n)}
      <span class="thumb">
        {#if src && n.kind === "image"}<img {src} alt="" />{:else if src}<video {src} muted playsinline></video>{:else}<span class="ph"></span>{/if}
      </span>
    {:else if n.kind !== "text"}
      <button class="file" onclick={(e) => { e.stopPropagation(); void attempt(() => api.saveNoteFile(n.id), t("toast.saved")); }} title={t("menu.save")}>
        <span class="fic"><Icon name="doc" size={18} /><b>{ext(n)}</b></span>
        <span class="fmeta"><span class="fname">{n.file_name || t("kind.document")}</span><small class="num">{bytes(n.size)}</small></span>
      </button>
    {/if}
    {#if n.text}<span class="ntext" class:done={n.done}>{n.text}</span>{/if}
  {/if}
{/snippet}

{#snippet marks(n: Note)}
  {#if n.pinned}<Icon name="pin" size={11} />{/if}
  {#if n.done}<Icon name="check" size={12} />{/if}
{/snippet}

<section class="notes" aria-label={t("notes.name")}>
  <header data-tauri-drag-region>
    <span class="mark"><Icon name="note" size={18} /></span>
    <span class="titles">
      <h1>{t("notes.name")}</h1>
      <span class="sub"><Icon name="shield" size={12} />{t("notes.sub")}</span>
    </span>
    <div class="views" role="tablist" aria-label={t("notes.view")}>
      {#each VIEWS as v (v.id)}
        <button role="tab" aria-selected={app.notesView === v.id} onclick={() => setView(v.id)} title={t(v.label)}>
          <Icon name={v.icon as never} size={15} /><span>{t(v.label)}</span>
        </button>
      {/each}
    </div>
  </header>

  {#if app.notes.length === 0}
    <div class="empty">
      <div class="stack" aria-hidden="true"><span class="p1"></span><span class="p2"></span><span class="p3"></span></div>
      <strong>{t("notes.emptyTitle")}</strong>
      <span>{t("notes.empty")}</span>
    </div>
  {:else if app.notesView === "chat"}
    <div class="log scroll" bind:this={scroller}>
      {#each ordered as n, i (n.id)}
        {@const prev = ordered[i - 1]}
        {#if !prev || dayKey(prev.ts) !== dayKey(n.ts)}
          <div class="day"><span>{dayLabel(n.ts)}</span></div>
        {/if}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <article class="nb" data-c={n.color || "plain"} oncontextmenu={(e) => menuAt(e, n)}>
          {@render body(n)}
          <span class="meta num">{@render marks(n)}{time(n.ts)}</span>
          <button class="chev" aria-label={t("msg.more")} onclick={(e) => menuAt(e, n)}><Icon name="chevron" size={14} /></button>
        </article>
      {/each}
    </div>
  {:else if app.notesView === "board"}
    <div class="board scroll">
      {#each columns as col (col.id)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="col"
          class:over={dropCol === col.id}
          ondragover={(e) => {
            e.preventDefault();
            dropCol = col.id;
          }}
          ondragleave={() => dropCol === col.id && (dropCol = null)}
          ondrop={(e) => {
            e.preventDefault();
            dropOn(col.id);
          }}
        >
          <h2>{col.label}<span class="num">{col.notes.length}</span></h2>
          <div class="cards">
            {#each col.notes as n (n.id)}
              <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
              <article
                class="card"
                data-c={n.color || "plain"}
                draggable={editing !== n.id}
                ondragstart={() => (dragId = n.id)}
                ondragend={() => {
                  dragId = null;
                  dropCol = null;
                }}
                ondblclick={() => n.text && startEdit(n)}
                oncontextmenu={(e) => menuAt(e, n)}
              >
                {@render body(n)}
                <footer>
                  <span class="num">{stamp(n.ts)}</span>
                  <button class="tick" class:on={n.done} onclick={() => save(n, { done: !n.done, pinned: false })} aria-label={n.done ? t("notes.undone") : t("notes.done")} title={n.done ? t("notes.undone") : t("notes.done")}>
                    <Icon name="check" size={12} />
                  </button>
                  <button class="chev2" aria-label={t("msg.more")} onclick={(e) => menuAt(e, n)}><Icon name="more" size={15} /></button>
                </footer>
              </article>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="wall scroll">
      {#each wall as n, i (n.id)}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_click_events_have_key_events -->
        <article
          class="postit"
          data-c={n.color || "plain"}
          style:--tilt="{((i * 37) % 5) - 2}deg"
          onclick={() => n.text && editing !== n.id && startEdit(n)}
          oncontextmenu={(e) => menuAt(e, n)}
        >
          {#if n.pinned}<span class="tack" aria-hidden="true"></span>{/if}
          {@render body(n)}
          <footer>
            <span class="num">{stamp(n.ts)}</span>
            <span class="swatches">
              {#each COLORS.slice(1) as c (c)}
                <button class="sw" data-c={c} class:on={n.color === c} onclick={(e) => { e.stopPropagation(); save(n, { color: n.color === c ? "" : c }); }} aria-label={t(`color.${c}` as Key)}></button>
              {/each}
            </span>
            <button class="chev2" aria-label={t("msg.more")} onclick={(e) => menuAt(e, n)}><Icon name="more" size={15} /></button>
          </footer>
        </article>
      {/each}
    </div>
  {/if}

  <footer class="composer-wrap">
    <div class="composer">
      <button class="icon-btn" onclick={() => fileInput?.click()} aria-label={t("notes.attach")} title={t("notes.attach")}>
        <Icon name="clip" size={18} />
      </button>
      <button class="dot" data-c={color || "plain"} onclick={cycleColor} aria-label="{t('notes.color')}: {t(`color.${color || 'none'}` as Key)}" title="{t('notes.color')}: {t(`color.${color || 'none'}` as Key)}"></button>
      <textarea
        bind:this={input}
        bind:value={draft}
        rows="1"
        placeholder={t("notes.placeholder")}
        aria-label={t("notes.placeholder")}
        onkeydown={onKey}
        onpaste={(e) => {
          const pasted = [...(e.clipboardData?.files ?? [])];
          if (pasted.length) {
            e.preventDefault();
            void addFiles(pasted);
          }
        }}
      ></textarea>
      <button class="sendbtn" onclick={add} disabled={!draft.trim() || busy} aria-label={t("notes.save")}>
        <Icon name="check" size={18} />
      </button>
    </div>
    <input
      bind:this={fileInput}
      type="file"
      multiple
      hidden
      onchange={(e) => {
        void addFiles(e.currentTarget.files);
        e.currentTarget.value = "";
      }}
    />
  </footer>
</section>

<style>
  .notes {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg);
    background-image: var(--wall);
  }
  [data-c="plain"] {
    --c: var(--note-plain);
  }
  [data-c="yellow"] {
    --c: var(--note-yellow);
  }
  [data-c="pink"] {
    --c: var(--note-pink);
  }
  [data-c="blue"] {
    --c: var(--note-blue);
  }
  [data-c="green"] {
    --c: var(--note-green);
  }
  [data-c="violet"] {
    --c: var(--note-violet);
  }
  header {
    flex: none;
    height: 58px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 12px 0 16px;
    border-bottom: 1px solid var(--line-soft);
    background: color-mix(in srgb, var(--bg) 86%, transparent);
    backdrop-filter: blur(18px) saturate(1.4);
  }
  .mark {
    flex: none;
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    border-radius: 12px;
    background: var(--note-yellow);
    color: var(--note-ink);
  }
  .titles {
    flex: 1;
    min-width: 0;
    display: grid;
    pointer-events: none;
  }
  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .sub {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .views {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 11px;
    background: var(--bg-2);
  }
  .views button {
    height: 28px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
    border-radius: 8px;
    font-size: 12.5px;
    font-weight: 500;
    color: var(--text-2);
    transition: background 140ms var(--ease), color 140ms var(--ease);
  }
  .views button:hover {
    color: var(--text);
  }
  .views button[aria-selected="true"] {
    background: var(--panel);
    color: var(--text);
    box-shadow: var(--shadow-s);
  }
  @media (max-width: 980px) {
    .views span {
      display: none;
    }
  }
  .empty {
    flex: 1;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 6px;
    padding: 24px;
    text-align: center;
    color: var(--text-2);
    text-wrap: pretty;
  }
  .empty strong {
    font-family: var(--rounded);
    font-size: 17px;
    color: var(--text);
  }
  .empty span {
    max-width: 42ch;
  }
  .stack {
    position: relative;
    width: 96px;
    height: 80px;
    margin-bottom: 16px;
  }
  .stack span {
    position: absolute;
    width: 58px;
    height: 58px;
    border-radius: 6px;
    box-shadow: var(--shadow-s);
  }
  .stack .p1 {
    left: 0;
    top: 14px;
    background: var(--note-blue);
    transform: rotate(-8deg);
  }
  .stack .p2 {
    right: 0;
    top: 10px;
    background: var(--note-pink);
    transform: rotate(7deg);
  }
  .stack .p3 {
    left: 19px;
    top: 0;
    background: var(--note-yellow);
    transform: rotate(-1deg);
  }

  /* ---- chat view ---- */
  .log {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
    padding: 12px clamp(14px, 5%, 64px);
  }
  .log > :first-child {
    margin-top: auto;
  }
  .day {
    align-self: center;
    margin: 12px 0 4px;
  }
  .day span {
    display: block;
    padding: 4px 12px;
    border-radius: 999px;
    background: var(--panel);
    box-shadow: var(--shadow-s), 0 0 0 0.5px var(--line-soft);
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }
  .nb {
    position: relative;
    max-width: min(72%, 560px);
    padding: 7px 12px;
    border-radius: 18px 18px 6px 18px;
    background: var(--c);
    color: var(--text);
    box-shadow: 0 1px 1.5px rgb(20 10 60 / 0.05), 0 0 0 1px var(--bubble-line);
    font-size: 14px;
    line-height: 1.4;
    user-select: text;
    -webkit-user-select: text;
  }
  .nb:not([data-c="plain"]) {
    color: var(--note-ink);
    box-shadow: 0 1px 2px rgb(20 10 60 / 0.08);
  }
  .meta {
    float: right;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    margin: 7px 0 -3px 12px;
    font-size: 10.5px;
    opacity: 0.62;
    user-select: none;
    -webkit-user-select: none;
  }
  .chev {
    position: absolute;
    top: 4px;
    right: 4px;
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    transform: rotate(90deg);
    background: radial-gradient(circle, var(--c) 55%, transparent 72%);
    opacity: 0;
    transition: opacity 120ms var(--ease);
  }
  .nb:hover .chev,
  .chev:focus-visible {
    opacity: 1;
  }
  .ntext {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .ntext.done {
    text-decoration: line-through;
    text-decoration-thickness: 1px;
    opacity: 0.6;
  }
  .thumb {
    display: block;
    margin: -3px -8px 6px;
    border-radius: 14px;
    overflow: hidden;
    background: var(--bg-3);
  }
  .thumb img,
  .thumb video {
    display: block;
    max-width: 100%;
    max-height: 320px;
    object-fit: cover;
  }
  .thumb .ph {
    display: block;
    width: 220px;
    height: 140px;
  }
  .file {
    width: 250px;
    max-width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 2px 0 4px;
    padding: 7px 9px 7px 7px;
    border-radius: 12px;
    background: color-mix(in srgb, currentColor 8%, transparent);
    text-align: left;
    color: inherit;
  }
  .file:hover {
    background: color-mix(in srgb, currentColor 13%, transparent);
  }
  .fic {
    position: relative;
    flex: none;
    width: 34px;
    height: 40px;
    display: grid;
    place-items: center;
    border-radius: 7px;
    background: var(--accent);
    color: var(--on-accent);
  }
  .fic b {
    position: absolute;
    bottom: 3px;
    font-size: 7.5px;
  }
  .fic :global(svg) {
    margin-top: -7px;
  }
  .fmeta {
    min-width: 0;
    display: grid;
  }
  .fname {
    font-weight: 600;
    font-size: 12.5px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .fmeta small {
    font-size: 11px;
    opacity: 0.7;
  }
  .editbox {
    width: 100%;
    min-width: 220px;
    min-height: 60px;
    padding: 6px 8px;
    border: 0;
    border-radius: 10px;
    background: color-mix(in srgb, var(--panel) 70%, transparent);
    color: inherit;
    box-shadow: inset 0 0 0 1.5px var(--accent);
    resize: vertical;
    outline: none;
    field-sizing: content;
    user-select: text;
    -webkit-user-select: text;
  }

  /* ---- board view ---- */
  .board {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: repeat(3, minmax(210px, 1fr));
    gap: 14px;
    padding: 16px;
    overflow-x: auto;
    align-items: start;
  }
  .col {
    min-height: 120px;
    padding: 10px;
    border-radius: 16px;
    background: color-mix(in srgb, var(--bg-2) 80%, transparent);
    box-shadow: inset 0 0 0 1px var(--line-soft);
    transition: box-shadow 140ms var(--ease), background 140ms var(--ease);
  }
  .col.over {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .col h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 2px 4px 10px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-2);
  }
  .col h2 span {
    padding: 1px 7px;
    border-radius: 8px;
    background: var(--bg-3);
    font-size: 11px;
  }
  .cards {
    display: grid;
    gap: 8px;
  }
  .card {
    padding: 10px 12px 6px;
    border-radius: 12px;
    background: var(--c);
    color: var(--text);
    box-shadow: var(--shadow-s), 0 0 0 1px var(--line-soft);
    font-size: 13px;
    line-height: 1.4;
    cursor: grab;
    transition: transform 160ms var(--ease), box-shadow 160ms var(--ease);
  }
  .card:not([data-c="plain"]),
  .postit:not([data-c="plain"]) {
    color: var(--note-ink);
  }
  .card:hover {
    transform: translateY(-1px);
    box-shadow: var(--shadow), 0 0 0 1px var(--line-soft);
  }
  .card:active {
    cursor: grabbing;
  }
  .card footer,
  .postit footer {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-top: 8px;
    font-size: 11px;
    opacity: 0.75;
  }
  .card footer > span:first-child,
  .postit footer > span:first-child {
    flex: 1;
  }
  .tick {
    width: 20px;
    height: 20px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: transparent;
    box-shadow: inset 0 0 0 1.5px currentColor;
  }
  .card .tick {
    color: inherit;
  }
  .tick :global(svg) {
    opacity: 0;
  }
  .tick.on {
    background: var(--accent);
    color: var(--on-accent);
    box-shadow: none;
  }
  .tick.on :global(svg) {
    opacity: 1;
  }
  .chev2 {
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    border-radius: 7px;
    color: inherit;
  }
  .chev2:hover {
    background: color-mix(in srgb, currentColor 10%, transparent);
  }

  /* ---- sticky notes view ---- */
  .wall {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    grid-auto-rows: minmax(190px, auto);
    gap: 22px;
    padding: 24px;
    align-content: start;
  }
  .postit {
    position: relative;
    display: flex;
    flex-direction: column;
    padding: 16px 16px 10px;
    border-radius: 4px 4px 14px 4px;
    background: var(--c);
    color: var(--text);
    box-shadow: 0 1px 1px rgb(0 0 0 / 0.06), 0 14px 22px -14px rgb(30 20 60 / 0.4);
    font-family: var(--rounded);
    font-size: 14.5px;
    line-height: 1.4;
    transform: rotate(var(--tilt));
    transition: transform 220ms var(--ease), box-shadow 220ms var(--ease);
    cursor: text;
  }
  .postit[data-c="plain"] {
    box-shadow: 0 0 0 1px var(--line-soft), 0 14px 22px -14px rgb(30 20 60 / 0.3);
  }
  .postit:hover {
    transform: rotate(0deg) translateY(-3px);
    box-shadow: 0 1px 1px rgb(0 0 0 / 0.06), 0 22px 30px -16px rgb(30 20 60 / 0.45);
  }
  .postit .ntext {
    flex: 1;
    display: -webkit-box;
    -webkit-line-clamp: 9;
    line-clamp: 9;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .postit .thumb {
    margin: -6px -6px 8px;
    border-radius: 6px;
  }
  .tack {
    position: absolute;
    top: -6px;
    left: 50%;
    width: 14px;
    height: 14px;
    margin-left: -7px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 2px 3px rgb(0 0 0 / 0.3), inset -2px -2px 0 rgb(0 0 0 / 0.15);
  }
  .swatches {
    display: flex;
    gap: 3px;
    opacity: 0;
    transition: opacity 140ms var(--ease);
  }
  .postit:hover .swatches,
  .swatches:focus-within {
    opacity: 1;
  }
  .sw {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--c);
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.18);
  }
  .sw.on {
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.18), 0 0 0 2px var(--accent);
  }

  /* ---- composer ---- */
  .composer-wrap {
    flex: none;
    padding: 8px 16px 14px;
  }
  .composer {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    padding: 5px 5px 5px 6px;
    border-radius: 22px;
    background: var(--panel);
    box-shadow: var(--shadow-s), 0 0 0 1px var(--line-soft);
  }
  .composer:focus-within {
    box-shadow: var(--shadow-s), 0 0 0 1px var(--text-3);
  }
  .composer .icon-btn {
    border-radius: 50%;
    flex: none;
  }
  .dot {
    flex: none;
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border-radius: 50%;
  }
  .dot::after {
    content: "";
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--c);
    box-shadow: inset 0 0 0 1.5px var(--line);
  }
  .dot:hover {
    background: var(--bg-3);
  }
  .composer textarea {
    flex: 1;
    min-width: 0;
    max-height: 160px;
    padding: 6px;
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
  .composer textarea::placeholder {
    color: var(--text-3);
  }
  .sendbtn {
    flex: none;
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--accent);
    color: var(--on-accent);
    transition: opacity 160ms var(--ease), transform 160ms var(--ease);
  }
  .sendbtn:disabled {
    opacity: 0.35;
  }
</style>
