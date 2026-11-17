//! Encrypted application database (SQLCipher). Holds chats, messages, the
//! assistant conversation and settings. Message content is never logged.

use crate::error::{Error, Result};
use crate::keychain::is_valid_key;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chat {
    pub jid: String,
    pub name: String,
    pub is_group: bool,
    pub last_ts: i64,
    pub last_preview: String,
    pub unread: i64,
    pub auto_reply: bool,
    pub instructions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub id: String,
    pub chat_jid: String,
    pub sender_jid: String,
    pub sender_name: String,
    pub from_me: bool,
    pub ts: i64,
    /// "text", "image", "video", "audio", "document", "sticker", "other"
    pub kind: String,
    pub text: String,
    pub by_ai: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiMessage {
    pub id: i64,
    /// "user", "assistant", "tool", "action"
    pub role: String,
    pub content: String,
    pub ts: i64,
    /// JSON payload: attachments for user turns, tool/action data otherwise.
    pub meta: String,
}

pub struct Store {
    conn: Mutex<Connection>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS chats (
  jid TEXT PRIMARY KEY,
  name TEXT NOT NULL DEFAULT '',
  is_group INTEGER NOT NULL DEFAULT 0,
  last_ts INTEGER NOT NULL DEFAULT 0,
  last_preview TEXT NOT NULL DEFAULT '',
  unread INTEGER NOT NULL DEFAULT 0,
  auto_reply INTEGER NOT NULL DEFAULT 0,
  instructions TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS messages (
  chat_jid TEXT NOT NULL,
  id TEXT NOT NULL,
  sender_jid TEXT NOT NULL DEFAULT '',
  sender_name TEXT NOT NULL DEFAULT '',
  from_me INTEGER NOT NULL DEFAULT 0,
  ts INTEGER NOT NULL,
  kind TEXT NOT NULL DEFAULT 'text',
  text TEXT NOT NULL DEFAULT '',
  by_ai INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (chat_jid, id)
);
CREATE INDEX IF NOT EXISTS idx_messages_chat_ts ON messages(chat_jid, ts);
CREATE TABLE IF NOT EXISTS ai_messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  role TEXT NOT NULL,
  content TEXT NOT NULL,
  ts INTEGER NOT NULL,
  meta TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
";

impl Store {
    /// Opens (or creates) the database at `path`, keyed with a 64-char hex key.
    pub fn open(path: &Path, key_hex: &str) -> Result<Self> {
        if !is_valid_key(key_hex) {
            return Err(Error::Invalid("database key".into()));
        }
        let conn = Connection::open(path)?;
        restrict_permissions(path);
        // Raw-key form: no KDF pass, the key is already 256 random bits.
        conn.execute_batch(&format!("PRAGMA key = \"x'{key_hex}'\";"))?;
        // Fails here if the key is wrong.
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
            r.get::<_, i64>(0)
        })?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A poisoned lock only means another thread panicked mid-query; the
        // connection itself is still usable.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ---- chats -----------------------------------------------------------

    pub fn list_chats(&self) -> Result<Vec<Chat>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT jid,name,is_group,last_ts,last_preview,unread,auto_reply,instructions
             FROM chats ORDER BY last_ts DESC",
        )?;
        let rows = stmt.query_map([], row_to_chat)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get_chat(&self, jid: &str) -> Result<Option<Chat>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                "SELECT jid,name,is_group,last_ts,last_preview,unread,auto_reply,instructions
                 FROM chats WHERE jid = ?1",
                [jid],
                row_to_chat,
            )
            .optional()?)
    }

    /// Sets a chat's display name, creating the chat row if needed. An empty
    /// name never overwrites an existing one.
    pub fn upsert_chat_name(&self, jid: &str, name: &str, is_group: bool) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO chats (jid, name, is_group) VALUES (?1, ?2, ?3)
             ON CONFLICT(jid) DO UPDATE SET
               name = CASE WHEN excluded.name != '' THEN excluded.name ELSE chats.name END",
            params![jid, name, is_group],
        )?;
        Ok(())
    }

    pub fn mark_read(&self, jid: &str) -> Result<()> {
        self.lock()
            .execute("UPDATE chats SET unread = 0 WHERE jid = ?1", [jid])?;
        Ok(())
    }

    pub fn set_chat_auto_reply(&self, jid: &str, enabled: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE chats SET auto_reply = ?2 WHERE jid = ?1",
            params![jid, enabled],
        )?;
        Ok(())
    }

    pub fn set_chat_instructions(&self, jid: &str, instructions: &str) -> Result<()> {
        self.lock().execute(
            "UPDATE chats SET instructions = ?2 WHERE jid = ?1",
            params![jid, instructions],
        )?;
        Ok(())
    }

    // ---- messages --------------------------------------------------------

    /// Stores a message and updates its chat summary. Returns `false` when the
    /// message was already stored (redelivery), so callers can skip side effects.
    pub fn insert_message(&self, m: &Message, is_group: bool) -> Result<bool> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO messages
               (chat_jid,id,sender_jid,sender_name,from_me,ts,kind,text,by_ai)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                m.chat_jid,
                m.id,
                m.sender_jid,
                m.sender_name,
                m.from_me,
                m.ts,
                m.kind,
                m.text,
                m.by_ai
            ],
        )? > 0;
        if inserted {
            let preview = preview_of(m);
            let unread_inc = i64::from(!m.from_me);
            tx.execute(
                "INSERT INTO chats (jid, is_group, last_ts, last_preview, unread)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(jid) DO UPDATE SET
                   last_preview = CASE WHEN excluded.last_ts >= chats.last_ts
                                       THEN excluded.last_preview ELSE chats.last_preview END,
                   last_ts = MAX(chats.last_ts, excluded.last_ts),
                   unread = chats.unread + excluded.unread",
                params![m.chat_jid, is_group, m.ts, preview, unread_inc],
            )?;
        }
        tx.commit()?;
        Ok(inserted)
    }

    /// Returns up to `limit` messages older than `before_ts` (or the newest
    /// ones when `None`), in chronological order.
    pub fn list_messages(
        &self,
        chat_jid: &str,
        before_ts: Option<i64>,
        limit: u32,
    ) -> Result<Vec<Message>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id,chat_jid,sender_jid,sender_name,from_me,ts,kind,text,by_ai
             FROM messages WHERE chat_jid = ?1 AND ts < ?2
             ORDER BY ts DESC, rowid DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            params![chat_jid, before_ts.unwrap_or(i64::MAX), limit.min(500)],
            row_to_message,
        )?;
        let mut out = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        out.reverse();
        Ok(out)
    }

    pub fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<Message>> {
        let pattern = format!("%{}%", escape_like(query));
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id,chat_jid,sender_jid,sender_name,from_me,ts,kind,text,by_ai
             FROM messages WHERE text LIKE ?1 ESCAPE '\\'
             ORDER BY ts DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit.min(200)], row_to_message)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Finds chats whose name contains `query` (case-insensitive).
    pub fn find_chats(&self, query: &str) -> Result<Vec<Chat>> {
        let pattern = format!("%{}%", escape_like(query));
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT jid,name,is_group,last_ts,last_preview,unread,auto_reply,instructions
             FROM chats WHERE name LIKE ?1 ESCAPE '\\' ORDER BY last_ts DESC LIMIT 20",
        )?;
        let rows = stmt.query_map([pattern], row_to_chat)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    // ---- assistant conversation -----------------------------------------

    pub fn add_ai_message(&self, role: &str, content: &str, ts: i64, meta: &str) -> Result<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO ai_messages (role, content, ts, meta) VALUES (?1,?2,?3,?4)",
            params![role, content, ts, meta],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_ai_message_meta(&self, id: i64, meta: &str) -> Result<()> {
        self.lock().execute(
            "UPDATE ai_messages SET meta = ?2 WHERE id = ?1",
            params![id, meta],
        )?;
        Ok(())
    }

    pub fn get_ai_message(&self, id: i64) -> Result<Option<AiMessage>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT id,role,content,ts,meta FROM ai_messages WHERE id = ?1",
                [id],
                row_to_ai,
            )
            .optional()?)
    }

    pub fn list_ai_messages(&self, limit: u32) -> Result<Vec<AiMessage>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare("SELECT id,role,content,ts,meta FROM ai_messages ORDER BY id DESC LIMIT ?1")?;
        let rows = stmt.query_map([limit.min(1000)], row_to_ai)?;
        let mut out = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        out.reverse();
        Ok(out)
    }

    pub fn clear_ai_messages(&self) -> Result<()> {
        self.lock().execute("DELETE FROM ai_messages", [])?;
        Ok(())
    }

    // ---- settings --------------------------------------------------------

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .lock()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.lock().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Reads a JSON-encoded setting, falling back to `T::default()` when the
    /// key is missing or unreadable.
    pub fn get_json<T: serde::de::DeserializeOwned + Default>(&self, key: &str) -> T {
        self.get_setting(key)
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default()
    }

    pub fn set_json<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.set_setting(key, &serde_json::to_string(value)?)
    }

    /// Removes all WhatsApp data (used on logout). Settings are kept.
    pub fn clear_whatsapp_data(&self) -> Result<()> {
        self.lock()
            .execute_batch("DELETE FROM messages; DELETE FROM chats;")?;
        Ok(())
    }
}

fn row_to_chat(r: &rusqlite::Row<'_>) -> rusqlite::Result<Chat> {
    Ok(Chat {
        jid: r.get(0)?,
        name: r.get(1)?,
        is_group: r.get(2)?,
        last_ts: r.get(3)?,
        last_preview: r.get(4)?,
        unread: r.get(5)?,
        auto_reply: r.get(6)?,
        instructions: r.get(7)?,
    })
}

fn row_to_message(r: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    Ok(Message {
        id: r.get(0)?,
        chat_jid: r.get(1)?,
        sender_jid: r.get(2)?,
        sender_name: r.get(3)?,
        from_me: r.get(4)?,
        ts: r.get(5)?,
        kind: r.get(6)?,
        text: r.get(7)?,
        by_ai: r.get(8)?,
    })
}

fn row_to_ai(r: &rusqlite::Row<'_>) -> rusqlite::Result<AiMessage> {
    Ok(AiMessage {
        id: r.get(0)?,
        role: r.get(1)?,
        content: r.get(2)?,
        ts: r.get(3)?,
        meta: r.get(4)?,
    })
}

fn preview_of(m: &Message) -> String {
    let base = if m.text.is_empty() {
        format!("[{}]", m.kind)
    } else {
        m.text.clone()
    };
    base.chars().take(120).collect()
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Restricts a file to its owner (0600). Best effort.
pub fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keychain::generate_key;

    fn msg(chat: &str, id: &str, ts: i64, text: &str, from_me: bool) -> Message {
        Message {
            id: id.into(),
            chat_jid: chat.into(),
            sender_jid: "s@s.whatsapp.net".into(),
            sender_name: "Ana".into(),
            from_me,
            ts,
            kind: "text".into(),
            text: text.into(),
            by_ai: false,
        }
    }

    #[test]
    fn roundtrip_and_chat_summary() {
        let dir = tempfile::tempdir().unwrap();
        let key = generate_key();
        let path = dir.path().join("t.db");
        let s = Store::open(&path, &key).unwrap();
        assert!(s
            .insert_message(&msg("a", "1", 10, "hola", false), false)
            .unwrap());
        assert!(s
            .insert_message(&msg("a", "2", 20, "chau", true), false)
            .unwrap());
        // Redelivery is ignored and does not bump unread.
        assert!(!s
            .insert_message(&msg("a", "1", 10, "hola", false), false)
            .unwrap());
        // An older message arriving late must not replace the preview.
        assert!(s
            .insert_message(&msg("a", "0", 5, "viejo", false), false)
            .unwrap());

        let chats = s.list_chats().unwrap();
        assert_eq!(chats.len(), 1);
        assert_eq!(chats[0].last_preview, "chau");
        assert_eq!(chats[0].last_ts, 20);
        assert_eq!(chats[0].unread, 2);

        let msgs = s.list_messages("a", None, 50).unwrap();
        assert_eq!(
            msgs.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["0", "1", "2"]
        );
        let older = s.list_messages("a", Some(20), 50).unwrap();
        assert_eq!(older.len(), 2);

        s.mark_read("a").unwrap();
        assert_eq!(s.get_chat("a").unwrap().unwrap().unread, 0);
    }

    #[test]
    fn names_and_search() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("t.db"), &generate_key()).unwrap();
        s.upsert_chat_name("j", "Juan Pérez", false).unwrap();
        s.upsert_chat_name("j", "", false).unwrap();
        assert_eq!(s.get_chat("j").unwrap().unwrap().name, "Juan Pérez");
        assert_eq!(s.find_chats("juan").unwrap().len(), 1);
        s.insert_message(&msg("j", "1", 1, "100% seguro", false), false)
            .unwrap();
        assert_eq!(s.search_messages("100%", 10).unwrap().len(), 1);
        assert_eq!(s.search_messages("%", 10).unwrap().len(), 1);
        assert_eq!(s.search_messages("_", 10).unwrap().len(), 0);
    }

    #[test]
    fn wrong_key_is_rejected_and_file_is_encrypted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let s = Store::open(&path, &generate_key()).unwrap();
            s.set_setting("k", "plaintext-marker").unwrap();
        }
        assert!(Store::open(&path, &generate_key()).is_err());
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.starts_with(b"SQLite format 3"));
        assert!(!raw
            .windows(b"plaintext-marker".len())
            .any(|w| w == b"plaintext-marker"));
    }

    #[test]
    fn settings_json() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("t.db"), &generate_key()).unwrap();
        let v: Vec<String> = s.get_json("missing");
        assert!(v.is_empty());
        s.set_json("list", &vec!["a".to_string()]).unwrap();
        let v: Vec<String> = s.get_json("list");
        assert_eq!(v, ["a"]);
    }
}
