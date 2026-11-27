//! Notification sound: on/off, and an optional MP3 chosen by the user.
//!
//! The file is copied into the app's data folder (`sounds/notify.mp3`, 0600
//! like every file the process writes) and checked to really be an MP3 before
//! it is stored. Without one, the default macOS sound is used.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Settings key for [`SoundConfig`].
pub const KEY: &str = "notify_sound";
/// Largest MP3 accepted as a notification sound.
pub const MAX_BYTES: usize = 5 * 1000 * 1000;
/// The sound played when the user has not chosen one (ships with macOS).
pub const DEFAULT_SOUND: &str = "/System/Library/Sounds/Glass.aiff";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SoundConfig {
    /// Play a sound with each notification.
    pub enabled: bool,
    /// File name of the user's MP3 as they chose it (for display); empty when
    /// the default sound is used.
    #[serde(default)]
    pub custom_name: String,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            custom_name: String::new(),
        }
    }
}

/// Where the user's MP3 is kept.
pub fn custom_path(data_dir: &Path) -> PathBuf {
    data_dir.join("sounds").join("notify.mp3")
}

/// The file to play for `config`: the user's MP3 when set and present,
/// otherwise the default sound.
pub fn sound_path(config: &SoundConfig, data_dir: &Path) -> PathBuf {
    let custom = custom_path(data_dir);
    if !config.custom_name.is_empty() && custom.is_file() {
        custom
    } else {
        PathBuf::from(DEFAULT_SOUND)
    }
}

/// True when `bytes` look like an MP3: an ID3v2 tag or an MPEG audio frame
/// header (11 sync bits, a valid version, layer III).
pub fn is_mp3(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    if bytes.starts_with(b"ID3") {
        return true;
    }
    let sync = bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0;
    let version_ok = (bytes[1] >> 3) & 0b11 != 0b01;
    let layer3 = (bytes[1] >> 1) & 0b11 == 0b01;
    let bitrate_ok = bytes[2] >> 4 != 0b1111;
    sync && version_ok && layer3 && bitrate_ok
}

/// A display name for the chosen file: its base name, without control
/// characters, at most 80 characters.
pub fn clean_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = base.chars().filter(|c| !c.is_control()).take(80).collect();
    if cleaned.trim().is_empty() {
        "sound.mp3".into()
    } else {
        cleaned
    }
}

/// Validates and stores the user's MP3, returning the updated config.
pub fn save_custom(
    data_dir: &Path,
    name: &str,
    bytes: &[u8],
    config: &SoundConfig,
) -> Result<SoundConfig> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err(Error::Invalid("sound size".into()));
    }
    if !is_mp3(bytes) {
        return Err(Error::Invalid("not an MP3 file".into()));
    }
    let path = custom_path(data_dir);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Write next to it first so a failed write never leaves half a file.
    let tmp = path.with_extension("mp3.part");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &path)?;
    Ok(SoundConfig {
        custom_name: clean_name(name),
        ..config.clone()
    })
}

/// Forgets the user's MP3 (back to the default sound).
pub fn clear_custom(data_dir: &Path, config: &SoundConfig) -> Result<SoundConfig> {
    match std::fs::remove_file(custom_path(data_dir)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(SoundConfig {
        custom_name: String::new(),
        ..config.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_mp3_files() {
        assert!(is_mp3(b"ID3\x04\x00\x00\x00\x00"));
        // MPEG-1 layer III frame, 128 kbps.
        assert!(is_mp3(&[0xFF, 0xFB, 0x90, 0x64]));
        // MPEG-2 layer III.
        assert!(is_mp3(&[0xFF, 0xF3, 0x48, 0xC4]));
        assert!(!is_mp3(b"RIFF....WAVE"));
        assert!(!is_mp3(b"\x89PNG\r\n"));
        assert!(!is_mp3(&[0xFF, 0xFF, 0x90, 0x64])); // layer I
        assert!(!is_mp3(&[0xFF, 0xFB]));
        assert!(!is_mp3(b""));
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("/Users/a/Music/Ding.mp3"), "Ding.mp3");
        assert_eq!(clean_name("C:\\x\\b\u{7}eep.mp3"), "beep.mp3");
        assert_eq!(clean_name(""), "sound.mp3");
        assert_eq!(clean_name(&"a".repeat(200)).chars().count(), 80);
    }

    #[test]
    fn custom_sound_is_saved_checked_and_cleared() {
        let dir = std::env::temp_dir().join(format!("notchat-sound-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let config = SoundConfig::default();
        assert!(config.enabled);
        assert_eq!(sound_path(&config, &dir), PathBuf::from(DEFAULT_SOUND));

        assert!(save_custom(&dir, "x.mp3", b"not audio", &config).is_err());
        assert!(save_custom(&dir, "x.mp3", &vec![0xFF; MAX_BYTES + 1], &config).is_err());
        assert!(!custom_path(&dir).exists());

        let saved = save_custom(&dir, "/tmp/Ding.mp3", b"ID3\x04\x00rest", &config).unwrap();
        assert_eq!(saved.custom_name, "Ding.mp3");
        assert_eq!(sound_path(&saved, &dir), custom_path(&dir));

        let cleared = clear_custom(&dir, &saved).unwrap();
        assert!(cleared.custom_name.is_empty());
        assert!(!custom_path(&dir).exists());
        assert_eq!(sound_path(&cleared, &dir), PathBuf::from(DEFAULT_SOUND));
        // Clearing twice is fine.
        assert!(clear_custom(&dir, &cleared).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
