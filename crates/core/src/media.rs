//! On-disk caches for downloaded media and profile pictures. Files live in
//! the app data folder with owner-only permissions; names are hashes so no
//! chat id or phone number appears on disk.

use crate::error::{Error, Result};
use crate::store::restrict_permissions;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Largest media file Notchat downloads or sends (WhatsApp's own limit is 2 GB,
/// but the webview receives files in memory).
pub const MAX_MEDIA_BYTES: usize = 64 * 1024 * 1024;

fn hashed(value: &str, len: usize) -> String {
    let digest = Sha256::digest(value.as_bytes());
    hex::encode(digest)[..len].to_string()
}

/// Total size of the regular files under `path`, recursively.
pub fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    restrict_permissions(path);
    Ok(())
}

pub struct MediaCache {
    root: PathBuf,
}

impl MediaCache {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("media"),
        }
    }

    /// A cache rooted somewhere else (notes keep their files apart from the
    /// WhatsApp media cache, so clearing storage never deletes them).
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn remove(&self, chat: &str, id: &str) {
        let _ = std::fs::remove_file(self.path(chat, id));
    }

    fn chat_dir(&self, chat: &str) -> PathBuf {
        self.root.join(hashed(chat, 16))
    }

    fn path(&self, chat: &str, id: &str) -> PathBuf {
        self.chat_dir(chat).join(hashed(id, 24))
    }

    pub fn get(&self, chat: &str, id: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(chat, id)).ok()
    }

    pub fn put(&self, chat: &str, id: &str, bytes: &[u8]) -> Result<()> {
        write_private(&self.path(chat, id), bytes)
    }

    pub fn chat_size(&self, chat: &str) -> u64 {
        dir_size(&self.chat_dir(chat))
    }

    pub fn total_size(&self) -> u64 {
        dir_size(&self.root)
    }

    pub fn clear_chat(&self, chat: &str) {
        let _ = std::fs::remove_dir_all(self.chat_dir(chat));
    }

    pub fn clear_all(&self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// What the avatar cache knows about a chat's picture.
#[derive(Debug, PartialEq)]
pub enum Avatar {
    Picture(Vec<u8>),
    /// Checked recently: the chat has no visible picture.
    None,
    /// Not cached or too old: ask WhatsApp.
    Unknown,
}

pub struct AvatarCache {
    root: PathBuf,
}

/// How long a cached picture (or the lack of one) is trusted.
const AVATAR_TTL: Duration = Duration::from_secs(24 * 3600);

impl AvatarCache {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("avatars"),
        }
    }

    fn path(&self, jid: &str) -> PathBuf {
        self.root.join(hashed(jid, 24))
    }

    fn fresh(path: &Path) -> bool {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < AVATAR_TTL)
    }

    pub fn get(&self, jid: &str) -> Avatar {
        let path = self.path(jid);
        let none = path.with_extension("none");
        if Self::fresh(&none) {
            return Avatar::None;
        }
        match std::fs::read(&path) {
            Ok(bytes) if Self::fresh(&path) => Avatar::Picture(bytes),
            _ => Avatar::Unknown,
        }
    }

    /// A stale picture, shown while a fresh one is fetched.
    pub fn stale(&self, jid: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(jid)).ok()
    }

    pub fn put(&self, jid: &str, picture: Option<&[u8]>) -> Result<()> {
        let path = self.path(jid);
        match picture {
            Some(bytes) => {
                let _ = std::fs::remove_file(path.with_extension("none"));
                write_private(&path, bytes)
            }
            None => {
                let _ = std::fs::remove_file(&path);
                write_private(&path.with_extension("none"), b"")
            }
        }
    }

    pub fn forget(&self, jid: &str) {
        let path = self.path(jid);
        let _ = std::fs::remove_file(path.with_extension("none"));
        let _ = std::fs::remove_file(path);
    }

    pub fn total_size(&self) -> u64 {
        dir_size(&self.root)
    }

    pub fn clear_all(&self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Only WhatsApp's own picture CDN is contacted for avatars.
pub fn check_avatar_url(raw: &str) -> Result<url::Url> {
    let url = url::Url::parse(raw).map_err(|_| Error::Invalid("picture url".into()))?;
    let host_ok = url
        .host_str()
        .is_some_and(|h| h == "whatsapp.net" || h.ends_with(".whatsapp.net"));
    if url.scheme() == "https" && host_ok {
        Ok(url)
    } else {
        Err(Error::Invalid("picture url".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_cache_roundtrip_and_sizes() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MediaCache::new(dir.path());
        assert!(cache.get("a@s.whatsapp.net", "1").is_none());
        cache.put("a@s.whatsapp.net", "1", b"hello").unwrap();
        cache.put("b@s.whatsapp.net", "1", b"hi").unwrap();
        assert_eq!(cache.get("a@s.whatsapp.net", "1").unwrap(), b"hello");
        assert_eq!(cache.chat_size("a@s.whatsapp.net"), 5);
        assert_eq!(cache.total_size(), 7);
        cache.clear_chat("a@s.whatsapp.net");
        assert_eq!(cache.total_size(), 2);
        cache.clear_all();
        assert_eq!(cache.total_size(), 0);
        // Names on disk never contain the chat id.
        cache.put("5491100@s.whatsapp.net", "x", b"1").unwrap();
        let names = format!("{:?}", walk(dir.path()));
        assert!(!names.contains("5491100"));
    }

    fn walk(p: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(p).unwrap().flatten() {
            out.push(e.path());
            if e.path().is_dir() {
                out.extend(walk(&e.path()));
            }
        }
        out
    }

    #[test]
    fn avatar_cache_states() {
        let dir = tempfile::tempdir().unwrap();
        let cache = AvatarCache::new(dir.path());
        assert_eq!(cache.get("j"), Avatar::Unknown);
        cache.put("j", Some(b"jpg")).unwrap();
        assert_eq!(cache.get("j"), Avatar::Picture(b"jpg".to_vec()));
        cache.put("j", None).unwrap();
        assert_eq!(cache.get("j"), Avatar::None);
        cache.forget("j");
        assert_eq!(cache.get("j"), Avatar::Unknown);
    }

    #[test]
    fn avatar_urls_are_restricted() {
        assert!(check_avatar_url("https://pps.whatsapp.net/v/t61/x.jpg?a=1").is_ok());
        assert!(check_avatar_url("http://pps.whatsapp.net/x.jpg").is_err());
        assert!(check_avatar_url("https://evil.com/x.jpg").is_err());
        assert!(check_avatar_url("https://whatsapp.net.evil.com/x").is_err());
        assert!(check_avatar_url("not a url").is_err());
    }
}
