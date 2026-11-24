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
    /// Who sent the last message, for group previews. Empty when it was me.
    #[serde(default)]
    pub last_sender: String,
    #[serde(default)]
    pub last_from_me: bool,
}

impl Chat {
    /// True when new messages must not notify: the chat is muted or archived
    /// (archived chats stay quiet, like WhatsApp's "keep chats archived").
    pub fn is_quiet(&self, now: i64) -> bool {
        self.archived || self.muted_until < 0 || self.muted_until > now
    }
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
    /// The message this one replies to, when it is a reply.
    #[serde(default)]
    pub quote_id: String,
    #[serde(default)]
    pub quote_sender: String,
    #[serde(default)]
    pub quote_text: String,
    #[serde(default)]
    pub quote_kind: String,
    #[serde(default)]
    pub forwarded: bool,
    #[serde(default)]
    pub edited: bool,
    /// Deleted for everyone: shown as a placeholder.
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub starred: bool,
    /// Pin timestamp; 0 when not pinned.
    #[serde(default)]
    pub pinned: i64,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    /// JIDs mentioned in the text (`@<number>`), as received.
    #[serde(default, skip_serializing)]
    pub mention_jids: Vec<String>,
    /// Who each `@<number>` in the text is, resolved when read.
    #[serde(default)]
    pub mentions: Vec<Person>,
    /// The sender's phone number (`+…`), when known.
    #[serde(default)]
    pub sender_phone: String,
    /// Display name of whoever wrote the quoted message.
    #[serde(default)]
    pub quote_name: String,
    /// Options and votes of a poll (`kind = "poll"`), resolved when read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll: Option<Poll>,
    /// The stored poll definition (JSON of [`PollDef`]); never sent to the UI.
    #[serde(default, skip)]
    pub poll_def: String,
}

/// A poll as WhatsApp defines it: what is stored with the message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PollDef {
    pub options: Vec<String>,
    /// How many options one person may pick; 0 means any number.
    #[serde(default)]
    pub selectable: u32,
    /// Hex of the message secret that votes are encrypted with.
    #[serde(default)]
    pub secret: String,
}

impl PollDef {
    pub fn parse(json: &str) -> Option<Self> {
        serde_json::from_str(json)
            .ok()
            .filter(|d: &Self| !d.options.is_empty())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Option names whose SHA-256 is among `hashes` (how votes name them).
    pub fn names_for(&self, hashes: &[Vec<u8>]) -> Vec<String> {
        use sha2::Digest;
        self.options
            .iter()
            .filter(|o| {
                let h = sha2::Sha256::digest(o.as_bytes());
                hashes.iter().any(|x| x.as_slice() == h.as_slice())
            })
            .cloned()
            .collect()
    }
}

/// A poll as shown: each option with who picked it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Poll {
    /// How many options one person may pick; 0 means any number.
    pub selectable: u32,
    pub options: Vec<PollOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PollOption {
    pub name: String,
    /// Everyone who picked this option except us.
    pub voters: Vec<Person>,
    /// True when we picked it.
    pub mine: bool,
}

impl Message {
    /// The text with `@<number>` mentions replaced by names.
    pub fn text_with_names(&self) -> String {
        let mut text = self.text.clone();
        for p in &self.mentions {
            if !p.user.is_empty() && !p.name.is_empty() {
                text = text.replace(&format!("@{}", p.user), &format!("@{}", p.name));
            }
        }
        text
    }
}

/// How a WhatsApp user is shown: their contact-book name, or `~` plus the
/// name they gave themselves, or their phone number.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Person {
    pub jid: String,
    /// The user part of the JID, as written after `@` in mentions.
    pub user: String,
    pub name: String,
    /// `+` and digits; empty when only a LID is known.
    pub phone: String,
    /// True when the name comes from the contact book.
    pub saved: bool,
}

/// One person's reaction to a message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Reaction {
    pub emoji: String,
    pub sender_jid: String,
    pub from_me: bool,
    /// Who reacted, resolved when read (contact name, `~` name or phone).
    #[serde(default)]
    pub name: String,
    /// Their phone number (`+…`), when known.
    #[serde(default)]
    pub phone: String,
}

/// A sticker the account has used or received, ready to send again.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Sticker {
    /// Hex SHA-256 of the sticker file: stable id across chats.
    pub id: String,
    pub favorite: bool,
    pub last_used: i64,
    #[serde(skip)]
    pub media: Vec<u8>,
}

/// A note in the private notes chat. Never sent anywhere.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Note {
    pub id: i64,
    pub text: String,
    /// Post-it colour name: "", "yellow", "pink", "blue", "green", "violet".
    pub color: String,
    pub pinned: bool,
    pub done: bool,
    pub ts: i64,
    /// "text" or a media kind when a file is attached.
    pub kind: String,
    pub mime: String,
    pub file_name: String,
    pub size: i64,
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
    /// Ids read on another device before the message itself arrived here:
    /// they must not count as unread when they do.
    read_elsewhere: Mutex<std::collections::HashSet<String>>,
}

/// Oldest stored message of a chat: the anchor for asking the phone for
/// older history.
#[derive(Debug, Clone, PartialEq)]
pub struct Oldest {
    pub id: String,
    pub from_me: bool,
    pub ts: i64,
}

/// Columns read by [`row_to_message`], in order.
const MSG_COLS: &str = "id,chat_jid,sender_jid,sender_name,from_me,ts,kind,text,by_ai,
    mime,file_name,size,seconds,media IS NOT NULL,quote_id,quote_sender,quote_text,
    quote_kind,forwarded,edited,deleted,starred,pinned,mentions,poll";

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
CREATE TABLE IF NOT EXISTS lid_map (
  lid TEXT PRIMARY KEY,
  pn TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_lid_map_pn ON lid_map(pn);
CREATE TABLE IF NOT EXISTS history_done (
  jid TEXT PRIMARY KEY
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
CREATE TABLE IF NOT EXISTS reactions (
  chat_jid TEXT NOT NULL,
  msg_id TEXT NOT NULL,
  sender_jid TEXT NOT NULL,
  from_me INTEGER NOT NULL DEFAULT 0,
  emoji TEXT NOT NULL,
  ts INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (chat_jid, msg_id, sender_jid)
);
CREATE TABLE IF NOT EXISTS poll_votes (
  chat_jid TEXT NOT NULL,
  msg_id TEXT NOT NULL,
  voter TEXT NOT NULL,
  options TEXT NOT NULL DEFAULT '[]',
  ts INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (chat_jid, msg_id, voter)
);
CREATE TABLE IF NOT EXISTS stickers (
  id TEXT PRIMARY KEY,
  media BLOB NOT NULL,
  last_used INTEGER NOT NULL DEFAULT 0,
  favorite INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS notes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  text TEXT NOT NULL DEFAULT '',
  color TEXT NOT NULL DEFAULT '',
  pinned INTEGER NOT NULL DEFAULT 0,
  done INTEGER NOT NULL DEFAULT 0,
  ts INTEGER NOT NULL,
  kind TEXT NOT NULL DEFAULT 'text',
  mime TEXT NOT NULL DEFAULT '',
  file_name TEXT NOT NULL DEFAULT '',
  size INTEGER NOT NULL DEFAULT 0
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
        // `people` finds a contact by either of its addresses (phone or LID).
        // People without a contact-book entry show as `~` + their own name.
        conn.execute_batch(
            "DROP VIEW IF EXISTS people;
             CREATE TEMP VIEW people AS
             SELECT jid, name, push_name FROM contacts
             UNION ALL
             SELECT m.lid, ct.name, ct.push_name FROM lid_map m JOIN contacts ct ON ct.jid = m.pn
             UNION ALL
             SELECT m.pn, ct.name, ct.push_name FROM lid_map m JOIN contacts ct ON ct.jid = m.lid;
             DROP VIEW IF EXISTS chat_view;
             CREATE TEMP VIEW chat_view AS
             SELECT c.jid,
                    CASE WHEN c.is_group THEN c.name
                         ELSE COALESCE(
                           (SELECT name FROM people WHERE jid = c.jid AND name != '' LIMIT 1),
                           NULLIF(NULLIF(c.name, ''),
                             (SELECT push_name FROM people WHERE jid = c.jid AND push_name != '' LIMIT 1)),
                           '~' || (SELECT push_name FROM people
                                   WHERE jid = c.jid AND push_name != '' LIMIT 1),
                           c.name) END
                      AS name,
                    c.is_group, c.last_ts, c.last_preview, c.unread, c.auto_reply,
                    c.instructions, c.archived, c.pinned, c.muted_until, c.marked_unread,
                    CASE WHEN c.last_from_me THEN ''
                         ELSE COALESCE(
                           (SELECT name FROM people WHERE jid = c.last_sender AND name != '' LIMIT 1),
                           '~' || (SELECT push_name FROM people
                                   WHERE jid = c.last_sender AND push_name != '' LIMIT 1),
                           '~' || NULLIF(c.last_sender_name, ''),
                           c.last_sender) END
                      AS last_sender,
                    c.last_from_me
             FROM chats c;",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
            read_elsewhere: Mutex::new(std::collections::HashSet::new()),
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

    /// Every known chat, including ones without messages yet (for name lookups).
    pub fn all_chats(&self) -> Result<Vec<Chat>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT * FROM chat_view ORDER BY last_ts DESC")?;
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

    /// Remembers that a LID and a phone-number JID are the same person.
    pub fn map_lid(&self, lid: &str, pn: &str) -> Result<()> {
        let (lid, pn) = (bare_jid(lid), bare_jid(pn));
        if !lid.ends_with("@lid") || !pn.ends_with("@s.whatsapp.net") {
            return Ok(());
        }
        self.lock().execute(
            "INSERT INTO lid_map (lid, pn) VALUES (?1, ?2)
             ON CONFLICT(lid) DO UPDATE SET pn = ?2",
            params![lid, pn],
        )?;
        Ok(())
    }

    /// Undoes wrong LID mappings that point a contact at our own number
    /// (0.1.4 learned them from our own messages) and drops copies of
    /// messages that landed in our own chat because of them.
    pub fn repair_own_mappings(&self, own: &[String]) -> Result<usize> {
        let own: Vec<String> = own.iter().map(|j| bare_jid(j)).collect();
        let Some(own_pn) = own.iter().find(|j| j.ends_with("@s.whatsapp.net")) else {
            return Ok(0);
        };
        let own_lid = own
            .iter()
            .find(|j| j.ends_with("@lid"))
            .cloned()
            .unwrap_or_default();
        let conn = self.lock();
        let fixed = conn.execute(
            "DELETE FROM lid_map WHERE pn = ?1 AND lid != ?2",
            params![own_pn, own_lid],
        )?;
        conn.execute(
            "DELETE FROM messages WHERE chat_jid = ?1
               AND id IN (SELECT id FROM messages WHERE chat_jid != ?1)",
            [own_pn],
        )?;
        if fixed > 0 {
            refresh_preview(&conn, own_pn)?;
        }
        Ok(fixed)
    }

    /// Moves messages from other people out of our own chat (where wrong
    /// mappings put them) into the chat of whoever sent them. Returns how
    /// many moved.
    pub fn move_strays(&self, own: &[String]) -> Result<usize> {
        let own: Vec<String> = own.iter().map(|j| bare_jid(j)).collect();
        let Some(own_pn) = own.iter().find(|j| j.ends_with("@s.whatsapp.net")).cloned() else {
            return Ok(0);
        };
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let strays: Vec<(String, String, i64)> = {
            let mut stmt = tx.prepare(
                "SELECT id, sender_jid, ts FROM messages
                 WHERE chat_jid = ?1 AND from_me = 0 AND sender_jid != ''",
            )?;
            let rows = stmt.query_map([&own_pn], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        let mut targets: Vec<String> = Vec::new();
        let mut moved = 0;
        for (id, sender, ts) in strays {
            let sender = bare_jid(&sender);
            if own.contains(&sender) {
                continue;
            }
            let target = if sender.ends_with("@lid") {
                tx.query_row("SELECT pn FROM lid_map WHERE lid = ?1", [&sender], |r| {
                    r.get::<_, String>(0)
                })
                .optional()?
                .unwrap_or(sender)
            } else {
                sender
            };
            if target == own_pn {
                continue;
            }
            tx.execute(
                "INSERT INTO chats (jid, is_group, last_ts) VALUES (?1, 0, ?2)
                 ON CONFLICT(jid) DO UPDATE SET last_ts = MAX(chats.last_ts, excluded.last_ts)",
                params![target, ts],
            )?;
            tx.execute(
                "UPDATE OR IGNORE messages SET chat_jid = ?2 WHERE chat_jid = ?1 AND id = ?3",
                params![own_pn, target, id],
            )?;
            tx.execute(
                "UPDATE OR IGNORE reactions SET chat_jid = ?2 WHERE chat_jid = ?1 AND msg_id = ?3",
                params![own_pn, target, id],
            )?;
            tx.execute(
                "DELETE FROM messages WHERE chat_jid = ?1 AND id = ?2",
                params![own_pn, id],
            )?;
            if !targets.contains(&target) {
                targets.push(target);
            }
            moved += 1;
        }
        targets.push(own_pn);
        for t in &targets {
            refresh_preview(&tx, t)?;
        }
        tx.commit()?;
        Ok(moved)
    }

    /// LIDs seen in contacts, chats or group messages with no phone number yet.
    pub fn unmapped_lids(&self, limit: u32) -> Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT jid FROM (
               SELECT jid FROM contacts WHERE jid LIKE '%@lid'
               UNION SELECT jid FROM chats WHERE jid LIKE '%@lid'
               UNION SELECT DISTINCT sender_jid FROM messages WHERE sender_jid LIKE '%@lid'
             ) WHERE jid NOT IN (SELECT lid FROM lid_map) LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// One-to-one chats (by phone number) with no contact-book name: their
    /// contact may be stored under a LID we don't know yet.
    pub fn unnamed_pn_chats(&self, limit: u32) -> Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT c.jid FROM chats c WHERE c.is_group = 0 AND c.jid LIKE '%@s.whatsapp.net'
               AND NOT EXISTS (SELECT 1 FROM people p WHERE p.jid = c.jid AND p.name != '')
               AND c.jid NOT IN (SELECT pn FROM lid_map)
             ORDER BY c.last_ts DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// True once the phone has sent all the older history it has for a chat.
    pub fn history_complete(&self, chat_jid: &str) -> bool {
        self.lock()
            .query_row(
                "SELECT 1 FROM history_done WHERE jid = ?1",
                [chat_jid],
                |_| Ok(()),
            )
            .is_ok()
    }

    pub fn set_history_complete(&self, chat_jid: &str) -> Result<()> {
        self.lock().execute(
            "INSERT OR IGNORE INTO history_done (jid) VALUES (?1)",
            [chat_jid],
        )?;
        Ok(())
    }

    /// The oldest stored message of a chat.
    pub fn oldest_message(&self, chat_jid: &str) -> Result<Option<Oldest>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT id, from_me, ts FROM messages WHERE chat_jid = ?1
                 ORDER BY ts ASC, rowid ASC LIMIT 1",
                [chat_jid],
                |r| {
                    Ok(Oldest {
                        id: r.get(0)?,
                        from_me: r.get(1)?,
                        ts: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    /// The phone-number JID for a LID, when known; other JIDs unchanged.
    pub fn canonical_jid(&self, jid: &str) -> String {
        if !jid.ends_with("@lid") {
            return jid.to_string();
        }
        self.lock()
            .query_row(
                "SELECT pn FROM lid_map WHERE lid = ?1",
                [bare_jid(jid)],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| jid.to_string())
    }

    /// How a user is shown: contact name, `~` + own name, or phone number.
    pub fn person(&self, jid: &str, push: &str) -> Person {
        person(&self.lock(), jid, push)
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

    /// Marks a chat read up to the newest of `ids` (read on another device):
    /// messages that arrived after it stay unread. When none of them is
    /// stored, the whole chat counts as read.
    pub fn mark_read_up_to(&self, jid: &str, ids: &[String]) -> Result<bool> {
        let conn = self.lock();
        let mut newest: Option<i64> = None;
        for id in ids {
            let ts: Option<i64> = conn
                .query_row(
                    "SELECT ts FROM messages WHERE chat_jid = ?1 AND id = ?2",
                    params![jid, id],
                    |r| r.get(0),
                )
                .optional()?;
            newest = newest.max(ts);
        }
        let left: i64 = match newest {
            Some(ts) => conn.query_row(
                "SELECT count(*) FROM messages WHERE chat_jid = ?1 AND from_me = 0 AND ts > ?2",
                params![jid, ts],
                |r| r.get(0),
            )?,
            None => 0,
        };
        let changed = conn.execute(
            "UPDATE chats SET unread = MIN(unread, ?2), marked_unread = 0
             WHERE jid = ?1 AND (unread > ?2 OR marked_unread = 1)",
            params![jid, left],
        )?;
        Ok(changed > 0)
    }

    /// Applies a "read on another device" receipt. `chat` is `None` when the
    /// receipt does not say which chat (one-to-one chats: WhatsApp names our
    /// own address), so the chat is found from the message ids. Returns the
    /// chats whose unread count changed.
    pub fn mark_read_elsewhere(&self, chat: Option<&str>, ids: &[String]) -> Result<Vec<String>> {
        let chats: Vec<String> = match chat {
            Some(c) => vec![c.to_string()],
            None => {
                let conn = self.lock();
                let mut found = Vec::new();
                for id in ids {
                    let jid: Option<String> = conn
                        .query_row(
                            "SELECT chat_jid FROM messages WHERE id = ?1 AND from_me = 0 LIMIT 1",
                            [id],
                            |r| r.get(0),
                        )
                        .optional()?;
                    if let Some(j) = jid.filter(|j| !found.contains(j)) {
                        found.push(j);
                    }
                }
                found
            }
        };
        if chats.is_empty() {
            // Not here yet: remember the ids so they arrive already read.
            let mut pending = self
                .read_elsewhere
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if pending.len() > 5000 {
                pending.clear();
            }
            pending.extend(ids.iter().cloned());
            return Ok(Vec::new());
        }
        let mut changed = Vec::new();
        for c in chats {
            if self.mark_read_up_to(&c, ids)? {
                changed.push(c);
            }
        }
        Ok(changed)
    }

    fn was_read_elsewhere(&self, id: &str) -> bool {
        self.read_elsewhere
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id)
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
        self.insert_message_full(m, is_group, None, None, true)
    }

    /// Like [`insert_message`](Self::insert_message), also storing the encoded
    /// media keys. `count_unread` is false for imported history, whose unread
    /// count comes from the phone instead.
    pub fn insert_message_full(
        &self,
        m: &Message,
        is_group: bool,
        media: Option<&[u8]>,
        raw: Option<&[u8]>,
        count_unread: bool,
    ) -> Result<bool> {
        let count_unread = count_unread && !(m.from_me || self.was_read_elsewhere(&m.id));
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO messages
               (chat_jid,id,sender_jid,sender_name,from_me,ts,kind,text,by_ai,
                mime,file_name,size,seconds,media,raw,
                quote_id,quote_sender,quote_text,quote_kind,forwarded,mentions,poll)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)",
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
                media,
                raw,
                m.quote_id,
                m.quote_sender,
                m.quote_text,
                m.quote_kind,
                m.forwarded,
                m.mention_jids.join(","),
                m.poll_def
            ],
        )? > 0;
        if inserted {
            let preview = named_mentions(&tx, &preview_of(m), &m.mention_jids);
            let unread_inc = i64::from(count_unread && !m.from_me);
            tx.execute(
                "INSERT INTO chats (jid, is_group, last_ts, last_preview, unread,
                                    last_sender, last_sender_name, last_from_me)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(jid) DO UPDATE SET
                   last_preview = CASE WHEN excluded.last_ts >= chats.last_ts
                                       THEN excluded.last_preview ELSE chats.last_preview END,
                   last_sender = CASE WHEN excluded.last_ts >= chats.last_ts
                                      THEN excluded.last_sender ELSE chats.last_sender END,
                   last_sender_name = CASE WHEN excluded.last_ts >= chats.last_ts
                                      THEN excluded.last_sender_name ELSE chats.last_sender_name END,
                   last_from_me = CASE WHEN excluded.last_ts >= chats.last_ts
                                      THEN excluded.last_from_me ELSE chats.last_from_me END,
                   last_ts = MAX(chats.last_ts, excluded.last_ts),
                   unread = chats.unread + excluded.unread",
                params![
                    m.chat_jid,
                    is_group,
                    m.ts,
                    preview,
                    unread_inc,
                    m.sender_jid,
                    m.sender_name,
                    m.from_me
                ],
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
        let conn = self.lock();
        let found = conn
            .query_row(
                &format!("SELECT {MSG_COLS} FROM messages WHERE chat_jid = ?1 AND id = ?2"),
                params![chat_jid, id],
                row_to_message,
            )
            .optional()?;
        let Some(m) = found else { return Ok(None) };
        let mut one = [m];
        attach_reactions(&conn, chat_jid, &mut one)?;
        attach_polls(&conn, &mut one)?;
        resolve_people(&conn, &mut one);
        let [m] = one;
        Ok(Some(m))
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
        tx.execute("DELETE FROM reactions WHERE chat_jid = ?1", [jid])?;
        tx.execute("DELETE FROM poll_votes WHERE chat_jid = ?1", [jid])?;
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
        tx.execute("DELETE FROM reactions WHERE chat_jid = ?1", [jid])?;
        tx.execute("DELETE FROM poll_votes WHERE chat_jid = ?1", [jid])?;
        tx.execute("DELETE FROM list_members WHERE jid = ?1", [jid])?;
        tx.execute("DELETE FROM chats WHERE jid = ?1", [jid])?;
        tx.commit()?;
        Ok(())
    }

    /// Every message of a chat, oldest first, for exporting.
    pub fn all_messages(&self, chat_jid: &str) -> Result<Vec<Message>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {MSG_COLS} FROM messages WHERE chat_jid = ?1 ORDER BY ts, rowid"
        ))?;
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
        let mut stmt = conn.prepare(&format!(
            "SELECT {MSG_COLS} FROM messages WHERE chat_jid = ?1 AND ts < ?2
                 ORDER BY ts DESC, rowid DESC LIMIT ?3"
        ))?;
        let rows = stmt.query_map(
            params![chat_jid, before_ts.unwrap_or(i64::MAX), limit.min(500)],
            row_to_message,
        )?;
        let mut out = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        out.reverse();
        drop(stmt);
        attach_reactions(&conn, chat_jid, &mut out)?;
        attach_polls(&conn, &mut out)?;
        resolve_people(&conn, &mut out);
        Ok(out)
    }

    pub fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<Message>> {
        let pattern = format!("%{}%", escape_like(query));
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {MSG_COLS} FROM messages WHERE text LIKE ?1 ESCAPE '\\'
                 ORDER BY ts DESC LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![pattern, limit.min(200)], row_to_message)?;
        let mut out = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        resolve_people(&conn, &mut out);
        Ok(out)
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

    // ---- message actions -------------------------------------------------

    /// The full encoded message, kept so it can be quoted, forwarded or
    /// resent (stickers).
    pub fn get_raw(&self, chat_jid: &str, id: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT raw FROM messages WHERE chat_jid = ?1 AND id = ?2 AND raw IS NOT NULL",
                params![chat_jid, id],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Sets or (with an empty emoji) removes one sender's reaction.
    pub fn set_reaction(
        &self,
        chat_jid: &str,
        msg_id: &str,
        sender_jid: &str,
        from_me: bool,
        emoji: &str,
        ts: i64,
    ) -> Result<()> {
        let conn = self.lock();
        // Our own reaction is keyed by a fixed sender so devices agree.
        let sender = if from_me { "me" } else { sender_jid };
        if emoji.is_empty() {
            conn.execute(
                "DELETE FROM reactions WHERE chat_jid = ?1 AND msg_id = ?2 AND sender_jid = ?3",
                params![chat_jid, msg_id, sender],
            )?;
        } else {
            conn.execute(
                "INSERT INTO reactions (chat_jid, msg_id, sender_jid, from_me, emoji, ts)
                 VALUES (?1,?2,?3,?4,?5,?6)
                 ON CONFLICT(chat_jid, msg_id, sender_jid) DO UPDATE SET
                   emoji = excluded.emoji, ts = excluded.ts",
                params![chat_jid, msg_id, sender, from_me, emoji, ts],
            )?;
        }
        Ok(())
    }

    /// Records one person's choice in a poll; `voter` is "me" for our own.
    /// Only a newer vote replaces an older one; an empty choice clears it.
    pub fn set_poll_vote(
        &self,
        chat_jid: &str,
        msg_id: &str,
        voter: &str,
        options: &[String],
        ts: i64,
    ) -> Result<()> {
        let voter = if voter == "me" {
            voter.to_string()
        } else {
            self.canonical_jid(&bare_jid(voter))
        };
        let picked = serde_json::to_string(options).unwrap_or_else(|_| "[]".into());
        self.lock().execute(
            "INSERT INTO poll_votes (chat_jid, msg_id, voter, options, ts)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(chat_jid, msg_id, voter) DO UPDATE SET
               options = excluded.options, ts = excluded.ts
             WHERE excluded.ts >= poll_votes.ts",
            params![chat_jid, msg_id, voter, picked, ts],
        )?;
        Ok(())
    }

    /// A stored poll's definition, with who created it (`from_me`, sender).
    pub fn poll_def(&self, chat_jid: &str, id: &str) -> Result<Option<(PollDef, bool, String)>> {
        let row = self
            .lock()
            .query_row(
                "SELECT poll, from_me, sender_jid FROM messages
                 WHERE chat_jid = ?1 AND id = ?2 AND kind = 'poll' AND deleted = 0",
                params![chat_jid, id],
                |r| Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        Ok(row.and_then(|(json, from_me, sender)| {
            PollDef::parse(&json).map(|d| (d, from_me, sender))
        }))
    }

    /// Replaces a message's text after an edit.
    pub fn edit_message(&self, chat_jid: &str, id: &str, text: &str) -> Result<bool> {
        let conn = self.lock();
        let n = conn.execute(
            "UPDATE messages SET text = ?3, edited = 1 WHERE chat_jid = ?1 AND id = ?2",
            params![chat_jid, id, text],
        )?;
        refresh_preview(&conn, chat_jid)?;
        Ok(n > 0)
    }

    /// Marks a message deleted for everyone and drops its content.
    pub fn mark_deleted(&self, chat_jid: &str, id: &str) -> Result<bool> {
        let conn = self.lock();
        let n = conn.execute(
            "UPDATE messages SET deleted = 1, text = '', media = NULL, raw = NULL,
                    quote_text = '', pinned = 0, poll = ''
             WHERE chat_jid = ?1 AND id = ?2",
            params![chat_jid, id],
        )?;
        conn.execute(
            "DELETE FROM reactions WHERE chat_jid = ?1 AND msg_id = ?2",
            params![chat_jid, id],
        )?;
        conn.execute(
            "DELETE FROM poll_votes WHERE chat_jid = ?1 AND msg_id = ?2",
            params![chat_jid, id],
        )?;
        refresh_preview(&conn, chat_jid)?;
        Ok(n > 0)
    }

    /// Removes a message from this Mac only.
    pub fn delete_message(&self, chat_jid: &str, id: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "DELETE FROM messages WHERE chat_jid = ?1 AND id = ?2",
            params![chat_jid, id],
        )?;
        conn.execute(
            "DELETE FROM reactions WHERE chat_jid = ?1 AND msg_id = ?2",
            params![chat_jid, id],
        )?;
        conn.execute(
            "DELETE FROM poll_votes WHERE chat_jid = ?1 AND msg_id = ?2",
            params![chat_jid, id],
        )?;
        refresh_preview(&conn, chat_jid)?;
        Ok(())
    }

    pub fn set_starred(&self, chat_jid: &str, id: &str, starred: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE messages SET starred = ?3 WHERE chat_jid = ?1 AND id = ?2",
            params![chat_jid, id, starred],
        )?;
        Ok(())
    }

    /// Pins (timestamp) or unpins (0) a message.
    pub fn set_message_pinned(&self, chat_jid: &str, id: &str, pinned: i64) -> Result<()> {
        self.lock().execute(
            "UPDATE messages SET pinned = ?3 WHERE chat_jid = ?1 AND id = ?2",
            params![chat_jid, id, pinned],
        )?;
        Ok(())
    }

    /// Pinned messages of a chat, newest pin first.
    pub fn pinned_messages(&self, chat_jid: &str) -> Result<Vec<Message>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {MSG_COLS} FROM messages WHERE chat_jid = ?1 AND pinned > 0
             ORDER BY pinned DESC LIMIT 3"
        ))?;
        let rows = stmt.query_map([chat_jid], row_to_message)?;
        let mut out = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        resolve_people(&conn, &mut out);
        Ok(out)
    }

    /// Starred messages across all chats, newest first.
    pub fn starred_messages(&self, limit: u32) -> Result<Vec<Message>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {MSG_COLS} FROM messages WHERE starred = 1 ORDER BY ts DESC LIMIT ?1"
        ))?;
        let rows = stmt.query_map([limit.min(500)], row_to_message)?;
        let mut out = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        resolve_people(&conn, &mut out);
        Ok(out)
    }

    // ---- stickers --------------------------------------------------------

    /// Remembers a sticker. `used` is when it was last sent or received.
    pub fn add_sticker(&self, id: &str, media: &[u8], used: i64) -> Result<()> {
        self.lock().execute(
            "INSERT INTO stickers (id, media, last_used) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET last_used = MAX(stickers.last_used, excluded.last_used)",
            params![id, media, used],
        )?;
        Ok(())
    }

    /// Favourites first, then the most recently used.
    pub fn stickers(&self, limit: u32) -> Result<Vec<Sticker>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, favorite, last_used, media FROM stickers
             ORDER BY favorite DESC, last_used DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit.min(500)], |r| {
            Ok(Sticker {
                id: r.get(0)?,
                favorite: r.get(1)?,
                last_used: r.get(2)?,
                media: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get_sticker(&self, id: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .lock()
            .query_row("SELECT media FROM stickers WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_sticker_favorite(&self, id: &str, favorite: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE stickers SET favorite = ?2 WHERE id = ?1",
            params![id, favorite],
        )?;
        Ok(())
    }

    pub fn remove_sticker(&self, id: &str) -> Result<()> {
        self.lock()
            .execute("DELETE FROM stickers WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---- notes -----------------------------------------------------------

    pub fn list_notes(&self) -> Result<Vec<Note>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, text, color, pinned, done, ts, kind, mime, file_name, size
             FROM notes ORDER BY ts, id",
        )?;
        let rows = stmt.query_map([], row_to_note)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get_note(&self, id: i64) -> Result<Option<Note>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT id, text, color, pinned, done, ts, kind, mime, file_name, size
                 FROM notes WHERE id = ?1",
                [id],
                row_to_note,
            )
            .optional()?)
    }

    /// Adds a note and returns it with its new id.
    pub fn add_note(&self, note: &Note) -> Result<Note> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO notes (text, color, pinned, done, ts, kind, mime, file_name, size)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                note.text,
                note.color,
                note.pinned,
                note.done,
                note.ts,
                note.kind,
                note.mime,
                note.file_name,
                note.size
            ],
        )?;
        Ok(Note {
            id: conn.last_insert_rowid(),
            ..note.clone()
        })
    }

    pub fn update_note(&self, note: &Note) -> Result<()> {
        self.lock().execute(
            "UPDATE notes SET text = ?2, color = ?3, pinned = ?4, done = ?5 WHERE id = ?1",
            params![note.id, note.text, note.color, note.pinned, note.done],
        )?;
        Ok(())
    }

    pub fn delete_note(&self, id: i64) -> Result<()> {
        self.lock()
            .execute("DELETE FROM notes WHERE id = ?1", [id])?;
        Ok(())
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
             DELETE FROM contacts; DELETE FROM lid_map; DELETE FROM history_done; DELETE FROM reactions; DELETE FROM poll_votes; DELETE FROM stickers;",
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
        ("messages", "raw", "BLOB"),
        ("messages", "quote_id", "TEXT NOT NULL DEFAULT ''"),
        ("messages", "quote_sender", "TEXT NOT NULL DEFAULT ''"),
        ("messages", "quote_text", "TEXT NOT NULL DEFAULT ''"),
        ("messages", "quote_kind", "TEXT NOT NULL DEFAULT ''"),
        ("messages", "forwarded", "INTEGER NOT NULL DEFAULT 0"),
        ("messages", "edited", "INTEGER NOT NULL DEFAULT 0"),
        ("messages", "deleted", "INTEGER NOT NULL DEFAULT 0"),
        ("messages", "starred", "INTEGER NOT NULL DEFAULT 0"),
        ("messages", "pinned", "INTEGER NOT NULL DEFAULT 0"),
        ("chats", "last_sender", "TEXT NOT NULL DEFAULT ''"),
        ("chats", "last_sender_name", "TEXT NOT NULL DEFAULT ''"),
        ("chats", "last_from_me", "INTEGER NOT NULL DEFAULT 0"),
        ("messages", "mentions", "TEXT NOT NULL DEFAULT ''"),
        ("messages", "poll", "TEXT NOT NULL DEFAULT ''"),
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
        last_sender: r.get(12)?,
        last_from_me: r.get(13)?,
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
        quote_id: r.get(14)?,
        quote_sender: r.get(15)?,
        quote_text: r.get(16)?,
        quote_kind: r.get(17)?,
        forwarded: r.get(18)?,
        edited: r.get(19)?,
        deleted: r.get(20)?,
        starred: r.get(21)?,
        pinned: r.get(22)?,
        reactions: Vec::new(),
        mention_jids: r
            .get::<_, String>(23)?
            .split(',')
            .filter(|j| !j.is_empty())
            .map(str::to_string)
            .collect(),
        poll_def: r.get(24)?,
        ..Default::default()
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

fn row_to_note(r: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        id: r.get(0)?,
        text: r.get(1)?,
        color: r.get(2)?,
        pinned: r.get(3)?,
        done: r.get(4)?,
        ts: r.get(5)?,
        kind: r.get(6)?,
        mime: r.get(7)?,
        file_name: r.get(8)?,
        size: r.get(9)?,
    })
}

/// Fills in the reactions of a page of messages from one chat.
/// Strips the device part: `123:4@s.whatsapp.net` → `123@s.whatsapp.net`.
pub fn bare_jid(jid: &str) -> String {
    match jid.split_once('@') {
        Some((user, server)) => {
            let user = user.split(':').next().unwrap_or(user);
            format!("{user}@{server}")
        }
        None => jid.to_string(),
    }
}

fn person(conn: &Connection, jid: &str, push: &str) -> Person {
    let jid = bare_jid(jid);
    let user = jid.split('@').next().unwrap_or_default().to_string();
    let lookup = |sql: &str, key: &str| -> Option<String> {
        conn.query_row(sql, [key], |r| r.get::<_, String>(0))
            .optional()
            .ok()
            .flatten()
            .filter(|v| !v.is_empty())
    };
    let pn = if jid.ends_with("@s.whatsapp.net") {
        Some(jid.clone())
    } else if jid.ends_with("@lid") {
        lookup("SELECT pn FROM lid_map WHERE lid = ?1", &jid)
    } else {
        None
    };
    let phone = pn
        .map(|p| format!("+{}", p.split('@').next().unwrap_or_default()))
        .unwrap_or_default();
    let saved = lookup(
        "SELECT name FROM people WHERE jid = ?1 AND name != '' LIMIT 1",
        &jid,
    );
    let own = lookup(
        "SELECT push_name FROM people WHERE jid = ?1 AND push_name != '' LIMIT 1",
        &jid,
    )
    .or_else(|| Some(push.to_string()).filter(|p| !p.is_empty()));
    let name = match (&saved, own) {
        (Some(n), _) => n.clone(),
        (None, Some(p)) => format!("~{p}"),
        (None, None) => phone.clone(),
    };
    Person {
        jid,
        user,
        name,
        phone,
        saved: saved.is_some(),
    }
}

/// Replaces `@<number>` in a preview with the mentioned person's name.
fn named_mentions(conn: &Connection, text: &str, jids: &[String]) -> String {
    let mut out = text.to_string();
    for jid in jids {
        let p = person(conn, jid, "");
        if !p.user.is_empty() && !p.name.is_empty() {
            let shown = p.name.trim_start_matches('~');
            out = out.replace(&format!("@{}", p.user), &format!("@{shown}"));
        }
    }
    out
}

/// Fills in display names: senders, quoted authors and mentions.
fn resolve_people(conn: &Connection, out: &mut [Message]) {
    let mut cache: std::collections::HashMap<String, Person> = std::collections::HashMap::new();
    let mut get = |jid: &str, push: &str| -> Person {
        cache
            .entry(bare_jid(jid))
            .or_insert_with(|| person(conn, jid, push))
            .clone()
    };
    for m in out.iter_mut() {
        if !m.from_me && !m.sender_jid.is_empty() {
            let p = get(&m.sender_jid, &m.sender_name);
            m.sender_name = p.name;
            m.sender_phone = p.phone;
        }
        if !m.quote_sender.is_empty() && m.quote_sender != "me" {
            m.quote_name = get(&m.quote_sender, "").name;
        }
        m.mentions = m.mention_jids.iter().map(|j| get(j, "")).collect();
        for r in m.reactions.iter_mut().filter(|r| !r.from_me) {
            let p = get(&r.sender_jid, "");
            r.name = p.name;
            r.phone = p.phone;
        }
        if let Some(poll) = m.poll.as_mut() {
            for v in poll.options.iter_mut().flat_map(|o| o.voters.iter_mut()) {
                *v = get(&v.jid, "");
            }
        }
    }
}

/// Builds the options and votes of the polls in a page of messages.
fn attach_polls(conn: &Connection, out: &mut [Message]) -> Result<()> {
    for m in out.iter_mut().filter(|m| m.kind == "poll") {
        let Some(def) = PollDef::parse(&m.poll_def) else {
            continue;
        };
        let mut options: Vec<PollOption> = def
            .options
            .iter()
            .map(|name| PollOption {
                name: name.clone(),
                ..Default::default()
            })
            .collect();
        let mut stmt = conn.prepare_cached(
            "SELECT voter, options FROM poll_votes WHERE chat_jid = ?1 AND msg_id = ?2
             ORDER BY ts",
        )?;
        let rows = stmt.query_map(params![m.chat_jid, m.id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (voter, picked) = row?;
            let picked: Vec<String> = serde_json::from_str(&picked).unwrap_or_default();
            for o in options.iter_mut().filter(|o| picked.contains(&o.name)) {
                if voter == "me" {
                    o.mine = true;
                } else {
                    o.voters.push(Person {
                        jid: voter.clone(),
                        ..Default::default()
                    });
                }
            }
        }
        m.poll = Some(Poll {
            selectable: def.selectable,
            options,
        });
    }
    Ok(())
}

fn attach_reactions(conn: &Connection, chat_jid: &str, out: &mut [Message]) -> Result<()> {
    if out.is_empty() {
        return Ok(());
    }
    let first = out.iter().map(|m| m.ts).min().unwrap_or(0);
    let mut stmt = conn.prepare(
        "SELECT r.msg_id, r.emoji, r.sender_jid, r.from_me FROM reactions r
         JOIN messages m ON m.chat_jid = r.chat_jid AND m.id = r.msg_id
         WHERE r.chat_jid = ?1 AND m.ts >= ?2 ORDER BY r.ts",
    )?;
    let rows = stmt.query_map(params![chat_jid, first], |r| {
        Ok((
            r.get::<_, String>(0)?,
            Reaction {
                emoji: r.get(1)?,
                sender_jid: r.get(2)?,
                from_me: r.get(3)?,
                ..Default::default()
            },
        ))
    })?;
    for row in rows {
        let (id, reaction) = row?;
        if let Some(m) = out.iter_mut().find(|m| m.id == id) {
            m.reactions.push(reaction);
        }
    }
    Ok(())
}

/// Recomputes a chat's preview from its newest message (after edits and
/// deletions).
fn refresh_preview(conn: &Connection, chat_jid: &str) -> Result<()> {
    let last = conn
        .query_row(
            &format!(
                "SELECT {MSG_COLS} FROM messages WHERE chat_jid = ?1
                 ORDER BY ts DESC, rowid DESC LIMIT 1"
            ),
            [chat_jid],
            row_to_message,
        )
        .optional()?;
    if let Some(m) = last {
        conn.execute(
            "UPDATE chats SET last_preview = ?2 WHERE jid = ?1",
            params![chat_jid, preview_of(&m)],
        )?;
    }
    Ok(())
}

fn preview_of(m: &Message) -> String {
    let base = if m.deleted {
        "[deleted]".to_string()
    } else if m.kind == "poll" {
        format!("📊 {}", m.text)
    } else if m.text.is_empty() {
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
        s.insert_message_full(&m, false, Some(b"keys"), None, true)
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
    fn names_follow_contacts_then_tilde_push_names() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("t.db"), &generate_key()).unwrap();
        let ana = "5491100000001@s.whatsapp.net";
        let beto = "5491100000002@s.whatsapp.net";
        let beto_lid = "99001:3@lid";
        // Saved contact: the address-book name, even if a push name exists.
        s.upsert_contact(ana, "Ana Gómez", false).unwrap();
        s.upsert_contact(ana, "Anita", true).unwrap();
        s.insert_message(&msg(ana, "1", 5, "hi", false), false)
            .unwrap();
        assert_eq!(s.get_chat(ana).unwrap().unwrap().name, "Ana Gómez");
        // Not saved: their own name with ~.
        s.upsert_contact(beto, "Beto", true).unwrap();
        s.insert_message(&msg(beto, "2", 6, "hey", false), false)
            .unwrap();
        assert_eq!(s.get_chat(beto).unwrap().unwrap().name, "~Beto");

        // A group message from Beto's LID, mentioning Ana's LID.
        s.map_lid(beto_lid, beto).unwrap();
        s.map_lid("77001@lid", ana).unwrap();
        let mut m = msg("g@g.us", "3", 7, "@77001 venís?", false);
        m.sender_jid = "99001@lid".into();
        m.sender_name = "Beto push".into();
        m.mention_jids = vec!["77001@lid".into()];
        s.insert_message(&m, true).unwrap();
        let got = s.get_message("g@g.us", "3").unwrap().unwrap();
        assert_eq!(got.sender_name, "~Beto");
        assert_eq!(got.sender_phone, "+5491100000002");
        assert_eq!(got.mentions.len(), 1);
        assert_eq!(got.mentions[0].user, "77001");
        assert_eq!(got.mentions[0].name, "Ana Gómez");
        assert!(got.mentions[0].saved);
        assert_eq!(got.text_with_names(), "@Ana Gómez venís?");
        let group = s.get_chat("g@g.us").unwrap().unwrap();
        assert_eq!(group.last_sender, "~Beto");
        assert_eq!(group.last_preview, "@Ana Gómez venís?");
        assert_eq!(s.canonical_jid("99001@lid"), beto);
        // Unknown LID without a name: nothing to show but the id itself.
        assert_eq!(s.person("123@lid", "").name, "");
        assert_eq!(s.person("123@lid", "Zoe").name, "~Zoe");
    }

    #[test]
    fn archived_and_muted_chats_are_quiet() {
        let chat = |archived, muted_until| Chat {
            archived,
            muted_until,
            ..Default::default()
        };
        assert!(!chat(false, 0).is_quiet(100));
        assert!(chat(true, 0).is_quiet(100), "archived chats never notify");
        assert!(chat(false, -1).is_quiet(100));
        assert!(chat(false, 200).is_quiet(100));
        assert!(!chat(false, 50).is_quiet(100), "mute expired");
    }

    #[test]
    fn reactions_and_poll_votes_carry_names() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("t.db"), &generate_key()).unwrap();
        let mut poll = msg("g@g.us", "p", 10, "¿Cine?", false);
        poll.kind = "poll".into();
        poll.poll_def = PollDef {
            options: vec!["Sí".into(), "No".into()],
            selectable: 0,
            secret: String::new(),
        }
        .to_json();
        s.insert_message(&poll, true).unwrap();
        s.upsert_contact("1@s.whatsapp.net", "Ana", false).unwrap();
        s.map_lid("77@lid", "1@s.whatsapp.net").unwrap();
        s.set_reaction("g@g.us", "p", "77@lid", false, "👍", 11)
            .unwrap();
        s.set_reaction("g@g.us", "p", "", true, "❤️", 12).unwrap();
        // The same person votes by private id and then by phone number.
        s.set_poll_vote("g@g.us", "p", "77@lid", &["No".into()], 20)
            .unwrap();
        s.set_poll_vote("g@g.us", "p", "1@s.whatsapp.net", &["Sí".into()], 30)
            .unwrap();
        // A late, older vote does not win.
        s.set_poll_vote("g@g.us", "p", "1@s.whatsapp.net", &["No".into()], 25)
            .unwrap();
        s.set_poll_vote("g@g.us", "p", "me", &["Sí".into(), "No".into()], 31)
            .unwrap();
        let m = s.get_message("g@g.us", "p").unwrap().unwrap();
        let theirs = m.reactions.iter().find(|r| !r.from_me).unwrap();
        assert_eq!(theirs.name, "Ana");
        assert_eq!(theirs.phone, "+1");
        let poll = m.poll.unwrap();
        assert_eq!(poll.options[0].voters.len(), 1);
        assert_eq!(poll.options[0].voters[0].name, "Ana");
        assert!(poll.options[0].mine && poll.options[1].mine);
        assert!(poll.options[1].voters.is_empty());
        // Clearing our vote, then deleting the poll, leaves nothing behind.
        s.set_poll_vote("g@g.us", "p", "me", &[], 40).unwrap();
        let m = s.get_message("g@g.us", "p").unwrap().unwrap();
        assert!(m.poll.unwrap().options.iter().all(|o| !o.mine));
        s.mark_deleted("g@g.us", "p").unwrap();
        assert!(s.poll_def("g@g.us", "p").unwrap().is_none());
    }

    #[test]
    fn poll_option_hashes_map_to_names() {
        use sha2::Digest;
        let def = PollDef {
            options: vec!["A".into(), "B".into()],
            ..Default::default()
        };
        let b = sha2::Sha256::digest(b"B").to_vec();
        assert_eq!(def.names_for(&[b, vec![0; 32]]), ["B"]);
        assert!(PollDef::parse("").is_none());
        assert_eq!(PollDef::parse(&def.to_json()), Some(def));
    }

    #[test]
    fn read_on_phone_clears_unread_up_to_that_message() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("t.db"), &generate_key()).unwrap();
        for (id, ts) in [("1", 10), ("2", 20), ("3", 30)] {
            s.insert_message(&msg("a", id, ts, "x", false), false)
                .unwrap();
        }
        assert_eq!(s.get_chat("a").unwrap().unwrap().unread, 3);
        assert!(s.mark_read_up_to("a", &["2".into()]).unwrap());
        assert_eq!(s.get_chat("a").unwrap().unwrap().unread, 1);
        assert!(
            !s.mark_read_up_to("a", &["1".into()]).unwrap(),
            "never raises"
        );
        assert!(s.mark_read_up_to("a", &["unknown".into()]).unwrap());
        assert_eq!(s.get_chat("a").unwrap().unwrap().unread, 0);
    }

    #[test]
    fn wrong_own_mappings_are_repaired_and_strays_moved_home() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("t.db"), &generate_key()).unwrap();
        let me = "5491100000000@s.whatsapp.net";
        let own = vec![me.to_string(), "111@lid".to_string()];
        s.map_lid("111@lid", me).unwrap();
        s.map_lid("555@lid", me).unwrap(); // the 0.1.4 bug
                                           // A contact's incoming message that landed in our own chat.
        let mut stray = msg(me, "x1", 10, "hola", false);
        stray.sender_jid = "555@lid".into();
        s.insert_message(&stray, false).unwrap();
        // A real note to self stays.
        s.insert_message(&msg(me, "n1", 11, "nota", true), false)
            .unwrap();
        assert_eq!(s.repair_own_mappings(&own).unwrap(), 1);
        assert_eq!(s.canonical_jid("555@lid"), "555@lid");
        assert_eq!(s.canonical_jid("111@lid"), me);
        s.map_lid("555@lid", "5491155555555@s.whatsapp.net")
            .unwrap();
        assert_eq!(s.move_strays(&own).unwrap(), 1);
        let home = s
            .list_messages("5491155555555@s.whatsapp.net", None, 10)
            .unwrap();
        assert_eq!(home.len(), 1);
        assert_eq!(home[0].id, "x1");
        let mine = s.list_messages(me, None, 10).unwrap();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].id, "n1");
        assert_eq!(s.get_chat(me).unwrap().unwrap().last_preview, "nota");
    }

    #[test]
    fn read_receipts_naming_us_find_the_chat_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("t.db"), &generate_key()).unwrap();
        s.insert_message(&msg("ana", "a1", 10, "x", false), false)
            .unwrap();
        s.insert_message(&msg("ana", "a2", 20, "y", false), false)
            .unwrap();
        assert_eq!(
            s.mark_read_elsewhere(None, &["a2".into()]).unwrap(),
            ["ana".to_string()]
        );
        assert_eq!(s.get_chat("ana").unwrap().unwrap().unread, 0);
        // Read before it arrived here: it arrives already read.
        assert!(s
            .mark_read_elsewhere(None, &["b1".into()])
            .unwrap()
            .is_empty());
        s.insert_message(&msg("beto", "b1", 30, "z", false), false)
            .unwrap();
        assert_eq!(s.get_chat("beto").unwrap().unwrap().unread, 0);
        s.insert_message(&msg("beto", "b2", 40, "w", false), false)
            .unwrap();
        assert_eq!(s.get_chat("beto").unwrap().unwrap().unread, 1);
        assert_eq!(s.oldest_message("beto").unwrap().unwrap().id, "b1");
        assert!(!s.history_complete("beto"));
        s.set_history_complete("beto").unwrap();
        assert!(s.history_complete("beto"));
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

    #[test]
    fn message_actions() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("t.db"), &generate_key()).unwrap();
        let mut reply = msg("g@g.us", "2", 20, "sí", false);
        reply.quote_id = "1".into();
        reply.quote_text = "¿vamos?".into();
        s.insert_message(&msg("g@g.us", "1", 10, "¿vamos?", true), true)
            .unwrap();
        s.insert_message_full(&reply, true, None, Some(b"raw"), true)
            .unwrap();
        // Group preview names the last sender; contacts win over push names.
        s.upsert_contact("s@s.whatsapp.net", "Ana María", false)
            .unwrap();
        let chat = s.get_chat("g@g.us").unwrap().unwrap();
        assert_eq!(chat.last_sender, "Ana María");
        assert!(!chat.last_from_me);
        assert_eq!(s.get_raw("g@g.us", "2").unwrap().unwrap(), b"raw");

        s.set_reaction("g@g.us", "1", "x@s.whatsapp.net", false, "👍", 30)
            .unwrap();
        s.set_reaction("g@g.us", "1", "", true, "❤️", 31).unwrap();
        s.set_reaction("g@g.us", "1", "", true, "😂", 32).unwrap();
        let page = s.list_messages("g@g.us", None, 10).unwrap();
        assert_eq!(page[0].reactions.len(), 2);
        assert_eq!(page[1].quote_text, "¿vamos?");
        s.set_reaction("g@g.us", "1", "x@s.whatsapp.net", false, "", 33)
            .unwrap();
        assert_eq!(
            s.get_message("g@g.us", "1")
                .unwrap()
                .unwrap()
                .reactions
                .len(),
            1
        );

        assert!(s.edit_message("g@g.us", "2", "no").unwrap());
        let edited = s.get_message("g@g.us", "2").unwrap().unwrap();
        assert!(edited.edited);
        assert_eq!(s.get_chat("g@g.us").unwrap().unwrap().last_preview, "no");

        s.set_starred("g@g.us", "1", true).unwrap();
        s.set_message_pinned("g@g.us", "1", 99).unwrap();
        assert_eq!(s.starred_messages(10).unwrap().len(), 1);
        assert_eq!(s.pinned_messages("g@g.us").unwrap()[0].id, "1");

        assert!(s.mark_deleted("g@g.us", "2").unwrap());
        let gone = s.get_message("g@g.us", "2").unwrap().unwrap();
        assert!(gone.deleted && gone.text.is_empty());
        assert!(s.get_raw("g@g.us", "2").unwrap().is_none());
        s.delete_message("g@g.us", "1").unwrap();
        assert!(s.get_message("g@g.us", "1").unwrap().is_none());
    }

    #[test]
    fn stickers_and_notes() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("t.db"), &generate_key()).unwrap();
        s.add_sticker("aa", b"one", 10).unwrap();
        s.add_sticker("bb", b"two", 20).unwrap();
        s.add_sticker("aa", b"one", 5).unwrap();
        assert_eq!(s.stickers(10).unwrap()[0].id, "bb");
        s.set_sticker_favorite("aa", true).unwrap();
        assert_eq!(s.stickers(10).unwrap()[0].id, "aa");
        assert_eq!(s.get_sticker("bb").unwrap().unwrap(), b"two");
        s.remove_sticker("bb").unwrap();
        assert_eq!(s.stickers(10).unwrap().len(), 1);

        let mut note = s
            .add_note(&Note {
                text: "comprar pan".into(),
                ts: 1,
                kind: "text".into(),
                ..Default::default()
            })
            .unwrap();
        note.done = true;
        note.color = "yellow".into();
        s.update_note(&note).unwrap();
        let notes = s.list_notes().unwrap();
        assert!(notes[0].done);
        assert_eq!(notes[0].color, "yellow");
        s.delete_note(note.id).unwrap();
        assert!(s.list_notes().unwrap().is_empty());
    }
}
