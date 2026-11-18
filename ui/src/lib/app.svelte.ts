import {
  api,
  on,
  type AiConfig,
  type AiRow,
  type AutoReply,
  type Chat,
  type ChatList,
  type Message,
  type Note,
  type Release,
  type SendKind,
  type Status,
} from "./api";
import { locale } from "./i18n";

export type Theme = "system" | "light" | "dark";

export interface MenuItem {
  label?: string;
  icon?: string;
  danger?: boolean;
  checked?: boolean;
  disabled?: boolean;
  separator?: boolean;
  action?: () => void;
  children?: MenuItem[];
}

export function openMenu(e: MouseEvent | { x: number; y: number }, items: MenuItem[]) {
  if ("preventDefault" in e) e.preventDefault();
  const x = "clientX" in e ? e.clientX : e.x;
  const y = "clientY" in e ? e.clientY : e.y;
  app.menu = { x, y, items };
}
/** "all", "unread", "groups" or "list:<id>". */
export type Filter = string;

function stored<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

export function remember(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Preferences are a convenience; private mode just forgets them.
  }
}

/** Id used in `app.selected` for the assistant conversation. */
export const AI = "ai";
/** Id used in `app.selected` for the private notes chat. */
export const NOTES = "notes";

export type NotesView = "chat" | "board" | "postits";

export const app = $state({
  status: { link: "starting", qr_svg: null, pair_code: null, error: null } as Status,
  chats: [] as Chat[],
  selected: null as string | null,
  messages: [] as Message[],
  hasOlder: false,
  loadingOlder: false,
  /** Bumped whenever a message is appended, so the view can follow the tail. */
  tail: 0,
  /** Id of the message that just arrived live; only that one animates in. */
  fresh: null as string | null,
  settingsOpen: false,
  settingsSection: "general" as string,
  infoOpen: false,
  lists: [] as ChatList[],
  filter: "all" as Filter,
  archived: false,
  theme: stored<Theme>("theme", "system"),
  sidebarWidth: stored<number>("sidebarWidth", 320),
  /** Profile pictures by chat: a data URL, null when there is none. */
  avatars: {} as Record<string, string | null>,
  update: null as Release | null,
  history: null as number | null,
  toast: null as { text: string; id: number } | null,
  confirm: null as null | {
    title: string;
    body: string;
    action: string;
    danger: boolean;
    resolve: (ok: boolean) => void;
  },
  prompt: null as null | {
    title: string;
    placeholder: string;
    value: string;
    action: string;
    resolve: (value: string | null) => void;
  },
  menu: null as null | { x: number; y: number; items: MenuItem[] },
  /** The message the composer is replying to, or editing. */
  reply: null as Message | null,
  editing: null as Message | null,
  /** Multi-select mode in the open chat, with the picked message ids. */
  selecting: false,
  picked: [] as string[],
  /** Pinned messages of the open chat. */
  pinned: [] as Message[],
  /** Forward dialog: what is being forwarded. */
  forward: null as null | { jid: string; ids: string[] } | { note: number },
  lightbox: null as string | null,
  /** A dialog offering several actions (delete for me / for everyone). */
  choice: null as null | {
    title: string;
    body: string;
    options: { id: string; label: string; danger?: boolean }[];
    resolve: (id: string | null) => void;
  },
  /** Message shown in the info dialog. */
  info: null as Message | null,
  notes: [] as Note[],
  notesView: stored<NotesView>("notesView", "chat"),
  ai: {
    config: { provider: null, base_url: "", model: "", confirm_send: true, web_access: true } as AiConfig,
    rows: [] as AiRow[],
    busy: false,
    streaming: "",
  },
  auto: {
    enabled: false,
    chats: "none",
    groups: "none",
    groups_only_mentions: true,
    max_per_chat_per_hour: 6,
    min_delay_secs: 4,
    max_delay_secs: 12,
    quiet_from: null,
    quiet_to: null,
    instructions: "",
  } as AutoReply,
});

const PAGE = 60;

export function chatTitle(chat: { jid: string; name: string }): string {
  if (chat.name) return chat.name;
  const user = chat.jid.split("@")[0].split(":")[0];
  return /^\d{7,15}$/.test(user) && chat.jid.endsWith("@s.whatsapp.net") ? `+${user}` : user;
}

let refreshTimer: ReturnType<typeof setTimeout> | undefined;

/** Coalesces bursts of updates (history import) into one reload. */
function refreshChats() {
  clearTimeout(refreshTimer);
  refreshTimer = setTimeout(async () => {
    app.chats = await api.chats();
  }, 120);
}

async function refreshLists() {
  app.lists = await api.lists();
}

export function isMuted(chat: Chat): boolean {
  return chat.muted_until < 0 || chat.muted_until > Date.now() / 1000;
}

export function isUnread(chat: Chat): boolean {
  return chat.unread > 0 || chat.marked_unread;
}

const loadingAvatars = new Set<string>();

/** Returns the cached picture and fetches it in the background the first time. */
export function avatarFor(jid: string): string | null {
  if (jid in app.avatars) return app.avatars[jid];
  if (!loadingAvatars.has(jid) && jid.includes("@")) {
    loadingAvatars.add(jid);
    api
      .avatar(jid)
      .then((url) => (app.avatars[jid] = url))
      .catch(() => (app.avatars[jid] = null))
      .finally(() => loadingAvatars.delete(jid));
  }
  return null;
}

export function applyTheme(theme: Theme) {
  app.theme = theme;
  remember("theme", theme);
  if (theme === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
}

let toastSeq = 0;
export function toast(text: string) {
  const id = ++toastSeq;
  app.toast = { text, id };
  setTimeout(() => {
    if (app.toast?.id === id) app.toast = null;
  }, 3200);
}

/** Asks the user to confirm a destructive action. */
export function confirmAction(opts: { title: string; body: string; action: string; danger?: boolean }): Promise<boolean> {
  return new Promise((resolve) => {
    app.confirm = { ...opts, danger: opts.danger ?? true, resolve };
  });
}

/** Offers a few actions; resolves with the chosen id, or null when cancelled. */
export function choose(opts: {
  title: string;
  body: string;
  options: { id: string; label: string; danger?: boolean }[];
}): Promise<string | null> {
  return new Promise((resolve) => {
    app.choice = { ...opts, resolve };
  });
}

/** Asks for a short piece of text (a list name). */
export function askText(opts: { title: string; placeholder: string; value?: string; action: string }): Promise<string | null> {
  return new Promise((resolve) => {
    app.prompt = { ...opts, value: opts.value ?? "", resolve };
  });
}

export function openSettings(section = "general") {
  app.settingsSection = section;
  app.settingsOpen = true;
}

function resetChatState() {
  app.reply = null;
  app.editing = null;
  app.selecting = false;
  app.picked = [];
  app.pinned = [];
}

export async function selectChat(jid: string) {
  if (app.selected !== jid) {
    app.infoOpen = false;
    resetChatState();
  }
  app.selected = jid;
  app.messages = [];
  if (jid === AI) return;
  if (jid === NOTES) {
    app.notes = await api.notes();
    app.tail++;
    return;
  }
  void refreshPinned(jid);
  const page = await api.messages(jid);
  if (app.selected !== jid) return;
  app.messages = page;
  app.hasOlder = page.length === PAGE;
  app.tail++;
  await api.markRead(jid);
}

export function closeChat() {
  resetChatState();
  app.infoOpen = false;
  app.selected = null;
  app.messages = [];
}

async function refreshPinned(jid: string) {
  try {
    const pinned = await api.pinnedMessages(jid);
    if (app.selected === jid) app.pinned = pinned;
  } catch {
    // Pins are a convenience; the conversation still works without them.
  }
}

/** Re-reads one message after a reaction, edit, deletion, star or pin. */
async function refreshMessage(jid: string, id: string) {
  if (app.selected !== jid) return;
  const fresh = await api.message(jid, id);
  const i = app.messages.findIndex((m) => m.id === id);
  if (fresh && i >= 0) app.messages[i] = fresh;
  else if (!fresh && i >= 0) app.messages.splice(i, 1);
  void refreshPinned(jid);
}

export async function loadOlder() {
  const jid = app.selected;
  if (!jid || app.loadingOlder || !app.hasOlder || app.messages.length === 0) return;
  app.loadingOlder = true;
  try {
    const page = await api.messages(jid, app.messages[0].ts);
    if (app.selected !== jid) return;
    const known = new Set(app.messages.map((m) => m.id));
    app.messages = [...page.filter((m) => !known.has(m.id)), ...app.messages];
    app.hasOlder = page.length === PAGE;
  } finally {
    app.loadingOlder = false;
  }
}

function append(message: Message) {
  if (message.chat_jid !== app.selected) return;
  if (app.messages.some((m) => m.id === message.id)) return;
  app.messages.push(message);
  app.fresh = message.id;
  app.tail++;
}

export async function send(text: string) {
  const jid = app.selected;
  if (!jid) return;
  if (app.editing) {
    const target = app.editing;
    await api.edit(jid, target.id, text);
    app.editing = null;
    void refreshMessage(jid, target.id);
    return;
  }
  const reply = app.reply;
  const message = await api.sendText(jid, text, reply?.id);
  app.reply = null;
  append(message);
}

/** Appends a message this window sent through another path (stickers). */
export function appendSent(message: Message) {
  append(message);
}

export async function refreshNotes() {
  app.notes = await api.notes();
}

function readBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result).split(",", 2)[1] ?? "");
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}

export const MAX_FILE = 64 * 1024 * 1024;

export function kindOf(file: File): SendKind {
  if (file.type.startsWith("image/") && file.type !== "image/svg+xml") return "image";
  if (file.type === "video/mp4" || file.type === "video/quicktime") return "video";
  if (file.type.startsWith("audio/")) return "audio";
  return "document";
}

/** Sends a file (or a recorded voice note) to the open chat. */
export async function sendFile(blob: Blob, opts: { name: string; kind: SendKind; caption?: string; seconds?: number }) {
  const jid = app.selected;
  if (!jid || jid === AI || jid === NOTES) return;
  if (blob.size > MAX_FILE) throw new Error("too-big");
  const base64 = await readBase64(blob);
  const mime = (blob.type || "application/octet-stream").split(";")[0];
  const message = await api.sendMedia({
    jid,
    kind: opts.kind,
    name: opts.name,
    mime: opts.kind === "voice" ? blob.type || mime : mime,
    base64,
    caption: opts.caption ?? "",
    seconds: Math.round(opts.seconds ?? 0),
  });
  append(message);
}

export function aiReady(): boolean {
  return app.ai.config.provider !== null && app.ai.config.model !== "";
}

export async function saveAiConfig(patch: Partial<AiConfig>) {
  app.ai.config = { ...app.ai.config, ...patch };
  await api.setAiConfig($state.snapshot(app.ai.config));
}

export async function saveAutoReply(patch: Partial<AutoReply>) {
  app.auto = { ...app.auto, ...patch };
  await api.setAutoReply($state.snapshot(app.auto));
}

export async function askAi(text: string, files: { name: string; base64: string }[]) {
  app.ai.streaming = "";
  await api.aiSend(text, files, locale);
}

async function refreshAi() {
  app.ai.rows = await api.aiHistory();
  // Whatever was streaming is now part of the stored history (or superseded by a tool step).
  app.ai.streaming = "";
  app.tail++;
}

export async function init() {
  applyTheme(app.theme);
  await on<Release>("update-available", (r) => (app.update = r));
  await on<number>("history-progress", (p) => {
    app.history = p >= 100 ? null : p;
  });
  await on<string>("picture-changed", (jid) => {
    delete app.avatars[jid];
  });
  await on<string>("ai-token", (token) => {
    app.ai.streaming += token;
    app.tail++;
  });
  await on<boolean>("ai-busy", (busy) => {
    app.ai.busy = busy;
  });
  await on<null>("ai-changed", () => void refreshAi());
  await on<string>("open-chat", (jid) => void selectChat(jid));
  await on<null>("auto-changed", async () => {
    app.auto = await api.autoReply();
  });
  app.ai.config = await api.aiConfig();
  app.auto = await api.autoReply();
  await refreshAi();
  await on<Status>("status", (s) => {
    app.status = s;
    if (s.link === "logged_out") {
      app.selected = null;
      app.messages = [];
    }
  });
  await on<Message>("message", (m) => {
    append(m);
    if (m.chat_jid === app.selected && !m.from_me && document.hasFocus()) {
      void api.markRead(m.chat_jid);
    }
  });
  await on<{ chat_jid: string; id: string }>("message-changed", (c) => void refreshMessage(c.chat_jid, c.id));
  await on<null>("chats-changed", () => {
    refreshChats();
    void refreshLists();
  });
  app.status = await api.status();
  app.chats = await api.chats();
  await refreshLists();
}
