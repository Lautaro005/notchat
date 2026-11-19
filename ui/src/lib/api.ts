import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** True inside the Tauri webview; false in a plain browser preview. */
export const native = "__TAURI_INTERNALS__" in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (native) return tauriInvoke<T>(cmd, args);
  if (import.meta.env.DEV) {
    const { mockInvoke } = await import("./mock");
    return (await mockInvoke(cmd, args)) as T;
  }
  throw new Error("Notchat must run inside its app window");
}

export type Link = "starting" | "pairing" | "connected" | "disconnected" | "logged_out" | "error";

export interface Status {
  link: Link;
  qr_svg: string | null;
  pair_code: string | null;
  error: string | null;
}

export interface Chat {
  jid: string;
  name: string;
  is_group: boolean;
  last_ts: number;
  last_preview: string;
  unread: number;
  auto_reply: boolean;
  instructions: string;
  archived: boolean;
  /** Pin timestamp, 0 when not pinned. */
  pinned: number;
  /** Unix seconds; -1 forever, 0 not muted. */
  muted_until: number;
  marked_unread: boolean;
  /** Who sent the last message (groups); empty when it was me. */
  last_sender: string;
  last_from_me: boolean;
}

export interface ChatList {
  id: number;
  name: string;
  jids: string[];
}

export interface Participant {
  jid: string;
  name: string;
  admin: boolean;
}

export interface GroupInfo {
  subject: string;
  description: string;
  created: number;
  participants: Participant[];
  i_am_admin: boolean;
}

export interface ChatStorage {
  jid: string;
  name: string;
  messages: number;
  media: number;
  cached: number;
}

export interface StorageInfo {
  database: number;
  media: number;
  avatars: number;
  session: number;
  chats: ChatStorage[];
}

export interface Release {
  version: string;
  notes: string;
  url: string;
}

export type SendKind = "image" | "video" | "audio" | "voice" | "document";

export interface Message {
  id: string;
  chat_jid: string;
  sender_jid: string;
  sender_name: string;
  from_me: boolean;
  ts: number;
  kind: string;
  text: string;
  by_ai: boolean;
  mime: string;
  file_name: string;
  size: number;
  seconds: number;
  has_media: boolean;
  quote_id: string;
  quote_sender: string;
  quote_text: string;
  quote_kind: string;
  forwarded: boolean;
  edited: boolean;
  deleted: boolean;
  starred: boolean;
  /** Pin timestamp, 0 when not pinned. */
  pinned: number;
  reactions: Reaction[];
  /** Who each `@<number>` in the text is. */
  mentions?: Person[];
  /** The sender's phone number (`+…`), when known. */
  sender_phone?: string;
  /** Display name of whoever wrote the quoted message. */
  quote_name?: string;
}

/** A WhatsApp user as shown: contact name, `~` + own name, or phone. */
export interface Person {
  jid: string;
  /** The digits written after `@` in a mention. */
  user: string;
  name: string;
  phone: string;
  /** True when the name comes from the contact book. */
  saved: boolean;
}

export interface Reaction {
  emoji: string;
  sender_jid: string;
  from_me: boolean;
}

export interface Sticker {
  id: string;
  favorite: boolean;
  last_used: number;
}

export type NoteColor = "" | "yellow" | "pink" | "blue" | "green" | "violet";

export interface Note {
  id: number;
  text: string;
  color: NoteColor;
  pinned: boolean;
  done: boolean;
  ts: number;
  kind: string;
  mime: string;
  file_name: string;
  size: number;
}

export interface ProviderPorts {
  ollama: number;
  lm_studio: number;
  llama_cpp: number;
}

export type ProviderKind = "ollama" | "lm_studio" | "llama_cpp" | "managed";

export interface Provider {
  kind: ProviderKind;
  name: string;
  base_url: string;
  installed: boolean;
  running: boolean;
  models: string[];
}

export interface AiConfig {
  provider: ProviderKind | null;
  base_url: string;
  model: string;
  confirm_send: boolean;
  web_access: boolean;
}

export type Mode = "none" | "selected" | "all";

export interface AutoReply {
  enabled: boolean;
  chats: Mode;
  groups: Mode;
  groups_only_mentions: boolean;
  max_per_chat_per_hour: number;
  min_delay_secs: number;
  max_delay_secs: number;
  quiet_from: number | null;
  quiet_to: number | null;
  instructions: string;
}

export interface AiRow {
  id: number;
  role: "user" | "assistant" | "tool" | "action" | "error";
  content: string;
  ts: number;
  meta: string;
}

export interface SendAction {
  chat_jid: string;
  chat_name: string;
  text: string;
  state: "pending" | "sent" | "cancelled" | "failed";
}

export interface Upload {
  name: string;
  base64: string;
}

export type NotifyMode = "notch" | "system" | "off";

export interface ManagedModel {
  id: string;
  name: string;
  note: string;
  size: number;
  min_ram_gb: number;
  recommended: boolean;
  installed: boolean;
}

export interface ManagedView {
  server_installed: boolean;
  running_model: string | null;
  busy: boolean;
  ram_gb: number;
  models: ManagedModel[];
}

export const api = {
  managedView: (lang: string) => invoke<ManagedView>("managed_view", { lang }),
  managedInstall: (modelId: string) => invoke<void>("managed_install", { modelId }),
  notifyMode: () => invoke<NotifyMode>("get_notify_mode"),
  setNotifyMode: (mode: NotifyMode) => invoke<void>("set_notify_mode", { mode }),
  enableSystemNotifications: (lang: string) => invoke<boolean>("enable_system_notifications", { lang }),
  notchExpand: (expanded: boolean) => invoke<void>("notch_expand", { expanded }),
  notchHide: () => invoke<void>("notch_hide"),
  notchOpenChat: (jid: string) => invoke<void>("notch_open_chat", { jid }),
  notchTest: (title: string, text: string) => invoke<void>("notch_test", { title, text }),
  suggestReply: (jid: string) => invoke<string>("suggest_reply", { jid }),
  providers: () => invoke<Provider[]>("detect_providers"),
  aiConfig: () => invoke<AiConfig>("get_ai_config"),
  setAiConfig: (config: AiConfig) => invoke<void>("set_ai_config", { config }),
  aiHistory: () => invoke<AiRow[]>("ai_history"),
  aiSend: (text: string, files: Upload[], lang: string) => invoke<void>("ai_send", { text, files, lang }),
  aiStop: () => invoke<void>("ai_stop"),
  aiClear: () => invoke<void>("ai_clear"),
  aiResolve: (id: number, approve: boolean) => invoke<void>("ai_resolve_action", { id, approve }),
  autoReply: () => invoke<AutoReply>("get_auto_reply"),
  setAutoReply: (config: AutoReply) => invoke<void>("set_auto_reply", { config }),
  setChatAutoReply: (jid: string, enabled: boolean) => invoke<void>("set_chat_auto_reply", { jid, enabled }),
  setChatInstructions: (jid: string, instructions: string) =>
    invoke<void>("set_chat_instructions", { jid, instructions }),
  status: () => invoke<Status>("get_status"),
  startSession: (phone?: string) => invoke<void>("start_session", { phone: phone ?? null }),
  chats: () => invoke<Chat[]>("list_chats"),
  messages: (jid: string, beforeTs?: number) =>
    invoke<Message[]>("list_messages", { jid, beforeTs: beforeTs ?? null, limit: 60 }),
  markRead: (jid: string) => invoke<void>("mark_read", { jid }),
  sendText: (jid: string, text: string, replyTo?: string) =>
    invoke<Message>("send_text", { jid, text, replyTo: replyTo ?? null }),
  message: (jid: string, id: string) => invoke<Message | null>("get_message", { jid, id }),
  react: (jid: string, id: string, emoji: string) => invoke<void>("msg_react", { jid, id, emoji }),
  edit: (jid: string, id: string, text: string) => invoke<void>("msg_edit", { jid, id, text }),
  deleteMessage: (jid: string, id: string, everyone: boolean) => invoke<void>("msg_delete", { jid, id, everyone }),
  star: (jid: string, id: string, starred: boolean) => invoke<void>("msg_star", { jid, id, starred }),
  pinMessage: (jid: string, id: string, pinned: boolean) => invoke<void>("msg_pin", { jid, id, pinned }),
  forward: (jid: string, id: string, to: string[]) => invoke<number>("msg_forward", { jid, id, to }),
  pinnedMessages: (jid: string) => invoke<Message[]>("pinned_messages", { jid }),
  starredMessages: () => invoke<Message[]>("starred_messages"),
  stickers: () => invoke<Sticker[]>("stickers"),
  stickerImage: (id: string) => invoke<string>("sticker_image", { id }),
  sendSticker: (jid: string, id: string) => invoke<Message>("sticker_send", { jid, id }),
  stickerFavorite: (id: string, favorite: boolean) => invoke<void>("sticker_favorite", { id, favorite }),
  removeSticker: (id: string) => invoke<void>("sticker_remove", { id }),
  saveSticker: (jid: string, id: string) => invoke<string>("sticker_save", { jid, id }),
  notes: () => invoke<Note[]>("notes_list"),
  addNote: (text: string, color: NoteColor) => invoke<Note>("note_add", { text, color }),
  addNoteFile: (a: { text: string; name: string; mime: string; base64: string }) => invoke<Note>("note_add_file", a),
  noteFromMessage: (jid: string, id: string) => invoke<Note>("note_from_message", { jid, id }),
  updateNote: (note: Note) => invoke<void>("note_update", { note }),
  deleteNote: (id: number) => invoke<void>("note_delete", { id }),
  noteMedia: (id: number) => invoke<string>("note_media", { id }),
  saveNoteFile: (id: number) => invoke<string>("note_save_file", { id }),
  sendNote: (id: number, to: string[]) => invoke<number>("note_send", { id, to }),
  aiPorts: () => invoke<ProviderPorts>("get_ai_ports"),
  setAiPorts: (ports: ProviderPorts) => invoke<void>("set_ai_ports", { ports }),
  logout: () => invoke<void>("logout"),
  archive: (jid: string, archived: boolean) => invoke<void>("chat_archive", { jid, archived }),
  pin: (jid: string, pinned: boolean) => invoke<void>("chat_pin", { jid, pinned }),
  mute: (jid: string, until: number) => invoke<void>("chat_mute", { jid, until }),
  markUnread: (jid: string, unread: boolean) => invoke<void>("chat_mark_unread", { jid, unread }),
  clearChat: (jid: string) => invoke<void>("chat_clear", { jid }),
  deleteChat: (jid: string) => invoke<void>("chat_delete", { jid }),
  exportChat: (jid: string) => invoke<string>("chat_export", { jid }),
  leaveGroup: (jid: string) => invoke<void>("group_leave", { jid }),
  groupInfo: (jid: string) => invoke<GroupInfo>("group_info", { jid }),
  typing: (jid: string, typing: boolean) => invoke<void>("set_typing", { jid, typing }),
  avatar: (jid: string) => invoke<string | null>("avatar", { jid }),
  media: (jid: string, id: string) => invoke<string>("media", { jid, id }),
  saveMedia: (jid: string, id: string) => invoke<string>("save_media", { jid, id }),
  sendMedia: (a: { jid: string; kind: SendKind; name: string; mime: string; base64: string; caption: string; seconds: number }) =>
    invoke<Message>("send_media", a),
  lists: () => invoke<ChatList[]>("list_lists"),
  createList: (name: string) => invoke<number>("create_list", { name }),
  renameList: (id: number, name: string) => invoke<void>("rename_list", { id, name }),
  deleteList: (id: number) => invoke<void>("delete_list", { id }),
  setListMember: (id: number, jid: string, member: boolean) => invoke<void>("set_list_member", { id, jid, member }),
  storage: () => invoke<StorageInfo>("storage_info"),
  clearMedia: (jid?: string) => invoke<void>("storage_clear_media", { jid: jid ?? null }),
  clearAvatars: () => invoke<void>("storage_clear_avatars"),
  deleteOlder: (days: number) => invoke<number>("storage_delete_older", { days }),
  updateAuto: () => invoke<boolean>("get_update_auto"),
  setUpdateAuto: (enabled: boolean) => invoke<void>("set_update_auto", { enabled }),
  checkUpdate: () => invoke<Release | null>("check_update"),
  installUpdate: (version: string) => invoke<void>("install_update", { version }),
  version: () => invoke<string>("app_version"),
};

export function on<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  if (!native) return Promise.resolve(() => {});
  return listen<T>(event, (e) => handler(e.payload));
}
