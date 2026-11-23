//! Bridges the WhatsApp session to the UI: starts it, tracks link status and
//! forwards events.

use crate::state::{AppState, Link};
use notchat_core::wa::{WaEvent, WaSession};
use notchat_core::Result;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc::unbounded_channel;

pub const EVENT_STATUS: &str = "status";
pub const EVENT_MESSAGE: &str = "message";
pub const EVENT_MESSAGE_CHANGED: &str = "message-changed";
pub const EVENT_CHATS: &str = "chats-changed";
pub const EVENT_HISTORY: &str = "history-progress";
pub const EVENT_PICTURE: &str = "picture-changed";
pub const EVENT_BACKFILL: &str = "history-backfill";

/// Setting: fetch the whole chat history from the phone in the background.
pub const KEY_FULL_HISTORY: &str = "full_history";

/// Fetches older history from the phone in the background, once per run,
/// reporting progress to the UI. Skipped when turned off in Settings.
pub fn start_backfill(app: &AppHandle, state: &Arc<AppState>) {
    use std::sync::atomic::Ordering;
    if !state
        .store
        .get_json::<Option<bool>>(KEY_FULL_HISTORY)
        .unwrap_or(true)
    {
        return;
    }
    if state.backfilling.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    let state = Arc::clone(state);
    tauri::async_runtime::spawn(async move {
        // Let the initial sync and the offline queue settle first.
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        if let Ok(client) = crate::chats::wa(&state).await {
            let emit = app.clone();
            let result = client
                .backfill_history(&move |done, total| {
                    let _ = emit.emit(
                        EVENT_BACKFILL,
                        serde_json::json!({ "done": done, "total": total }),
                    );
                    let _ = emit.emit(EVENT_CHATS, ());
                })
                .await;
            if let Err(e) = result {
                log::warn!("history backfill stopped: {e}");
            }
        }
        let _ = app.emit(EVENT_BACKFILL, serde_json::Value::Null);
        state.backfilling.store(false, Ordering::SeqCst);
    });
}

/// True when the chat is muted right now.
fn muted(state: &AppState, jid: &str) -> bool {
    state
        .store
        .get_chat(jid)
        .ok()
        .flatten()
        .is_some_and(|c| c.muted_until < 0 || c.muted_until > crate::assistant::now())
}

pub async fn start(app: AppHandle, phone: Option<String>) -> Result<()> {
    let state = app.state::<Arc<AppState>>().inner().clone();
    let mut slot = state.session.lock().await;
    if let Some(mut old) = slot.take() {
        old.shutdown().await;
    }
    let status = state.update_status(|s| {
        s.link = Link::Starting;
        s.qr_svg = None;
        s.pair_code = None;
        s.error = None;
    });
    let _ = app.emit(EVENT_STATUS, status);

    let (tx, mut rx) = unbounded_channel::<WaEvent>();
    let session = WaSession::start(
        &state.session_db(),
        &state.key,
        Arc::clone(&state.store),
        phone,
        tx,
    )
    .await?;
    *slot = Some(session);
    drop(slot);

    let app_for_loop = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            on_event(&app_for_loop, event).await;
        }
    });
    Ok(())
}

async fn on_event(app: &AppHandle, event: WaEvent) {
    let state = app.state::<Arc<AppState>>().inner().clone();
    match event {
        WaEvent::Qr { svg, .. } => {
            log::info!("pairing code ready");
            let status = state.update_status(|s| {
                s.link = Link::Pairing;
                s.qr_svg = Some(svg);
            });
            let _ = app.emit(EVENT_STATUS, status);
        }
        WaEvent::PairCode { code } => {
            let status = state.update_status(|s| {
                s.link = Link::Pairing;
                s.pair_code = Some(code);
            });
            let _ = app.emit(EVENT_STATUS, status);
        }
        WaEvent::Connected => {
            log::info!("connected");
            let status = state.update_status(|s| {
                s.link = Link::Connected;
                s.qr_svg = None;
                s.pair_code = None;
            });
            let _ = app.emit(EVENT_STATUS, status);
            start_backfill(app, &state);
        }
        WaEvent::Disconnected => {
            let status = state.update_status(|s| {
                if s.link == Link::Connected {
                    s.link = Link::Disconnected;
                }
            });
            let _ = app.emit(EVENT_STATUS, status);
        }
        WaEvent::LoggedOut => {
            let _ = state.store.clear_whatsapp_data();
            let status = state.update_status(|s| {
                s.link = Link::LoggedOut;
                s.qr_svg = None;
                s.pair_code = None;
            });
            let _ = app.emit(EVENT_STATUS, status);
            let _ = app.emit(EVENT_CHATS, ());
        }
        WaEvent::Message {
            message,
            is_group,
            mentions_me,
            live,
        } => {
            // Re-read it so names and mentions are resolved like the rest.
            let message = state
                .store
                .get_message(&message.chat_jid, &message.id)
                .ok()
                .flatten()
                .map(Box::new)
                .unwrap_or(message);
            crate::autoreply::on_incoming(app, &state, &message, is_group, mentions_me, live);
            if live && !message.from_me && !muted(&state, &message.chat_jid) {
                crate::notify::deliver(app, &state, &message, is_group, false);
            }
            let _ = app.emit(EVENT_MESSAGE, &message);
            let _ = app.emit(EVENT_CHATS, ());
        }
        WaEvent::ChatUpdated { .. } | WaEvent::ChatsChanged => {
            let _ = app.emit(EVENT_CHATS, ());
        }
        WaEvent::HistoryProgress { progress } => {
            let _ = app.emit(EVENT_HISTORY, progress);
        }
        WaEvent::MessageChanged { chat_jid, id } => {
            let _ = app.emit(
                EVENT_MESSAGE_CHANGED,
                serde_json::json!({ "chat_jid": chat_jid, "id": id }),
            );
            let _ = app.emit(EVENT_CHATS, ());
        }
        WaEvent::PictureChanged { jid } => {
            state.avatars.forget(&jid);
            let _ = app.emit(EVENT_PICTURE, jid);
        }
    }
}
