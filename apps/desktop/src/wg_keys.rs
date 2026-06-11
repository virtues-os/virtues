//! WireGuard keypair generation.
//!
//! Curve25519 X25519 keypair using `x25519-dalek`. Pure Rust, no platform
//! dependencies — same code path on Mac, Linux, Windows, future iOS / Android.
//!
//! Encoding follows the WireGuard convention: base64 standard alphabet with
//! padding (`Key=`-style strings you see in `wg show` output).

use anyhow::{Context, Result};
use base64::Engine as _;
use rand::rngs::OsRng;
use x25519_dalek::{PublicKey, StaticSecret};

/// A freshly minted WG keypair.
pub struct Keypair {
    pub private: StaticSecret,
    pub public: PublicKey,
}

impl Keypair {
    /// Generate a new keypair from the OS RNG. Each device generates its own
    /// once, at pair time. The public key goes to the box; the private key
    /// stays in the OS keychain.
    pub fn generate() -> Self {
        let private = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&private);
        Self { private, public }
    }

    /// Reconstruct a keypair from a stored base64-encoded private key.
    pub fn from_private_b64(s: &str) -> Result<Self> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(s)
            .context("decode private key base64")?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("private key is not 32 bytes"))?;
        let private = StaticSecret::from(arr);
        let public = PublicKey::from(&private);
        Ok(Self { private, public })
    }

    pub fn private_b64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.private.as_bytes())
    }

    pub fn public_b64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.public.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_yields_32_byte_keys() {
        let kp = Keypair::generate();
        let priv_b = base64::engine::general_purpose::STANDARD
            .decode(kp.private_b64())
            .unwrap();
        let pub_b = base64::engine::general_purpose::STANDARD
            .decode(kp.public_b64())
            .unwrap();
        assert_eq!(priv_b.len(), 32);
        assert_eq!(pub_b.len(), 32);
    }

    #[test]
    fn from_private_b64_round_trips() {
        let kp = Keypair::generate();
        let priv_str = kp.private_b64();
        let pub_str = kp.public_b64();

        let restored = Keypair::from_private_b64(&priv_str).unwrap();
        assert_eq!(restored.private_b64(), priv_str);
        assert_eq!(restored.public_b64(), pub_str);
    }

    #[test]
    fn two_generates_are_distinct() {
        let a = Keypair::generate();
        let b = Keypair::generate();
        // OS RNG produces distinct keys with overwhelming probability.
        assert_ne!(a.public_b64(), b.public_b64());
    }
}
