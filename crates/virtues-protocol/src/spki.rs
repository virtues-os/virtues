//! SPKI fingerprint computation — the box's identity primitive.
//!
//! Every paired device pins the box by the SHA-256 of its WG static public key.
//! The WG handshake (Noise IK) verifies this implicitly; this module gives every
//! component a uniform string form for display, comparison, and out-of-band
//! verification (e.g. user comparing the fingerprint shown on the box's screen
//! to what the daemon reports).
//!
//! Format: `"sha256-<base64>"` — matches the [W3C subresource-integrity hash
//! string format](https://www.w3.org/TR/SRI/#the-integrity-attribute) so it's
//! recognizable and easy to grep for.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A SPKI fingerprint in canonical string form. Wrap a raw fingerprint string
/// in this newtype to make "pinned" peers explicit in function signatures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpkiFingerprint(pub String);

impl SpkiFingerprint {
    /// Build a fingerprint from raw 32 bytes (a Curve25519 public key).
    pub fn from_wg_pubkey(pubkey: &[u8; 32]) -> Self {
        Self(spki_fingerprint(pubkey))
    }

    /// The string form: `"sha256-<base64>"`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SpkiFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Compute the SPKI fingerprint of a WG (Curve25519) public key.
///
/// Returns `"sha256-<base64>"`. Both the box and each device call this with the
/// box's WG public key; if they agree, they're talking to the same identity.
pub fn spki_fingerprint(wg_pubkey: &[u8; 32]) -> String {
    let mut h = Sha256::new();
    h.update(wg_pubkey);
    let digest = h.finalize();
    format!(
        "sha256-{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_deterministic() {
        let pk = [7u8; 32];
        assert_eq!(spki_fingerprint(&pk), spki_fingerprint(&pk));
    }

    #[test]
    fn fingerprint_is_sensitive_to_input() {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        b[0] = 1;
        assert_ne!(spki_fingerprint(&a), spki_fingerprint(&b));
        // also test that flipping a tail byte changes the output
        a[31] = 1;
        assert_ne!(spki_fingerprint(&a), spki_fingerprint(&b));
    }

    #[test]
    fn fingerprint_has_expected_format() {
        let pk = [0u8; 32];
        let f = spki_fingerprint(&pk);
        assert!(f.starts_with("sha256-"));
        // base64-encoded sha256 (32 bytes) is 43 chars when unpadded
        assert_eq!(f.len(), "sha256-".len() + 43);
    }

    #[test]
    fn newtype_display_matches_string() {
        let pk = [42u8; 32];
        let fp = SpkiFingerprint::from_wg_pubkey(&pk);
        assert_eq!(fp.to_string(), spki_fingerprint(&pk));
    }
}
