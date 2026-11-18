//! Chat-list actions, media, profile pictures, lists and storage. Every
//! input from the webview is validated before use.

use crate::commands::check_jid;
use crate::session::{EVENT_CHATS, EVENT_MESSAGE};
use crate::state::AppState;
use base64::Engine;
use notchat_core::media::{Avatar, MAX_MEDIA_BYTES};
use notchat_core::store::{ChatList, ChatUsage, Message};
use notchat_core::wa::{GroupInfo, Outgoing, SendKind, WaClient};
use notchat_core::{Error, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

type St<'a> = State<'a, Arc<AppState>>;

/// WhatsApp lets you pin up to three chats.
const MAX_PINNED: i64 = 3;

async fn wa(state: &AppState) -> Result<WaClient> {
    state
        .session
        .lock()
        .await
        .as_ref()
        .map(|s| s.client())
        .ok_or(Error::NotConnected)
}

fn now() -> i64 {
    crate::assistant::now()
}

fn check_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    if ok {
        Ok(())
    } else {
        Err(Error::Invalid("message id".into()))
    }
}

fn changed(app: &AppHandle) {
    let _ = app.emit(EVENT_CHATS, ());
}

/// Changes are applied locally first and then on the phone. A failure to
/// sync is logged, not fatal, so the app keeps working offline.
fn synced(what: &str, result: Result<()>) {
    if let Err(e) = result {
        log::warn!("{what} not synced to the phone: {e}");
    }
}

#[tauri::command]
pub async fn chat_archive(
    app: AppHandle,
    state: St<'_>,
    jid: String,
    archived: bool,
) -> Result<()> {
    check_jid(&jid)?;
    state.store.set_archived(&jid, archived)?;
    changed(&app);
    if let Ok(c) = wa(&state).await {
        synced("archive", c.set_archived(&jid, archived).await);
    }
    Ok(())
}

#[tauri::command]
pub async fn chat_pin(app: AppHandle, state: St<'_>, jid: String, pinned: bool) -> Result<()> {
    check_jid(&jid)?;
    if pinned && state.store.pinned_count()? >= MAX_PINNED {
        return Err(Error::Invalid("pin limit".into()));
    }
    state
        .store
        .set_pinned(&jid, if pinned { now() } else { 0 })?;
    changed(&app);
    if let Ok(c) = wa(&state).await {
        synced("pin", c.set_pinned(&jid, pinned).await);
    }
    Ok(())
}

/// `until`: Unix seconds, `-1` forever, `0` unmute.
#[tauri::command]
pub async fn chat_mute(app: AppHandle, state: St<'_>, jid: String, until: i64) -> Result<()> {
    check_jid(&jid)?;
    if until > 0 && until <= now() {
        return Err(Error::Invalid("mute time".into()));
    }
    let until = until.max(-1);
    state.store.set_muted_until(&jid, until)?;
    changed(&app);
    if let Ok(c) = wa(&state).await {
        synced("mute", c.set_muted(&jid, until).await);
    }
    Ok(())
}

#[tauri::command]
pub async fn chat_mark_unread(
    app: AppHandle,
    state: St<'_>,
    jid: String,
    unread: bool,
) -> Result<()> {
    check_jid(&jid)?;
    if unread {
        state.store.set_marked_unread(&jid, true)?;
    } else {
        state.store.mark_read(&jid)?;
    }
    changed(&app);
    if let Ok(c) = wa(&state).await {
        synced("read state", c.set_read(&jid, !unread).await);
    }
    Ok(())
}

#[tauri::command]
pub async fn chat_clear(app: AppHandle, state: St<'_>, jid: String) -> Result<()> {
    check_jid(&jid)?;
    state.store.clear_chat(&jid)?;
    state.media.clear_chat(&jid);
    changed(&app);
    if let Ok(c) = wa(&state).await {
        synced("clear", c.clear_chat(&jid).await);
    }
    Ok(())
}

#[tauri::command]
pub async fn chat_delete(app: AppHandle, state: St<'_>, jid: String) -> Result<()> {
    check_jid(&jid)?;
    state.store.delete_chat(&jid)?;
    state.media.clear_chat(&jid);
    changed(&app);
    if let Ok(c) = wa(&state).await {
        synced("delete", c.delete_chat(&jid).await);
    }
    Ok(())
}

#[tauri::command]
pub async fn group_leave(app: AppHandle, state: St<'_>, jid: String) -> Result<()> {
    check_jid(&jid)?;
    if !jid.ends_with("@g.us") {
        return Err(Error::Invalid("not a group".into()));
    }
    wa(&state).await?.leave_group(&jid).await?;
    state.store.set_archived(&jid, true)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn group_info(state: St<'_>, jid: String) -> Result<GroupInfo> {
    check_jid(&jid)?;
    if !jid.ends_with("@g.us") {
        return Err(Error::Invalid("not a group".into()));
    }
    wa(&state).await?.group_info(&jid).await
}

#[tauri::command]
pub async fn set_typing(state: St<'_>, jid: String, typing: bool) -> Result<()> {
    check_jid(&jid)?;
    wa(&state).await?.set_typing(&jid, typing).await
}

fn downloads_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| Error::Invalid("home folder".into()))?;
    Ok(PathBuf::from(home).join("Downloads"))
}

/// A file name safe to create in Downloads: no path separators or control
/// characters, not hidden, and not overwriting an existing file.
fn unique_download(name: &str) -> Result<PathBuf> {
    let clean: String = name
        .chars()
        .map(|c| {
            if c == '/' || c == ':' || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_start_matches('.');
    let clean = if clean.is_empty() {
        "Notchat file"
    } else {
        clean
    };
    let clean: String = clean.chars().take(120).collect();
    let dir = downloads_dir()?;
    let path = std::path::Path::new(&clean);
    let stem = path
        .file_stem()
        .map_or_else(|| clean.clone(), |s| s.to_string_lossy().into_owned());
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for n in 0..1000 {
        let candidate = if n == 0 {
            dir.join(format!("{stem}{ext}"))
        } else {
            dir.join(format!("{stem} ({n}){ext}"))
        };
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(Error::Invalid("file name".into()))
}

fn reveal(path: &std::path::Path) {
    let _ = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .spawn();
}

/// Writes a chat's text history to Downloads and shows it in Finder.
#[tauri::command]
pub fn chat_export(state: St<'_>, jid: String) -> Result<String> {
    check_jid(&jid)?;
    let chat = state.store.get_chat(&jid)?;
    let title = chat
        .as_ref()
        .map(|c| c.name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| notchat_core::wa::user_part(&jid).to_string());
    let mut out = String::new();
    for m in state.store.all_messages(&jid)? {
        let when = chrono::DateTime::from_timestamp(m.ts, 0)
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();
        let who = if m.from_me {
            "Me".to_string()
        } else if !m.sender_name.is_empty() {
            m.sender_name.clone()
        } else {
            title.clone()
        };
        let body = match (m.kind.as_str(), m.text.is_empty()) {
            ("text", _) => m.text.clone(),
            (kind, true) => format!("<{kind}>"),
            (kind, false) => format!("<{kind}> {}", m.text),
        };
        out.push_str(&format!("[{when}] {who}: {body}\n"));
    }
    let path = unique_download(&format!("Notchat - {title}.txt"))?;
    std::fs::write(&path, out)?;
    reveal(&path);
    Ok(path.to_string_lossy().into_owned())
}

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// A chat's profile picture as a data URL, or `None`. Cached for a day.
#[tauri::command]
pub async fn avatar(state: St<'_>, jid: String) -> Result<Option<String>> {
    check_jid(&jid)?;
    match state.avatars.get(&jid) {
        Avatar::Picture(bytes) => return Ok(Some(data_url("image/jpeg", &bytes))),
        Avatar::None => return Ok(None),
        Avatar::Unknown => {}
    }
    let Ok(client) = wa(&state).await else {
        return Ok(state
            .avatars
            .stale(&jid)
            .map(|b| data_url("image/jpeg", &b)));
    };
    let _slot = state
        .avatar_slots
        .acquire()
        .await
        .map_err(|_| Error::NotConnected)?;
    // Another request may have filled the cache while this one waited.
    if let Avatar::Picture(bytes) = state.avatars.get(&jid) {
        return Ok(Some(data_url("image/jpeg", &bytes)));
    }
    match client.profile_picture(&jid).await {
        Ok(picture) => {
            state.avatars.put(&jid, picture.as_deref())?;
            Ok(picture.map(|b| data_url("image/jpeg", &b)))
        }
        Err(e) => {
            log::debug!("profile picture unavailable: {e}");
            Ok(state
                .avatars
                .stale(&jid)
                .map(|b| data_url("image/jpeg", &b)))
        }
    }
}

async fn media_bytes(state: &AppState, jid: &str, id: &str) -> Result<(Message, Vec<u8>)> {
    let message = state
        .store
        .get_message(jid, id)?
        .ok_or_else(|| Error::Invalid("message".into()))?;
    if let Some(bytes) = state.media.get(jid, id) {
        return Ok((message, bytes));
    }
    let (kind, keys) = state
        .store
        .get_media(jid, id)?
        .ok_or_else(|| Error::Invalid("no media".into()))?;
    let bytes = wa(state).await?.download(&kind, &keys).await?;
    state.media.put(jid, id, &bytes)?;
    Ok((message, bytes))
}

fn mime_of(m: &Message) -> String {
    let base = m.mime.split(';').next().unwrap_or("").trim();
    let playable = ["image/", "video/", "audio/"]
        .iter()
        .any(|p| base.starts_with(p));
    let safe = playable
        && base.len() < 80
        && base
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'-' | b'+'));
    if safe {
        base.to_string()
    } else {
        match m.kind.as_str() {
            "image" => "image/jpeg",
            "sticker" => "image/webp",
            "video" => "video/mp4",
            "audio" | "voice" => "audio/ogg",
            _ => "application/octet-stream",
        }
        .to_string()
    }
}

/// A message's media as a data URL, downloading it the first time.
#[tauri::command]
pub async fn media(state: St<'_>, jid: String, id: String) -> Result<String> {
    check_jid(&jid)?;
    check_id(&id)?;
    let (message, bytes) = media_bytes(&state, &jid, &id).await?;
    Ok(data_url(&mime_of(&message), &bytes))
}

fn extension_for(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "video/mp4" => "mp4",
        "audio/ogg" => "ogg",
        "audio/mp4" | "audio/m4a" | "audio/x-m4a" => "m4a",
        "audio/mpeg" => "mp3",
        "application/pdf" => "pdf",
        _ => "bin",
    }
}

/// Saves a message's media to Downloads and shows it in Finder.
#[tauri::command]
pub async fn save_media(state: St<'_>, jid: String, id: String) -> Result<String> {
    check_jid(&jid)?;
    check_id(&id)?;
    let (message, bytes) = media_bytes(&state, &jid, &id).await?;
    let name = if message.file_name.is_empty() {
        let when = chrono::DateTime::from_timestamp(message.ts, 0)
            .map(|d| d.format("%Y-%m-%d at %H.%M.%S").to_string())
            .unwrap_or_default();
        format!("Notchat {when}.{}", extension_for(&mime_of(&message)))
    } else {
        message.file_name.clone()
    };
    let path = unique_download(&name)?;
    std::fs::write(&path, bytes)?;
    reveal(&path);
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn send_media(
    app: AppHandle,
    state: St<'_>,
    jid: String,
    kind: SendKind,
    name: String,
    mime: String,
    base64: String,
    caption: String,
    seconds: u32,
) -> Result<Message> {
    check_jid(&jid)?;
    if base64.len() > MAX_MEDIA_BYTES / 3 * 4 + 8 {
        return Err(Error::Invalid("file size".into()));
    }
    let data = base64::engine::general_purpose::STANDARD
        .decode(base64.as_bytes())
        .map_err(|_| Error::Invalid("file encoding".into()))?;
    let client = wa(&state).await?;
    let local = data.clone();
    let message = client
        .send_media(
            &jid,
            Outgoing {
                kind,
                data,
                mime,
                file_name: name.chars().take(200).collect(),
                caption,
                seconds: seconds.min(60 * 60 * 6),
            },
        )
        .await?;
    // Keep our own copy so the message shows without downloading it back.
    if let Err(e) = state.media.put(&jid, &message.id, &local) {
        log::warn!("could not cache sent media: {e}");
    }
    let _ = app.emit(EVENT_MESSAGE, &message);
    changed(&app);
    Ok(message)
}

// ---- lists ------------------------------------------------------------------

fn check_list_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 40 {
        return Err(Error::Invalid("list name".into()));
    }
    Ok(name.to_string())
}

#[tauri::command]
pub fn list_lists(state: St<'_>) -> Result<Vec<ChatList>> {
    state.store.lists()
}

#[tauri::command]
pub fn create_list(app: AppHandle, state: St<'_>, name: String) -> Result<i64> {
    let name = check_list_name(&name)?;
    if state.store.lists()?.len() >= 20 {
        return Err(Error::Invalid("too many lists".into()));
    }
    let id = state.store.create_list(&name)?;
    changed(&app);
    Ok(id)
}

#[tauri::command]
pub fn rename_list(app: AppHandle, state: St<'_>, id: i64, name: String) -> Result<()> {
    state.store.rename_list(id, &check_list_name(&name)?)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn delete_list(app: AppHandle, state: St<'_>, id: i64) -> Result<()> {
    state.store.delete_list(id)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn set_list_member(
    app: AppHandle,
    state: St<'_>,
    id: i64,
    jid: String,
    member: bool,
) -> Result<()> {
    check_jid(&jid)?;
    state.store.set_list_member(id, &jid, member)?;
    changed(&app);
    Ok(())
}

// ---- storage ----------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct ChatStorage {
    #[serde(flatten)]
    usage: ChatUsage,
    /// Bytes of downloaded media kept on this Mac.
    cached: u64,
}

#[derive(serde::Serialize)]
pub struct StorageInfo {
    database: u64,
    media: u64,
    avatars: u64,
    session: u64,
    chats: Vec<ChatStorage>,
}

fn file_size(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map_or(0, |m| m.len())
}

#[tauri::command]
pub fn storage_info(state: St<'_>) -> Result<StorageInfo> {
    let db = state.data_dir.join("notchat.db");
    let database = file_size(&db)
        + file_size(&db.with_extension("db-wal"))
        + file_size(&db.with_extension("db-shm"));
    let session = state.session_db();
    let session = file_size(&session) + file_size(&session.with_extension("db-wal"));
    let chats = state
        .store
        .usage()?
        .into_iter()
        .map(|usage| ChatStorage {
            cached: state.media.chat_size(&usage.jid),
            usage,
        })
        .collect();
    Ok(StorageInfo {
        database,
        media: state.media.total_size(),
        avatars: state.avatars.total_size(),
        session,
        chats,
    })
}

/// Deletes downloaded media (of one chat, or all). Messages stay; media can
/// be downloaded again while WhatsApp still keeps it.
#[tauri::command]
pub fn storage_clear_media(state: St<'_>, jid: Option<String>) -> Result<()> {
    match jid {
        Some(jid) => {
            check_jid(&jid)?;
            state.media.clear_chat(&jid);
        }
        None => state.media.clear_all(),
    }
    Ok(())
}

#[tauri::command]
pub fn storage_clear_avatars(state: St<'_>) {
    state.avatars.clear_all();
}

/// Deletes messages older than `days` from this Mac (not from the phone)
/// and compacts the database.
#[tauri::command]
pub async fn storage_delete_older(app: AppHandle, state: St<'_>, days: u32) -> Result<usize> {
    if !(1..=3650).contains(&days) {
        return Err(Error::Invalid("days".into()));
    }
    let cutoff = now() - i64::from(days) * 86_400;
    let deleted = state.store.delete_messages_before(cutoff)?;
    let store = Arc::clone(&state.store);
    tauri::async_runtime::spawn_blocking(move || store.vacuum())
        .await
        .map_err(|e| Error::Invalid(e.to_string()))??;
    changed(&app);
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_validated() {
        assert!(check_id("3EB0A1B2C3D4").is_ok());
        assert!(check_id("../etc").is_err());
        assert!(check_id("").is_err());
    }

    #[test]
    fn mime_is_sanitized() {
        let mut m = Message {
            kind: "voice".into(),
            mime: "audio/ogg; codecs=opus".into(),
            ..Default::default()
        };
        assert_eq!(mime_of(&m), "audio/ogg");
        m.mime = "text/html;<script>".into();
        assert_eq!(
            mime_of(&m),
            "audio/ogg",
            "only media types reach the webview"
        );
        m.mime = "image/svg+xml\"onload".into();
        assert_eq!(mime_of(&m), "audio/ogg");
    }
}
