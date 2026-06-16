//! Curve25519 key generation for pairing.
//!
//! The device mints a fresh keypair at pair time, sends the **public** key to
//! the box (`wg_public_key` in `/api/pair/consume`), and keeps the **private**
//! key in the OS keychain. The box uses the public key to provision a WG peer
//! and returns a [`PairingBundle`](virtues_protocol::PairingBundle); the private
//! key never leaves the device.

use base64::Engine;
use rand::rngs::OsRng;
use x25519_dalek::{PublicKey, StaticSecret};

/// A freshly generated keypair, base64-encoded (standard, padded) to match the
/// box's WG key encoding (`WgParams::server_public_key`, `preshared_key`).
#[derive(Debug, Clone)]
pub struct Keypair {
    /// Base64 private key — store in the keychain, never transmit.
    pub private_key_b64: String,
    /// Base64 public key — send to the box as `wg_public_key`.
    pub public_key_b64: String,
}

/// Generate a new Curve25519 keypair for pairing.
pub fn generate_keypair() -> Keypair {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    let b64 = base64::engine::general_purpose::STANDARD;
    Keypair {
        private_key_b64: b64.encode(secret.to_bytes()),
        public_key_b64: b64.encode(public.as_bytes()),
    }
}

/// Decode a base64 WG key into raw 32 bytes. Accepts standard base64 (padded),
/// which is what the box emits for every key in the bundle.
pub(crate) fn decode_key_b64(s: &str) -> Result<[u8; 32], crate::TunnelError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .map_err(|e| crate::TunnelError::BadKey(format!("base64: {e}")))?;
    let arr: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| crate::TunnelError::BadKey(format!("expected 32 bytes, got {}", bytes.len())))?;
    Ok(arr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_roundtrips_to_32_bytes() {
        let kp = generate_keypair();
        let priv_raw = decode_key_b64(&kp.private_key_b64).unwrap();
        let pub_raw = decode_key_b64(&kp.public_key_b64).unwrap();
        // Public key is derivable from the private key — proves they're a pair.
        let derived = PublicKey::from(&StaticSecret::from(priv_raw));
        assert_eq!(derived.as_bytes(), &pub_raw);
    }

    #[test]
    fn keypairs_are_unique() {
        assert_ne!(
            generate_keypair().private_key_b64,
            generate_keypair().private_key_b64
        );
    }
}
