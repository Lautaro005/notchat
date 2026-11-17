import { api, on, type AiConfig, type AiRow, type AutoReply, type Chat, type Message, type Status } from "./api";
import { locale } from "./i18n";

/** Id used in `app.selected` for the assistant conversation. */
export const AI = "ai";

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

async function refreshChats() {
  app.chats = await api.chats();
}

export async function selectChat(jid: string) {
  app.selected = jid;
  app.messages = [];
  if (jid === AI) return;
  const page = await api.messages(jid);
  if (app.selected !== jid) return;
  app.messages = page;
  app.hasOlder = page.length === PAGE;
  app.tail++;
  await api.markRead(jid);
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
  append(await api.sendText(jid, text));
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
  await on<null>("chats-changed", () => void refreshChats());
  app.status = await api.status();
  await refreshChats();
}
