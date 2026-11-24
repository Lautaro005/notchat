//! WhatsApp session: pairing, receiving and sending. Wraps `whatsapp-rust`
//! and persists everything into the encrypted [`Store`].

use crate::error::{Error, Result};
use crate::keychain::is_valid_key;
use crate::media::{check_avatar_url, MAX_MEDIA_BYTES};
use crate::store::{restrict_permissions, HistoryChat, Message, PollDef, Store};
use diesel::RunQueryDsl;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;
use whatsapp_rust::buffa::Message as _;
use whatsapp_rust::download::MediaType;
use whatsapp_rust::prelude::*;
use whatsapp_rust::upload::UploadOptions;
use whatsapp_rust_sqlite_storage::{SqliteStore, SqliteStoreConfig};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WaEvent {
    /// A pairing QR code, already rendered to SVG.
    Qr {
        svg: String,
        timeout_secs: u64,
    },
    PairCode {
        code: String,
    },
    Connected,
    Disconnected,
    LoggedOut,
    /// A new message was stored. `mentions_me` is only meaningful in groups.
    Message {
        message: Box<Message>,
        is_group: bool,
        mentions_me: bool,
        live: bool,
    },
    ChatUpdated {
        jid: String,
    },
    /// Many chats changed at once (history import, group list).
    ChatsChanged,
    /// History import progress, 0–100.
    HistoryProgress {
        progress: u32,
    },
    PictureChanged {
        jid: String,
    },
    /// A stored message changed (reaction, edit, deletion, star, pin).
    MessageChanged {
        chat_jid: String,
        id: String,
    },
}

pub struct WaSession {
    inner: WaClient,
    handle: Option<BotHandle>,
}

/// A cheap, cloneable handle for talking to WhatsApp, so slow work (uploads,
/// downloads) does not hold the session lock.
#[derive(Clone)]
pub struct WaClient {
    client: Arc<Client>,
    store: Arc<Store>,
}

impl std::ops::Deref for WaSession {
    type Target = WaClient;
    fn deref(&self) -> &WaClient {
        &self.inner
    }
}

/// What a group looks like to the info panel.
#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct GroupInfo {
    pub subject: String,
    pub description: String,
    pub created: i64,
    pub participants: Vec<Participant>,
    pub i_am_admin: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Participant {
    pub jid: String,
    /// Contact-book name, or `~` + their WhatsApp name; empty when unknown.
    pub name: String,
    /// `+` and digits; empty when WhatsApp only gave a private id.
    pub phone: String,
    pub admin: bool,
    /// True for this account.
    pub me: bool,
}

/// Media kinds Notchat can send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SendKind {
    Image,
    Video,
    Audio,
    Voice,
    Document,
}

/// A file to send, as received from the webview.
pub struct Outgoing {
    pub kind: SendKind,
    pub data: Vec<u8>,
    pub mime: String,
    pub file_name: String,
    pub caption: String,
    pub seconds: u32,
}

/// Renders a QR payload to a compact SVG string. Pure Rust, no network.
pub fn qr_svg(payload: &str) -> Result<String> {
    let code = qrcode::QrCode::new(payload.as_bytes()).map_err(|e| Error::Wa(e.to_string()))?;
    Ok(code
        .render::<qrcode::render::svg::Color<'_>>()
        .min_dimensions(240, 240)
        .quiet_zone(true)
        .dark_color(qrcode::render::svg::Color("#111111"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build())
}

/// Validates a phone number for pair-code linking: digits only, 7–15 long.
pub fn normalize_phone(input: &str) -> Result<String> {
    let digits: String = input.chars().filter(char::is_ascii_digit).collect();
    if (7..=15).contains(&digits.len()) {
        Ok(digits)
    } else {
        Err(Error::Invalid("phone number".into()))
    }
}

/// A message's kind, visible text and media details.
#[derive(Debug, Default, PartialEq)]
struct Described {
    kind: &'static str,
    text: String,
    mime: String,
    file_name: String,
    size: i64,
    seconds: i64,
    /// The encoded media sub-message: everything needed to download it later.
    keys: Option<Vec<u8>>,
    quote_id: String,
    quote_sender: String,
    quote_text: String,
    quote_kind: String,
    forwarded: bool,
    /// JIDs mentioned with `@` in the text.
    mentions: Vec<String>,
    /// A poll's options (`kind = "poll"`).
    poll: Option<PollDef>,
}

impl Described {
    /// Copies the details into a stored message.
    fn fill(self, m: &mut Message) -> Option<Vec<u8>> {
        m.kind = self.kind.to_string();
        m.text = self.text;
        m.mime = self.mime;
        m.file_name = self.file_name;
        m.size = self.size;
        m.seconds = self.seconds;
        m.has_media = self.keys.is_some();
        m.quote_id = self.quote_id;
        m.quote_sender = self.quote_sender;
        m.quote_text = self.quote_text;
        m.quote_kind = self.quote_kind;
        m.forwarded = self.forwarded;
        m.mention_jids = self.mentions;
        m.poll_def = self.poll.map(|p| p.to_json()).unwrap_or_default();
        self.keys
    }
}

/// The reply/forward context of a message, whatever its kind.
fn context_of(base: &wa::Message) -> Option<&wa::ContextInfo> {
    base.extended_text_message
        .as_option()
        .and_then(|m| m.context_info.as_option())
        .or_else(|| {
            base.image_message
                .as_option()
                .and_then(|m| m.context_info.as_option())
        })
        .or_else(|| {
            base.video_message
                .as_option()
                .and_then(|m| m.context_info.as_option())
        })
        .or_else(|| {
            base.audio_message
                .as_option()
                .and_then(|m| m.context_info.as_option())
        })
        .or_else(|| {
            base.document_message
                .as_option()
                .and_then(|m| m.context_info.as_option())
        })
        .or_else(|| {
            base.sticker_message
                .as_option()
                .and_then(|m| m.context_info.as_option())
        })
        .or_else(|| poll_of(base).and_then(|m| m.context_info.as_option()))
}

/// The poll a message creates, whichever version of the field carries it.
fn poll_of(base: &wa::Message) -> Option<&wa::message::PollCreationMessage> {
    base.poll_creation_message
        .as_option()
        .or_else(|| base.poll_creation_message_v2.as_option())
        .or_else(|| base.poll_creation_message_v3.as_option())
        .or_else(|| base.poll_creation_message_v5.as_option())
        .or_else(|| base.poll_creation_message_v6.as_option())
}

/// The secret a message's votes (or edits) are encrypted with.
fn message_secret(message: &wa::Message) -> Option<Vec<u8>> {
    let base = message.get_base_message();
    [message, base]
        .into_iter()
        .filter_map(|m| m.message_context_info.as_option())
        .find_map(|c| c.message_secret.clone())
        .filter(|s| !s.is_empty())
}

/// Adds the quoted message and forward flag to a description.
fn with_context(mut d: Described, base: &wa::Message) -> Described {
    if let Some(ctx) = context_of(base) {
        d.forwarded = ctx.is_forwarded == Some(true);
        d.mentions = ctx.mentioned_jid.clone();
        if let (Some(id), Some(quoted)) = (&ctx.stanza_id, ctx.quoted_message.as_option()) {
            let q = describe_body(quoted);
            d.quote_id = id.clone();
            d.quote_sender = ctx.participant.clone().unwrap_or_default();
            d.quote_text = q.text.chars().take(300).collect();
            d.quote_kind = q.kind.to_string();
        }
    }
    d
}

fn size_of(len: Option<u64>) -> i64 {
    len.map_or(0, |l| i64::try_from(l).unwrap_or(i64::MAX))
}

/// Classifies a message and extracts its visible text, media keys and
/// reply context.
fn describe(message: &wa::Message) -> Described {
    let base = message.get_base_message();
    let mut d = with_context(describe_body(base), base);
    // View-once media can only be opened on the phone: keep the message as
    // a placeholder (with any caption) instead of the media itself.
    if message.is_view_once() && d.kind != "text" {
        d.mime = match d.kind {
            "video" => "video/*".into(),
            "audio" | "voice" => "audio/*".into(),
            _ => "image/*".into(),
        };
        d.kind = "viewonce";
        d.keys = None;
        d.file_name.clear();
        d.size = 0;
    }
    if let (Some(poll), Some(secret)) = (d.poll.as_mut(), message_secret(message)) {
        poll.secret = hex::encode(secret);
    }
    d
}

fn describe_body(message: &wa::Message) -> Described {
    let base = message.get_base_message();
    if let Some(text) = message.text_content() {
        return Described {
            kind: "text",
            text: text.to_string(),
            ..Default::default()
        };
    }
    if let Some(m) = base.image_message.as_option() {
        return Described {
            kind: "image",
            text: m.caption.clone().unwrap_or_default(),
            mime: m.mimetype.clone().unwrap_or_default(),
            size: size_of(m.file_length),
            keys: Some(m.encode_to_vec()),
            ..Default::default()
        };
    }
    if let Some(m) = base.video_message.as_option() {
        return Described {
            kind: "video",
            text: m.caption.clone().unwrap_or_default(),
            mime: m.mimetype.clone().unwrap_or_default(),
            size: size_of(m.file_length),
            seconds: i64::from(m.seconds.unwrap_or(0)),
            keys: Some(m.encode_to_vec()),
            ..Default::default()
        };
    }
    if let Some(m) = base.audio_message.as_option() {
        return Described {
            kind: if m.ptt == Some(true) {
                "voice"
            } else {
                "audio"
            },
            mime: m.mimetype.clone().unwrap_or_default(),
            size: size_of(m.file_length),
            seconds: i64::from(m.seconds.unwrap_or(0)),
            keys: Some(m.encode_to_vec()),
            ..Default::default()
        };
    }
    if let Some(m) = base.document_message.as_option() {
        return Described {
            kind: "document",
            text: m.caption.clone().unwrap_or_default(),
            mime: m.mimetype.clone().unwrap_or_default(),
            file_name: m
                .file_name
                .clone()
                .or_else(|| m.title.clone())
                .unwrap_or_default(),
            size: size_of(m.file_length),
            keys: Some(m.encode_to_vec()),
            ..Default::default()
        };
    }
    if let Some(m) = base.sticker_message.as_option() {
        return Described {
            kind: "sticker",
            mime: m.mimetype.clone().unwrap_or_default(),
            size: size_of(m.file_length),
            keys: Some(m.encode_to_vec()),
            ..Default::default()
        };
    }
    if let Some(p) = poll_of(base) {
        let options: Vec<String> = p
            .options
            .iter()
            .filter_map(|o| o.option_name.clone())
            .filter(|o| !o.is_empty())
            .collect();
        if !options.is_empty() {
            return Described {
                kind: "poll",
                text: p.name.clone().unwrap_or_default(),
                poll: Some(PollDef {
                    options,
                    selectable: p.selectable_options_count.unwrap_or(0),
                    secret: String::new(),
                }),
                ..Default::default()
            };
        }
    }
    Described {
        kind: "other",
        ..Default::default()
    }
}

fn mentions(message: &wa::Message, me: &[String]) -> bool {
    let base = message.get_base_message();
    let Some(ext) = base.extended_text_message.as_option() else {
        return false;
    };
    let Some(ctx) = ext.context_info.as_option() else {
        return false;
    };
    ctx.mentioned_jid
        .iter()
        .any(|j| me.iter().any(|m| user_part(j) == user_part(m)))
}

/// The user portion of a JID string, without device or server suffix.
pub fn user_part(jid: &str) -> &str {
    let user = jid.split('@').next().unwrap_or(jid);
    user.split(':').next().unwrap_or(user)
}

/// Status broadcasts and newsletters are not chats.
fn is_chat(jid: &str) -> bool {
    !(jid.is_empty() || jid.ends_with("@broadcast") || jid.ends_with("@newsletter"))
}

/// Normalizes a mute end time from the phone: milliseconds or seconds,
/// with huge or negative values meaning "forever". Returns Unix seconds,
/// `-1` for forever and `0` for not muted.
pub fn mute_seconds(raw: i64) -> i64 {
    if !(0..=100_000_000_000_000).contains(&raw) {
        -1
    } else if raw > 100_000_000_000 {
        raw / 1000
    } else {
        raw
    }
}

fn parse_jid(jid: &str) -> Result<Jid> {
    jid.parse().map_err(|_| Error::Invalid("chat id".into()))
}

fn app_state_err(e: impl std::fmt::Display) -> Error {
    Error::Wa(e.to_string())
}

impl WaSession {
    /// Starts the session. Emits a QR code when the device is not linked yet;
    /// pass `phone` to additionally request a pair code.
    pub async fn start(
        session_db: &Path,
        key_hex: &str,
        store: Arc<Store>,
        phone: Option<String>,
        events: UnboundedSender<WaEvent>,
    ) -> Result<Self> {
        if !is_valid_key(key_hex) {
            return Err(Error::Invalid("database key".into()));
        }
        let pragma = format!("PRAGMA key = \"x'{key_hex}'\";");
        let config = SqliteStoreConfig::default().with_connection_init(move |conn| {
            diesel::sql_query(pragma.as_str()).execute(conn)?;
            diesel::sql_query("SELECT count(*) FROM sqlite_master;").execute(conn)?;
            Ok(())
        });
        let url = session_db
            .to_str()
            .ok_or_else(|| Error::Invalid("session path".into()))?;
        let backend = SqliteStore::with_config(url, config)
            .await
            .map_err(|e| Error::Wa(e.to_string()))?;
        restrict_permissions(session_db);

        let tx_qr = events.clone();
        let tx_pair = events.clone();
        let tx_ev = events;
        let store_ev = Arc::clone(&store);

        // Ask the phone for the whole history when linking. It only applies
        // to new links; older ones fill in on demand (`backfill_history`).
        let props = whatsapp_rust::wacore::store::device::DevicePropsOverride::new()
            .with_require_full_sync(true)
            .with_history_sync_config(wa::device_props::HistorySyncConfig {
                full_sync_days_limit: Some(3650),
                full_sync_size_mb_limit: Some(4096),
                storage_quota_mb: Some(16384),
                on_demand_ready: Some(true),
                complete_on_demand_ready: Some(true),
                ..whatsapp_rust::wacore::store::device::default_history_sync_config()
            });
        let mut builder = Bot::builder()
            .with_backend(backend)
            .with_device_props(props)
            .on_qr_code(move |code, timeout| {
                let tx = tx_qr.clone();
                async move {
                    match qr_svg(&code) {
                        Ok(svg) => {
                            let _ = tx.send(WaEvent::Qr {
                                svg,
                                timeout_secs: timeout.as_secs(),
                            });
                        }
                        Err(e) => log::warn!("qr render failed: {e}"),
                    }
                }
            })
            .on_pair_code(move |code, _timeout| {
                let tx = tx_pair.clone();
                async move {
                    let _ = tx.send(WaEvent::PairCode { code });
                }
            })
            .on_event(move |event, client| {
                let tx = tx_ev.clone();
                let store = Arc::clone(&store_ev);
                async move { handle_event(&event, &client, &store, &tx).await }
            });

        if let Some(phone) = phone {
            builder = builder.with_pair_code(whatsapp_rust::pair_code::PairCodeOptions {
                phone_number: normalize_phone(&phone)?,
                ..Default::default()
            });
        }

        let bot = builder
            .build()
            .await
            .map_err(|e| Error::Wa(e.to_string()))?;
        let client = bot.client();
        let handle = bot.spawn();
        Ok(Self {
            inner: WaClient { client, store },
            handle: Some(handle),
        })
    }

    pub fn client(&self) -> WaClient {
        self.inner.clone()
    }

    /// Unlinks this device from the account.
    pub async fn logout(&mut self) {
        self.inner.client.logout().await;
        self.shutdown().await;
    }

    pub async fn shutdown(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.shutdown().await;
        }
    }
}

impl WaClient {
    pub fn is_logged_in(&self) -> bool {
        self.client.is_logged_in()
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_connected()
    }

    fn ready(&self) -> Result<()> {
        if self.client.is_logged_in() {
            Ok(())
        } else {
            Err(Error::NotConnected)
        }
    }

    /// Sends a text message and stores it. Returns the stored message.
    pub async fn send_text(&self, chat_jid: &str, text: &str, by_ai: bool) -> Result<Message> {
        self.send_reply(chat_jid, text, None, by_ai).await
    }

    /// Sends a text message, optionally as a reply to `reply_to` (a message
    /// id in the same chat).
    pub async fn send_reply(
        &self,
        chat_jid: &str,
        text: &str,
        reply_to: Option<&str>,
        by_ai: bool,
    ) -> Result<Message> {
        let text = text.trim();
        if text.is_empty() || text.len() > 16_000 {
            return Err(Error::Invalid("message text".into()));
        }
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let body = match reply_to.map(|id| self.quote_context(chat_jid, &jid, id)) {
            Some(ctx) => wa::Message::text_with_context(text, ctx?),
            None => wa::Message::text(text),
        };
        self.send_and_store(chat_jid, &jid, body, by_ai).await
    }

    /// Builds the context that makes a message a reply to `id`.
    fn quote_context(&self, chat_jid: &str, jid: &Jid, id: &str) -> Result<wa::ContextInfo> {
        let original = self
            .store
            .get_message(chat_jid, id)?
            .ok_or_else(|| Error::Invalid("quoted message".into()))?;
        let quoted = match self.store.get_raw(chat_jid, id)? {
            Some(raw) => wa::Message::decode_from_slice(&raw)
                .map_err(|_| Error::Invalid("quoted message".into()))?,
            None => wa::Message::text(original.text.clone()),
        };
        let sender = if original.from_me {
            self.client
                .pn()
                .map(|j| j.to_non_ad_string())
                .unwrap_or_default()
        } else if original.sender_jid.is_empty() {
            chat_jid.to_string()
        } else {
            original.sender_jid.clone()
        };
        let sender: Jid = sender
            .parse()
            .map_err(|_| Error::Invalid("quoted sender".into()))?;
        Ok(whatsapp_rust::proto_helpers::build_quote_context_with_info(
            id, &sender, jid, jid, &quoted,
        ))
    }

    /// Sends any message body and stores what was sent.
    async fn send_and_store(
        &self,
        chat_jid: &str,
        jid: &Jid,
        body: wa::Message,
        by_ai: bool,
    ) -> Result<Message> {
        let raw = body.encode_to_vec();
        let described = describe(&body);
        let sent = self
            .client
            .send_message(jid, body)
            .await
            .map_err(|e| Error::Wa(e.to_string()))?;
        let mut message = Message {
            id: sent.message_id.to_string(),
            chat_jid: chat_jid.to_string(),
            from_me: true,
            ts: chrono::Utc::now().timestamp(),
            by_ai,
            ..Default::default()
        };
        let keys = described.fill(&mut message);
        mark_own_quote(&mut message, &own_jids(&self.client));
        if message.kind == "sticker" {
            self.remember_sticker(keys.as_deref(), message.ts);
        }
        self.store.insert_message_full(
            &message,
            chat_jid.ends_with("@g.us"),
            keys.as_deref(),
            Some(&raw),
            true,
        )?;
        Ok(message)
    }

    /// The key WhatsApp uses to point at a stored message.
    fn key_of(&self, chat_jid: &str, id: &str) -> Result<(Message, wa::MessageKey)> {
        let m = self
            .store
            .get_message(chat_jid, id)?
            .ok_or_else(|| Error::Invalid("message".into()))?;
        let participant = (chat_jid.ends_with("@g.us") && !m.from_me && !m.sender_jid.is_empty())
            .then(|| m.sender_jid.clone());
        let key = wa::MessageKey {
            remote_jid: Some(chat_jid.to_string()),
            from_me: Some(m.from_me),
            id: Some(id.to_string()),
            participant,
        };
        Ok((m, key))
    }

    /// Reacts to a message; an empty emoji removes our reaction.
    pub async fn react(&self, chat_jid: &str, id: &str, emoji: &str) -> Result<()> {
        if emoji.chars().count() > 8 {
            return Err(Error::Invalid("reaction".into()));
        }
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let (_, key) = self.key_of(chat_jid, id)?;
        self.client
            .send_reaction(jid, key, emoji)
            .await
            .map_err(|e| Error::Wa(e.to_string()))?;
        self.store.set_reaction(
            chat_jid,
            id,
            "me",
            true,
            emoji,
            chrono::Utc::now().timestamp(),
        )
    }

    /// Votes in a poll with the chosen option names; none clears our vote.
    pub async fn vote_poll(&self, chat_jid: &str, id: &str, options: &[String]) -> Result<()> {
        let (def, from_me, sender) = self
            .store
            .poll_def(chat_jid, id)?
            .ok_or_else(|| Error::Invalid("poll".into()))?;
        if options.iter().any(|o| !def.options.contains(o))
            || (def.selectable > 0 && options.len() > def.selectable as usize)
        {
            return Err(Error::Invalid("poll choice".into()));
        }
        let secret = hex::decode(&def.secret)
            .ok()
            .filter(|s| s.len() == 32)
            .ok_or_else(|| Error::Invalid("this poll can only be answered on your phone".into()))?;
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let creator = if from_me {
            // Groups address polls by our private id when WhatsApp knows it.
            let own = if chat_jid.ends_with("@g.us") {
                self.client.lid().or_else(|| self.client.pn())
            } else {
                self.client.pn()
            };
            own.map(|j| j.to_non_ad())
                .ok_or_else(|| Error::Wa("not logged in".into()))?
        } else if sender.is_empty() {
            parse_jid(chat_jid)?
        } else {
            parse_jid(&sender)?
        };
        self.client
            .polls()
            .vote(jid, id, &creator, &secret, options)
            .await
            .map_err(|e| Error::Wa(e.to_string()))?;
        self.store
            .set_poll_vote(chat_jid, id, "me", options, chrono::Utc::now().timestamp())
    }

    /// Edits one of our own text messages.
    pub async fn edit(&self, chat_jid: &str, id: &str, text: &str) -> Result<()> {
        let text = text.trim();
        if text.is_empty() || text.len() > 16_000 {
            return Err(Error::Invalid("message text".into()));
        }
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let (m, _) = self.key_of(chat_jid, id)?;
        if !m.from_me || m.kind != "text" || m.deleted {
            return Err(Error::Invalid(
                "only your own text messages can be edited".into(),
            ));
        }
        self.client
            .edit_message(jid, id, wa::Message::text(text))
            .await
            .map_err(|e| Error::Wa(e.to_string()))?;
        self.store.edit_message(chat_jid, id, text)?;
        Ok(())
    }

    /// Deletes a message for everyone: ours as sender, others' as group admin.
    pub async fn revoke(&self, chat_jid: &str, id: &str) -> Result<()> {
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let (m, _) = self.key_of(chat_jid, id)?;
        let kind = if m.from_me {
            whatsapp_rust::send::RevokeType::Sender
        } else {
            let original_sender = parse_jid(&m.sender_jid)?;
            whatsapp_rust::send::RevokeType::Admin { original_sender }
        };
        self.client
            .revoke_message(jid, id, kind)
            .await
            .map_err(|e| Error::Wa(e.to_string()))?;
        self.store.mark_deleted(chat_jid, id)?;
        Ok(())
    }

    /// Deletes a message on this account's devices only.
    pub async fn delete_for_me(&self, chat_jid: &str, id: &str) -> Result<()> {
        let (m, _) = self.key_of(chat_jid, id)?;
        self.store.delete_message(chat_jid, id)?;
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let participant = participant_of(chat_jid, &m)?;
        self.client
            .chat_actions()
            .delete_message_for_me(&jid, participant.as_ref(), id, m.from_me, true, Some(m.ts))
            .await
            .map_err(app_state_err)
    }

    /// Stars or unstars a message on every device.
    pub async fn star(&self, chat_jid: &str, id: &str, starred: bool) -> Result<()> {
        let (m, _) = self.key_of(chat_jid, id)?;
        self.store.set_starred(chat_jid, id, starred)?;
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let participant = participant_of(chat_jid, &m)?;
        let actions = self.client.chat_actions();
        if starred {
            actions
                .star_message(&jid, participant.as_ref(), id, m.from_me)
                .await
        } else {
            actions
                .unstar_message(&jid, participant.as_ref(), id, m.from_me)
                .await
        }
        .map_err(app_state_err)
    }

    /// Pins a message for everyone in the chat (seven days) or unpins it.
    pub async fn pin_message(&self, chat_jid: &str, id: &str, pinned: bool) -> Result<()> {
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let (_, key) = self.key_of(chat_jid, id)?;
        if pinned {
            self.client
                .pin_message(jid, key, whatsapp_rust::send::PinDuration::Days7)
                .await
        } else {
            self.client.unpin_message(jid, key).await
        }
        .map_err(|e| Error::Wa(e.to_string()))?;
        let ts = if pinned {
            chrono::Utc::now().timestamp()
        } else {
            0
        };
        self.store.set_message_pinned(chat_jid, id, ts)
    }

    /// Forwards a stored message to another chat, reusing its media.
    pub async fn forward(&self, from_chat: &str, id: &str, to_chat: &str) -> Result<Message> {
        self.ready()?;
        let to = parse_jid(to_chat)?;
        let original = self
            .store
            .get_message(from_chat, id)?
            .ok_or_else(|| Error::Invalid("message".into()))?;
        let source = match self.store.get_raw(from_chat, id)? {
            Some(raw) => wa::Message::decode_from_slice(&raw)
                .map_err(|_| Error::Invalid("message".into()))?,
            None if original.kind == "text" && !original.text.is_empty() => {
                wa::Message::text(original.text.clone())
            }
            None => return Err(Error::Invalid("this message can't be forwarded".into())),
        };
        use whatsapp_rust::proto_helpers::MessageExt;
        let body = *source.get_base_message().prepare_for_forward();
        self.send_and_store(to_chat, &to, body, false).await
    }

    /// Sends a sticker from the account's sticker list.
    pub async fn send_sticker(&self, chat_jid: &str, sticker_id: &str) -> Result<Message> {
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let media = self
            .store
            .get_sticker(sticker_id)?
            .ok_or_else(|| Error::Invalid("sticker".into()))?;
        let sticker = wa::message::StickerMessage::decode_from_slice(&media)
            .map_err(|_| Error::Invalid("sticker".into()))?;
        let body = wa::Message {
            sticker_message: MessageField::some(sticker),
            ..Default::default()
        };
        self.send_and_store(chat_jid, &jid, body, false).await
    }

    /// Downloads a sticker from the account's sticker list.
    pub async fn download_sticker(&self, sticker_id: &str) -> Result<Vec<u8>> {
        let media = self
            .store
            .get_sticker(sticker_id)?
            .ok_or_else(|| Error::Invalid("sticker".into()))?;
        self.download("sticker", &media).await
    }

    fn remember_sticker(&self, keys: Option<&[u8]>, ts: i64) {
        remember_sticker(&self.store, keys, ts);
    }

    /// Uploads and sends a file. Returns the stored message; the caller keeps
    /// the bytes in the media cache so it shows without a download.
    pub async fn send_media(&self, chat_jid: &str, out: Outgoing) -> Result<Message> {
        if out.data.is_empty() || out.data.len() > MAX_MEDIA_BYTES {
            return Err(Error::Invalid("file size".into()));
        }
        if out.caption.len() > 4000 || out.file_name.len() > 255 || out.mime.len() > 100 {
            return Err(Error::Invalid("file details".into()));
        }
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let size = i64::try_from(out.data.len()).unwrap_or(i64::MAX);
        let media_type = match out.kind {
            SendKind::Image => MediaType::Image,
            SendKind::Video => MediaType::Video,
            SendKind::Audio | SendKind::Voice => MediaType::Audio,
            SendKind::Document => MediaType::Document,
        };
        let upload = self
            .client
            .upload(out.data, media_type, UploadOptions::new())
            .await
            .map_err(|e| Error::Wa(e.to_string()))?;
        let caption = (!out.caption.trim().is_empty()).then(|| out.caption.trim().to_string());
        let mime = (!out.mime.is_empty()).then(|| out.mime.clone());
        use whatsapp_rust::media;
        let message = match out.kind {
            SendKind::Image => media::image_message(
                upload,
                media::ImageOptions {
                    caption: caption.clone(),
                    mimetype: mime,
                    ..Default::default()
                },
            ),
            SendKind::Video => media::video_message(
                upload,
                media::VideoOptions {
                    caption: caption.clone(),
                    mimetype: mime,
                    duration_seconds: Some(out.seconds),
                    ..Default::default()
                },
            ),
            SendKind::Audio | SendKind::Voice => media::audio_message(
                upload,
                media::AudioOptions {
                    mimetype: mime,
                    duration_seconds: Some(out.seconds),
                    ptt: Some(out.kind == SendKind::Voice),
                    ..Default::default()
                },
            ),
            SendKind::Document => media::document_message(
                upload,
                media::DocumentOptions {
                    mimetype: mime,
                    file_name: Some(out.file_name.clone()),
                    title: Some(out.file_name.clone()),
                    caption: caption.clone(),
                    ..Default::default()
                },
            ),
        };
        let mut stored = self.send_and_store(chat_jid, &jid, message, false).await?;
        stored.size = size;
        Ok(stored)
    }

    /// Downloads and decrypts a stored message's media.
    pub async fn download(&self, kind: &str, keys: &[u8]) -> Result<Vec<u8>> {
        self.ready()?;
        let bad = |_| Error::Invalid("media keys".into());
        let result = match kind {
            "image" => {
                let m = wa::message::ImageMessage::decode_from_slice(keys).map_err(bad)?;
                self.client.download(&m).await
            }
            "video" => {
                let m = wa::message::VideoMessage::decode_from_slice(keys).map_err(bad)?;
                self.client.download(&m).await
            }
            "audio" | "voice" => {
                let m = wa::message::AudioMessage::decode_from_slice(keys).map_err(bad)?;
                self.client.download(&m).await
            }
            "document" => {
                let m = wa::message::DocumentMessage::decode_from_slice(keys).map_err(bad)?;
                self.client.download(&m).await
            }
            "sticker" => {
                let m = wa::message::StickerMessage::decode_from_slice(keys).map_err(bad)?;
                self.client.download(&m).await
            }
            _ => return Err(Error::Invalid("media kind".into())),
        };
        let bytes = result.map_err(|e| Error::Wa(e.to_string()))?;
        if bytes.len() > MAX_MEDIA_BYTES {
            return Err(Error::Invalid("file size".into()));
        }
        Ok(bytes)
    }

    /// Fetches a chat's profile picture thumbnail from WhatsApp's CDN.
    /// `Ok(None)` when there is no picture or it is private.
    pub async fn profile_picture(&self, chat_jid: &str) -> Result<Option<Vec<u8>>> {
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let picture = self
            .client
            .contacts()
            .get_profile_picture_with_timeout(&jid, true, Some(Duration::from_secs(10)))
            .await
            .map_err(|e| Error::Wa(e.to_string()))?;
        let Some(picture) = picture else {
            return Ok(None);
        };
        let url = check_avatar_url(&picture.url)?;
        let response = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| Error::Net(e.to_string()))?
            .get(url)
            .send()
            .await
            .map_err(|e| Error::Net(e.to_string()))?
            .error_for_status()
            .map_err(|e| Error::Net(e.to_string()))?;
        let bytes = response
            .bytes()
            .await
            .map_err(|e| Error::Net(e.to_string()))?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err(Error::Invalid("picture size".into()));
        }
        Ok(Some(bytes.to_vec()))
    }

    pub async fn group_info(&self, chat_jid: &str) -> Result<GroupInfo> {
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let meta = self
            .client
            .groups()
            .get_metadata(&jid)
            .await
            .map_err(|e| Error::Wa(e.to_string()))?;
        learn_participants(&meta.participants, &self.store);
        let me = own_jids(&self.client);
        let mut i_am_admin = false;
        let participants = meta
            .participants
            .iter()
            .map(|p| {
                let jid = p
                    .phone_number
                    .as_ref()
                    .map_or_else(|| p.jid.to_string(), ToString::to_string);
                let admin = p.is_admin();
                let is_me = me
                    .iter()
                    .any(|m| user_part(m) == user_part(&jid) || user_part(m) == p.jid.user);
                if is_me {
                    i_am_admin = admin;
                }
                let person = self.store.person(&jid, "");
                // `person` falls back to the phone number: only keep real names.
                let name = if person.name == person.phone {
                    String::new()
                } else {
                    person.name
                };
                Participant {
                    jid,
                    name,
                    phone: person.phone,
                    admin,
                    me: is_me,
                }
            })
            .collect();
        Ok(GroupInfo {
            subject: meta.subject,
            description: meta.description.unwrap_or_default(),
            created: meta
                .creation_time
                .map_or(0, |t| i64::try_from(t).unwrap_or(0)),
            participants,
            i_am_admin,
        })
    }

    pub async fn set_archived(&self, chat_jid: &str, archived: bool) -> Result<()> {
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let actions = self.client.chat_actions();
        if archived {
            actions.archive_chat(&jid, None).await
        } else {
            actions.unarchive_chat(&jid, None).await
        }
        .map_err(app_state_err)
    }

    pub async fn set_pinned(&self, chat_jid: &str, pinned: bool) -> Result<()> {
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let actions = self.client.chat_actions();
        if pinned {
            actions.pin_chat(&jid).await
        } else {
            actions.unpin_chat(&jid).await
        }
        .map_err(app_state_err)
    }

    /// `until`: Unix seconds, `-1` forever, `0` unmute.
    pub async fn set_muted(&self, chat_jid: &str, until: i64) -> Result<()> {
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let actions = self.client.chat_actions();
        match until {
            0 => actions.unmute_chat(&jid).await,
            u if u < 0 => actions.mute_chat(&jid).await,
            u => actions.mute_chat_until(&jid, u.saturating_mul(1000)).await,
        }
        .map_err(app_state_err)
    }

    pub async fn set_read(&self, chat_jid: &str, read: bool) -> Result<()> {
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        self.client
            .chat_actions()
            .mark_chat_as_read(&jid, read, None)
            .await
            .map_err(app_state_err)
    }

    pub async fn clear_chat(&self, chat_jid: &str) -> Result<()> {
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        self.client
            .chat_actions()
            .clear_chat(&jid, false, true, None)
            .await
            .map_err(app_state_err)
    }

    pub async fn delete_chat(&self, chat_jid: &str) -> Result<()> {
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        self.client
            .chat_actions()
            .delete_chat(&jid, true, None)
            .await
            .map_err(app_state_err)
    }

    pub async fn leave_group(&self, chat_jid: &str) -> Result<()> {
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        self.client
            .groups()
            .leave(jid)
            .await
            .map_err(|e| Error::Wa(e.to_string()))
    }

    /// Reconnects right away if the connection dropped (sleep, network
    /// change), so receipts and messages waiting on the server arrive now.
    pub async fn ensure_connected(&self) {
        if self.client.is_logged_in() && !self.client.is_connected() {
            self.client.reconnect_immediately().await;
        }
    }

    /// Asks the phone for up to `count` messages older than the oldest one
    /// stored for this chat. They arrive later as an on-demand history
    /// chunk. Returns false when there is nothing to anchor the request on.
    pub async fn request_older(&self, chat_jid: &str, count: i32) -> Result<bool> {
        self.ready()?;
        let Some(oldest) = self.store.oldest_message(chat_jid)? else {
            return Ok(false);
        };
        let jid = parse_jid(chat_jid)?;
        self.client
            .fetch_message_history(
                &jid,
                &oldest.id,
                oldest.from_me,
                oldest.ts.saturating_mul(1000),
                count.clamp(1, 100),
            )
            .await
            .map_err(|e| Error::Wa(e.to_string()))?;
        Ok(true)
    }

    /// Fetches older history for every chat, newest chats first, one request
    /// at a time with pauses so the phone is never flooded. A chat is done
    /// when the phone stops sending anything older. Reports `(done, total)`.
    pub async fn backfill_history(
        &self,
        progress: &(dyn Fn(usize, usize) + Send + Sync),
    ) -> Result<()> {
        const ROUNDS: usize = 40;
        const WAIT: Duration = Duration::from_secs(12);
        let mut pending: Vec<String> = self
            .store
            .list_chats()?
            .into_iter()
            .map(|c| c.jid)
            .filter(|j| !self.store.history_complete(j))
            .collect();
        let total = pending.len();
        progress(0, total);
        for _ in 0..ROUNDS {
            if pending.is_empty() {
                break;
            }
            let mut again = Vec::new();
            let mut done_now = 0;
            let remaining = pending.len();
            for jid in pending {
                let before = self.store.oldest_message(&jid)?.map(|o| o.ts);
                let mut grew = false;
                if self.request_older(&jid, 100).await? {
                    let start = std::time::Instant::now();
                    while start.elapsed() < WAIT {
                        tokio::time::sleep(Duration::from_millis(600)).await;
                        let now = self.store.oldest_message(&jid)?.map(|o| o.ts);
                        if now < before {
                            grew = true;
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                }
                if grew {
                    again.push(jid);
                } else {
                    self.store.set_history_complete(&jid)?;
                    done_now += 1;
                }
                progress(total - remaining + done_now, total);
            }
            pending = again;
        }
        for jid in &pending {
            self.store.set_history_complete(jid)?;
        }
        progress(total, total);
        Ok(())
    }

    /// Tells the contact we are typing (or stopped).
    pub async fn set_typing(&self, chat_jid: &str, typing: bool) -> Result<()> {
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let chatstate = self.client.chatstate();
        if typing {
            chatstate.send_composing(&jid).await
        } else {
            chatstate.send_paused(&jid).await
        }
        .map_err(|e| Error::Wa(e.to_string()))
    }
}

async fn handle_event(
    event: &Event,
    client: &Arc<Client>,
    store: &Arc<Store>,
    tx: &UnboundedSender<WaEvent>,
) {
    let changed = |jid: &Jid| {
        let _ = tx.send(WaEvent::ChatUpdated {
            jid: jid.to_string(),
        });
    };
    match event {
        Event::Connected(_) => {
            let _ = tx.send(WaEvent::Connected);
            match store.repair_own_mappings(&own_jids(client)) {
                Ok(n) if n > 0 => {
                    log::info!("repaired {n} wrong address mappings");
                    let _ = tx.send(WaEvent::ChatsChanged);
                }
                Ok(_) => {}
                Err(e) => log::warn!("mapping repair failed: {e}"),
            }
            let client = Arc::clone(client);
            let store = Arc::clone(store);
            let tx = tx.clone();
            tokio::spawn(async move {
                sync_groups(&client, &store, &tx).await;
                learn_known_addresses(&client, &store, &tx).await;
            });
        }
        Event::Disconnected(_) => {
            let _ = tx.send(WaEvent::Disconnected);
        }
        Event::LoggedOut(_) => {
            let _ = tx.send(WaEvent::LoggedOut);
        }
        Event::Messages(batch) => {
            let me = own_jids(client);
            for inbound in batch.iter() {
                on_message(inbound, client, store, tx, &me).await;
            }
        }
        // Linked devices never receive view-once media, only a notice that
        // it exists. Show it as a placeholder so the message isn't lost.
        Event::UndecryptableMessage(u)
            if u.is_unavailable
                && matches!(
                    u.unavailable_type,
                    whatsapp_rust::wacore::types::events::UnavailableType::ViewOnce
                ) =>
        {
            store_view_once(&u.info, store, tx);
        }
        Event::HistorySync(lazy) => {
            let progress = lazy.progress();
            if let Some(history) = lazy.get() {
                let imported = import_history(history, store, &own_jids(client));
                log::info!("history chunk imported: {imported} messages");
            }
            if let Some(progress) = progress {
                let _ = tx.send(WaEvent::HistoryProgress { progress });
            }
            let _ = tx.send(WaEvent::ChatsChanged);
        }
        Event::Receipt(receipt)
            if receipt.r#type == whatsapp_rust::wacore::types::presence::ReceiptType::ReadSelf =>
        {
            // Read on the phone (or another linked device). In one-to-one
            // chats the receipt names our own address, not the chat: the
            // chat is found from the message ids instead.
            let chat = store.canonical_jid(&receipt.source.chat.to_string());
            let is_me = own_jids(client)
                .iter()
                .any(|j| user_part(j) == user_part(&chat));
            let ids: Vec<String> = receipt
                .message_ids
                .iter()
                .map(ToString::to_string)
                .collect();
            let hint = (!is_me).then_some(chat.as_str());
            if let Ok(changed) = store.mark_read_elsewhere(hint, &ids) {
                for jid in changed {
                    let _ = tx.send(WaEvent::ChatUpdated { jid });
                }
            }
        }
        Event::ContactUpdate(update) => {
            for (lid, pn) in [
                (update.action.lid_jid.clone(), update.action.pn_jid.clone()),
                (update.action.lid_jid.clone(), Some(update.jid.to_string())),
                (Some(update.jid.to_string()), update.action.pn_jid.clone()),
            ] {
                if let (Some(lid), Some(pn)) = (lid, pn) {
                    let _ = store.map_lid(&lid, &pn);
                }
            }
            let name = update
                .action
                .full_name
                .clone()
                .or_else(|| update.action.first_name.clone())
                .unwrap_or_default();
            if store
                .upsert_contact(&update.jid.to_string(), &name, false)
                .is_ok()
                && !update.from_full_sync
            {
                changed(&update.jid);
            }
        }
        Event::PushNameUpdate(update) => {
            if store
                .upsert_contact(&update.jid.to_string(), &update.new_push_name, true)
                .is_ok()
            {
                changed(&update.jid);
            }
        }
        Event::PinUpdate(update) => {
            let pinned = if update.action.pinned == Some(true) {
                update.timestamp.timestamp().max(1)
            } else {
                0
            };
            if store.set_pinned(&update.jid.to_string(), pinned).is_ok() {
                changed(&update.jid);
            }
        }
        Event::ArchiveUpdate(update) => {
            let archived = update.action.archived == Some(true);
            if store
                .set_archived(&update.jid.to_string(), archived)
                .is_ok()
            {
                changed(&update.jid);
            }
        }
        Event::MuteUpdate(update) => {
            let until = if update.action.muted == Some(true) {
                mute_seconds(update.action.mute_end_timestamp.unwrap_or(-1))
            } else {
                0
            };
            if store
                .set_muted_until(&update.jid.to_string(), until)
                .is_ok()
            {
                changed(&update.jid);
            }
        }
        Event::MarkChatAsReadUpdate(update) => {
            let jid = update.jid.to_string();
            let result = if update.action.read == Some(true) {
                store.mark_read(&jid)
            } else {
                store.set_marked_unread(&jid, true)
            };
            if result.is_ok() {
                changed(&update.jid);
            }
        }
        Event::StarUpdate(update) => {
            let chat = update.chat_jid.to_string();
            let starred = update.action.starred == Some(true);
            if store
                .set_starred(&chat, &update.message_id, starred)
                .is_ok()
                && !update.from_full_sync
            {
                let _ = tx.send(WaEvent::MessageChanged {
                    chat_jid: chat,
                    id: update.message_id.clone(),
                });
            }
        }
        Event::DeleteMessageForMeUpdate(update) => {
            let chat = update.chat_jid.to_string();
            if store.delete_message(&chat, &update.message_id).is_ok() && !update.from_full_sync {
                let _ = tx.send(WaEvent::MessageChanged {
                    chat_jid: chat,
                    id: update.message_id.clone(),
                });
            }
        }
        Event::PictureUpdate(update) => {
            let _ = tx.send(WaEvent::PictureChanged {
                jid: update.jid.to_string(),
            });
        }
        Event::GroupUpdate(update) => {
            if let Ok(meta) = client.groups().get_metadata(&update.group_jid).await {
                learn_participants(&meta.participants, store);
                let _ = store.upsert_group(&update.group_jid.to_string(), &meta.subject, 0);
                changed(&update.group_jid);
            }
        }
        _ => {}
    }
}

/// Makes every group the account belongs to show in the list, even without
/// recent messages.
async fn sync_groups(client: &Arc<Client>, store: &Arc<Store>, tx: &UnboundedSender<WaEvent>) {
    match client.groups().get_participating().await {
        Ok(groups) => {
            for (jid, meta) in &groups {
                learn_participants(&meta.participants, store);
                let ts = meta
                    .subject_time
                    .or(meta.creation_time)
                    .map_or(1, |t| i64::try_from(t).unwrap_or(1));
                if let Err(e) = store.upsert_group(&jid.to_string(), &meta.subject, ts) {
                    log::warn!("failed to store group: {e}");
                }
            }
            log::info!("groups synced: {}", groups.len());
            let _ = tx.send(WaEvent::ChatsChanged);
        }
        Err(e) => log::warn!("group list failed: {e}"),
    }
}

/// Stores one history-sync chunk: chat state, push names and messages.
/// Returns the number of new messages.
fn import_history(history: &wa::HistorySync, store: &Store, me: &[String]) -> usize {
    for m in &history.phone_number_to_lid_mappings {
        if let (Some(pn), Some(lid)) = (&m.pn_jid, &m.lid_jid) {
            let _ = store.map_lid(lid, pn);
        }
    }
    for c in &history.inline_contacts {
        if let (Some(pn), Some(lid)) = (&c.pn_jid, &c.lid_jid) {
            let _ = store.map_lid(lid, pn);
        }
        let name = c.full_name.clone().or_else(|| c.first_name.clone());
        if let (Some(jid), Some(name)) = (c.pn_jid.as_ref().or(c.lid_jid.as_ref()), name) {
            let _ = store.upsert_contact(jid, &name, false);
        }
    }
    for p in &history.pushnames {
        if let (Some(id), Some(name)) = (&p.id, &p.pushname) {
            let _ = store.upsert_contact(id, name, true);
        }
    }
    for st in &history.recent_stickers {
        let sticker = wa::message::StickerMessage {
            url: st.url.clone(),
            file_sha256: st.file_sha256.clone(),
            file_enc_sha256: st.file_enc_sha256.clone(),
            media_key: st.media_key.clone(),
            mimetype: st.mimetype.clone(),
            height: st.height,
            width: st.width,
            direct_path: st.direct_path.clone(),
            file_length: st.file_length,
            is_lottie: st.is_lottie,
            ..Default::default()
        };
        let used = st.last_sticker_sent_ts.unwrap_or(0) / 1000;
        remember_sticker(store, Some(&sticker.encode_to_vec()), used);
    }
    let mut imported = 0;
    for conv in &history.conversations {
        let lid = conv.lid_jid.clone().filter(|_| !conv.id.ends_with("@lid"));
        if let (Some(pn), Some(lid)) = (&conv.pn_jid, lid.as_ref().or(Some(&conv.id))) {
            let _ = store.map_lid(lid, pn);
        }
        // Prefer the phone-number address so names from the contact book match.
        let jid = match (&conv.pn_jid, conv.id.ends_with("@lid")) {
            (Some(pn), true) if !pn.is_empty() => pn.clone(),
            _ => conv.id.clone(),
        };
        if !is_chat(&jid) {
            continue;
        }
        let is_group = jid.ends_with("@g.us");
        let ts = conv
            .conversation_timestamp
            .or(conv.last_msg_timestamp)
            .map_or(0, |t| i64::try_from(t).unwrap_or(0));
        let on_demand = history.sync_type == wa::history_sync::HistorySyncType::ON_DEMAND;
        let chat = HistoryChat {
            jid: jid.clone(),
            name: conv
                .name
                .clone()
                .or_else(|| conv.display_name.clone())
                .unwrap_or_default(),
            is_group,
            last_ts: ts,
            unread: i64::from(conv.unread_count.unwrap_or(0)),
            archived: conv.archived == Some(true),
            pinned: i64::from(conv.pinned.unwrap_or(0)),
            muted_until: conv
                .mute_end_time
                .map_or(0, |t| mute_seconds(i64::try_from(t).unwrap_or(-1))),
            marked_unread: conv.marked_as_unread == Some(true),
        };
        // Older history asked for on demand carries no chat state: keep ours.
        if !on_demand {
            if let Err(e) = store.apply_history_chat(&chat) {
                log::warn!("failed to store chat: {e}");
                continue;
            }
        }
        for item in &conv.messages {
            let Some(info) = item.message.as_option() else {
                continue;
            };
            let Some(body) = info.message.as_option() else {
                continue;
            };
            let Some(key) = info.key.as_option() else {
                continue;
            };
            let Some(id) = key.id.clone() else {
                continue;
            };
            let d = describe(body);
            if d.kind == "other" {
                continue;
            }
            let raw = body.get_base_message().encode_to_vec();
            let from_me = key.from_me == Some(true);
            let sender = if from_me {
                String::new()
            } else if is_group {
                key.participant
                    .clone()
                    .or_else(|| info.participant.clone())
                    .unwrap_or_default()
            } else {
                jid.clone()
            };
            let mut message = Message {
                id,
                chat_jid: jid.clone(),
                sender_jid: sender,
                sender_name: info.push_name.clone().unwrap_or_default(),
                from_me,
                ts: info
                    .message_timestamp
                    .map_or(0, |t| i64::try_from(t).unwrap_or(0)),
                starred: info.starred == Some(true),
                ..Default::default()
            };
            let mut d = d;
            if let (Some(poll), Some(secret)) = (d.poll.as_mut(), &info.message_secret) {
                if poll.secret.is_empty() && !secret.is_empty() {
                    poll.secret = hex::encode(secret);
                }
            }
            let options = d.poll.clone();
            let keys = d.fill(&mut message);
            mark_own_quote(&mut message, me);
            if message.kind == "sticker" {
                remember_sticker(store, keys.as_deref(), message.ts);
            }
            if let Some(def) = options {
                for u in &info.poll_updates {
                    let (Some(key), Some(vote)) =
                        (u.poll_update_message_key.as_option(), u.vote.as_option())
                    else {
                        continue;
                    };
                    let who = if key.from_me == Some(true) {
                        "me".to_string()
                    } else if is_group {
                        key.participant.clone().unwrap_or_default()
                    } else {
                        jid.clone()
                    };
                    if who.is_empty() {
                        continue;
                    }
                    let ts = u.sender_timestamp_ms.unwrap_or(0) / 1000;
                    let picked = def.names_for(&vote.selected_options);
                    let _ = store.set_poll_vote(&jid, &message.id, &who, &picked, ts);
                }
            }
            for r in &info.reactions {
                if let (Some(key), Some(emoji)) = (r.key.as_option(), &r.text) {
                    let mine = key.from_me == Some(true);
                    let who = key.participant.clone().or_else(|| key.remote_jid.clone());
                    let _ = store.set_reaction(
                        &jid,
                        &message.id,
                        &who.unwrap_or_default(),
                        mine,
                        emoji,
                        r.sender_timestamp_ms.unwrap_or(0) / 1000,
                    );
                }
            }
            match store.insert_message_full(&message, is_group, keys.as_deref(), Some(&raw), false)
            {
                Ok(true) => imported += 1,
                Ok(false) => {}
                Err(e) => log::warn!("failed to store message: {e}"),
            }
        }
    }
    imported
}

/// Learns LID ↔ phone-number pairs from a message's addressing.
fn learn_addresses(src: &whatsapp_rust::wacore::types::message::MessageSource, store: &Store) {
    // Our own messages carry *our* alternate address in `sender_alt`: pairing
    // it with the chat would turn the contact into our own number.
    let incoming_dm = !src.is_group && !src.is_from_me;
    let pairs = [
        (Some(&src.sender), src.sender_alt.as_ref()),
        (Some(&src.chat), src.recipient_alt.as_ref()),
        (
            Some(&src.chat),
            src.sender_alt.as_ref().filter(|_| incoming_dm),
        ),
    ];
    for (a, b) in pairs {
        let (Some(a), Some(b)) = (a, b) else { continue };
        let (a, b) = (a.to_non_ad_string(), b.to_non_ad_string());
        let _ = if a.ends_with("@lid") {
            store.map_lid(&a, &b)
        } else {
            store.map_lid(&b, &a)
        };
    }
}

/// Asks the library's own address book for the phone numbers of LIDs we
/// have seen, and the LIDs of unnamed phone-number chats, so names stay
/// consistent wherever WhatsApp used the other address.
async fn learn_known_addresses(
    client: &Arc<Client>,
    store: &Arc<Store>,
    tx: &UnboundedSender<WaEvent>,
) {
    let mut learned = 0;
    let lids = store.unmapped_lids(5000).unwrap_or_default();
    let pns = store.unnamed_pn_chats(2000).unwrap_or_default();
    for jid in lids.iter().chain(pns.iter()) {
        let Ok(parsed) = jid.parse::<Jid>() else {
            continue;
        };
        if let Ok(Some(entry)) = client.get_lid_pn_entry(&parsed).await {
            let lid = format!("{}@lid", entry.lid);
            let pn = format!("{}@s.whatsapp.net", entry.phone_number);
            if store.map_lid(&lid, &pn).is_ok() {
                learned += 1;
            }
        }
    }
    let moved = store.move_strays(&own_jids(client)).unwrap_or(0);
    if learned > 0 || moved > 0 {
        log::info!("learned {learned} address mappings, moved {moved} messages");
        let _ = tx.send(WaEvent::ChatsChanged);
    }
}

fn learn_participants(list: &[whatsapp_rust::GroupParticipant], store: &Store) {
    for p in list {
        let lid = p.lid.as_ref().or(Some(&p.jid).filter(|j| j.is_lid()));
        let pn = p
            .phone_number
            .as_ref()
            .or(Some(&p.jid).filter(|j| !j.is_lid()));
        if let (Some(lid), Some(pn)) = (lid, pn) {
            let _ = store.map_lid(&lid.to_string(), &pn.to_string());
        }
    }
}

/// Replies to our own messages carry our JID (phone or LID); store "me" so
/// the interface can say "You" without knowing our addresses.
fn mark_own_quote(m: &mut Message, me: &[String]) {
    if !m.quote_sender.is_empty()
        && me
            .iter()
            .any(|j| user_part(j) == user_part(&m.quote_sender))
    {
        m.quote_sender = "me".into();
    }
}

fn own_jids(client: &Arc<Client>) -> Vec<String> {
    [client.pn(), client.lid()]
        .into_iter()
        .flatten()
        .map(|jid| jid.to_string())
        .collect()
}

async fn on_message(
    inbound: &InboundMessage,
    client: &Arc<Client>,
    store: &Arc<Store>,
    tx: &UnboundedSender<WaEvent>,
    me: &[String],
) {
    let info = &inbound.info;
    learn_addresses(&info.source, store);
    let chat = store.canonical_jid(&info.source.chat.to_string());
    if !is_chat(&chat) {
        return;
    }
    let base = inbound.message.get_base_message();
    if let Some(update) = base.poll_update_message.as_option() {
        apply_vote(update, info, &chat, client, store, tx).await;
        return;
    }
    if apply_change(base, info, &chat, store, tx) {
        return;
    }
    let d = describe(&inbound.message);
    if d.kind == "other" {
        return;
    }
    let raw = base.encode_to_vec();
    let is_group = info.source.is_group;
    let mut message = Message {
        id: info.id.to_string(),
        chat_jid: chat.clone(),
        sender_jid: info.source.sender.to_non_ad_string(),
        sender_name: info.push_name.clone(),
        from_me: info.source.is_from_me,
        ts: info.timestamp.timestamp(),
        ..Default::default()
    };
    let keys = d.fill(&mut message);
    mark_own_quote(&mut message, me);
    if message.kind == "sticker" {
        remember_sticker(store, keys.as_deref(), message.ts);
    }
    match store.insert_message_full(&message, is_group, keys.as_deref(), Some(&raw), true) {
        Ok(true) => {}
        Ok(false) => return,
        Err(e) => {
            log::error!("failed to store message: {e}");
            return;
        }
    }
    if !info.source.is_from_me && !is_group && !info.push_name.is_empty() {
        let _ = store.upsert_contact(&chat, &info.push_name, true);
    }
    ensure_group_name(&chat, is_group, info, client, store).await;
    let _ = tx.send(WaEvent::Message {
        mentions_me: is_group && mentions(&inbound.message, me),
        message: Box::new(message),
        is_group,
        live: !info.is_offline,
    });
}

/// Decrypts a vote in a stored poll and records it.
async fn apply_vote(
    update: &wa::message::PollUpdateMessage,
    info: &MessageInfo,
    chat: &str,
    client: &Arc<Client>,
    store: &Store,
    tx: &UnboundedSender<WaEvent>,
) {
    let Some(poll_id) = update
        .poll_creation_message_key
        .as_option()
        .and_then(|k| k.id.clone())
    else {
        return;
    };
    let Some(vote) = update.vote.as_option() else {
        return;
    };
    let (Some(payload), Some(iv)) = (&vote.enc_payload, &vote.enc_iv) else {
        return;
    };
    let Ok(Some((def, from_me, sender))) = store.poll_def(chat, &poll_id) else {
        return;
    };
    let Some(secret) = hex::decode(&def.secret).ok().filter(|s| !s.is_empty()) else {
        return;
    };
    let voter = info.source.sender.to_non_ad();
    let creator = if from_me {
        let own = if voter.is_lid() {
            client.lid().or_else(|| client.pn())
        } else {
            client.pn().or_else(|| client.lid())
        };
        match own {
            Some(j) => j.to_non_ad(),
            None => return,
        }
    } else {
        let who = if sender.is_empty() {
            chat
        } else {
            sender.as_str()
        };
        match parse_jid(who) {
            Ok(j) => j,
            Err(_) => return,
        }
    };
    let hashes = match client
        .polls()
        .decrypt_vote(
            whatsapp_rust::PollVoteCiphertext {
                enc_payload: payload,
                enc_iv: iv,
            },
            &secret,
            &poll_id,
            &creator,
            &voter,
        )
        .await
    {
        Ok(h) => h,
        Err(e) => {
            log::warn!("could not read a poll vote: {e}");
            return;
        }
    };
    let who = if info.source.is_from_me {
        "me".to_string()
    } else {
        voter.to_string()
    };
    let ts = update
        .sender_timestamp_ms
        .map_or_else(|| info.timestamp.timestamp(), |ms| ms / 1000);
    if store
        .set_poll_vote(chat, &poll_id, &who, &def.names_for(&hashes), ts)
        .is_ok()
    {
        let _ = tx.send(WaEvent::MessageChanged {
            chat_jid: chat.to_string(),
            id: poll_id,
        });
    }
}

fn store_view_once(info: &MessageInfo, store: &Store, tx: &UnboundedSender<WaEvent>) {
    learn_addresses(&info.source, store);
    let chat = store.canonical_jid(&info.source.chat.to_string());
    if !is_chat(&chat) {
        return;
    }
    let is_group = info.source.is_group;
    let message = Message {
        id: info.id.to_string(),
        chat_jid: chat,
        sender_jid: info.source.sender.to_non_ad_string(),
        sender_name: info.push_name.clone(),
        from_me: info.source.is_from_me,
        ts: info.timestamp.timestamp(),
        kind: "viewonce".into(),
        mime: "image/*".into(),
        ..Default::default()
    };
    if let Ok(true) = store.insert_message_full(&message, is_group, None, None, true) {
        let _ = tx.send(WaEvent::Message {
            mentions_me: false,
            message: Box::new(message),
            is_group,
            live: !info.is_offline,
        });
    }
}

/// Applies reactions, edits and deletions to stored messages. Returns true
/// when the message was one of those (and so has nothing else to show).
fn apply_change(
    base: &wa::Message,
    info: &MessageInfo,
    chat: &str,
    store: &Store,
    tx: &UnboundedSender<WaEvent>,
) -> bool {
    let changed = |id: &str| {
        let _ = tx.send(WaEvent::MessageChanged {
            chat_jid: chat.to_string(),
            id: id.to_string(),
        });
    };
    if let Some(r) = base.reaction_message.as_option() {
        if let Some(id) = r.key.as_option().and_then(|k| k.id.clone()) {
            let emoji = r.text.clone().unwrap_or_default();
            let ts = r
                .sender_timestamp_ms
                .map_or_else(|| info.timestamp.timestamp(), |ms| ms / 1000);
            let sender = info.source.sender.to_string();
            if store
                .set_reaction(chat, &id, &sender, info.source.is_from_me, &emoji, ts)
                .is_ok()
            {
                changed(&id);
            }
        }
        return true;
    }
    if let Some(p) = base.protocol_message.as_option() {
        use wa::message::protocol_message::Type;
        let Some(id) = p.key.as_option().and_then(|k| k.id.clone()) else {
            return true;
        };
        match p.r#type {
            Some(Type::Revoke) => {
                if store.mark_deleted(chat, &id).unwrap_or(false) {
                    changed(&id);
                }
            }
            Some(Type::MESSAGE_EDIT) => {
                let text = p
                    .edited_message
                    .as_option()
                    .map(|m| describe_body(m).text)
                    .unwrap_or_default();
                if !text.is_empty() && store.edit_message(chat, &id, &text).unwrap_or(false) {
                    changed(&id);
                }
            }
            _ => {}
        }
        return true;
    }
    if let Some(pin) = base.pin_in_chat_message.as_option() {
        use wa::message::pin_in_chat_message::Type;
        if let Some(id) = pin.key.as_option().and_then(|k| k.id.clone()) {
            let ts = match pin.r#type {
                Some(Type::PinForAll) => info.timestamp.timestamp().max(1),
                _ => 0,
            };
            if store.set_message_pinned(chat, &id, ts).is_ok() {
                changed(&id);
            }
        }
        return true;
    }
    false
}

/// Adds a sticker (encoded `StickerMessage`) to the account's sticker list.
/// Saves the sticker of a received or sent message to the user's favourite
/// stickers. Returns the sticker id.
pub fn save_sticker_from_message(store: &Store, chat_jid: &str, id: &str) -> Result<String> {
    let bad = || Error::Invalid("this message has no sticker to save".into());
    let (kind, keys) = store.get_media(chat_jid, id)?.ok_or_else(bad)?;
    if kind != "sticker" {
        return Err(bad());
    }
    let ts = chrono::Utc::now().timestamp();
    let sticker_id = remember_sticker(store, Some(&keys), ts).ok_or_else(bad)?;
    store.set_sticker_favorite(&sticker_id, true)?;
    Ok(sticker_id)
}

fn remember_sticker(store: &Store, keys: Option<&[u8]>, ts: i64) -> Option<String> {
    let keys = keys?;
    let sticker = wa::message::StickerMessage::decode_from_slice(keys).ok()?;
    // Animated Lottie stickers need a player we don't ship.
    if sticker.is_lottie == Some(true) {
        return None;
    }
    let sha = sticker.file_sha256.as_deref().filter(|s| !s.is_empty())?;
    let id: String = sha.iter().map(|b| format!("{b:02x}")).collect();
    if let Err(e) = store.add_sticker(&id, keys, ts) {
        log::warn!("failed to remember sticker: {e}");
        return None;
    }
    Some(id)
}

/// The original sender, needed by app-state actions on others' group messages.
fn participant_of(chat_jid: &str, m: &Message) -> Result<Option<Jid>> {
    if chat_jid.ends_with("@g.us") && !m.from_me && !m.sender_jid.is_empty() {
        parse_jid(&m.sender_jid).map(Some)
    } else {
        Ok(None)
    }
}

async fn ensure_group_name(
    chat: &str,
    is_group: bool,
    info: &MessageInfo,
    client: &Arc<Client>,
    store: &Arc<Store>,
) {
    if !is_group {
        return;
    }
    let known = store
        .get_chat(chat)
        .ok()
        .flatten()
        .is_some_and(|c| !c.name.is_empty());
    if known {
        return;
    }
    if let Ok(meta) = client.groups().get_metadata(&info.source.chat).await {
        let _ = store.upsert_chat_name(chat, &meta.subject, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_renders_svg() {
        let svg = qr_svg("2@abc,def,ghi").unwrap();
        assert!(svg.starts_with("<?xml") || svg.starts_with("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn phone_normalization() {
        assert_eq!(
            normalize_phone("+54 9 11 5555-1234").unwrap(),
            "5491155551234"
        );
        assert!(normalize_phone("123").is_err());
        assert!(normalize_phone("abc").is_err());
    }

    #[test]
    fn user_part_strips_device_and_server() {
        assert_eq!(user_part("5491100:12@s.whatsapp.net"), "5491100");
        assert_eq!(user_part("5491100@s.whatsapp.net"), "5491100");
        assert_eq!(user_part("123@lid"), "123");
    }

    #[test]
    fn view_once_media_becomes_a_placeholder() {
        let inner = wa::Message {
            image_message: MessageField::some(wa::message::ImageMessage {
                caption: Some("mirá".into()),
                media_key: Some(vec![1; 32]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let msg = wa::Message {
            view_once_message_v2: MessageField::some(wa::message::FutureProofMessage {
                message: MessageField::some(inner),
            }),
            ..Default::default()
        };
        let d = describe(&msg);
        assert_eq!(d.kind, "viewonce");
        assert!(d.keys.is_none(), "media keys are never kept");
        assert_eq!(d.mime, "image/*");
    }

    #[test]
    fn own_messages_never_map_a_contact_to_us() {
        use whatsapp_rust::wacore::types::message::MessageSource;
        let dir = tempfile::tempdir().unwrap();
        let store =
            Store::open(&dir.path().join("t.db"), &crate::keychain::generate_key()).unwrap();
        // Sent from our phone to a contact addressed by LID: sender_alt is
        // OUR phone number, not the contact's.
        let mine = MessageSource {
            chat: "555@lid".parse().unwrap(),
            sender: "111@lid".parse().unwrap(),
            sender_alt: Some("5491100000000@s.whatsapp.net".parse().unwrap()),
            is_from_me: true,
            ..Default::default()
        };
        learn_addresses(&mine, &store);
        assert_eq!(store.canonical_jid("555@lid"), "555@lid");
        assert_eq!(
            store.canonical_jid("111@lid"),
            "5491100000000@s.whatsapp.net",
            "our own pair is still learned"
        );
        // From the contact: their own alternate address is theirs.
        let theirs = MessageSource {
            chat: "555@lid".parse().unwrap(),
            sender: "555@lid".parse().unwrap(),
            sender_alt: Some("5491155555555@s.whatsapp.net".parse().unwrap()),
            ..Default::default()
        };
        learn_addresses(&theirs, &store);
        assert_eq!(
            store.canonical_jid("555@lid"),
            "5491155555555@s.whatsapp.net"
        );
    }

    #[test]
    fn own_quotes_become_me() {
        let me = [
            "5491100:3@s.whatsapp.net".to_string(),
            "777@lid".to_string(),
        ];
        let mut m = Message {
            quote_sender: "777@lid".into(),
            ..Default::default()
        };
        mark_own_quote(&mut m, &me);
        assert_eq!(m.quote_sender, "me");
        m.quote_sender = "5491199@s.whatsapp.net".into();
        mark_own_quote(&mut m, &me);
        assert_eq!(m.quote_sender, "5491199@s.whatsapp.net");
    }

    #[test]
    fn mute_normalization() {
        assert_eq!(mute_seconds(-1), -1);
        assert_eq!(mute_seconds(1_900_000_000), 1_900_000_000);
        assert_eq!(mute_seconds(1_900_000_000_000), 1_900_000_000);
        assert_eq!(mute_seconds(i64::MAX), -1);
    }

    #[test]
    fn describes_media_and_keeps_keys() {
        let msg = wa::Message {
            document_message: MessageField::some(wa::message::DocumentMessage {
                file_name: Some("plan.pdf".into()),
                mimetype: Some("application/pdf".into()),
                file_length: Some(2048),
                media_key: Some(vec![7; 32]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let d = describe(&msg);
        assert_eq!(d.kind, "document");
        assert_eq!(d.file_name, "plan.pdf");
        assert_eq!(d.size, 2048);
        let keys = d.keys.unwrap();
        let back = wa::message::DocumentMessage::decode_from_slice(&keys).unwrap();
        assert_eq!(back.media_key, Some(vec![7; 32]));
        assert_eq!(describe(&wa::Message::text("hola")).kind, "text");
        assert_eq!(describe(&wa::Message::default()).kind, "other");
    }

    #[test]
    fn history_import_creates_chats_and_messages() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            Store::open(&dir.path().join("t.db"), &crate::keychain::generate_key()).unwrap();
        let web = |id: &str, from_me: bool, text: &str, ts: u64| wa::HistorySyncMsg {
            message: MessageField::some(wa::WebMessageInfo {
                key: MessageField::some(wa::MessageKey {
                    remote_jid: Some("1@s.whatsapp.net".into()),
                    from_me: Some(from_me),
                    id: Some(id.into()),
                    participant: None,
                }),
                message: MessageField::some(wa::Message::text(text)),
                message_timestamp: Some(ts),
                ..Default::default()
            }),
            ..Default::default()
        };
        let history = wa::HistorySync {
            conversations: vec![
                wa::Conversation {
                    id: "1@s.whatsapp.net".into(),
                    messages: vec![web("a", false, "hola", 100), web("b", true, "chau", 200)],
                    conversation_timestamp: Some(200),
                    unread_count: Some(1),
                    archived: Some(true),
                    ..Default::default()
                },
                wa::Conversation {
                    id: "status@broadcast".into(),
                    ..Default::default()
                },
            ],
            pushnames: vec![wa::Pushname {
                id: Some("1@s.whatsapp.net".into()),
                pushname: Some("Uno".into()),
            }],
            ..Default::default()
        };
        assert_eq!(import_history(&history, &store, &[]), 2);
        assert_eq!(
            import_history(&history, &store, &[]),
            0,
            "re-import is a no-op"
        );
        let chats = store.list_chats().unwrap();
        assert_eq!(chats.len(), 1);
        // Not in the contact book: the person's own name, marked with ~.
        assert_eq!(chats[0].name, "~Uno");
        assert_eq!(chats[0].unread, 1);
        assert!(chats[0].archived);
        assert_eq!(chats[0].last_preview, "chau");
    }

    fn poll_message(name: &str, options: &[&str], secret: Option<Vec<u8>>) -> wa::Message {
        wa::Message {
            poll_creation_message_v3: MessageField::some(wa::message::PollCreationMessage {
                name: Some(name.into()),
                options: options
                    .iter()
                    .map(|o| wa::message::poll_creation_message::Option {
                        option_name: Some((*o).into()),
                        option_hash: None,
                    })
                    .collect(),
                selectable_options_count: Some(1),
                ..Default::default()
            }),
            message_context_info: match secret {
                Some(s) => MessageField::some(wa::MessageContextInfo {
                    message_secret: Some(s),
                    ..Default::default()
                }),
                None => MessageField::none(),
            },
            ..Default::default()
        }
    }

    #[test]
    fn polls_are_described_with_options_and_secret() {
        let d = describe(&poll_message(
            "¿Cuándo?",
            &["Hoy", "Mañana"],
            Some(vec![7; 32]),
        ));
        assert_eq!(d.kind, "poll");
        assert_eq!(d.text, "¿Cuándo?");
        let poll = d.poll.expect("poll definition");
        assert_eq!(poll.options, ["Hoy", "Mañana"]);
        assert_eq!(poll.selectable, 1);
        assert_eq!(poll.secret, hex::encode([7u8; 32]));
        // A poll without options is not something we can show.
        assert_eq!(describe(&poll_message("?", &[], None)).kind, "other");
    }

    #[test]
    fn history_polls_keep_their_votes() {
        use sha2::Digest;
        let dir = tempfile::tempdir().unwrap();
        let store =
            Store::open(&dir.path().join("t.db"), &crate::keychain::generate_key()).unwrap();
        let hash = |o: &str| sha2::Sha256::digest(o.as_bytes()).to_vec();
        let vote = |participant: Option<&str>, from_me: bool, pick: &str, ts: i64| wa::PollUpdate {
            poll_update_message_key: MessageField::some(wa::MessageKey {
                remote_jid: Some("g@g.us".into()),
                from_me: Some(from_me),
                id: Some(format!("v{ts}")),
                participant: participant.map(Into::into),
            }),
            vote: MessageField::some(wa::message::PollVoteMessage {
                selected_options: vec![hash(pick)],
            }),
            sender_timestamp_ms: Some(ts * 1000),
            ..Default::default()
        };
        let history = wa::HistorySync {
            conversations: vec![wa::Conversation {
                id: "g@g.us".into(),
                messages: vec![wa::HistorySyncMsg {
                    message: MessageField::some(wa::WebMessageInfo {
                        key: MessageField::some(wa::MessageKey {
                            remote_jid: Some("g@g.us".into()),
                            from_me: Some(false),
                            id: Some("p1".into()),
                            participant: Some("1@s.whatsapp.net".into()),
                        }),
                        message: MessageField::some(poll_message("¿Pizza?", &["Sí", "No"], None)),
                        message_timestamp: Some(100),
                        message_secret: Some(vec![9; 32]),
                        poll_updates: vec![
                            vote(Some("2@s.whatsapp.net"), false, "No", 110),
                            vote(Some("2@s.whatsapp.net"), false, "Sí", 120),
                            vote(None, true, "Sí", 115),
                        ],
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                conversation_timestamp: Some(100),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(import_history(&history, &store, &[]), 1);
        let (def, from_me, sender) = store.poll_def("g@g.us", "p1").unwrap().unwrap();
        assert_eq!(def.secret, hex::encode([9u8; 32]));
        assert!(!from_me);
        assert_eq!(sender, "1@s.whatsapp.net");
        let page = store.list_messages("g@g.us", None, 10).unwrap();
        let poll = page[0].poll.as_ref().expect("poll with votes");
        let yes = &poll.options[0];
        assert_eq!(yes.name, "Sí");
        assert!(yes.mine);
        assert_eq!(
            yes.voters.len(),
            1,
            "the later vote replaced the earlier one"
        );
        assert_eq!(yes.voters[0].phone, "+2");
        assert!(poll.options[1].voters.is_empty());
        assert_eq!(
            store.get_chat("g@g.us").unwrap().unwrap().last_preview,
            "📊 ¿Pizza?"
        );
    }
}
