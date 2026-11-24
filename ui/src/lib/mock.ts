// Browser preview data, used only by `vite dev` outside Tauri so the design
// can be reviewed in a plain browser. Never part of a production build.
import type { Chat, Message, Note, Status } from "./api";

const now = Math.floor(Date.now() / 1000);
const view = new URLSearchParams(location.search).get("view") ?? "chats";

const base = { auto_reply: false, instructions: "", archived: false, pinned: 0, muted_until: 0, marked_unread: false, last_sender: "", last_from_me: false };
const chats: Chat[] = [
  { ...base, jid: "5491155550001@s.whatsapp.net", name: "Ana Suárez", is_group: false, last_ts: now - 90, last_preview: "Dale, nos vemos a las 8 entonces", unread: 2, pinned: now },
  { ...base, jid: "120363000000000001@g.us", name: "Fútbol de los jueves", is_group: true, last_ts: now - 2400, last_preview: "Falta uno para completar", unread: 5, auto_reply: true, muted_until: -1, last_sender: "Lucía R." },
  { ...base, jid: "5491155550002@s.whatsapp.net", name: "", is_group: false, last_ts: now - 90000, last_preview: "[document]", unread: 0 },
  { ...base, jid: "5491155550003@s.whatsapp.net", name: "Mamá", is_group: false, last_ts: now - 260000, last_preview: "[voice]", unread: 0, marked_unread: true },
  { ...base, jid: "120363000000000002@g.us", name: "Depto Palermo · expensas y arreglos del edificio", is_group: true, last_ts: now - 400000, last_preview: "Ya transferí lo de este mes, mañana paso el comprobante por el grupo así lo tienen todos", unread: 0, last_from_me: true },
  { ...base, jid: "5491155550004@s.whatsapp.net", name: "Martín (trabajo)", is_group: false, last_ts: now - 600000, last_preview: "Te paso el informe el lunes", unread: 0 },
  { ...base, jid: "5491155550005@s.whatsapp.net", name: "Viejo chat", is_group: false, last_ts: now - 9000000, last_preview: "Gracias!", unread: 1, archived: true },
];

const lists = [{ id: 1, name: "Familia", jids: [chats[3].jid] }, { id: 2, name: "Trabajo", jids: [chats[5].jid, chats[4].jid] }, { id: 3, name: "Fútbol", jids: [chats[1].jid] }];

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
    quote_id: "",
    quote_sender: "",
    quote_text: "",
    quote_kind: "",
    forwarded: false,
    edited: false,
    deleted: false,
    starred: false,
    pinned: 0,
    reactions: [],
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
    m("5", chats[0].jid, 400, "Perfecto. Llevo algo para tomar, ¿hace falta algo más? Puedo pasar por el súper de camino si me avisás antes de las 6.", true, { reactions: [{ emoji: "❤️", sender_jid: chats[0].jid, from_me: false, name: "Ana Suárez", phone: "+5491155550001" }] }),
    m("5b", chats[0].jid, 390, "Ah, y el código del portón es 4521", true, { edited: true, starred: true }),
    m("6", chats[0].jid, 200, "Llego 20:15, estoy saliendo de una reunión.", true, { by_ai: true }),
    m("6b", chats[0].jid, 150, "", false, { deleted: true }),
    m("7", chats[0].jid, 90, "Dale, nos vemos a las 8 entonces", false, { quote_id: "6", quote_sender: "", quote_text: "Llego 20:15, estoy saliendo de una reunión.", quote_kind: "text", reactions: [{ emoji: "👍", sender_jid: "me", from_me: true, name: "", phone: "" }, { emoji: "👍", sender_jid: chats[0].jid, from_me: false, name: "Ana Suárez", phone: "+5491155550001" }] }),
  ],
  [chats[1].jid]: [
    m("2c", chats[1].jid, 86400 * 2, "¿Qué día jugamos la semana que viene?", false, {
      kind: "poll",
      sender_jid: "5491155550009@s.whatsapp.net",
      sender_name: "Tomás",
      poll: {
        selectable: 1,
        options: [
          { name: "Martes", voters: [], mine: false },
          { name: "Jueves", voters: [{ jid: "5491155550008@s.whatsapp.net", user: "5491155550008", name: "Lucía R.", phone: "+5491155550008", saved: true }], mine: true },
          { name: "Sábado", voters: [{ jid: "77120@lid", user: "77120", name: "~Nacho", phone: "+5491155550010", saved: false }], mine: false },
        ],
      },
      reactions: [
        { emoji: "🔥", sender_jid: "5491155550008@s.whatsapp.net", from_me: false, name: "Lucía R.", phone: "+5491155550008" },
        { emoji: "👍", sender_jid: "77120@lid", from_me: false, name: "~Nacho", phone: "+5491155550010" },
        { emoji: "👍", sender_jid: "me", from_me: true, name: "", phone: "" },
      ],
    }),
    m("1", chats[1].jid, 3000, "¿Quién juega hoy?", false, { sender_jid: "5491155550009@s.whatsapp.net", sender_name: "Tomás" }),
    m("2", chats[1].jid, 2900, "Yo voy", true),
    m("2b", chats[1].jid, 2600, "", false, { kind: "sticker", has_media: true, sender_jid: "5491155550008@s.whatsapp.net", sender_name: "Lucía R." }),
    m("3", chats[1].jid, 2400, "Falta uno para completar", false, { sender_jid: "5491155550008@s.whatsapp.net", sender_name: "Lucía R.", forwarded: true }),
    m("3b", chats[1].jid, 2390, "Alguien que traiga pelota", false, { sender_jid: "5491155550008@s.whatsapp.net", sender_name: "Lucía R." }),
    m("3d", chats[1].jid, 2350, "Les paso la cancha: https://maps.app.goo.gl/futbol5palermo", false, { sender_jid: "5491155550009@s.whatsapp.net", sender_name: "Tomás" }),
    m("3e", chats[1].jid, 2320, "", false, { kind: "viewonce", mime: "image/*", sender_jid: "5491155550008@s.whatsapp.net", sender_name: "Lucía R." }),
    m("3c", chats[1].jid, 2300, "@5491155550009 vos tenías una, ¿no? Si no @48291 la trae", false, {
      sender_jid: "77120@lid",
      sender_name: "~Nacho",
      sender_phone: "+5491155550010",
      mentions: [
        { jid: "5491155550009@s.whatsapp.net", user: "5491155550009", name: "Tomás", phone: "+5491155550009", saved: true },
        { jid: "48291@lid", user: "48291", name: "~Fede", phone: "", saved: false },
      ],
    }),
  ],
};

const notes: Note[] = [
  { id: 1, text: "Código del portón: 4521", color: "yellow", pinned: true, done: false, ts: now - 86400 * 3, kind: "text", mime: "", file_name: "", size: 0 },
  { id: 2, text: "Comprar: pan, queso, tomates, yerba", color: "green", pinned: false, done: false, ts: now - 86400, kind: "text", mime: "", file_name: "", size: 0 },
  { id: 3, text: "Turno dentista jueves 10:30", color: "pink", pinned: false, done: true, ts: now - 7200, kind: "text", mime: "", file_name: "", size: 0 },
  { id: 4, text: "", color: "", pinned: false, done: false, ts: now - 3600, kind: "document", mime: "application/pdf", file_name: "Contrato alquiler 2026.pdf", size: 1240000 },
  { id: 5, text: "Ideas para el finde: feria de San Telmo, cine, asado en lo de Tomás", color: "blue", pinned: false, done: false, ts: now - 600, kind: "text", mime: "", file_name: "", size: 0 },
];

const sticker = (h: number) =>
  `data:image/svg+xml;utf8,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><circle cx="50" cy="50" r="44" fill="hsl(${h} 80% 62%)" stroke="#fff" stroke-width="5"/><circle cx="36" cy="42" r="6" fill="#222"/><circle cx="64" cy="42" r="6" fill="#222"/><path d="M32 60 Q50 78 68 60" stroke="#222" stroke-width="6" fill="none" stroke-linecap="round"/></svg>`)}`;

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
  { id: 2, role: "tool", content: "read_chat", ts: now - 118, meta: JSON.stringify({ chat: { jid: chats[0].jid, name: "Ana Suárez", group: false }, count: 30 }) },
  { id: 3, role: "assistant", content: "Ana te preguntó si ibas el viernes y quedaron en verse a las 8. Te pasó la dirección por foto.", ts: now - 110, meta: "" },
  { id: 4, role: "tool", content: "send_message", ts: now - 108, meta: "" },
  { id: 5, role: "action", content: "", ts: now - 107, meta: JSON.stringify({ chat_jid: chats[0].jid, chat_name: "Ana Suárez", text: "Llego 20:15, estoy saliendo de una reunión.", state: "pending" }) },
  { id: 6, role: "assistant", content: "Te dejé el mensaje listo para que lo confirmes.", ts: now - 100, meta: "" },
  { id: 7, role: "user", content: "¿Qué hay de nuevo? Y @Fútbol de los jueves qué dicen", ts: now - 80, meta: "" },
  { id: 8, role: "tool", content: "list_unread", ts: now - 78, meta: JSON.stringify({ chats: [{ jid: chats[0].jid, name: "Ana Suárez", unread: 2, group: false, preview: "" }, { jid: chats[1].jid, name: "Fútbol de los jueves", unread: 5, group: true, preview: "" }] }) },
  { id: 9, role: "tool", content: "web_search", ts: now - 76, meta: JSON.stringify({ query: "pronóstico Buenos Aires jueves", sources: [{ n: 1, title: "Pronóstico extendido para Buenos Aires", url: "https://www.smn.gob.ar/pronostico" }, { n: 2, title: "Lluvias el jueves por la noche en AMBA", url: "https://www.lanacion.com.ar/sociedad/clima-jueves" }] }) },
  { id: 10, role: "assistant", content: "### Sin leer\n- **Ana Suárez** — confirma la cena del viernes a las 8.\n- **Fútbol de los jueves** — falta uno para completar el partido de las 21; *Lucía* pregunta quién lleva pelota.\n\n### El jueves\nSe esperan lluvias por la noche [2] y unos 14 °C a las 21 [1], así que conviene confirmar la cancha techada.\n\n¿Querés que haga algo?\n[[Responder a Ana]] [[Avisar al grupo]] [[Ver todos los no leídos]]", ts: now - 70, meta: "" },
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
    case "mark_chats_read":
      for (const c of chats) if ((args.jids as string[]).includes(c.jid)) Object.assign(c, { unread: 0, marked_unread: false });
      return null;
    case "get_full_history":
      return true;
    case "avatar": {
      const i = chats.findIndex((c) => c.jid === args.jid);
      return i >= 0 && i % 3 !== 2 ? photo(i * 47 + 20) : null;
    }
    case "media":
      return args.id === "2b" ? sticker(40) : landscape;
    case "notes_list":
      return notes;
    case "note_add":
      return { id: Date.now(), text: args.text, color: args.color, pinned: false, done: false, ts: Math.floor(Date.now() / 1000), kind: "text", mime: "", file_name: "", size: 0 };
    case "stickers":
      return Array.from({ length: 14 }, (_, i) => ({ id: String(i).padStart(64, "a"), favorite: i < 2, last_used: now - i }));
    case "sticker_image":
      return sticker(Number.parseInt(String(args.id).slice(-2), 10) * 26);
    case "pinned_messages":
      return args.jid === chats[0].jid ? [messages[chats[0].jid][6]] : [];
    case "get_ai_ports":
      return { ollama: 11434, lm_studio: 1234, llama_cpp: 8080 };
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
          { jid: "5491155550009@s.whatsapp.net", name: "Tomás", phone: "+5491155550009", admin: true, me: false },
          { jid: "5491155550008@s.whatsapp.net", name: "Lucía R.", phone: "+5491155550008", admin: false, me: false },
          { jid: "5491155550001@s.whatsapp.net", name: "Ana Suárez", phone: "+5491155550001", admin: false, me: false },
          { jid: "77120@lid", name: "~Nacho", phone: "+5491155550010", admin: false, me: false },
          { jid: "5491155550022@s.whatsapp.net", name: "", phone: "+5491155550022", admin: false, me: false },
          { jid: "5491155550000@s.whatsapp.net", name: "", phone: "+5491155550000", admin: false, me: true },
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
