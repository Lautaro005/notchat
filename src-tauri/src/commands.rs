//! Tauri commands. Every input coming from the webview is validated here.

use crate::assistant::{self, KEY_AI_CONFIG};
use crate::autoreply::KEY_AUTO_REPLY;
use crate::session;
use crate::state::{AppState, Link, Status};
use base64::Engine;
use notchat_core::ai::{self, AiConfig, ProviderInfo};
use notchat_core::attach::{self, Attachment};
use notchat_core::policy::AutoReplyConfig;
use notchat_core::store::{AiMessage, Chat, Message};
use notchat_core::{Error, Result};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

type St<'a> = State<'a, Arc<AppState>>;

fn check_jid(jid: &str) -> Result<()> {
    let ok = jid.len() <= 128
        && jid.contains('@')
        && jid
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'@' | b'.' | b'-' | b'_' | b':'));
    if ok {
        Ok(())
    } else {
        Err(Error::Invalid("chat id".into()))
    }
}

#[tauri::command]
pub fn get_status(state: St<'_>) -> Status {
    state.status()
}

#[tauri::command]
pub async fn start_session(app: AppHandle, phone: Option<String>) -> Result<()> {
    session::start(app, phone).await
}

#[tauri::command]
pub fn list_chats(state: St<'_>) -> Result<Vec<Chat>> {
    state.store.list_chats()
}

#[tauri::command]
pub fn list_messages(
    state: St<'_>,
    jid: String,
    before_ts: Option<i64>,
    limit: Option<u32>,
) -> Result<Vec<Message>> {
    check_jid(&jid)?;
    state
        .store
        .list_messages(&jid, before_ts, limit.unwrap_or(60))
}

#[tauri::command]
pub fn mark_read(app: AppHandle, state: St<'_>, jid: String) -> Result<()> {
    check_jid(&jid)?;
    state.store.mark_read(&jid)?;
    let _ = app.emit(session::EVENT_CHATS, ());
    Ok(())
}

#[tauri::command]
pub async fn send_text(
    app: AppHandle,
    state: St<'_>,
    jid: String,
    text: String,
) -> Result<Message> {
    check_jid(&jid)?;
    let guard = state.session.lock().await;
    let session = guard.as_ref().ok_or(Error::NotConnected)?;
    let message = session.send_text(&jid, &text, false).await?;
    let _ = app.emit(session::EVENT_CHATS, ());
    Ok(message)
}

#[tauri::command]
pub async fn logout(app: AppHandle, state: St<'_>) -> Result<()> {
    {
        let mut guard = state.session.lock().await;
        if let Some(mut session) = guard.take() {
            session.logout().await;
        }
    }
    state.store.clear_whatsapp_data()?;
    let _ = std::fs::remove_file(state.session_db());
    let status = state.update_status(|s| {
        s.link = Link::LoggedOut;
        s.qr_svg = None;
        s.pair_code = None;
    });
    let _ = app.emit(session::EVENT_STATUS, status);
    let _ = app.emit(session::EVENT_CHATS, ());
    // Start a fresh session so a new QR code is offered right away.
    session::start(app, None).await
}

// ---- AI ---------------------------------------------------------------------

#[tauri::command]
pub async fn detect_providers(state: St<'_>) -> Result<Vec<ProviderInfo>> {
    let catalog = notchat_core::managed::catalog()?;
    let installed =
        notchat_core::managed::Paths::new(&state.data_dir).server_installed(&catalog.llama_server);
    let url = crate::managed::running_url(&state).await;
    Ok(ai::detect(url, installed).await)
}

#[tauri::command]
pub async fn managed_view(state: St<'_>, lang: String) -> Result<crate::managed::ManagedView> {
    crate::managed::view(&state, lang == "es").await
}

/// Starts installing in the background; progress arrives as events.
#[tauri::command]
pub fn managed_install(app: AppHandle, state: St<'_>, model_id: String) {
    let state = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::managed::install_and_use(app.clone(), state, model_id).await {
            let _ = app.emit("managed-error", e.to_string());
        }
        let _ = app.emit(assistant::EVENT_AI_CHANGED, ());
    });
}

#[tauri::command]
pub fn get_ai_config(state: St<'_>) -> AiConfig {
    state.store.get_json(KEY_AI_CONFIG)
}

#[tauri::command]
pub fn set_ai_config(state: St<'_>, config: AiConfig) -> Result<()> {
    if config.provider.is_some() {
        ai::check_local_url(&config.base_url)?;
    }
    if config.model.len() > 300 {
        return Err(Error::Invalid("model".into()));
    }
    state.store.set_json(KEY_AI_CONFIG, &config)
}

#[tauri::command]
pub fn ai_history(state: St<'_>) -> Result<Vec<AiMessage>> {
    let mut rows = state.store.list_ai_messages(300)?;
    // Image data is not needed to draw the history; keep the payload small.
    for row in &mut rows {
        if row.role == "user" && !row.meta.is_empty() {
            let mut attachments: Vec<Attachment> =
                serde_json::from_str(&row.meta).unwrap_or_default();
            for a in &mut attachments {
                a.data.clear();
            }
            row.meta = serde_json::to_string(&attachments)?;
        }
    }
    Ok(rows)
}

#[derive(serde::Deserialize)]
pub struct Upload {
    name: String,
    /// File content, base64-encoded by the webview.
    base64: String,
}

#[tauri::command]
pub async fn ai_send(
    app: AppHandle,
    state: St<'_>,
    text: String,
    files: Vec<Upload>,
    lang: String,
) -> Result<()> {
    let text = text.trim().to_string();
    if text.is_empty() && files.is_empty() {
        return Err(Error::Invalid("empty message".into()));
    }
    if text.chars().count() > 20_000 || files.len() > 6 {
        return Err(Error::Invalid("message too large".into()));
    }
    let config: AiConfig = state.store.get_json(KEY_AI_CONFIG);
    if !config.is_ready() {
        return Err(Error::Ai("no AI model selected".into()));
    }
    let mut attachments = Vec::with_capacity(files.len());
    for file in &files {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&file.base64)
            .map_err(|_| Error::Invalid("file encoding".into()))?;
        attachments.push(attach::extract(&file.name, &bytes)?);
    }
    let mut slot = state.ai_task.lock().unwrap_or_else(|e| e.into_inner());
    if slot
        .as_ref()
        .is_some_and(|task| !task.inner().is_finished())
    {
        return Err(Error::Ai("the assistant is still answering".into()));
    }
    let meta = if attachments.is_empty() {
        String::new()
    } else {
        serde_json::to_string(&attachments)?
    };
    state
        .store
        .add_ai_message("user", &text, assistant::now(), &meta)?;
    let _ = app.emit(assistant::EVENT_AI_CHANGED, ());
    let lang = if lang == "es" { "es" } else { "en" }.to_string();
    *slot = Some(tauri::async_runtime::spawn(assistant::run_turn(
        app.clone(),
        state.inner().clone(),
        lang,
    )));
    Ok(())
}

#[tauri::command]
pub fn ai_stop(app: AppHandle, state: St<'_>) {
    if let Some(task) = state
        .ai_task
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
    {
        task.abort();
    }
    let _ = app.emit(assistant::EVENT_AI_BUSY, false);
    let _ = app.emit(assistant::EVENT_AI_CHANGED, ());
}

#[tauri::command]
pub fn ai_clear(app: AppHandle, state: St<'_>) -> Result<()> {
    state.store.clear_ai_messages()?;
    let _ = app.emit(assistant::EVENT_AI_CHANGED, ());
    Ok(())
}

#[tauri::command]
pub async fn ai_resolve_action(
    app: AppHandle,
    state: St<'_>,
    id: i64,
    approve: bool,
) -> Result<()> {
    assistant::resolve_action(&app, state.inner(), id, approve).await
}

// ---- auto-reply ----------------------------------------------------------------

#[tauri::command]
pub fn get_auto_reply(state: St<'_>) -> AutoReplyConfig {
    state.store.get_json(KEY_AUTO_REPLY)
}

#[tauri::command]
pub fn set_auto_reply(app: AppHandle, state: St<'_>, config: AutoReplyConfig) -> Result<()> {
    let valid_hour = |h: Option<u8>| h.is_none_or(|h| h < 24);
    if config.instructions.chars().count() > 2000
        || config.max_delay_secs > 600
        || config.min_delay_secs > 600
        || config.max_per_chat_per_hour > 60
        || !valid_hour(config.quiet_from)
        || !valid_hour(config.quiet_to)
    {
        return Err(Error::Invalid("auto-reply settings".into()));
    }
    state.store.set_json(KEY_AUTO_REPLY, &config)?;
    crate::tray::sync_auto(&app, config.enabled);
    Ok(())
}

#[tauri::command]
pub fn set_chat_auto_reply(
    app: AppHandle,
    state: St<'_>,
    jid: String,
    enabled: bool,
) -> Result<()> {
    check_jid(&jid)?;
    state.store.set_chat_auto_reply(&jid, enabled)?;
    let _ = app.emit(session::EVENT_CHATS, ());
    Ok(())
}

#[tauri::command]
pub fn set_chat_instructions(
    app: AppHandle,
    state: St<'_>,
    jid: String,
    instructions: String,
) -> Result<()> {
    check_jid(&jid)?;
    if instructions.chars().count() > 1000 {
        return Err(Error::Invalid("instructions too long".into()));
    }
    state
        .store
        .set_chat_instructions(&jid, instructions.trim())?;
    let _ = app.emit(session::EVENT_CHATS, ());
    Ok(())
}

// ---- notifications ---------------------------------------------------------------

pub const KEY_NOTIFY: &str = "notify_mode";

/// "notch", "system" or "off". Defaults to the notch panel.
pub fn notify_mode(state: &AppState) -> String {
    match state
        .store
        .get_setting(KEY_NOTIFY)
        .ok()
        .flatten()
        .as_deref()
    {
        Some("system") => "system".into(),
        Some("off") => "off".into(),
        _ => "notch".into(),
    }
}

#[tauri::command]
pub fn get_notify_mode(state: St<'_>) -> String {
    notify_mode(&state)
}

#[tauri::command]
pub fn set_notify_mode(state: St<'_>, mode: String) -> Result<()> {
    if !matches!(mode.as_str(), "notch" | "system" | "off") {
        return Err(Error::Invalid("notification mode".into()));
    }
    state.store.set_setting(KEY_NOTIFY, &mode)
}

/// Asks macOS for notification permission. Returns false when system
/// notifications are unavailable (not running from an app bundle).
#[tauri::command]
pub fn enable_system_notifications(app: AppHandle, lang: String) -> bool {
    crate::notify::setup_system(&app, lang == "es");
    crate::sysnotify::available()
}

#[tauri::command]
pub fn notch_expand(app: AppHandle, expanded: bool) {
    crate::notch::set_expanded(&app, expanded);
}

#[tauri::command]
pub fn notch_hide(app: AppHandle) {
    crate::notch::hide(&app);
}

#[tauri::command]
pub fn notch_open_chat(app: AppHandle, jid: String) -> Result<()> {
    check_jid(&jid)?;
    crate::notch::open_chat(&app, &jid);
    Ok(())
}

/// Shows a sample notification so the user can see where it appears.
#[tauri::command]
pub fn notch_test(app: AppHandle, state: St<'_>, title: String, text: String) {
    let sample = Message {
        id: "preview".into(),
        chat_jid: "preview@notchat".into(),
        sender_jid: String::new(),
        sender_name: title.chars().take(60).collect(),
        from_me: false,
        ts: assistant::now(),
        kind: "text".into(),
        text: text.chars().take(200).collect(),
        by_ai: false,
    };
    crate::notify::deliver(&app, state.inner(), &sample, false, true);
}

/// Drafts a reply for the user to edit; nothing is sent.
#[tauri::command]
pub async fn suggest_reply(state: St<'_>, jid: String) -> Result<String> {
    check_jid(&jid)?;
    let config: AiConfig = state.store.get_json(KEY_AI_CONFIG);
    let llm = ai::LocalLlm::from_config(&config)?;
    let chat = state
        .store
        .get_chat(&jid)?
        .ok_or_else(|| Error::Invalid("chat".into()))?;
    Ok(
        notchat_core::agent::suggest_reply(&llm, &state.store, &chat)
            .await?
            .unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::check_jid;

    #[test]
    fn jid_validation() {
        assert!(check_jid("5491155551234@s.whatsapp.net").is_ok());
        assert!(check_jid("120363-1234@g.us").is_ok());
        assert!(check_jid("no-at-sign").is_err());
        assert!(check_jid("a@b' OR 1=1").is_err());
        assert!(check_jid(&format!("{}@g.us", "1".repeat(200))).is_err());
    }
}
