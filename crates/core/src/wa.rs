//! WhatsApp session: pairing, receiving and sending. Wraps `whatsapp-rust`
//! and persists everything into the encrypted [`Store`].

use crate::error::{Error, Result};
use crate::keychain::is_valid_key;
use crate::media::{check_avatar_url, MAX_MEDIA_BYTES};
use crate::store::{restrict_permissions, HistoryChat, Message, Store};
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
        message: Message,
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
    pub name: String,
    pub admin: bool,
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
}

fn size_of(len: Option<u64>) -> i64 {
    len.map_or(0, |l| i64::try_from(l).unwrap_or(i64::MAX))
}

/// Classifies a message and extracts its visible text and media keys.
fn describe(message: &wa::Message) -> Described {
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

        let mut builder = Bot::builder()
            .with_backend(backend)
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
        let text = text.trim();
        if text.is_empty() || text.len() > 16_000 {
            return Err(Error::Invalid("message text".into()));
        }
        self.ready()?;
        let jid = parse_jid(chat_jid)?;
        let sent = self
            .client
            .send_message(&jid, wa::Message::text(text))
            .await
            .map_err(|e| Error::Wa(e.to_string()))?;
        let message = Message {
            id: sent.message_id.to_string(),
            chat_jid: chat_jid.to_string(),
            from_me: true,
            ts: chrono::Utc::now().timestamp(),
            kind: "text".into(),
            text: text.to_string(),
            by_ai,
            ..Default::default()
        };
        self.store
            .insert_message(&message, chat_jid.ends_with("@g.us"))?;
        Ok(message)
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
        let described = describe(&message);
        let sent = self
            .client
            .send_message(&jid, message)
            .await
            .map_err(|e| Error::Wa(e.to_string()))?;
        let stored = Message {
            id: sent.message_id.to_string(),
            chat_jid: chat_jid.to_string(),
            from_me: true,
            ts: chrono::Utc::now().timestamp(),
            kind: described.kind.to_string(),
            text: described.text,
            mime: described.mime,
            file_name: described.file_name,
            size,
            seconds: described.seconds,
            has_media: described.keys.is_some(),
            ..Default::default()
        };
        self.store.insert_message_full(
            &stored,
            chat_jid.ends_with("@g.us"),
            described.keys.as_deref(),
            true,
        )?;
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
                if me
                    .iter()
                    .any(|m| user_part(m) == user_part(&jid) || user_part(m) == p.jid.user)
                {
                    i_am_admin = admin;
                }
                let name = self
                    .store
                    .get_chat(&jid)
                    .ok()
                    .flatten()
                    .map(|c| c.name)
                    .unwrap_or_default();
                Participant { jid, name, admin }
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
            let client = Arc::clone(client);
            let store = Arc::clone(store);
            let tx = tx.clone();
            tokio::spawn(async move { sync_groups(&client, &store, &tx).await });
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
        Event::HistorySync(lazy) => {
            let progress = lazy.progress();
            if let Some(history) = lazy.get() {
                let imported = import_history(history, store);
                log::info!("history chunk imported: {imported} messages");
            }
            if let Some(progress) = progress {
                let _ = tx.send(WaEvent::HistoryProgress { progress });
            }
            let _ = tx.send(WaEvent::ChatsChanged);
        }
        Event::ContactUpdate(update) => {
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
        Event::PictureUpdate(update) => {
            let _ = tx.send(WaEvent::PictureChanged {
                jid: update.jid.to_string(),
            });
        }
        Event::GroupUpdate(update) => {
            if let Ok(meta) = client.groups().get_metadata(&update.group_jid).await {
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
fn import_history(history: &wa::HistorySync, store: &Store) -> usize {
    for p in &history.pushnames {
        if let (Some(id), Some(name)) = (&p.id, &p.pushname) {
            let _ = store.upsert_contact(id, name, true);
        }
    }
    let mut imported = 0;
    for conv in &history.conversations {
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
        if let Err(e) = store.apply_history_chat(&chat) {
            log::warn!("failed to store chat: {e}");
            continue;
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
            let message = Message {
                id,
                chat_jid: jid.clone(),
                sender_jid: sender,
                sender_name: info.push_name.clone().unwrap_or_default(),
                from_me,
                ts: info
                    .message_timestamp
                    .map_or(0, |t| i64::try_from(t).unwrap_or(0)),
                kind: d.kind.to_string(),
                text: d.text,
                by_ai: false,
                mime: d.mime,
                file_name: d.file_name,
                size: d.size,
                seconds: d.seconds,
                has_media: d.keys.is_some(),
            };
            match store.insert_message_full(&message, is_group, d.keys.as_deref(), false) {
                Ok(true) => imported += 1,
                Ok(false) => {}
                Err(e) => log::warn!("failed to store message: {e}"),
            }
        }
    }
    imported
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
    let chat = info.source.chat.to_string();
    if !is_chat(&chat) {
        return;
    }
    let d = describe(&inbound.message);
    if d.kind == "other" {
        // Protocol messages, reactions, edits: nothing to show yet.
        return;
    }
    let is_group = info.source.is_group;
    let message = Message {
        id: info.id.to_string(),
        chat_jid: chat.clone(),
        sender_jid: info.source.sender.to_string(),
        sender_name: info.push_name.clone(),
        from_me: info.source.is_from_me,
        ts: info.timestamp.timestamp(),
        kind: d.kind.to_string(),
        text: d.text,
        by_ai: false,
        mime: d.mime,
        file_name: d.file_name,
        size: d.size,
        seconds: d.seconds,
        has_media: d.keys.is_some(),
    };
    match store.insert_message_full(&message, is_group, d.keys.as_deref(), true) {
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
        message,
        is_group,
        live: !info.is_offline,
    });
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
        assert_eq!(import_history(&history, &store), 2);
        assert_eq!(import_history(&history, &store), 0, "re-import is a no-op");
        let chats = store.list_chats().unwrap();
        assert_eq!(chats.len(), 1);
        assert_eq!(chats[0].name, "Uno");
        assert_eq!(chats[0].unread, 1);
        assert!(chats[0].archived);
        assert_eq!(chats[0].last_preview, "chau");
    }
}
