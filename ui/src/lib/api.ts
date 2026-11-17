import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** True inside the Tauri webview; false in a plain browser preview. */
const native = "__TAURI_INTERNALS__" in window;

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
}

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
  sendText: (jid: string, text: string) => invoke<Message>("send_text", { jid, text }),
  logout: () => invoke<void>("logout"),
};

export function on<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  if (!native) return Promise.resolve(() => {});
  return listen<T>(event, (e) => handler(e.payload));
}
