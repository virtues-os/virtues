//! A publication token: 128 random bits, base64url without padding, so
//! exactly 22 characters from `[A-Za-z0-9_-]`.
//!
//! The check is strict on purpose. A token is used as a directory name, so
//! anything that is not exactly this shape (a slash, a dot, a different
//! length) is refused before it is ever joined to a path.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

pub const LEN: usize = 22;

/// A fresh token from the OS random source.
pub fn generate() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the OS random source is unavailable");
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Whether `s` is shaped like a token. Says nothing about whether one exists.
pub fn is_valid(s: &str) -> bool {
    s.len() == LEN && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_valid_and_distinct() {
        let a = generate();
        let b = generate();
        assert!(is_valid(&a), "{a}");
        assert_ne!(a, b);
    }

    #[test]
    fn anything_path_shaped_is_refused() {
        for bad in [
            "",
            "..",
            "../../../etc/passwd",
            "aaaaaaaaaaaaaaaaaaaa/a",
            "aaaaaaaaaaaaaaaaaaa..a",
            "aaaaaaaaaaaaaaaaaaaaa",
            "aaaaaaaaaaaaaaaaaaaaaaa",
            "aaaaaaaaaaaaaaaaaaaaa\0",
            "aaaaaaaaaaaaaaaaaaaaa%",
        ] {
            assert!(!is_valid(bad), "{bad:?}");
        }
    }
}
