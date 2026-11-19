//! Routes incoming messages to the notch panel or to system notifications,
//! depending on the user's setting.

use crate::commands::notify_mode;
use crate::state::AppState;
use crate::{notch, sysnotify};
use notchat_core::store::Message;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

fn main_window_focused(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .is_some_and(|w| w.is_focused().unwrap_or(false) && w.is_visible().unwrap_or(false))
}

fn on_system_response(app: &AppHandle, response: sysnotify::Response) {
    match response {
        sysnotify::Response::Open { chat_jid } => notch::open_chat(app, &chat_jid),
        sysnotify::Response::Reply { chat_jid, text } => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<Arc<AppState>>().inner().clone();
                let guard = state.session.lock().await;
                let Some(session) = guard.as_ref() else {
                    return;
                };
                match session.send_text(&chat_jid, &text, false).await {
                    Ok(sent) => {
                        let _ = state.store.mark_read(&chat_jid);
                        let _ = app.emit(crate::session::EVENT_MESSAGE, &sent);
                        let _ = app.emit(crate::session::EVENT_CHATS, ());
                    }
                    Err(e) => log::warn!("reply from notification failed: {e}"),
                }
            });
        }
    }
}

/// Prepares system notifications (permission prompt, reply action).
pub fn setup_system(app: &AppHandle, lang_es: bool) {
    let (reply, send, placeholder) = if lang_es {
        ("Responder", "Enviar", "Mensaje")
    } else {
        ("Reply", "Send", "Message")
    };
    sysnotify::setup(app, on_system_response, reply, send, placeholder);
}

/// Notifies the user about `message`. `force` shows it even when the main
/// window is focused (used by the preview button in Settings).
pub fn deliver(
    app: &AppHandle,
    state: &Arc<AppState>,
    message: &Message,
    is_group: bool,
    force: bool,
) {
    let mode = notify_mode(state);
    if mode == "off" && !force {
        return;
    }
    if !force && main_window_focused(app) {
        return;
    }
    if mode == "system" && sysnotify::available() {
        let chat = state.store.get_chat(&message.chat_jid).ok().flatten();
        let mut title = chat
            .as_ref()
            .map(notchat_core::agent::chat_label)
            .unwrap_or_else(|| message.sender_name.clone());
        if is_group && !message.sender_name.is_empty() {
            title = format!("{} · {}", message.sender_name, title);
        }
        let body = if message.text.is_empty() {
            format!("[{}]", message.kind)
        } else {
            message.text_with_names().chars().take(240).collect()
        };
        sysnotify::post(&message.id, &message.chat_jid, &title, &body);
    } else {
        notch::notify(app, state, message, is_group);
    }
}
