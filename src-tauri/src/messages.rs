//! Message actions (react, edit, delete, star, pin, forward), the account's
//! stickers and the private notes chat. Every input is validated here.

use crate::chats::{check_id, data_url, media_bytes, mime_of, wa};
use crate::commands::check_jid;
use crate::session::{EVENT_CHATS, EVENT_MESSAGE, EVENT_MESSAGE_CHANGED};
use crate::state::AppState;
use base64::Engine;
use notchat_core::media::MAX_MEDIA_BYTES;
use notchat_core::store::{Message, Note, Sticker};
use notchat_core::wa::{Outgoing, SendKind};
use notchat_core::{Error, Result};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

type St<'a> = State<'a, Arc<AppState>>;

/// Media cache "chat" folders for things that are not WhatsApp chats.
const STICKERS: &str = "stickers@notchat";
const NOTES: &str = "notes@notchat";

fn now() -> i64 {
    crate::assistant::now()
}

fn changed(app: &AppHandle, jid: &str, id: &str) {
    let _ = app.emit(
        EVENT_MESSAGE_CHANGED,
        serde_json::json!({ "chat_jid": jid, "id": id }),
    );
    let _ = app.emit(EVENT_CHATS, ());
}

fn check(jid: &str, id: &str) -> Result<()> {
    check_jid(jid)?;
    check_id(id)
}

/// A message as stored now, with its reactions.
#[tauri::command]
pub fn get_message(state: St<'_>, jid: String, id: String) -> Result<Option<Message>> {
    check(&jid, &id)?;
    state.store.get_message(&jid, &id)
}

#[tauri::command]
pub async fn msg_react(
    app: AppHandle,
    state: St<'_>,
    jid: String,
    id: String,
    emoji: String,
) -> Result<()> {
    check(&jid, &id)?;
    wa(&state).await?.react(&jid, &id, &emoji).await?;
    changed(&app, &jid, &id);
    Ok(())
}

/// Votes in a poll; an empty list clears our vote.
#[tauri::command]
pub async fn msg_vote_poll(
    app: AppHandle,
    state: St<'_>,
    jid: String,
    id: String,
    options: Vec<String>,
) -> Result<()> {
    check(&jid, &id)?;
    if options.len() > 12 || options.iter().any(|o| o.is_empty() || o.len() > 1024) {
        return Err(Error::Invalid("poll choice".into()));
    }
    wa(&state).await?.vote_poll(&jid, &id, &options).await?;
    changed(&app, &jid, &id);
    Ok(())
}

#[tauri::command]
pub async fn msg_edit(
    app: AppHandle,
    state: St<'_>,
    jid: String,
    id: String,
    text: String,
) -> Result<()> {
    check(&jid, &id)?;
    wa(&state).await?.edit(&jid, &id, &text).await?;
    changed(&app, &jid, &id);
    Ok(())
}

/// Deletes a message for everyone, or only on this account's devices.
#[tauri::command]
pub async fn msg_delete(
    app: AppHandle,
    state: St<'_>,
    jid: String,
    id: String,
    everyone: bool,
) -> Result<()> {
    check(&jid, &id)?;
    if everyone {
        wa(&state).await?.revoke(&jid, &id).await?;
        state.media.remove(&jid, &id);
    } else {
        match wa(&state).await {
            Ok(client) => {
                if let Err(e) = client.delete_for_me(&jid, &id).await {
                    log::warn!("delete for me not synced to the phone: {e}");
                }
            }
            Err(_) => state.store.delete_message(&jid, &id)?,
        }
        state.media.remove(&jid, &id);
    }
    changed(&app, &jid, &id);
    Ok(())
}

#[tauri::command]
pub async fn msg_star(
    app: AppHandle,
    state: St<'_>,
    jid: String,
    id: String,
    starred: bool,
) -> Result<()> {
    check(&jid, &id)?;
    match wa(&state).await {
        Ok(client) => {
            if let Err(e) = client.star(&jid, &id, starred).await {
                log::warn!("star not synced to the phone: {e}");
            }
        }
        Err(_) => state.store.set_starred(&jid, &id, starred)?,
    }
    changed(&app, &jid, &id);
    Ok(())
}

#[tauri::command]
pub async fn msg_pin(
    app: AppHandle,
    state: St<'_>,
    jid: String,
    id: String,
    pinned: bool,
) -> Result<()> {
    check(&jid, &id)?;
    wa(&state).await?.pin_message(&jid, &id, pinned).await?;
    changed(&app, &jid, &id);
    Ok(())
}

/// Forwards one message to up to five chats. Returns how many got it.
#[tauri::command]
pub async fn msg_forward(
    app: AppHandle,
    state: St<'_>,
    jid: String,
    id: String,
    to: Vec<String>,
) -> Result<usize> {
    check(&jid, &id)?;
    if to.is_empty() || to.len() > 5 {
        return Err(Error::Invalid("forward recipients".into()));
    }
    for t in &to {
        check_jid(t)?;
    }
    let client = wa(&state).await?;
    let mut sent = 0;
    for target in &to {
        match client.forward(&jid, &id, target).await {
            Ok(message) => {
                // Reuse the cached bytes so the copy shows without a download.
                if let Some(bytes) = state.media.get(&jid, &id) {
                    let _ = state.media.put(target, &message.id, &bytes);
                }
                let _ = app.emit(EVENT_MESSAGE, &message);
                sent += 1;
            }
            Err(e) => log::warn!("forward failed: {e}"),
        }
    }
    let _ = app.emit(EVENT_CHATS, ());
    if sent == 0 {
        return Err(Error::Wa("forward failed".into()));
    }
    Ok(sent)
}

#[tauri::command]
pub fn pinned_messages(state: St<'_>, jid: String) -> Result<Vec<Message>> {
    check_jid(&jid)?;
    state.store.pinned_messages(&jid)
}

#[tauri::command]
pub fn starred_messages(state: St<'_>) -> Result<Vec<Message>> {
    state.store.starred_messages(200)
}

// ---- stickers ---------------------------------------------------------------

fn check_sticker(id: &str) -> Result<()> {
    if id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(Error::Invalid("sticker".into()))
    }
}

#[tauri::command]
pub fn stickers(state: St<'_>) -> Result<Vec<Sticker>> {
    state.store.stickers(120)
}

/// A sticker image as a data URL, downloaded once and then cached.
#[tauri::command]
pub async fn sticker_image(state: St<'_>, id: String) -> Result<String> {
    check_sticker(&id)?;
    if let Some(bytes) = state.media.get(STICKERS, &id) {
        return Ok(data_url("image/webp", &bytes));
    }
    let bytes = wa(&state).await?.download_sticker(&id).await?;
    state.media.put(STICKERS, &id, &bytes)?;
    Ok(data_url("image/webp", &bytes))
}

#[tauri::command]
pub async fn sticker_send(
    app: AppHandle,
    state: St<'_>,
    jid: String,
    id: String,
) -> Result<Message> {
    check_jid(&jid)?;
    check_sticker(&id)?;
    let message = wa(&state).await?.send_sticker(&jid, &id).await?;
    if let Some(bytes) = state.media.get(STICKERS, &id) {
        let _ = state.media.put(&jid, &message.id, &bytes);
    }
    let _ = app.emit(EVENT_MESSAGE, &message);
    let _ = app.emit(EVENT_CHATS, ());
    Ok(message)
}

#[tauri::command]
pub fn sticker_favorite(state: St<'_>, id: String, favorite: bool) -> Result<()> {
    check_sticker(&id)?;
    state.store.set_sticker_favorite(&id, favorite)
}

/// Saves the sticker of a message to the favourites, ready to send later.
#[tauri::command]
pub fn sticker_save(state: St<'_>, jid: String, id: String) -> Result<String> {
    check(&jid, &id)?;
    notchat_core::wa::save_sticker_from_message(&state.store, &jid, &id)
}

#[tauri::command]
pub fn sticker_remove(state: St<'_>, id: String) -> Result<()> {
    check_sticker(&id)?;
    state.media.remove(STICKERS, &id);
    state.store.remove_sticker(&id)
}

// ---- notes ------------------------------------------------------------------

const COLORS: [&str; 6] = ["", "yellow", "pink", "blue", "green", "violet"];

fn check_note(note: &Note) -> Result<()> {
    if note.text.len() > 20_000 || !COLORS.contains(&note.color.as_str()) {
        return Err(Error::Invalid("note".into()));
    }
    Ok(())
}

fn note_key(id: i64) -> String {
    format!("n{id}")
}

#[tauri::command]
pub fn notes_list(state: St<'_>) -> Result<Vec<Note>> {
    state.store.list_notes()
}

#[tauri::command]
pub fn note_add(state: St<'_>, text: String, color: String) -> Result<Note> {
    let note = Note {
        text: text.trim().to_string(),
        color,
        ts: now(),
        kind: "text".into(),
        ..Default::default()
    };
    check_note(&note)?;
    if note.text.is_empty() {
        return Err(Error::Invalid("note".into()));
    }
    state.store.add_note(&note)
}

fn kind_for(mime: &str) -> &'static str {
    match mime.split('/').next().unwrap_or("") {
        "image" => "image",
        "video" => "video",
        "audio" => "audio",
        _ => "document",
    }
}

fn add_file_note(
    state: &AppState,
    text: String,
    name: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<Note> {
    if bytes.is_empty() || bytes.len() > MAX_MEDIA_BYTES || mime.len() > 100 {
        return Err(Error::Invalid("file".into()));
    }
    let note = Note {
        text,
        ts: now(),
        kind: kind_for(mime).into(),
        mime: mime.to_string(),
        file_name: name.chars().take(200).collect(),
        size: i64::try_from(bytes.len()).unwrap_or(i64::MAX),
        ..Default::default()
    };
    check_note(&note)?;
    let note = state.store.add_note(&note)?;
    if let Err(e) = state.note_files.put(NOTES, &note_key(note.id), bytes) {
        let _ = state.store.delete_note(note.id);
        return Err(e);
    }
    Ok(note)
}

#[tauri::command]
pub fn note_add_file(
    state: St<'_>,
    text: String,
    name: String,
    mime: String,
    base64: String,
) -> Result<Note> {
    if base64.len() > MAX_MEDIA_BYTES / 3 * 4 + 8 {
        return Err(Error::Invalid("file size".into()));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64.as_bytes())
        .map_err(|_| Error::Invalid("file encoding".into()))?;
    add_file_note(&state, text.trim().to_string(), &name, &mime, &bytes)
}

/// Copies a WhatsApp message (text and media) into the notes.
#[tauri::command]
pub async fn note_from_message(state: St<'_>, jid: String, id: String) -> Result<Note> {
    check(&jid, &id)?;
    let message = state
        .store
        .get_message(&jid, &id)?
        .ok_or_else(|| Error::Invalid("message".into()))?;
    if message.has_media {
        let (m, bytes) = media_bytes(&state, &jid, &id).await?;
        let mime = if m.mime.is_empty() {
            mime_of(&m)
        } else {
            m.mime.clone()
        };
        add_file_note(&state, m.text.clone(), &m.file_name, &mime, &bytes)
    } else if !message.text.is_empty() {
        let note = Note {
            text: message.text,
            ts: now(),
            kind: "text".into(),
            ..Default::default()
        };
        state.store.add_note(&note)
    } else {
        Err(Error::Invalid("nothing to save".into()))
    }
}

#[tauri::command]
pub fn note_update(state: St<'_>, note: Note) -> Result<()> {
    check_note(&note)?;
    state.store.update_note(&note)
}

#[tauri::command]
pub fn note_delete(state: St<'_>, id: i64) -> Result<()> {
    state.note_files.remove(NOTES, &note_key(id));
    state.store.delete_note(id)
}

#[tauri::command]
pub fn note_media(state: St<'_>, id: i64) -> Result<String> {
    let note = state
        .store
        .get_note(id)?
        .ok_or_else(|| Error::Invalid("note".into()))?;
    let bytes = state
        .note_files
        .get(NOTES, &note_key(id))
        .ok_or_else(|| Error::Invalid("note file".into()))?;
    let mime = if note.kind == "document" {
        "application/octet-stream".to_string()
    } else {
        note.mime.clone()
    };
    Ok(data_url(&mime, &bytes))
}

/// Saves a note's file to Downloads and shows it in Finder.
#[tauri::command]
pub fn note_save_file(state: St<'_>, id: i64) -> Result<String> {
    let note = state
        .store
        .get_note(id)?
        .ok_or_else(|| Error::Invalid("note".into()))?;
    let bytes = state
        .note_files
        .get(NOTES, &note_key(id))
        .ok_or_else(|| Error::Invalid("note file".into()))?;
    let name = if note.file_name.is_empty() {
        format!(
            "Notchat note {id}.{}",
            crate::chats::extension_for(&note.mime)
        )
    } else {
        note.file_name.clone()
    };
    let path = crate::chats::unique_download(&name)?;
    std::fs::write(&path, bytes)?;
    crate::chats::reveal(&path);
    Ok(path.to_string_lossy().into_owned())
}

/// Sends a note to WhatsApp chats (forward). Returns how many got it.
#[tauri::command]
pub async fn note_send(app: AppHandle, state: St<'_>, id: i64, to: Vec<String>) -> Result<usize> {
    if to.is_empty() || to.len() > 5 {
        return Err(Error::Invalid("forward recipients".into()));
    }
    for t in &to {
        check_jid(t)?;
    }
    let note = state
        .store
        .get_note(id)?
        .ok_or_else(|| Error::Invalid("note".into()))?;
    let client = wa(&state).await?;
    let bytes = (note.kind != "text")
        .then(|| state.note_files.get(NOTES, &note_key(id)))
        .flatten();
    let mut sent = 0;
    for target in &to {
        let result = match &bytes {
            Some(data) => {
                let kind = match note.kind.as_str() {
                    "image" => SendKind::Image,
                    "video" => SendKind::Video,
                    "audio" => SendKind::Audio,
                    _ => SendKind::Document,
                };
                client
                    .send_media(
                        target,
                        Outgoing {
                            kind,
                            data: data.clone(),
                            mime: note.mime.clone(),
                            file_name: note.file_name.clone(),
                            caption: note.text.clone(),
                            seconds: 0,
                        },
                    )
                    .await
            }
            None => client.send_text(target, &note.text, false).await,
        };
        match result {
            Ok(message) => {
                if let Some(data) = &bytes {
                    let _ = state.media.put(target, &message.id, data);
                }
                let _ = app.emit(EVENT_MESSAGE, &message);
                sent += 1;
            }
            Err(e) => log::warn!("note send failed: {e}"),
        }
    }
    let _ = app.emit(EVENT_CHATS, ());
    if sent == 0 {
        return Err(Error::Wa("send failed".into()));
    }
    Ok(sent)
}
