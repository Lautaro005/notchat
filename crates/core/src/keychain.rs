//! Database key storage in the macOS Keychain. The key never touches disk in
//! plaintext and is never logged.

use crate::error::{Error, Result};
use rand::RngCore;
use security_framework::passwords::{get_generic_password, set_generic_password};

const SERVICE: &str = "app.notchat.desktop";
const ACCOUNT: &str = "db-key-v1";

/// Returns the 256-bit database key as lowercase hex, creating it on first use.
pub fn db_key() -> Result<String> {
    if let Ok(bytes) = get_generic_password(SERVICE, ACCOUNT) {
        let key = String::from_utf8(bytes).map_err(|_| Error::Keychain("corrupt key".into()))?;
        if is_valid_key(&key) {
            return Ok(key);
        }
        return Err(Error::Keychain("stored key has unexpected format".into()));
    }
    let key = generate_key();
    set_generic_password(SERVICE, ACCOUNT, key.as_bytes())
        .map_err(|e| Error::Keychain(e.to_string()))?;
    Ok(key)
}

pub fn generate_key() -> String {
    let mut raw = [0u8; 32];
    rand::rng().fill_bytes(&mut raw);
    hex::encode(raw)
}

pub fn is_valid_key(key: &str) -> bool {
    key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keys_are_valid_and_unique() {
        let a = generate_key();
        let b = generate_key();
        assert!(is_valid_key(&a));
        assert_ne!(a, b);
        assert!(!is_valid_key("abc"));
        assert!(!is_valid_key(&"z".repeat(64)));
    }
}
