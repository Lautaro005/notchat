//! WhatsApp session: pairing, receiving and sending. Wraps `whatsapp-rust`
//! and persists everything into the encrypted [`Store`].

use crate::error::{Error, Result};
use crate::keychain::is_valid_key;
use crate::store::{restrict_permissions, Message, Store};
use diesel::RunQueryDsl;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use whatsapp_rust::prelude::*;
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
}

pub struct WaSession {
    client: Arc<Client>,
    handle: Option<BotHandle>,
    store: Arc<Store>,
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

/// Classifies a message and extracts its visible text.
fn describe(message: &wa::Message) -> (&'static str, String) {
    let base = message.get_base_message();
    let caption = message.get_caption().unwrap_or_default().to_string();
    if let Some(text) = message.text_content() {
        return ("text", text.to_string());
    }
    if base.image_message.as_option().is_some() {
        ("image", caption)
    } else if base.video_message.as_option().is_some() {
        ("video", caption)
    } else if base.audio_message.as_option().is_some() {
        ("audio", String::new())
    } else if base.document_message.as_option().is_some() {
        ("document", caption)
    } else if base.sticker_message.as_option().is_some() {
        ("sticker", String::new())
    } else {
        ("other", String::new())
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
            .skip_history_sync()
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
            client,
            handle: Some(handle),
            store,
        })
    }

    pub fn is_logged_in(&self) -> bool {
        self.client.is_logged_in()
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_connected()
    }

    /// Sends a text message and stores it. Returns the stored message.
    pub async fn send_text(&self, chat_jid: &str, text: &str, by_ai: bool) -> Result<Message> {
        let text = text.trim();
        if text.is_empty() || text.len() > 16_000 {
            return Err(Error::Invalid("message text".into()));
        }
        if !self.client.is_logged_in() {
            return Err(Error::NotConnected);
        }
        let jid: Jid = chat_jid
            .parse()
            .map_err(|_| Error::Invalid("chat id".into()))?;
        let sent = self
            .client
            .send_message(&jid, wa::Message::text(text))
            .await
            .map_err(|e| Error::Wa(e.to_string()))?;
        let message = Message {
            id: sent.message_id.to_string(),
            chat_jid: chat_jid.to_string(),
            sender_jid: String::new(),
            sender_name: String::new(),
            from_me: true,
            ts: chrono::Utc::now().timestamp(),
            kind: "text".into(),
            text: text.to_string(),
            by_ai,
        };
        self.store
            .insert_message(&message, chat_jid.ends_with("@g.us"))?;
        Ok(message)
    }

    /// Unlinks this device from the account.
    pub async fn logout(&mut self) {
        self.client.logout().await;
        self.shutdown().await;
    }

    pub async fn shutdown(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.shutdown().await;
        }
    }
}

async fn handle_event(
    event: &Event,
    client: &Arc<Client>,
    store: &Arc<Store>,
    tx: &UnboundedSender<WaEvent>,
) {
    match event {
        Event::Connected(_) => {
            let _ = tx.send(WaEvent::Connected);
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
        Event::ContactUpdate(update) => {
            let name = update
                .action
                .full_name
                .clone()
                .or_else(|| update.action.first_name.clone())
                .unwrap_or_default();
            if !name.is_empty() {
                let jid = update.jid.to_string();
                if store.upsert_chat_name(&jid, &name, false).is_ok() {
                    let _ = tx.send(WaEvent::ChatUpdated { jid });
                }
            }
        }
        _ => {}
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
    let chat = info.source.chat.to_string();
    // Status broadcasts and newsletters are not chats.
    if chat.ends_with("@broadcast") || chat.ends_with("@newsletter") {
        return;
    }
    let (kind, text) = describe(&inbound.message);
    if kind == "other" {
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
        kind: kind.to_string(),
        text,
        by_ai: false,
    };
    match store.insert_message(&message, is_group) {
        Ok(true) => {}
        Ok(false) => return,
        Err(e) => {
            log::error!("failed to store message: {e}");
            return;
        }
    }
    ensure_chat_name(&chat, is_group, info, client, store).await;
    let _ = tx.send(WaEvent::Message {
        mentions_me: is_group && mentions(&inbound.message, me),
        message,
        is_group,
        live: !info.is_offline,
    });
}

async fn ensure_chat_name(
    chat: &str,
    is_group: bool,
    info: &MessageInfo,
    client: &Arc<Client>,
    store: &Arc<Store>,
) {
    let known = store
        .get_chat(chat)
        .ok()
        .flatten()
        .is_some_and(|c| !c.name.is_empty());
    if known {
        return;
    }
    let name = if is_group {
        match client.groups().get_metadata(&info.source.chat).await {
            Ok(meta) => meta.subject,
            Err(_) => String::new(),
        }
    } else if !info.source.is_from_me {
        info.push_name.clone()
    } else {
        String::new()
    };
    let _ = store.upsert_chat_name(chat, &name, is_group);
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
}
