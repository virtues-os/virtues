//! Blind-rendezvous blob crypto + endpoint model (box side).
//!
//! The box encrypts its current WireGuard endpoint under a per-box key `K`
//! (minted at pairing, never sent to Virtues) and PUTs the ciphertext to the
//! rendezvous via [`crate::virtues_api::client::BearerClient::put_bytes`]. A
//! paired phone GETs it and decrypts with the same K. virtues-api stores only
//! the opaque ciphertext, so it learns nothing about the address or who owns
//! it.
//!
//! See `docs/wireguard-pairing.md` §6 and
//! `services/virtues-api/src/routes/rendezvous.rs`.

use anyhow::{anyhow, Result};
use base64::Engine;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

use virtues_helpers::crypto::{open_aes_256_gcm, seal_aes_256_gcm};

/// The plaintext published to the rendezvous (encrypted under K before it
/// leaves the box). Kept tiny — it must fit the route's blob size cap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointBlob {
    /// Schema version (currently 1).
    pub v: u8,
    /// The box's current reachable global address (typically IPv6).
    pub ip: String,
    /// WireGuard listen port.
    pub port: u16,
    /// Base64 WG server public key, so the phone can repin if it rotated.
    pub wg_pub: String,
    /// Unix seconds at publish time; the phone rejects a blob older than its
    /// cached endpoint.
    pub ts: i64,
}

/// Mint a fresh per-box rendezvous key K (32 random bytes). Lives only on the
/// box + its paired devices; never sent to Virtues.
pub fn generate_key() -> [u8; 32] {
    let mut k = [0u8; 32];
    SystemRandom::new()
        .fill(&mut k)
        .expect("SystemRandom should always produce bytes");
    k
}

/// Mint an opaque publish_id capability (16 random bytes → base64url, no pad).
/// Holding it is the read capability; it identifies no customer and no bearer.
pub fn generate_publish_id() -> String {
    let mut id = [0u8; 16];
    SystemRandom::new()
        .fill(&mut id)
        .expect("SystemRandom should always produce bytes");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(id)
}

/// Encrypt an endpoint blob under K → `nonce||ct||tag` bytes for the PUT body.
pub fn encrypt_endpoint(key: &[u8; 32], blob: &EndpointBlob) -> Result<Vec<u8>> {
    let plaintext = serde_json::to_vec(blob)?;
    seal_aes_256_gcm(key, &plaintext).map_err(|e| anyhow!("rendezvous seal failed: {e}"))
}

/// Decrypt rendezvous bytes under K → endpoint blob. Fails closed on a wrong
/// key or tampered ciphertext.
pub fn decrypt_endpoint(key: &[u8; 32], data: &[u8]) -> Result<EndpointBlob> {
    let plaintext =
        open_aes_256_gcm(key, data).map_err(|e| anyhow!("rendezvous open failed: {e}"))?;
    Ok(serde_json::from_slice(&plaintext)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> EndpointBlob {
        EndpointBlob {
            v: 1,
            ip: "2001:db8::1".into(),
            port: 51820,
            wg_pub: "c2VydmVycHVibGlja2V5".into(),
            ts: 1_700_000_000,
        }
    }

    #[test]
    fn blob_round_trip() {
        let k = generate_key();
        let blob = sample();
        let ct = encrypt_endpoint(&k, &blob).unwrap();
        let back = decrypt_endpoint(&k, &ct).unwrap();
        assert_eq!(blob, back);
    }

    #[test]
    fn wrong_key_fails() {
        let ct = encrypt_endpoint(&generate_key(), &sample()).unwrap();
        assert!(decrypt_endpoint(&generate_key(), &ct).is_err());
    }

    #[test]
    fn publish_id_is_urlsafe_and_sized() {
        let id = generate_publish_id();
        assert_eq!(id.len(), 22); // 16 bytes, base64url no-pad
        assert!(id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    }

    #[test]
    fn blob_stays_under_cap() {
        // The route caps the PUT body at 1024 bytes; a realistic blob must fit.
        let ct = encrypt_endpoint(&generate_key(), &sample()).unwrap();
        assert!(ct.len() < 1024);
    }
}
