//! Files the user attaches in the assistant chat. Content is extracted
//! locally and handed to the model as text (or as an image for vision models).

use crate::error::{Error, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};

pub const MAX_FILE_BYTES: usize = 15 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 24_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    /// "text" or "image"
    pub kind: String,
    /// Extracted text, or a `data:` URL for images.
    pub data: String,
}

fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Keeps only the file name, dropping any path the webview may have sent.
fn clean_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base.chars().filter(|c| !c.is_control()).take(120).collect();
    if cleaned.is_empty() {
        "file".into()
    } else {
        cleaned
    }
}

fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xff, 0xd8, 0xff, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("image/webp"),
        _ => None,
    }
}

pub fn extract(name: &str, bytes: &[u8]) -> Result<Attachment> {
    if bytes.is_empty() {
        return Err(Error::Invalid("empty file".into()));
    }
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Error::Invalid("file is larger than 15 MB".into()));
    }
    let name = clean_name(name);
    if let Some(mime) = image_mime(bytes) {
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        return Ok(Attachment {
            name,
            kind: "image".into(),
            data: format!("data:{mime};base64,{encoded}"),
        });
    }
    let text = if bytes.starts_with(b"%PDF") || extension(&name) == "pdf" {
        // The PDF parser can panic on malformed files; contain it.
        let owned = bytes.to_vec();
        std::panic::catch_unwind(move || pdf_extract::extract_text_from_mem(&owned))
            .map_err(|_| Error::Invalid("could not read this PDF".into()))?
            .map_err(|_| Error::Invalid("could not read this PDF".into()))?
    } else {
        match std::str::from_utf8(bytes) {
            Ok(text) => text.to_string(),
            Err(_) => return Err(Error::Invalid("unsupported file type".into())),
        }
    };
    let text = text.trim();
    if text.is_empty() {
        return Err(Error::Invalid("the file has no readable text".into()));
    }
    Ok(Attachment {
        name,
        kind: "text".into(),
        data: crate::net::truncate_chars(text, MAX_TEXT_CHARS),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_images() {
        let a = extract("../../etc/notes.txt", "hola mundo".as_bytes()).unwrap();
        assert_eq!(
            (a.name.as_str(), a.kind.as_str(), a.data.as_str()),
            ("notes.txt", "text", "hola mundo")
        );
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0];
        let img = extract("a.png", &png).unwrap();
        assert_eq!(img.kind, "image");
        assert!(img.data.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn rejects_binary_empty_and_oversized() {
        assert!(extract("a.bin", &[0xff, 0xfe, 0x00, 0x80]).is_err());
        assert!(extract("a.txt", b"").is_err());
        assert!(extract("a.txt", b"   ").is_err());
        assert!(extract("a.txt", &vec![b'a'; MAX_FILE_BYTES + 1]).is_err());
        assert!(extract("broken.pdf", b"%PDF-1.4 garbage").is_err());
    }

    #[test]
    fn long_text_is_truncated() {
        let a = extract("big.txt", "x".repeat(50_000).as_bytes()).unwrap();
        assert!(a.data.ends_with("[truncated]"));
        assert!(a.data.chars().count() < 25_000);
    }
}
