use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// A random string from `bytes` bytes of the system's secure random source, in URL-safe base64.
pub fn random(bytes: usize) -> String {
    let mut buffer = vec![0; bytes];
    getrandom::fill(&mut buffer).expect("the system's random source should work");
    URL_SAFE_NO_PAD.encode(buffer)
}

/// A device's token: 32 random bytes.
pub fn token() -> String {
    random(32)
}

/// What the server keeps in place of a token or code.
pub fn hash(secret: &str) -> Vec<u8> {
    Sha256::digest(secret.as_bytes()).to_vec()
}

/// Epoch milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_long_and_different() {
        let (a, b) = (token(), token());
        assert_eq!(a.len(), 43);
        assert_ne!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn hashes_are_stable() {
        assert_eq!(hash("abc"), hash("abc"));
        assert_ne!(hash("abc"), hash("abd"));
        assert_eq!(hash("abc").len(), 32);
    }
}
