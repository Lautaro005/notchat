import { api, type Message } from "./api";
import { app, choose, toast, type MenuItem } from "./app.svelte";
import { t, tf } from "./i18n";

/** Reactions offered in one click; the full picker covers the rest. */
export const QUICK_REACTIONS = ["👍", "❤️", "😂", "😮", "😢", "🙏"];

/** WhatsApp only allows editing your own text for 15 minutes. */
const EDIT_WINDOW = 15 * 60;

async function attempt(fn: () => Promise<unknown>, done?: string) {
  try {
    await fn();
    if (done) toast(done);
  } catch (e) {
    toast(String(e).includes("not connected") ? t("toast.offline") : t("toast.failed"));
  }
}

export function canEdit(m: Message): boolean {
  return m.from_me && m.kind === "text" && !m.deleted && Date.now() / 1000 - m.ts < EDIT_WINDOW;
}

/** The reaction I already left on a message, if any. */
export function myReaction(m: Message): string {
  return m.reactions.find((r) => r.from_me)?.emoji ?? "";
}

export function react(m: Message, emoji: string) {
  const next = myReaction(m) === emoji ? "" : emoji;
  void attempt(() => api.react(m.chat_jid, m.id, next));
}

export function startReply(m: Message) {
  app.editing = null;
  app.reply = m;
}

export function startEdit(m: Message) {
  app.reply = null;
  app.editing = m;
}

export function copyText(ms: Message[]) {
  const text = ms
    .filter((m) => m.text)
    .map((m) => m.text)
    .join("\n\n");
  if (!text) return;
  void navigator.clipboard.writeText(text).then(
    () => toast(t("msg.copied")),
    () => toast(t("toast.failed")),
  );
}

export function forward(ms: Message[]) {
  if (!ms.length) return;
  app.forward = { jid: ms[0].chat_jid, ids: ms.map((m) => m.id) };
}

export function toNotes(ms: Message[]) {
  void attempt(async () => {
    for (const m of ms) await api.noteFromMessage(m.chat_jid, m.id);
  }, t("msg.savedNote"));
}

export function star(ms: Message[], starred: boolean) {
  void attempt(async () => {
    for (const m of ms) await api.star(m.chat_jid, m.id, starred);
  });
}

/** Asks how to delete (for me / for everyone) and does it. */
export async function remove(ms: Message[]) {
  if (!ms.length) return;
  const mine = ms.every((m) => m.from_me && !m.deleted);
  const options = [
    ...(mine ? [{ id: "all", label: t("msg.deleteAll"), danger: true }] : []),
    { id: "me", label: t("msg.deleteMe"), danger: !mine },
  ];
  const picked = await choose({
    title: ms.length === 1 ? t("msg.deleteTitle") : tf("msg.deleteManyTitle", { n: ms.length }),
    body: mine ? t("msg.deleteBody") : t("msg.deleteBodyMine"),
    options,
  });
  if (!picked) return;
  await attempt(async () => {
    for (const m of ms) await api.deleteMessage(m.chat_jid, m.id, picked === "all");
  });
  app.selecting = false;
  app.picked = [];
}

function saveMedia(m: Message) {
  void attempt(() => api.saveMedia(m.chat_jid, m.id), t("toast.saved"));
}

/** Keeps a sticker in the favourites so it can be sent later. */
function saveSticker(m: Message) {
  void attempt(async () => {
    await api.saveSticker(m.chat_jid, m.id);
    app.stickersVersion++;
  }, t("msg.stickerSaved"));
}

/** The dropdown on a message, in WhatsApp's order. */
export function messageMenu(m: Message, openReact: () => void): MenuItem[] {
  if (m.deleted) {
    return [
      { label: t("msg.select"), icon: "select", action: () => startSelect(m) },
      { separator: true },
      { label: t("msg.delete"), icon: "trash", danger: true, action: () => void remove([m]) },
    ];
  }
  const items: MenuItem[] = [
    { label: t("msg.reply"), icon: "reply", action: () => startReply(m) },
    { label: t("msg.react"), icon: "smile", action: openReact },
    m.starred
      ? { label: t("msg.unstar"), icon: "star", action: () => star([m], false) }
      : { label: t("msg.star"), icon: "star", action: () => star([m], true) },
    m.pinned > 0
      ? { label: t("msg.unpin"), icon: "pin", action: () => void attempt(() => api.pinMessage(m.chat_jid, m.id, false)) }
      : {
          label: t("msg.pin"),
          icon: "pin",
          action: () => void attempt(() => api.pinMessage(m.chat_jid, m.id, true), t("msg.pinExpires")),
        },
    { label: t("msg.forward"), icon: "forward", action: () => forward([m]) },
  ];
  if (m.text) items.push({ label: t("msg.copy"), icon: "copy", action: () => copyText([m]) });
  if (canEdit(m)) items.push({ label: t("msg.edit"), icon: "pencil", action: () => startEdit(m) });
  if (m.kind === "sticker" && m.has_media) {
    items.push({ label: t("msg.saveSticker"), icon: "sticker", action: () => saveSticker(m) });
  } else if (m.has_media) {
    items.push({ label: t("menu.save"), icon: "download", action: () => saveMedia(m) });
  }
  items.push(
    { label: t("msg.toNotes"), icon: "note", action: () => toNotes([m]) },
    { label: t("msg.info"), icon: "info", action: () => (app.info = m) },
    { separator: true },
    { label: t("msg.delete"), icon: "trash", danger: true, action: () => void remove([m]) },
    { separator: true },
    { label: t("msg.select"), icon: "select", action: () => startSelect(m) },
  );
  return items;
}

export function startSelect(m?: Message) {
  app.reply = null;
  app.editing = null;
  app.selecting = true;
  app.picked = m ? [m.id] : [];
}

export function togglePick(id: string) {
  app.picked = app.picked.includes(id) ? app.picked.filter((x) => x !== id) : [...app.picked, id];
}
