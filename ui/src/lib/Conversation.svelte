<script lang="ts">
  import { tick } from "svelte";
  import Avatar from "./Avatar.svelte";
  import ChatInfo from "./ChatInfo.svelte";
  import Composer from "./Composer.svelte";
  import Icon from "./Icon.svelte";
  import MediaView from "./MediaView.svelte";
  import ReactBar from "./ReactBar.svelte";
  import ReactionList from "./ReactionList.svelte";
  import VoicePlayer from "./VoicePlayer.svelte";
  import { api } from "./api";
  import { app, chatTitle, fetchOlderFromPhone, isMuted, loadOlder, openMenu, toast } from "./app.svelte";
  import { chatMenu } from "./chatMenu";
  import {
    copyText,
    forward,
    messageMenu,
    myReaction,
    react,
    remove,
    star,
    startReply,
    toNotes,
    togglePick,
  } from "./messageMenu";
  import { bytes, dayKey, dayLabel, t, tf, time, type Key } from "./i18n";
  import type { Message, Person } from "./api";
  import { linkParts } from "./md";

  let scroller: HTMLElement | undefined = $state();
  let composer: Composer | undefined = $state();
  let files = $state<File[]>([]);
  let dragging = $state(0);
  let reacting = $state<{ m: Message; x: number; y: number } | null>(null);
  /** The "who reacted" list, anchored to the pill that opened it. */
  let reactors = $state<{ id: string; x: number; top: number; bottom: number } | null>(null);
  const reactorsMsg = $derived(reactors ? (app.messages.find((m) => m.id === reactors!.id) ?? null) : null);
  /** Polls whose voters are listed under each option. */
  let openVotes = $state<string[]>([]);
  /** Polls with a vote on its way, so a double click doesn't send two. */
  let voting = $state<string[]>([]);
  let pinIndex = $state(0);
  let flash = $state<string | null>(null);

  const chat = $derived(app.chats.find((c) => c.jid === app.selected) ?? null);
  const title = $derived(chat ? chatTitle(chat) : app.selected ? chatTitle({ jid: app.selected, name: "" }) : "");
  const isGroup = $derived(chat?.is_group ?? app.selected?.endsWith("@g.us") ?? false);
  const picked = $derived(app.messages.filter((m) => app.picked.includes(m.id)));
  const pin = $derived(app.pinned.length ? app.pinned[pinIndex % app.pinned.length] : null);
  /** The user's lists ("tags") this chat belongs to. */
  const tags = $derived(app.selected ? app.lists.filter((l) => l.jids.includes(app.selected!)) : []);

  const visual = new Set(["image", "video", "sticker"]);

  /** Messages grouped by day: each day's label sticks only while its own
   *  messages are on screen, so labels never pile up at the top. */
  const days = $derived.by(() => {
    const out: { key: number; ts: number; items: { m: Message; i: number }[] }[] = [];
    app.messages.forEach((m, i) => {
      const key = dayKey(m.ts);
      const last = out[out.length - 1];
      if (last && last.key === key) last.items.push({ m, i });
      else out.push({ key, ts: m.ts, items: [{ m, i }] });
    });
    return out;
  });

  // Follow the tail when a message is appended or a chat is opened.
  $effect(() => {
    void app.tail;
    void tick().then(() => scroller?.scrollTo({ top: scroller.scrollHeight }));
  });

  /** The first unread message: the count-th incoming message from the end. */
  const firstUnread = $derived.by(() => {
    const mark = app.unreadMark;
    if (!mark || mark.jid !== app.selected) return null;
    let seen = 0;
    for (let i = app.messages.length - 1; i >= 0; i--) {
      if (!app.messages[i].from_me && ++seen === mark.count) return app.messages[i].id;
    }
    return app.messages.find((m) => !m.from_me)?.id ?? null;
  });

  // Only scrolling done by the user clears the divider: not our own jump,
  // nor images that finish loading and move things around.
  let userScrolled = false;
  function userScroll() {
    userScrolled = true;
    requestAnimationFrame(clearAtEnd);
  }

  /** Reading down to the end clears the unread divider. */
  function clearAtEnd() {
    if (
      scroller &&
      app.unreadMark &&
      userScrolled &&
      scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 8
    ) {
      app.unreadMark = null;
    }
  }

  // Opening a chat with unread messages lands on the divider, not the end.
  $effect(() => {
    if (!app.jumpUnread) return;
    userScrolled = false;
    void tick().then(() => {
      const el = document.getElementById("unread-divider");
      if (!el || !scroller) return scroller?.scrollTo({ top: scroller.scrollHeight });
      scroller.scrollTop = el.offsetTop - scroller.clientHeight / 3;
    });
  });

  $effect(() => {
    void app.selected;
    pinIndex = 0;
    reacting = null;
    reactors = null;
    openVotes = [];
  });

  async function older() {
    if (!scroller) return;
    const before = scroller.scrollHeight;
    await loadOlder();
    await tick();
    scroller.scrollTop += scroller.scrollHeight - before;
  }

  function onScroll() {
    clearAtEnd();
    if (!scroller || scroller.scrollTop >= 120 || app.loadingOlder) return;
    if (app.hasOlder) void older();
    else if (app.messages.length > 0) void fetchOlderFromPhone();
  }

  function nameOf(jid: string, fallback = ""): string {
    const known = app.chats.find((c) => c.jid === jid);
    if (known) return chatTitle(known);
    const seen = app.messages.find((m) => m.sender_jid === jid && m.sender_name);
    return seen?.sender_name || fallback || chatTitle({ jid, name: "" });
  }

  function sender(m: Message): string {
    return m.sender_name || nameOf(m.sender_jid);
  }

  /** Who wrote the quoted message: "You", the contact, or a group member. */
  function quoteName(m: Message): string {
    const original = app.messages.find((x) => x.id === m.quote_id);
    if (original) return original.from_me ? t("chat.you") : sender(original);
    if (m.quote_sender === "me") return t("chat.you");
    if (!isGroup) return title;
    return m.quote_name || (m.quote_sender ? nameOf(m.quote_sender) : "");
  }

  type Part = { text: string } | { p: Person } | { url: string; label: string };

  /** The text cut into plain runs, links and `@mentions` that know who they are. */
  function parts(m: Message): Part[] {
    return mentionParts(m).flatMap((part): Part[] => ("text" in part ? linkParts(part.text) : [part]));
  }

  function mentionParts(m: Message): ({ text: string } | { p: Person })[] {
    const known = new Map((m.mentions ?? []).map((p) => [p.user, p]));
    if (!known.size) return [{ text: m.text }];
    const out: ({ text: string } | { p: Person })[] = [];
    let last = 0;
    for (const match of m.text.matchAll(/@(\d{5,})/g)) {
      const p = known.get(match[1]);
      if (!p || match.index === undefined) continue;
      if (match.index > last) out.push({ text: m.text.slice(last, match.index) });
      out.push({ p });
      last = match.index + match[0].length;
    }
    out.push({ text: m.text.slice(last) });
    return out;
  }

  function showPerson(e: MouseEvent, p: Person) {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    app.person = { p, x: r.left, y: r.bottom + 6 };
  }

  function senderPerson(m: Message): Person {
    const name = sender(m);
    return {
      jid: m.sender_jid,
      user: m.sender_jid.split("@")[0],
      name,
      phone: m.sender_phone ?? "",
      saved: !!name && !name.startsWith("~") && !name.startsWith("+"),
    };
  }

  function quotePreview(m: Message): string {
    if (m.quote_text) return m.quote_text;
    return m.quote_kind && m.quote_kind !== "text" ? t(`kind.${m.quote_kind}` as Key) : "";
  }

  // A stable, low-chroma colour per group member so names are easy to scan.
  function hue(jid: string): number {
    return [...jid].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 11);
  }

  /** Reactions grouped by emoji, mine first. */
  function grouped(m: Message): { emoji: string; count: number; mine: boolean }[] {
    const out = new Map<string, { emoji: string; count: number; mine: boolean }>();
    for (const r of m.reactions) {
      const g = out.get(r.emoji) ?? { emoji: r.emoji, count: 0, mine: false };
      g.count++;
      g.mine ||= r.from_me;
      out.set(r.emoji, g);
    }
    return [...out.values()].sort((a, b) => Number(b.mine) - Number(a.mine) || b.count - a.count);
  }

  function sameRun(a: Message | undefined, b: Message | undefined): boolean {
    return (
      !!a &&
      !!b &&
      dayKey(a.ts) === dayKey(b.ts) &&
      a.from_me === b.from_me &&
      a.sender_jid === b.sender_jid &&
      Math.abs(b.ts - a.ts) < 300
    );
  }

  function showReactors(m: Message, anchor: HTMLElement) {
    const r = anchor.getBoundingClientRect();
    reactors = { id: m.id, x: r.left + r.width / 2, top: r.top, bottom: r.bottom };
  }

  function votes(o: { voters: Person[]; mine: boolean }): number {
    return o.voters.length + Number(o.mine);
  }

  /** Picks or unpicks an option: one-choice polls move the vote, the rest add to it. */
  async function vote(m: Message, name: string) {
    if (!m.poll || m.deleted || voting.includes(m.id)) return;
    const mine = m.poll.options.filter((o) => o.mine).map((o) => o.name);
    let next: string[];
    if (mine.includes(name)) next = mine.filter((n) => n !== name);
    else if (m.poll.selectable === 1) next = [name];
    else if (m.poll.selectable > 1 && mine.length >= m.poll.selectable) return;
    else next = [...mine, name];
    voting = [...voting, m.id];
    try {
      await api.votePoll(m.chat_jid, m.id, next);
    } catch (e) {
      toast(typeof e === "string" ? e : t("poll.failed"));
    } finally {
      voting = voting.filter((id) => id !== m.id);
    }
  }

  function toggleVotes(id: string) {
    openVotes = openVotes.includes(id) ? openVotes.filter((x) => x !== id) : [...openVotes, id];
  }

  function voterName(p: Person): string {
    return p.name || p.phone || p.user;
  }

  function openReact(m: Message, anchor: HTMLElement) {
    const r = anchor.getBoundingClientRect();
    reacting = { m, x: r.left + r.width / 2, y: r.top };
  }

  function menuFor(e: MouseEvent, m: Message) {
    if (app.selecting) {
      e.preventDefault();
      return;
    }
    const target = e.currentTarget as HTMLElement;
    const bubble = target.closest(".msg")?.querySelector<HTMLElement>(".bubble") ?? target;
    const r = target.getBoundingClientRect();
    const items = messageMenu(m, () => openReact(m, bubble));
    if (e.type === "contextmenu") openMenu(e, items);
    else openMenu({ x: m.from_me ? r.right - 210 : r.left, y: r.bottom + 4 }, items);
  }

  async function jumpTo(id: string) {
    const el = document.getElementById(`msg-${id}`);
    if (!el) return;
    el.scrollIntoView({ block: "center", behavior: "smooth" });
    flash = id;
    setTimeout(() => {
      if (flash === id) flash = null;
    }, 1400);
  }

  function nextPin() {
    if (!pin) return;
    void jumpTo(pin.id);
    pinIndex = (pinIndex + 1) % app.pinned.length;
  }

  async function saveDoc(m: Message) {
    try {
      await api.saveMedia(m.chat_jid, m.id);
    } catch {
      /* the menu offers saving again */
    }
  }

  function extOf(m: Message): string {
    const fromName = /\.([a-z0-9]{1,5})$/i.exec(m.file_name)?.[1];
    if (fromName) return fromName.toUpperCase();
    return (m.mime.split("/")[1] ?? "").split(/[;+.-]/)[0].toUpperCase().slice(0, 5);
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    dragging = 0;
    const dt = e.dataTransfer;
    if (!dt) return;
    // Folders can't be sent: keep only real files.
    const items = [...dt.items].filter((it) => it.kind === "file");
    const dropped = items.length
      ? items.filter((it) => !it.webkitGetAsEntry?.()?.isDirectory).map((it) => it.getAsFile()).filter((f): f is File => !!f)
      : [...dt.files];
    if (dropped.length) composer?.add(dropped);
  }

  function headerMore(e: MouseEvent) {
    if (!chat) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openMenu({ x: r.right - 220, y: r.bottom + 6 }, chatMenu(chat, { openInfo: () => (app.infoOpen = true) }));
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== "Escape" || e.defaultPrevented) return;
    if (app.lightbox) {
      e.preventDefault();
      app.lightbox = null;
    }
  }
</script>

<svelte:window onkeydown={onKey} />

{#if !app.selected}
  <section class="blank" data-tauri-drag-region>
    <div class="mark" aria-hidden="true">
      <span class="b1"></span><span class="b2"></span>
    </div>
    <strong>{t("chat.pick")}</strong>
    <span>{t("chat.pickHint")}</span>
  </section>
{:else}
  <div class="pane">
    <section
      class="conv"
      aria-label={title}
      ondragenter={(e) => {
        if (e.dataTransfer?.types.includes("Files")) dragging++;
      }}
      ondragleave={() => (dragging = Math.max(0, dragging - 1))}
      ondragover={(e) => e.preventDefault()}
      ondrop={onDrop}
    >
      <header data-tauri-drag-region>
        <button class="who-btn" onclick={() => (app.infoOpen = !app.infoOpen)} disabled={!chat} title={t("chat.tapInfo")}>
          <Avatar {title} jid={app.selected} group={isGroup} size={36} />
          <span class="titles">
            <h1>{title}</h1>
            <span class="subtitle">
              {#if chat && isMuted(chat)}<Icon name="mute" size={12} />{/if}
              {isGroup ? t("chat.group") : t("chat.tapInfo")}
            </span>
          </span>
        </button>
        {#if tags.length}
          <div class="tags" aria-label={t("chat.tags")}>
            {#each tags as l (l.id)}
              <button class="tag" onclick={() => (app.filter = `list:${l.id}`)} title={t("chat.showList")}>
                <Icon name="list" size={12} />{l.name}
              </button>
            {/each}
          </div>
        {/if}
        {#if chat && (isGroup ? app.auto.groups : app.auto.chats) === "selected"}
          <button
            class="autobtn"
            class:on={chat.auto_reply}
            aria-pressed={chat.auto_reply}
            title={app.auto.enabled ? t("chat.auto") : t("chat.autoOff")}
            onclick={() => api.setChatAutoReply(chat.jid, !chat.auto_reply)}
          >
            <Icon name="spark" size={14} />Auto
          </button>
        {/if}
        {#if chat}
          <button class="icon-btn" class:on={app.infoOpen} onclick={() => (app.infoOpen = !app.infoOpen)} aria-label={t("chat.info")} title={t("chat.info")}>
            <Icon name="info" size={18} />
          </button>
          <button class="icon-btn" onclick={headerMore} aria-label={t("info.more")} title={t("info.more")}>
            <Icon name="more" size={18} />
          </button>
        {/if}
      </header>

      {#if pin}
        <button class="pinbar" onclick={nextPin} title={t("msg.jump")}>
          <span class="pinic"><Icon name="pin" size={15} /></span>
          <span class="pinbody">
            <b>{app.pinned.length > 1 ? tf("msg.pinnedCount", { i: (pinIndex % app.pinned.length) + 1, n: app.pinned.length }) : t("msg.pinnedBar")}</b>
            <span>{pin.text || t(`kind.${pin.kind}` as Key)}</span>
          </span>
        </button>
      {/if}

      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div
        class="log scroll"
        role="log"
        aria-label={title}
        class:selecting={app.selecting}
        bind:this={scroller}
        onscroll={onScroll}
        onwheel={userScroll}
        ontouchmove={userScroll}
        onkeydown={userScroll}
      >
        {#if app.fetchingOlder}
          <p class="older phone" role="status"><span class="spin" aria-hidden="true"></span>{t("chat.olderPhone")}</p>
        {:else if app.hasOlder}
          <button class="btn quiet older" onclick={older} disabled={app.loadingOlder}>{t("chat.older")}</button>
        {/if}
        {#each days as day (day.key)}
        <div class="dayblock">
        <div class="day"><span>{dayLabel(day.ts)}</span></div>
        {#each day.items as { m, i } (m.id)}
          {@const prev = app.messages[i - 1]}
          {@const next = app.messages[i + 1]}
          {#if m.id === firstUnread && app.unreadMark}
            <div class="unread-divider" id="unread-divider" role="status">
              <span>{app.unreadMark.count === 1 ? t("chat.unreadOne") : tf("chat.unreadMany", { n: app.unreadMark.count })}</span>
            </div>
          {/if}
          {@const first = !sameRun(prev, m)}
          {@const last = !sameRun(m, next)}
          {@const reactions = grouped(m)}
          {@const isPicked = app.picked.includes(m.id)}
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_click_events_have_key_events -->
          <article
            id="msg-{m.id}"
            class="msg"
            class:me={m.from_me}
            class:first
            class:last
            class:fresh={m.id === app.fresh}
            class:flash={flash === m.id}
            class:picked={isPicked}
            class:reacted={reactions.length > 0}
            oncontextmenu={(e) => menuFor(e, m)}
            onclick={() => app.selecting && togglePick(m.id)}
          >
            {#if app.selecting}
              <span class="tick" class:on={isPicked} aria-hidden="true"><Icon name="check" size={13} /></span>
            {/if}
            <div class="lane">
              <div
                class="bubble"
                class:visual={visual.has(m.kind) && !m.text && !m.quote_id && !m.deleted}
                class:sticker={m.kind === "sticker" && !m.deleted}
                class:gone={m.deleted}
              >
                {#if isGroup && !m.from_me && first}
                  <button class="who" style:--h={hue(m.sender_jid)} onclick={(e) => showPerson(e, senderPerson(m))}>{sender(m)}</button>
                {/if}
                {#if m.deleted}
                  <span class="deleted"><Icon name="close" size={13} />{t("msg.deleted")}</span>
                {:else}
                  {#if m.forwarded}
                    <span class="fwd"><Icon name="forward" size={13} />{t("msg.forwarded")}</span>
                  {/if}
                  {#if m.quote_id}
                    <button class="quote" onclick={(e) => { e.stopPropagation(); void jumpTo(m.quote_id); }} title={t("msg.jump")}>
                      <b>{quoteName(m)}</b>
                      <span>{quotePreview(m)}</span>
                    </button>
                  {/if}
                  {#if visual.has(m.kind)}
                    <MediaView {m} onopen={(src) => (app.lightbox = src)} />
                  {:else if m.kind === "voice" || m.kind === "audio"}
                    <VoicePlayer {m} />
                  {:else if m.kind === "document"}
                    <button class="doc" onclick={() => saveDoc(m)} disabled={!m.has_media} title={t("menu.save")}>
                      <span class="doc-ic"><Icon name="doc" size={20} /><b>{extOf(m)}</b></span>
                      <span class="doc-meta">
                        <span class="doc-name">{m.file_name || t("kind.document")}</span>
                        <small class="num">{[m.size ? bytes(m.size) : "", extOf(m)].filter(Boolean).join(" · ")}</small>
                      </span>
                      {#if m.has_media}<Icon name="download" size={17} />{/if}
                    </button>
                  {:else if m.kind === "viewonce"}
                    <span class="viewonce">
                      <span class="vo-ic"><Icon name={m.mime.startsWith("video") ? "camera" : "image"} size={17} /><b>1</b></span>
                      <span class="vo-meta">
                        <strong>{m.mime.startsWith("video") ? t("msg.viewOnceVideo") : t("msg.viewOncePhoto")}</strong>
                        <small>{t("msg.viewOnceHint")}</small>
                      </span>
                    </span>
                  {:else if m.kind === "poll" && m.poll}
                    {@const poll = m.poll}
                    {@const total = poll.options.reduce((n, o) => n + votes(o), 0)}
                    {@const top = Math.max(1, ...poll.options.map(votes))}
                    {@const showVoters = openVotes.includes(m.id)}
                    <div class="poll" class:busy={voting.includes(m.id)}>
                      <span class="poll-head"><Icon name="poll" size={14} />{t("kind.poll")}</span>
                      <strong class="poll-q">{m.text}</strong>
                      <small class="poll-hint">{poll.selectable === 1 ? t("poll.one") : t("poll.many")}</small>
                      <ul class="poll-opts">
                        {#each poll.options as o (o.name)}
                          {@const n = votes(o)}
                          <li>
                            <button
                              class="poll-opt"
                              class:mine={o.mine}
                              aria-pressed={o.mine}
                              disabled={app.selecting}
                              onclick={(e) => { e.stopPropagation(); void vote(m, o.name); }}
                            >
                              <span class="pick" class:round={poll.selectable === 1} aria-hidden="true">{#if o.mine}<Icon name="check" size={11} />{/if}</span>
                              <span class="poll-line">
                                <span class="poll-name">{o.name}</span>
                                <span class="poll-bar" aria-hidden="true"><i style:width="{(n / top) * 100}%"></i></span>
                              </span>
                              <span class="poll-n num">{n}</span>
                            </button>
                            {#if showVoters && n > 0}
                              <span class="voters">{[...(o.mine ? [t("chat.you")] : []), ...o.voters.map(voterName)].join(", ")}</span>
                            {/if}
                          </li>
                        {/each}
                      </ul>
                      <button class="poll-more" disabled={total === 0} onclick={(e) => { e.stopPropagation(); toggleVotes(m.id); }}>
                        {total === 0 ? t("poll.noVotes") : showVoters ? t("poll.hideVotes") : tf(total === 1 ? "poll.viewVote" : "poll.viewVotes", { n: total })}
                      </button>
                    </div>
                  {:else if m.kind !== "text"}
                    <span class="kind">{t(`kind.${m.kind}` as Key)}</span>
                  {/if}
                  {#if m.text && m.kind !== "poll"}<span class="text" class:aftermedia={visual.has(m.kind)}>{#each parts(m) as part, j (j)}{#if "p" in part}<button class="mention" onclick={(e) => showPerson(e, part.p)}>@{part.p.name || part.p.phone || part.p.user}</button>{:else if "url" in part}<a class="link" href={part.url} rel="noreferrer" onclick={(e) => e.stopPropagation()}>{part.label}</a>{:else}{part.text}{/if}{/each}</span>{/if}
                {/if}
                <span class="meta num">
                  {#if m.starred}<Icon name="star" size={11} />{/if}
                  {#if m.pinned > 0}<Icon name="pin" size={11} />{/if}
                  {#if m.edited && !m.deleted}<span>{t("msg.edited")}</span>{/if}
                  {#if m.by_ai}<span class="ai"><Icon name="spark" size={11} /></span>{/if}
                  {time(m.ts)}
                </span>
                {#if !app.selecting}
                  <button class="chev" aria-label={t("msg.more")} onclick={(e) => { e.stopPropagation(); menuFor(e, m); }}>
                    <Icon name="chevron" size={14} />
                  </button>
                {/if}
              </div>
              {#if reactions.length}
                <div class="reactions">
                  {#each reactions as r (r.emoji)}
                    <button
                      class="rx"
                      class:mine={r.mine}
                      aria-haspopup="dialog"
                      title={t("rx.title")}
                      onclick={(e) => { e.stopPropagation(); showReactors(m, e.currentTarget); }}
                    >
                      <span>{r.emoji}</span>{#if r.count > 1}<small class="num">{r.count}</small>{/if}
                    </button>
                  {/each}
                </div>
              {/if}
            </div>
            {#if !app.selecting && !m.deleted}
              <div class="hover-acts">
                <button aria-label={t("msg.react")} title={t("msg.react")} onclick={(e) => { e.stopPropagation(); openReact(m, e.currentTarget); }}>
                  <Icon name="smile" size={16} />
                </button>
                <button aria-label={t("msg.reply")} title={t("msg.reply")} onclick={(e) => { e.stopPropagation(); startReply(m); }}>
                  <Icon name="reply" size={16} />
                </button>
              </div>
            {/if}
          </article>
        {/each}
        </div>
        {/each}
      </div>

      {#if app.selecting}
        <div class="selbar" role="toolbar" aria-label={tf("msg.selected", { n: picked.length })}>
          <button class="icon-btn" onclick={() => { app.selecting = false; app.picked = []; }} aria-label={t("msg.cancelSelect")} title={t("msg.cancelSelect")}>
            <Icon name="close" size={18} />
          </button>
          <span class="count">{tf("msg.selected", { n: picked.length })}</span>
          <button class="icon-btn" disabled={!picked.some((m) => m.text)} onclick={() => copyText(picked)} aria-label={t("msg.copy")} title={t("msg.copy")}>
            <Icon name="copy" size={18} />
          </button>
          <button class="icon-btn" disabled={!picked.length} onclick={() => star(picked, !picked.every((m) => m.starred))} aria-label={t("msg.star")} title={t("msg.star")}>
            <Icon name="star" size={18} />
          </button>
          <button class="icon-btn" disabled={!picked.length} onclick={() => toNotes(picked)} aria-label={t("msg.toNotes")} title={t("msg.toNotes")}>
            <Icon name="note" size={18} />
          </button>
          <button class="icon-btn" disabled={!picked.length || picked.some((m) => m.deleted)} onclick={() => forward(picked)} aria-label={t("msg.forward")} title={t("msg.forward")}>
            <Icon name="forward" size={18} />
          </button>
          <button class="icon-btn danger" disabled={!picked.length} onclick={() => remove(picked)} aria-label={t("msg.delete")} title={t("msg.delete")}>
            <Icon name="trash" size={18} />
          </button>
        </div>
      {:else}
        <Composer bind:this={composer} bind:files />
      {/if}

      {#if dragging > 0}
        <div class="drop" aria-hidden="true">
          <div><Icon name="download" size={28} /><span>{t("chat.drop")}</span></div>
        </div>
      {/if}
    </section>

    {#if app.infoOpen && chat}
      <ChatInfo {chat} />
    {/if}
  </div>
{/if}

{#if reactors && reactorsMsg}
  {@const target = reactorsMsg}
  <ReactionList
    reactions={target.reactions}
    x={reactors.x}
    top={reactors.top}
    bottom={reactors.bottom}
    onremove={() => {
      react(target, myReaction(target));
      reactors = null;
    }}
    onclose={() => (reactors = null)}
  />
{/if}

{#if reacting}
  {@const target = reacting.m}
  <ReactBar x={reacting.x} y={reacting.y} current={myReaction(target)} onpick={(e) => react(target, e)} onclose={() => (reacting = null)} />
{/if}

{#if app.lightbox}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="lightbox" onclick={() => (app.lightbox = null)}>
    <img src={app.lightbox} alt="" />
    <button class="icon-btn close" aria-label={t("chat.close")}><Icon name="close" size={20} /></button>
  </div>
{/if}

<style>
  .blank,
  .pane {
    flex: 1;
    min-width: 0;
    margin: 8px 8px 8px 0;
    border-radius: 16px;
    background: var(--bg);
    box-shadow: var(--shadow-s), 0 0 0 0.5px var(--line);
    overflow: hidden;
  }
  .blank {
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 4px;
    color: var(--text-2);
    background-image: var(--wall);
  }
  .mark {
    position: relative;
    width: 84px;
    height: 64px;
    margin-bottom: 18px;
  }
  .mark span {
    position: absolute;
    height: 30px;
    border-radius: 15px;
    box-shadow: var(--shadow-s);
  }
  .mark .b1 {
    left: 0;
    top: 0;
    width: 58px;
    background: var(--bubble);
    box-shadow: var(--shadow-s), 0 0 0 1px var(--bubble-line);
    border-top-left-radius: 6px;
  }
  .mark .b2 {
    right: 0;
    bottom: 0;
    width: 66px;
    background: var(--mine);
    border-bottom-right-radius: 6px;
  }
  .blank strong {
    font-family: var(--rounded);
    font-size: 17px;
    font-weight: 700;
    color: var(--text);
  }
  .pane {
    position: relative;
    display: flex;
    min-height: 0;
  }
  /* On narrow windows the info panel floats over the conversation. */
  @media (max-width: 1040px) {
    .pane > :global(.info) {
      position: absolute;
      top: 0;
      right: 0;
      bottom: 0;
      z-index: 5;
      box-shadow: var(--shadow);
    }
  }
  .conv {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background-image: var(--wall);
  }
  header {
    flex: none;
    height: 58px;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 10px 0 8px;
    background: color-mix(in srgb, var(--bg) 86%, transparent);
    backdrop-filter: blur(18px) saturate(1.4);
    border-bottom: 1px solid var(--line-soft);
    z-index: 2;
  }
  .who-btn {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 8px;
    border-radius: 12px;
    text-align: left;
  }
  .who-btn:hover:not(:disabled) {
    background: var(--bg-2);
  }
  .titles {
    min-width: 0;
    display: grid;
  }
  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .subtitle {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .icon-btn.on {
    background: var(--bg-3);
    color: var(--text);
  }
  .autobtn {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    margin-right: 4px;
    padding: 0 10px;
    border-radius: 13px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
    box-shadow: inset 0 0 0 1px var(--line);
    transition: background 160ms var(--ease), color 160ms var(--ease);
  }
  .autobtn.on {
    background: var(--accent);
    color: var(--on-accent);
    box-shadow: none;
  }

  /* ---- pinned bar ---- */
  .pinbar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--line-soft);
    background: color-mix(in srgb, var(--bg) 92%, transparent);
    text-align: left;
    z-index: 1;
  }
  .pinbar:hover {
    background: var(--bg-2);
  }
  .pinic {
    flex: none;
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 8px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .pinbody {
    min-width: 0;
    display: grid;
    font-size: 12.5px;
  }
  .pinbody b {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--accent);
  }
  .pinbody span {
    color: var(--text-2);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  /* ---- log ---- */
  .log {
    flex: 1;
    min-height: 0;
    padding: 12px clamp(14px, 5%, 64px) 12px;
    display: flex;
    flex-direction: column;
  }
  .log > :first-child {
    margin-top: auto;
  }
  .older.phone {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-3);
  }
  .spin {
    width: 12px;
    height: 12px;
    border: 1.5px solid var(--line);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 800ms linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spin {
      animation: none;
    }
  }
  .older {
    align-self: center;
    margin-bottom: 8px;
  }
  .dayblock {
    display: flex;
    flex-direction: column;
  }
  .day {
    position: sticky;
    top: 6px;
    z-index: 1;
    align-self: center;
    margin: 16px 0 6px;
  }
  .day span {
    display: block;
    padding: 4px 12px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--panel) 90%, transparent);
    backdrop-filter: blur(10px);
    box-shadow: var(--shadow-s), 0 0 0 0.5px var(--line-soft);
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }

  /* Every message stands on its own, 12px apart, whoever sent it. Only the
     first bubble of a run keeps the small tail corner. */
  .msg {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
    border-radius: 12px;
    transition: background 200ms var(--ease);
  }
  .msg.reacted {
    margin-bottom: 18px;
  }
  .msg.me {
    flex-direction: row-reverse;
  }
  .msg.me .lane {
    align-items: flex-end;
  }
  .msg.fresh .lane {
    animation: rise 260ms var(--ease);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
  }
  .msg.flash .bubble {
    animation: flash 1.3s var(--ease);
  }
  @keyframes flash {
    0%,
    40% {
      box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 55%, transparent);
    }
  }
  .lane {
    position: relative;
    max-width: min(72%, 580px);
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
  }
  .bubble {
    --r: 18px;
    --rs: 6px;
    position: relative;
    max-width: 100%;
    padding: 7px 12px 7px;
    border-radius: var(--r);
    background: var(--bubble);
    box-shadow: 0 1px 1.5px rgb(20 10 60 / 0.05), 0 0 0 1px var(--bubble-line);
    font-size: 14px;
    line-height: 1.4;
    user-select: text;
    -webkit-user-select: text;
    cursor: text;
  }
  .first .bubble {
    border-top-left-radius: var(--rs);
  }
  .me .bubble {
    background: var(--mine);
    color: var(--on-mine);
    box-shadow: 0 1px 2px rgb(30 15 110 / 0.18);
  }
  .me.first .bubble {
    border-top-left-radius: var(--r);
    border-top-right-radius: var(--rs);
  }
  .bubble.visual {
    padding: 4px;
  }
  .bubble.visual .meta {
    position: absolute;
    right: 10px;
    bottom: 9px;
    margin: 0;
    padding: 2px 7px;
    border-radius: 9px;
    background: rgb(0 0 0 / 0.45);
    color: #fff;
    opacity: 1;
  }
  .bubble.sticker {
    background: none;
    box-shadow: none;
  }
  .bubble.sticker .meta {
    background: color-mix(in srgb, var(--panel) 85%, transparent);
    color: var(--text-2);
  }
  .bubble.gone {
    background: transparent;
    color: var(--text-3);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .me .bubble.gone {
    color: var(--text-3);
  }
  .bubble.visual .who {
    margin: 3px 8px 5px;
  }
  .who {
    display: block;
    max-width: 100%;
    margin-bottom: 2px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 12px;
    font-weight: 600;
    text-align: left;
    color: oklch(0.56 0.14 var(--h));
    cursor: pointer;
  }
  .who:hover {
    text-decoration: underline;
  }
  .link {
    color: var(--accent);
    text-decoration: underline;
    text-decoration-color: color-mix(in srgb, var(--accent) 45%, transparent);
    text-underline-offset: 2px;
    overflow-wrap: anywhere;
    cursor: pointer;
  }
  .me .link {
    color: inherit;
    text-decoration-color: currentColor;
  }
  .unread-divider {
    display: flex;
    justify-content: center;
    margin: 18px -16px 2px;
    padding: 5px 0;
    background: color-mix(in srgb, var(--accent) 9%, transparent);
  }
  .unread-divider span {
    padding: 3px 12px;
    border-radius: 999px;
    background: var(--panel);
    box-shadow: 0 0 0 1px var(--line);
    color: var(--accent);
    font-size: 12px;
    font-weight: 600;
  }
  .viewonce {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 2px 4px 2px 0;
  }
  .vo-ic {
    position: relative;
    width: 38px;
    height: 38px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 50%;
    border: 1.5px dashed currentColor;
    opacity: 0.85;
  }
  .vo-ic b {
    position: absolute;
    right: -3px;
    bottom: -3px;
    width: 16px;
    height: 16px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 10px;
  }
  .vo-meta {
    display: grid;
    gap: 1px;
  }
  .vo-meta strong {
    font-size: 13.5px;
    font-weight: 600;
  }
  .vo-meta small {
    font-size: 11.5px;
    opacity: 0.7;
  }
  .mention {
    display: inline;
    padding: 0 1px;
    border-radius: 4px;
    font: inherit;
    font-weight: 600;
    color: var(--accent);
    cursor: pointer;
  }
  .me .mention {
    color: inherit;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .mention:hover {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .tags {
    flex: none;
    display: flex;
    gap: 4px;
    max-width: 40%;
    overflow: hidden;
  }
  .tag {
    flex: none;
    max-width: 140px;
    height: 24px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 9px 0 7px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    border-radius: 12px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
    font-size: 11.5px;
    font-weight: 600;
  }
  .tag:hover {
    background: color-mix(in srgb, var(--accent) 20%, transparent);
  }
  .fwd,
  .deleted {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    font-style: italic;
    opacity: 0.72;
  }
  .fwd {
    margin-bottom: 2px;
  }
  .deleted {
    font-size: 13px;
  }
  .quote {
    width: 100%;
    min-width: 160px;
    display: grid;
    gap: 1px;
    margin: 2px 0 5px;
    padding: 6px 10px;
    border-radius: 10px;
    background: color-mix(in srgb, currentColor 8%, transparent);
    text-align: left;
    font-size: 12.5px;
    line-height: 1.35;
    transition: background 140ms var(--ease);
  }
  .quote:hover {
    background: color-mix(in srgb, currentColor 13%, transparent);
  }
  .quote b {
    font-weight: 600;
    color: var(--accent);
  }
  .me .quote b {
    color: inherit;
  }
  .quote span {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    opacity: 0.8;
  }
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .text.aftermedia {
    display: block;
    padding: 6px 8px 0;
  }
  .kind {
    font-style: italic;
    opacity: 0.75;
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
  .me .meta {
    color: var(--mine-2);
    opacity: 1;
  }
  .meta .ai {
    display: inline-grid;
    color: var(--accent);
  }
  .me .meta .ai {
    color: inherit;
  }

  /* ---- hover affordances ---- */
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
    color: inherit;
    background: radial-gradient(circle, var(--bubble) 55%, transparent 72%);
    opacity: 0;
    transition: opacity 120ms var(--ease);
  }
  .me .chev {
    background: radial-gradient(circle, var(--mine) 55%, transparent 72%);
  }
  .bubble.visual .chev,
  .bubble.sticker .chev {
    background: rgb(0 0 0 / 0.42);
    color: #fff;
  }
  .msg:hover .chev,
  .chev:focus-visible {
    opacity: 1;
  }
  .hover-acts {
    display: flex;
    gap: 2px;
    opacity: 0;
    transition: opacity 120ms var(--ease);
  }
  .hover-acts button {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: var(--text-3);
  }
  .hover-acts button:hover {
    background: var(--bg-3);
    color: var(--text);
  }
  .msg:hover .hover-acts,
  .hover-acts:focus-within {
    opacity: 1;
  }

  /* ---- reactions ---- */
  .reactions {
    position: absolute;
    bottom: -15px;
    left: 10px;
    display: flex;
    gap: 3px;
    z-index: 1;
  }
  .me .reactions {
    left: auto;
    right: 10px;
  }
  .rx {
    height: 24px;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 0 7px;
    border-radius: 12px;
    background: var(--panel);
    box-shadow: var(--shadow-s), 0 0 0 1px var(--line-soft);
    font-size: 13px;
    line-height: 1;
    transition: transform 140ms var(--ease);
  }
  .rx:hover {
    transform: translateY(-1px);
  }
  .rx.mine {
    background: var(--accent-soft);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .rx small {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }

  /* ---- selection ---- */
  .log.selecting .msg {
    cursor: default;
    padding: 0 6px;
  }
  .log.selecting .bubble {
    cursor: default;
    user-select: none;
    -webkit-user-select: none;
  }
  .msg.picked {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .tick {
    flex: none;
    width: 20px;
    height: 20px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: transparent;
    box-shadow: inset 0 0 0 1.5px var(--text-3);
    transition: background 120ms var(--ease);
  }
  .tick.on {
    background: var(--accent);
    color: var(--on-accent);
    box-shadow: none;
  }
  .msg.me .tick {
    order: 1;
  }
  .selbar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 2px;
    margin: 8px 16px 14px;
    padding: 6px 8px;
    border-radius: 22px;
    background: var(--panel);
    box-shadow: var(--shadow-s), 0 0 0 1px var(--line-soft);
    animation: rise 200ms var(--ease);
  }
  .selbar .count {
    flex: 1;
    padding-left: 6px;
    font-weight: 600;
  }
  .selbar .icon-btn {
    border-radius: 50%;
  }
  .icon-btn.danger {
    color: var(--danger);
  }

  /* ---- poll ---- */
  .poll {
    width: 290px;
    max-width: 100%;
    display: grid;
    gap: 2px;
    padding: 2px 0 0;
    transition: opacity 160ms var(--ease);
  }
  .poll.busy {
    opacity: 0.7;
  }
  .poll-head {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    opacity: 0.65;
  }
  .poll-q {
    font-size: 14px;
    font-weight: 600;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  .poll-hint {
    font-size: 11.5px;
    opacity: 0.65;
  }
  .poll-opts {
    margin: 6px 0 2px;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 4px;
  }
  .poll-opt {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 6px 6px 6px 4px;
    border-radius: 10px;
    text-align: left;
    color: inherit;
    transition: background 140ms var(--ease);
  }
  .poll-opt:hover:not(:disabled),
  .poll-opt:focus-visible {
    background: color-mix(in srgb, currentColor 8%, transparent);
    outline: none;
  }
  .pick {
    flex: none;
    width: 18px;
    height: 18px;
    display: grid;
    place-items: center;
    border-radius: 5px;
    box-shadow: inset 0 0 0 1.5px color-mix(in srgb, currentColor 45%, transparent);
  }
  .pick.round {
    border-radius: 50%;
  }
  .poll-opt.mine .pick {
    background: var(--accent);
    color: var(--on-accent);
    box-shadow: none;
  }
  .me .poll-opt.mine .pick {
    background: var(--on-mine);
    color: var(--mine);
  }
  .poll-line {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 5px;
  }
  .poll-name {
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  .poll-bar {
    height: 5px;
    border-radius: 3px;
    background: color-mix(in srgb, currentColor 14%, transparent);
    overflow: hidden;
  }
  .poll-bar i {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
    transition: width 240ms var(--ease);
  }
  .me .poll-bar i {
    background: var(--on-mine);
  }
  @media (prefers-reduced-motion: reduce) {
    .poll-bar i {
      transition: none;
    }
  }
  .poll-n {
    flex: none;
    min-width: 14px;
    font-size: 12px;
    text-align: right;
    opacity: 0.75;
  }
  .voters {
    display: block;
    padding: 0 6px 4px 31px;
    font-size: 11.5px;
    opacity: 0.7;
    overflow-wrap: anywhere;
  }
  .poll-more {
    justify-self: center;
    margin-top: 2px;
    padding: 4px 10px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 600;
    color: inherit;
    opacity: 0.85;
  }
  .poll-more:hover:not(:disabled) {
    background: color-mix(in srgb, currentColor 8%, transparent);
  }
  .poll-more:disabled {
    font-weight: 500;
    opacity: 0.55;
  }

  /* ---- document ---- */
  .doc {
    width: 270px;
    max-width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 2px 0;
    padding: 8px 10px 8px 8px;
    border-radius: 12px;
    background: color-mix(in srgb, currentColor 8%, transparent);
    text-align: left;
  }
  .doc:hover:not(:disabled) {
    background: color-mix(in srgb, currentColor 13%, transparent);
  }
  .doc-ic {
    position: relative;
    flex: none;
    width: 38px;
    height: 44px;
    display: grid;
    place-items: center;
    border-radius: 8px;
    background: var(--accent);
    color: var(--on-accent);
  }
  .me .doc-ic {
    background: var(--on-mine);
    color: var(--mine);
  }
  .doc-ic b {
    position: absolute;
    bottom: 3px;
    font-size: 8px;
    letter-spacing: 0.04em;
  }
  .doc-ic :global(svg) {
    margin-top: -8px;
  }
  .doc-meta {
    flex: 1;
    min-width: 0;
    display: grid;
  }
  .doc-name {
    font-weight: 600;
    font-size: 13px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .doc-meta small {
    font-size: 11px;
    opacity: 0.7;
  }
  .drop {
    position: absolute;
    inset: 8px;
    z-index: 10;
    display: grid;
    place-items: center;
    border-radius: 12px;
    border: 2px dashed var(--accent);
    background: color-mix(in srgb, var(--bg) 82%, transparent);
    backdrop-filter: blur(4px);
    pointer-events: none;
    animation: fadein 140ms var(--ease);
  }
  .drop div {
    display: grid;
    justify-items: center;
    gap: 8px;
    color: var(--accent);
    font-weight: 600;
  }
  @keyframes fadein {
    from {
      opacity: 0;
    }
  }
  .lightbox {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: grid;
    place-items: center;
    padding: 48px;
    background: rgb(10 8 20 / 0.86);
    animation: fadein 160ms var(--ease);
    cursor: zoom-out;
  }
  .lightbox img {
    max-width: 100%;
    max-height: 100%;
    border-radius: 8px;
    box-shadow: 0 30px 80px rgb(0 0 0 / 0.5);
  }
  .lightbox .close {
    position: absolute;
    top: 16px;
    right: 16px;
    color: #fff;
  }
</style>
