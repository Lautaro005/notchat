// Browser preview data, used only by `vite dev` outside Tauri so the design
// can be reviewed in a plain browser. Never part of a production build.
import type { Chat, Message, Status } from "./api";

const now = Math.floor(Date.now() / 1000);
const view = new URLSearchParams(location.search).get("view") ?? "chats";

const base = { auto_reply: false, instructions: "", archived: false, pinned: 0, muted_until: 0, marked_unread: false };
const chats: Chat[] = [
  { ...base, jid: "5491155550001@s.whatsapp.net", name: "Ana Suárez", is_group: false, last_ts: now - 90, last_preview: "Dale, nos vemos a las 8 entonces", unread: 2, pinned: now },
  { ...base, jid: "120363000000000001@g.us", name: "Fútbol de los jueves", is_group: true, last_ts: now - 2400, last_preview: "Falta uno para completar", unread: 5, auto_reply: true, muted_until: -1 },
  { ...base, jid: "5491155550002@s.whatsapp.net", name: "", is_group: false, last_ts: now - 90000, last_preview: "[document]", unread: 0 },
  { ...base, jid: "5491155550003@s.whatsapp.net", name: "Mamá", is_group: false, last_ts: now - 260000, last_preview: "[voice]", unread: 0, marked_unread: true },
  { ...base, jid: "120363000000000002@g.us", name: "Depto Palermo · expensas", is_group: true, last_ts: now - 400000, last_preview: "Ya transferí lo de este mes", unread: 0 },
  { ...base, jid: "5491155550004@s.whatsapp.net", name: "Martín (trabajo)", is_group: false, last_ts: now - 600000, last_preview: "Te paso el informe el lunes", unread: 0 },
  { ...base, jid: "5491155550005@s.whatsapp.net", name: "Viejo chat", is_group: false, last_ts: now - 9000000, last_preview: "Gracias!", unread: 1, archived: true },
];

const lists = [{ id: 1, name: "Familia", jids: [chats[3].jid] }, { id: 2, name: "Trabajo", jids: [chats[5].jid, chats[4].jid] }];

const photo = (h: number) =>
  `data:image/svg+xml;utf8,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 120"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${h} 45% 62%)"/><stop offset="1" stop-color="hsl(${h + 40} 40% 38%)"/></linearGradient></defs><rect width="120" height="120" fill="url(#g)"/><circle cx="60" cy="48" r="22" fill="rgba(255,255,255,.75)"/><ellipse cx="60" cy="112" rx="40" ry="30" fill="rgba(255,255,255,.75)"/></svg>`)}`;
const landscape = `data:image/svg+xml;utf8,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 300"><rect width="400" height="300" fill="#e9c9a4"/><circle cx="300" cy="80" r="36" fill="#f6e7c8"/><path d="M0 220 L110 120 L190 200 L260 140 L400 250 L400 300 L0 300Z" fill="#7a5a44"/><path d="M0 250 L140 180 L250 260 L400 210 L400 300 L0 300Z" fill="#4c3a2e"/></svg>`)}`;

function m(id: string, chat: string, ago: number, text: string, from_me = false, extra: Partial<Message> = {}): Message {
  return {
    id,
    chat_jid: chat,
    sender_jid: from_me ? "" : chat,
    sender_name: "",
    from_me,
    ts: now - ago,
    kind: "text",
    text,
    by_ai: false,
    mime: "",
    file_name: "",
    size: 0,
    seconds: 0,
    has_media: false,
    ...extra,
  };
}

const messages: Record<string, Message[]> = {
  [chats[0].jid]: [
    m("1", chats[0].jid, 90500, "¿Al final venís el viernes?"),
    m("2", chats[0].jid, 90400, "Sí, salgo del trabajo a las 7 y voy directo", true),
    m("3", chats[0].jid, 700, "Te mando la dirección", false),
    m("4", chats[0].jid, 690, "", false, { kind: "image", has_media: true, size: 182000, mime: "image/jpeg" }),
    m("4b", chats[0].jid, 650, "", true, { kind: "voice", has_media: true, seconds: 14, mime: "audio/ogg" }),
    m("4c", chats[0].jid, 600, "", false, { kind: "document", has_media: true, size: 1240000, file_name: "Contrato alquiler 2026.pdf", mime: "application/pdf" }),
    m("5", chats[0].jid, 400, "Perfecto. Llevo algo para tomar, ¿hace falta algo más? Puedo pasar por el súper de camino si me avisás antes de las 6.", true),
    m("6", chats[0].jid, 200, "Llego 20:15, estoy saliendo de una reunión.", true, { by_ai: true }),
    m("7", chats[0].jid, 90, "Dale, nos vemos a las 8 entonces"),
  ],
  [chats[1].jid]: [
    m("1", chats[1].jid, 3000, "¿Quién juega hoy?", false, { sender_jid: "5491155550009@s.whatsapp.net", sender_name: "Tomás" }),
    m("2", chats[1].jid, 2900, "Yo voy", true),
    m("3", chats[1].jid, 2400, "Falta uno para completar", false, { sender_jid: "5491155550008@s.whatsapp.net", sender_name: "Lucía R." }),
  ],
};

const qr = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 29 29" shape-rendering="crispEdges"><rect width="29" height="29" fill="#fff"/><path fill="#111" d="${Array.from({ length: 25 * 25 }, (_, i) => ((i * 7919) % 11 < 5 ? `M${2 + (i % 25)} ${2 + Math.floor(i / 25)}h1v1h-1z` : "")).join("")}"/></svg>`;

const status: Status =
  view === "pair"
    ? { link: "pairing", qr_svg: qr, pair_code: null, error: null }
    : view === "code"
      ? { link: "pairing", qr_svg: qr, pair_code: "K7QM2XPD", error: null }
      : view === "empty"
        ? { link: "connected", qr_svg: null, pair_code: null, error: null }
        : { link: "connected", qr_svg: null, pair_code: null, error: null };

const aiRows = [
  { id: 1, role: "user", content: "¿Qué me dijo Ana sobre el viernes? Y decile que llego 20:15.", ts: now - 120, meta: "" },
  { id: 2, role: "tool", content: "read_chat", ts: now - 118, meta: "" },
  { id: 3, role: "assistant", content: "Ana te preguntó si ibas el viernes y quedaron en verse a las 8. Te pasó la dirección por foto.", ts: now - 110, meta: "" },
  { id: 4, role: "tool", content: "send_message", ts: now - 108, meta: "" },
  { id: 5, role: "action", content: "", ts: now - 107, meta: JSON.stringify({ chat_jid: chats[0].jid, chat_name: "Ana Suárez", text: "Llego 20:15, estoy saliendo de una reunión.", state: "pending" }) },
  { id: 6, role: "assistant", content: "Te dejé el mensaje listo para que lo confirmes.", ts: now - 100, meta: "" },
];

const providers = [
  { kind: "ollama", name: "Ollama", base_url: "http://127.0.0.1:11434", installed: true, running: true, models: ["gemma3:4b", "qwen3:4b", "llama3.2:3b"] },
  { kind: "lm_studio", name: "LM Studio", base_url: "http://127.0.0.1:1234", installed: true, running: false, models: [] },
  { kind: "llama_cpp", name: "llama.cpp", base_url: "http://127.0.0.1:8080", installed: false, running: false, models: [] },
];

export async function mockInvoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  switch (cmd) {
    case "get_status":
      return status;
    case "list_chats":
      return view === "empty" ? [] : chats;
    case "list_messages":
      return args.beforeTs ? [] : (messages[args.jid as string] ?? []);
    case "send_text":
      return m(String(Math.random()), args.jid as string, 0, args.text as string, true);
    case "list_lists":
      return lists;
    case "avatar": {
      const i = chats.findIndex((c) => c.jid === args.jid);
      return i >= 0 && i % 3 !== 2 ? photo(i * 47 + 20) : null;
    }
    case "media":
      return landscape;
    case "app_version":
      return "0.1.2";
    case "get_update_auto":
      return true;
    case "check_update":
      return { version: "0.1.3", notes: "Mejoras de estabilidad.", url: "" };
    case "group_info":
      return {
        subject: "Fútbol de los jueves",
        description: "Jueves 21 h en la cancha de Juan B. Justo. Traer pechera.",
        created: now - 86400 * 400,
        i_am_admin: false,
        participants: [
          { jid: "5491155550009@s.whatsapp.net", name: "Tomás", admin: true },
          { jid: "5491155550008@s.whatsapp.net", name: "Lucía R.", admin: false },
          { jid: "5491155550001@s.whatsapp.net", name: "Ana Suárez", admin: false },
        ],
      };
    case "storage_info":
      return {
        database: 48_200_000,
        media: 612_000_000,
        avatars: 3_400_000,
        session: 9_800_000,
        chats: [
          { jid: chats[1].jid, name: chats[1].name, messages: 18452, media: 1_200_000_000, cached: 380_000_000 },
          { jid: chats[0].jid, name: chats[0].name, messages: 6021, media: 240_000_000, cached: 120_000_000 },
          { jid: chats[3].jid, name: chats[3].name, messages: 3310, media: 90_000_000, cached: 0 },
        ],
      };
    case "managed_view":
      return { server_installed: false, running_model: null, busy: false, ram_gb: 16, models: [
        { id: "qwen3.5-0.8b-q8", name: "Qwen3.5 0.8B", note: "The lightest. Short replies, any Mac.", size: 833592096, min_ram_gb: 8, recommended: false, installed: false },
        { id: "gemma-4-e2b-q4", name: "Gemma 4 E2B", note: "Recommended. Good balance of quality and speed.", size: 2841481184, min_ram_gb: 8, recommended: true, installed: false },
        { id: "qwen3.5-4b-q4", name: "Qwen3.5 4B", note: "Better at using tools (chats, web).", size: 3032091584, min_ram_gb: 16, recommended: false, installed: false },
      ] };
    case "get_notify_mode":
      return "notch";
    case "detect_providers":
      return providers;
    case "get_ai_config":
      return view === "noai"
        ? { provider: null, base_url: "", model: "", confirm_send: true, web_access: true }
        : { provider: "ollama", base_url: "http://127.0.0.1:11434", model: "gemma3:4b", confirm_send: true, web_access: true };
    case "ai_history":
      return view === "noai" || view === "aiempty" ? [] : aiRows;
    case "get_auto_reply":
      return { enabled: view === "auto", chats: "selected", groups: "none", groups_only_mentions: true, max_per_chat_per_hour: 6, min_delay_secs: 4, max_delay_secs: 12, quiet_from: null, quiet_to: null, instructions: "" };
    default:
      return null;
  }
}
