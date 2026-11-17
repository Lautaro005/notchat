//! Auto-reply engine. Incoming messages are untrusted: the model sees only
//! the chat it is answering and has no tools (see `notchat_core::agent`).

use crate::assistant::{now, KEY_AI_CONFIG};
use crate::session::{EVENT_CHATS, EVENT_MESSAGE};
use crate::state::AppState;
use notchat_core::agent;
use notchat_core::ai::{AiConfig, LocalLlm};
use notchat_core::policy::{self, AutoReplyConfig, Decision, Incoming};
use notchat_core::store::Message;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub const KEY_AUTO_REPLY: &str = "auto_reply";

fn local_hour() -> u8 {
    // `date +%H` equivalent without pulling a timezone database.
    let secs = now();
    let offset = local_utc_offset_secs();
    (((secs + offset).rem_euclid(86_400)) / 3600) as u8
}

fn local_utc_offset_secs() -> i64 {
    // SAFETY: `localtime_r` only writes into the provided `tm` buffer.
    unsafe {
        let t: libc::time_t = now() as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            0
        } else {
            tm.tm_gmtoff as i64
        }
    }
}

pub fn on_incoming(
    app: &AppHandle,
    state: &Arc<AppState>,
    message: &Message,
    is_group: bool,
    mentions_me: bool,
    live: bool,
) {
    let cfg: AutoReplyConfig = state.store.get_json(KEY_AUTO_REPLY);
    if !cfg.enabled {
        return;
    }
    let chat_selected = state
        .store
        .get_chat(&message.chat_jid)
        .ok()
        .flatten()
        .is_some_and(|c| c.auto_reply);
    let replies_last_hour = state
        .rate
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .count(&message.chat_jid, now());
    let incoming = Incoming {
        is_group,
        from_me: message.from_me,
        mentions_me,
        live,
        has_text: !message.text.trim().is_empty(),
        chat_selected,
        replies_last_hour,
        hour: local_hour(),
    };
    if policy::decide(&cfg, &incoming) != Decision::Reply {
        return;
    }
    // One pending reply per chat: a burst of messages gets a single answer
    // that reads the whole burst when it runs.
    {
        let mut pending = state.pending_auto.lock().unwrap_or_else(|e| e.into_inner());
        if !pending.insert(message.chat_jid.clone()) {
            return;
        }
    }
    let delay = policy::delay_secs(&cfg, rand_unit());
    let app = app.clone();
    let state = Arc::clone(state);
    let chat_jid = message.chat_jid.clone();
    let trigger_ts = message.ts;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(u64::from(delay))).await;
        if let Err(e) = reply(&app, &state, &chat_jid, trigger_ts).await {
            log::warn!("auto-reply skipped: {e}");
        }
        state
            .pending_auto
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&chat_jid);
    });
}

fn rand_unit() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default();
    f64::from(nanos % 1000) / 1000.0
}

async fn reply(
    app: &AppHandle,
    state: &Arc<AppState>,
    chat_jid: &str,
    trigger_ts: i64,
) -> notchat_core::Result<()> {
    // Re-read everything after the delay: the user may have flipped the kill
    // switch, or answered the chat themselves in the meantime.
    let cfg: AutoReplyConfig = state.store.get_json(KEY_AUTO_REPLY);
    if !cfg.enabled {
        return Ok(());
    }
    let Some(chat) = state.store.get_chat(chat_jid)? else {
        return Ok(());
    };
    let recent = state.store.list_messages(chat_jid, None, 5)?;
    if recent.iter().any(|m| m.from_me && m.ts >= trigger_ts) {
        return Ok(());
    }
    let ai: AiConfig = state.store.get_json(KEY_AI_CONFIG);
    let llm = LocalLlm::from_config(&ai)?;
    let Some(text) = agent::auto_reply(&llm, &state.store, &chat, &cfg.instructions).await? else {
        return Ok(());
    };
    let sent = {
        let guard = state.session.lock().await;
        let session = guard.as_ref().ok_or(notchat_core::Error::NotConnected)?;
        session.send_text(chat_jid, &text, true).await?
    };
    state
        .rate
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .record(chat_jid, now());
    let _ = app.emit(EVENT_MESSAGE, &sent);
    let _ = app.emit(EVENT_CHATS, ());
    Ok(())
}
