import { api, type Chat } from "./api";
import { app, askText, confirmAction, isMuted, isUnread, selectChat, toast, type MenuItem } from "./app.svelte";
import { t, tf } from "./i18n";

/** Runs a chat action and tells the user when it fails. */
async function attempt(fn: () => Promise<unknown>, done?: string) {
  try {
    await fn();
    if (done) toast(done);
  } catch (e) {
    const msg = String(e);
    toast(msg.includes("pin limit") ? t("menu.pinLimit") : msg.includes("not connected") ? t("toast.offline") : t("toast.failed"));
  }
}

export async function newList(addJid?: string) {
  const name = await askText({ title: t("list.newTitle"), placeholder: t("list.namePh"), action: t("list.create") });
  if (!name) return;
  await attempt(async () => {
    const id = await api.createList(name);
    if (addJid) await api.setListMember(id, addJid, true);
    if (!addJid) app.filter = `list:${id}`;
  });
}

/** Right-click on a filter (All, Unread, Groups or a list): mark every
 *  unread chat it shows as read. */
export function filterMenu(filter: string): MenuItem[] {
  const members = filter.startsWith("list:")
    ? new Set(app.lists.find((l) => `list:${l.id}` === filter)?.jids ?? [])
    : null;
  const targets = app.chats.filter(
    (c) =>
      !c.archived &&
      isUnread(c) &&
      (filter === "groups" ? c.is_group : true) &&
      (members ? members.has(c.jid) : true),
  );
  return [
    {
      label: targets.length ? tf("menu.markAllRead", { n: targets.length }) : t("menu.allRead"),
      icon: "check",
      disabled: targets.length === 0,
      action: () => attempt(() => api.markChatsRead(targets.map((c) => c.jid))),
    },
  ];
}

export function listMenu(id: number): MenuItem[] {
  const list = app.lists.find((l) => l.id === id);
  if (!list) return [];
  return [
    {
      label: t("menu.renameList"),
      icon: "pencil",
      action: async () => {
        const name = await askText({ title: t("list.renameTitle"), placeholder: t("list.namePh"), value: list.name, action: t("list.save") });
        if (name) await attempt(() => api.renameList(id, name));
      },
    },
    {
      label: t("menu.deleteList"),
      icon: "trash",
      danger: true,
      action: async () => {
        const ok = await confirmAction({ title: t("confirm.deleteListTitle"), body: t("confirm.deleteListBody"), action: t("confirm.delete") });
        if (!ok) return;
        if (app.filter === `list:${id}`) app.filter = "all";
        await attempt(() => api.deleteList(id));
      },
    },
  ];
}

const HOUR = 3600;

export function chatMenu(chat: Chat, opts: { openInfo?: () => void } = {}): MenuItem[] {
  const jid = chat.jid;
  const muted = isMuted(chat);
  const now = () => Math.floor(Date.now() / 1000);
  const items: MenuItem[] = [
    chat.archived
      ? { label: t("menu.unarchive"), icon: "unarchive", action: () => attempt(() => api.archive(jid, false)) }
      : { label: t("menu.archive"), icon: "archive", action: () => attempt(() => api.archive(jid, true)) },
  ];
  if (!chat.archived) {
    items.push(
      chat.pinned > 0
        ? { label: t("menu.unpin"), icon: "pin", action: () => attempt(() => api.pin(jid, false)) }
        : { label: t("menu.pin"), icon: "pin", action: () => attempt(() => api.pin(jid, true)) },
    );
  }
  items.push(
    muted
      ? { label: t("menu.unmute"), icon: "bell", action: () => attempt(() => api.mute(jid, 0)) }
      : {
          label: t("menu.mute"),
          icon: "mute",
          children: [
            { label: t("mute.8h"), action: () => attempt(() => api.mute(jid, now() + 8 * HOUR)) },
            { label: t("mute.week"), action: () => attempt(() => api.mute(jid, now() + 7 * 24 * HOUR)) },
            { label: t("mute.always"), action: () => attempt(() => api.mute(jid, -1)) },
          ],
        },
    isUnread(chat)
      ? { label: t("menu.markRead"), icon: "check", action: () => attempt(() => api.markUnread(jid, false)) }
      : { label: t("menu.markUnread"), icon: "unread", action: () => attempt(() => api.markUnread(jid, true)) },
    {
      label: t("menu.addToList"),
      icon: "list",
      children: [
        ...app.lists.map((l) => {
          const member = l.jids.includes(jid);
          return { label: l.name, checked: member, action: () => attempt(() => api.setListMember(l.id, jid, !member)) };
        }),
        ...(app.lists.length ? [{ separator: true }] : []),
        { label: t("menu.newList"), icon: "plus", action: () => void newList(jid) },
      ],
    },
    { separator: true },
    {
      label: chat.is_group ? t("menu.groupInfo") : t("menu.info"),
      icon: "info",
      action: () => {
        if (opts.openInfo) opts.openInfo();
        else void selectChat(jid).then(() => (app.infoOpen = true));
      },
    },
    { label: t("menu.export"), icon: "export", action: () => attempt(() => api.exportChat(jid), t("toast.exported")) },
    { separator: true },
    {
      label: t("menu.clear"),
      icon: "eraser",
      danger: true,
      action: async () => {
        const ok = await confirmAction({ title: t("confirm.clearTitle"), body: t("confirm.clearBody"), action: t("confirm.clear") });
        if (ok) await attempt(() => api.clearChat(jid));
        if (ok && app.selected === jid) void selectChat(jid);
      },
    },
  );
  if (chat.is_group) {
    items.push({
      label: t("menu.leave"),
      icon: "exit",
      danger: true,
      action: async () => {
        const ok = await confirmAction({ title: t("confirm.leaveTitle"), body: t("confirm.leaveBody"), action: t("confirm.leave") });
        if (ok) await attempt(() => api.leaveGroup(jid));
      },
    });
  }
  items.push({
    label: t("menu.delete"),
    icon: "trash",
    danger: true,
    action: async () => {
      const ok = await confirmAction({ title: t("confirm.deleteTitle"), body: t("confirm.deleteBody"), action: t("confirm.delete") });
      if (!ok) return;
      await attempt(() => api.deleteChat(jid));
      if (app.selected === jid) {
        app.selected = null;
        app.messages = [];
      }
    },
  });
  return items;
}
