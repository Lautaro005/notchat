//! Encrypted application database (SQLCipher). Holds chats, messages, the
//! assistant conversation and settings. Message content is never logged.

use crate::error::{Error, Result};
use crate::keychain::is_valid_key;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Chat {
    pub jid: String,
    pub name: String,
    pub is_group: bool,
    pub last_ts: i64,
    pub last_preview: String,
    pub unread: i64,
    pub auto_reply: bool,
    pub instructions: String,
    #[serde(default)]
    pub archived: bool,
    /// Pin timestamp; 0 when the chat is not pinned.
    #[serde(default)]
    pub pinned: i64,
    /// Unix seconds until which the chat is muted; -1 forever, 0 not muted.
    #[serde(default)]
    pub muted_until: i64,
    #[serde(default)]
    pub marked_unread: bool,
}

/// A user-made group of chats ("list"), stored only on this Mac.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ChatList {
    pub id: i64,
    pub name: String,
    pub jids: Vec<String>,
}

/// Disk usage of one chat, for the storage screen.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct ChatUsage {
    pub jid: String,
    pub name: String,
    pub messages: i64,
    pub media: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
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
    /// Media details; empty for text messages.
    #[serde(default)]
    pub mime: String,
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub size: i64,
    /// Duration of audio/video, in seconds.
    #[serde(default)]
    pub seconds: i64,
    /// True when the media can be downloaded (its keys are stored).
    #[serde(default)]
    pub has_media: bool,
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
CREATE TABLE IF NOT EXISTS contacts (
  jid TEXT PRIMARY KEY,
  name TEXT NOT NULL DEFAULT '',
  push_name TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS lists (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS list_members (
  list_id INTEGER NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
  jid TEXT NOT NULL,
  PRIMARY KEY (list_id, jid)
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
        migrate(&conn)?;
        // Chats named after the contact book (or push name) when WhatsApp
        // did not give the chat a name of its own.
        conn.execute_batch(
            "DROP VIEW IF EXISTS chat_view;
             CREATE TEMP VIEW chat_view AS
             SELECT c.jid,
                    CASE WHEN c.name != '' THEN c.name
                         ELSE COALESCE(NULLIF(ct.name, ''), NULLIF(ct.push_name, ''), '') END
                      AS name,
                    c.is_group, c.last_ts, c.last_preview, c.unread, c.auto_reply,
                    c.instructions, c.archived, c.pinned, c.muted_until, c.marked_unread
             FROM chats c LEFT JOIN contacts ct ON ct.jid = c.jid;",
        )?;
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
            "SELECT * FROM chat_view WHERE last_ts > 0 OR pinned > 0
             ORDER BY pinned DESC, last_ts DESC",
        )?;
        let rows = stmt.query_map([], row_to_chat)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get_chat(&self, jid: &str) -> Result<Option<Chat>> {
        let conn = self.lock();
        Ok(conn
            .query_row("SELECT * FROM chat_view WHERE jid = ?1", [jid], row_to_chat)
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

    /// Saves a contact-book name (`push = false`) or the name the contact
    /// set for themselves (`push = true`). Does not create a chat.
    pub fn upsert_contact(&self, jid: &str, name: &str, push: bool) -> Result<()> {
        if name.is_empty() {
            return Ok(());
        }
        let sql = if push {
            "INSERT INTO contacts (jid, push_name) VALUES (?1, ?2)
             ON CONFLICT(jid) DO UPDATE SET push_name = ?2"
        } else {
            "INSERT INTO contacts (jid, name) VALUES (?1, ?2)
             ON CONFLICT(jid) DO UPDATE SET name = ?2"
        };
        self.lock().execute(sql, params![jid, name])?;
        Ok(())
    }

    /// Makes sure a group shows in the list, dated at least `ts`.
    pub fn upsert_group(&self, jid: &str, subject: &str, ts: i64) -> Result<()> {
        self.lock().execute(
            "INSERT INTO chats (jid, name, is_group, last_ts) VALUES (?1, ?2, 1, ?3)
             ON CONFLICT(jid) DO UPDATE SET
               name = CASE WHEN excluded.name != '' THEN excluded.name ELSE chats.name END,
               is_group = 1,
               last_ts = MAX(chats.last_ts, excluded.last_ts)",
            params![jid, subject, ts],
        )?;
        Ok(())
    }

    pub fn mark_read(&self, jid: &str) -> Result<()> {
        self.lock().execute(
            "UPDATE chats SET unread = 0, marked_unread = 0 WHERE jid = ?1",
            [jid],
        )?;
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
        self.insert_message_full(m, is_group, None, true)
    }

    /// Like [`insert_message`](Self::insert_message), also storing the encoded
    /// media keys. `count_unread` is false for imported history, whose unread
    /// count comes from the phone instead.
    pub fn insert_message_full(
        &self,
        m: &Message,
        is_group: bool,
        media: Option<&[u8]>,
        count_unread: bool,
    ) -> Result<bool> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO messages
               (chat_jid,id,sender_jid,sender_name,from_me,ts,kind,text,by_ai,
                mime,file_name,size,seconds,media)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![
                m.chat_jid,
                m.id,
                m.sender_jid,
                m.sender_name,
                m.from_me,
                m.ts,
                m.kind,
                m.text,
                m.by_ai,
                m.mime,
                m.file_name,
                m.size,
                m.seconds,
                media
            ],
        )? > 0;
        if inserted {
            let preview = preview_of(m);
            let unread_inc = i64::from(count_unread && !m.from_me);
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

    /// The encoded media keys of a message, with its kind.
    pub fn get_media(&self, chat_jid: &str, id: &str) -> Result<Option<(String, Vec<u8>)>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT kind, media FROM messages
                 WHERE chat_jid = ?1 AND id = ?2 AND media IS NOT NULL",
                params![chat_jid, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    pub fn get_message(&self, chat_jid: &str, id: &str) -> Result<Option<Message>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT id,chat_jid,sender_jid,sender_name,from_me,ts,kind,text,by_ai,
                        mime,file_name,size,seconds,media IS NOT NULL
                 FROM messages WHERE chat_jid = ?1 AND id = ?2",
                params![chat_jid, id],
                row_to_message,
            )
            .optional()?)
    }

    /// Applies chat state imported from the phone's history.
    pub fn apply_history_chat(&self, h: &HistoryChat) -> Result<()> {
        self.lock().execute(
            "INSERT INTO chats (jid, name, is_group, last_ts, unread, archived, pinned,
                                muted_until, marked_unread)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(jid) DO UPDATE SET
               name = CASE WHEN excluded.name != '' THEN excluded.name ELSE chats.name END,
               last_ts = MAX(chats.last_ts, excluded.last_ts),
               unread = MAX(chats.unread, excluded.unread),
               archived = excluded.archived,
               pinned = excluded.pinned,
               muted_until = excluded.muted_until,
               marked_unread = excluded.marked_unread",
            params![
                h.jid,
                h.name,
                h.is_group,
                h.last_ts,
                h.unread,
                h.archived,
                h.pinned,
                h.muted_until,
                h.marked_unread
            ],
        )?;
        Ok(())
    }

    pub fn set_archived(&self, jid: &str, archived: bool) -> Result<()> {
        self.lock().execute(
            "INSERT INTO chats (jid, archived, pinned) VALUES (?1, ?2, 0)
             ON CONFLICT(jid) DO UPDATE SET archived = ?2,
               pinned = CASE WHEN ?2 THEN 0 ELSE chats.pinned END",
            params![jid, archived],
        )?;
        Ok(())
    }

    /// Pins (with the given timestamp) or unpins (`0`) a chat.
    pub fn set_pinned(&self, jid: &str, pinned: i64) -> Result<()> {
        self.lock().execute(
            "INSERT INTO chats (jid, pinned) VALUES (?1, ?2)
             ON CONFLICT(jid) DO UPDATE SET pinned = ?2",
            params![jid, pinned],
        )?;
        Ok(())
    }

    pub fn pinned_count(&self) -> Result<i64> {
        Ok(self
            .lock()
            .query_row("SELECT count(*) FROM chats WHERE pinned > 0", [], |r| {
                r.get(0)
            })?)
    }

    pub fn set_muted_until(&self, jid: &str, until: i64) -> Result<()> {
        self.lock().execute(
            "INSERT INTO chats (jid, muted_until) VALUES (?1, ?2)
             ON CONFLICT(jid) DO UPDATE SET muted_until = ?2",
            params![jid, until],
        )?;
        Ok(())
    }

    pub fn set_marked_unread(&self, jid: &str, unread: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE chats SET marked_unread = ?2,
               unread = CASE WHEN ?2 THEN unread ELSE 0 END WHERE jid = ?1",
            params![jid, unread],
        )?;
        Ok(())
    }

    /// Deletes every message of a chat but keeps the chat in the list.
    pub fn clear_chat(&self, jid: &str) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM messages WHERE chat_jid = ?1", [jid])?;
        tx.execute(
            "UPDATE chats SET last_preview = '', unread = 0, marked_unread = 0 WHERE jid = ?1",
            [jid],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Deletes a chat, its messages and its list memberships.
    pub fn delete_chat(&self, jid: &str) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM messages WHERE chat_jid = ?1", [jid])?;
        tx.execute("DELETE FROM list_members WHERE jid = ?1", [jid])?;
        tx.execute("DELETE FROM chats WHERE jid = ?1", [jid])?;
        tx.commit()?;
        Ok(())
    }

    /// Every message of a chat, oldest first, for exporting.
    pub fn all_messages(&self, chat_jid: &str) -> Result<Vec<Message>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id,chat_jid,sender_jid,sender_name,from_me,ts,kind,text,by_ai,
                    mime,file_name,size,seconds,media IS NOT NULL
             FROM messages WHERE chat_jid = ?1 ORDER BY ts, rowid",
        )?;
        let rows = stmt.query_map([chat_jid], row_to_message)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    // ---- lists -----------------------------------------------------------

    pub fn lists(&self) -> Result<Vec<ChatList>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT id, name FROM lists ORDER BY id")?;
        let mut lists = stmt
            .query_map([], |r| {
                Ok(ChatList {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    jids: Vec::new(),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut members = conn.prepare("SELECT jid FROM list_members WHERE list_id = ?1")?;
        for list in &mut lists {
            list.jids = members
                .query_map([list.id], |r| r.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
        }
        Ok(lists)
    }

    pub fn create_list(&self, name: &str) -> Result<i64> {
        let conn = self.lock();
        conn.execute("INSERT INTO lists (name) VALUES (?1)", [name])?;
        Ok(conn.last_insert_rowid())
    }

    pub fn rename_list(&self, id: i64, name: &str) -> Result<()> {
        self.lock().execute(
            "UPDATE lists SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        Ok(())
    }

    pub fn delete_list(&self, id: i64) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM list_members WHERE list_id = ?1", [id])?;
        tx.execute("DELETE FROM lists WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_list_member(&self, id: i64, jid: &str, member: bool) -> Result<()> {
        let conn = self.lock();
        if member {
            conn.execute(
                "INSERT OR IGNORE INTO list_members (list_id, jid)
                 SELECT id, ?2 FROM lists WHERE id = ?1",
                params![id, jid],
            )?;
        } else {
            conn.execute(
                "DELETE FROM list_members WHERE list_id = ?1 AND jid = ?2",
                params![id, jid],
            )?;
        }
        Ok(())
    }

    // ---- storage ---------------------------------------------------------

    /// Message counts and media bytes (as reported by WhatsApp) per chat,
    /// biggest first.
    pub fn usage(&self) -> Result<Vec<ChatUsage>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT m.chat_jid, COALESCE(c.name, ''), count(*), COALESCE(sum(m.size), 0)
             FROM messages m LEFT JOIN chats c ON c.jid = m.chat_jid
             GROUP BY m.chat_jid ORDER BY 4 DESC, 3 DESC LIMIT 200",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ChatUsage {
                jid: r.get(0)?,
                name: r.get(1)?,
                messages: r.get(2)?,
                media: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Deletes messages older than `ts`. Chats stay in the list.
    pub fn delete_messages_before(&self, ts: i64) -> Result<usize> {
        Ok(self
            .lock()
            .execute("DELETE FROM messages WHERE ts < ?1", [ts])?)
    }

    /// Rebuilds the database file so deleted rows free disk space.
    pub fn vacuum(&self) -> Result<()> {
        self.lock().execute_batch("VACUUM;")?;
        Ok(())
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
            "SELECT id,chat_jid,sender_jid,sender_name,from_me,ts,kind,text,by_ai,
                    mime,file_name,size,seconds,media IS NOT NULL
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
            "SELECT id,chat_jid,sender_jid,sender_name,from_me,ts,kind,text,by_ai,
                    mime,file_name,size,seconds,media IS NOT NULL
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
            "SELECT * FROM chat_view WHERE name LIKE ?1 ESCAPE '\\' ORDER BY last_ts DESC LIMIT 20",
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
        self.lock().execute_batch(
            "DELETE FROM messages; DELETE FROM chats; DELETE FROM list_members;
             DELETE FROM contacts;",
        )?;
        Ok(())
    }
}

/// Chat state imported from the phone's history sync.
#[derive(Debug, Clone, Default)]
pub struct HistoryChat {
    pub jid: String,
    pub name: String,
    pub is_group: bool,
    pub last_ts: i64,
    pub unread: i64,
    pub archived: bool,
    pub pinned: i64,
    pub muted_until: i64,
    pub marked_unread: bool,
}

/// Adds columns introduced after 0.1.1 to an existing database.
fn migrate(conn: &Connection) -> Result<()> {
    let has = |table: &str, column: &str| -> Result<bool> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let names = stmt
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(names.iter().any(|n| n == column))
    };
    let columns = [
        ("chats", "archived", "INTEGER NOT NULL DEFAULT 0"),
        ("chats", "pinned", "INTEGER NOT NULL DEFAULT 0"),
        ("chats", "muted_until", "INTEGER NOT NULL DEFAULT 0"),
        ("chats", "marked_unread", "INTEGER NOT NULL DEFAULT 0"),
        ("messages", "mime", "TEXT NOT NULL DEFAULT ''"),
        ("messages", "file_name", "TEXT NOT NULL DEFAULT ''"),
        ("messages", "size", "INTEGER NOT NULL DEFAULT 0"),
        ("messages", "seconds", "INTEGER NOT NULL DEFAULT 0"),
        ("messages", "media", "BLOB"),
    ];
    for (table, column, decl) in columns {
        if !has(table, column)? {
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl};"))?;
        }
    }
    Ok(())
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
        archived: r.get(8)?,
        pinned: r.get(9)?,
        muted_until: r.get(10)?,
        marked_unread: r.get(11)?,
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
        mime: r.get(9)?,
        file_name: r.get(10)?,
        size: r.get(11)?,
        seconds: r.get(12)?,
        has_media: r.get(13)?,
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
            ..Default::default()
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
    fn flags_lists_and_media() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("t.db"), &generate_key()).unwrap();
        s.insert_message(&msg("a", "1", 10, "hola", false), false)
            .unwrap();
        s.insert_message(&msg("b", "1", 20, "x", false), false)
            .unwrap();
        s.set_pinned("a", 99).unwrap();
        assert_eq!(s.list_chats().unwrap()[0].jid, "a");
        assert_eq!(s.pinned_count().unwrap(), 1);
        s.set_archived("a", true).unwrap();
        let a = s.get_chat("a").unwrap().unwrap();
        assert!(a.archived);
        assert_eq!(a.pinned, 0, "archiving unpins");
        // Like WhatsApp, archived chats stay archived when messages arrive.
        s.insert_message(&msg("a", "2", 30, "volví", false), false)
            .unwrap();
        assert!(s.get_chat("a").unwrap().unwrap().archived);

        let id = s.create_list("Trabajo").unwrap();
        s.set_list_member(id, "a", true).unwrap();
        s.set_list_member(id, "a", true).unwrap();
        assert_eq!(s.lists().unwrap()[0].jids, ["a"]);
        s.delete_chat("a").unwrap();
        assert!(s.lists().unwrap()[0].jids.is_empty());
        s.delete_list(id).unwrap();
        assert!(s.lists().unwrap().is_empty());

        let mut m = msg("b", "img", 40, "", false);
        m.kind = "image".into();
        m.size = 1234;
        s.insert_message_full(&m, false, Some(b"keys"), true)
            .unwrap();
        assert!(s.get_message("b", "img").unwrap().unwrap().has_media);
        assert_eq!(s.get_media("b", "img").unwrap().unwrap().1, b"keys");
        assert_eq!(s.usage().unwrap()[0].media, 1234);
        s.clear_chat("b").unwrap();
        assert!(s.list_messages("b", None, 10).unwrap().is_empty());
        assert_eq!(s.list_chats().unwrap().len(), 1);
    }

    #[test]
    fn contacts_name_chats_without_creating_them() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("t.db"), &generate_key()).unwrap();
        s.upsert_contact("x", "Xavier", false).unwrap();
        assert!(s.list_chats().unwrap().is_empty());
        s.insert_message(&msg("x", "1", 5, "hi", false), false)
            .unwrap();
        assert_eq!(s.list_chats().unwrap()[0].name, "Xavier");
        s.upsert_group("g@g.us", "Asado", 7).unwrap();
        let chats = s.list_chats().unwrap();
        assert_eq!(chats[0].name, "Asado");
        assert!(chats[0].is_group);
    }

    #[test]
    fn migrates_old_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let key = generate_key();
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(&format!("PRAGMA key = \"x'{key}'\";"))
                .unwrap();
            conn.execute_batch(
                "CREATE TABLE chats (jid TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '',
                   is_group INTEGER NOT NULL DEFAULT 0, last_ts INTEGER NOT NULL DEFAULT 0,
                   last_preview TEXT NOT NULL DEFAULT '', unread INTEGER NOT NULL DEFAULT 0,
                   auto_reply INTEGER NOT NULL DEFAULT 0, instructions TEXT NOT NULL DEFAULT '');
                 INSERT INTO chats (jid, name) VALUES ('a', 'Ana');",
            )
            .unwrap();
        }
        let s = Store::open(&path, &key).unwrap();
        let chat = s.get_chat("a").unwrap().unwrap();
        assert_eq!(chat.name, "Ana");
        assert!(!chat.archived);
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
