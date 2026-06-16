//! Blind-rendezvous client: recover the box's current WG endpoint after its
//! ISP rotates the IPv6 prefix.
//!
//! The box encrypts its current endpoint under a per-box key `K` (minted at
//! pairing, in `RendezvousParams::key`) and PUTs the ciphertext to
//! virtues-api. We GET it and decrypt with the same `K`. virtues-api only ever
//! holds opaque ciphertext, so it learns nothing about the address.
//!
//! This is the **client** half of the box-side
//! `virtues_core::virtues_api::rendezvous` module — the blob shape and the
//! AES-256-GCM construction (12-byte nonce prefix ‖ ciphertext ‖ tag, empty
//! AAD) are matched exactly; see the round-trip test.

use std::time::Duration;

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use serde::Deserialize;
use virtues_protocol::RendezvousParams;

use crate::keys::decode_key_b64;
use crate::TunnelError;

/// Must match `virtues_helpers::crypto::NONCE_LENGTH` (12, standard GCM).
const NONCE_LENGTH: usize = 12;

/// The plaintext the box publishes. Mirror of the box's `EndpointBlob`; extra
/// fields the box might add later are ignored (serde default behavior).
#[derive(Debug, Clone, Deserialize)]
pub struct EndpointBlob {
    /// Schema version (currently 1).
    pub v: u8,
    /// The box's current reachable global address (typically IPv6).
    pub ip: String,
    /// WireGuard listen port.
    pub port: u16,
    /// Base64 WG server public key, so we can repin if the box rotated it.
    pub wg_pub: String,
    /// Unix seconds at publish time; we reject a blob older than our cached one.
    pub ts: i64,
}

/// Decrypt rendezvous bytes (`nonce ‖ ciphertext ‖ tag`) under `K`. Fails
/// closed on a wrong key or tampered ciphertext. Exact inverse of the box's
/// `seal_aes_256_gcm`.
pub fn decrypt_endpoint(key: &[u8; 32], data: &[u8]) -> Result<EndpointBlob, TunnelError> {
    if data.len() < NONCE_LENGTH + AES_256_GCM.tag_len() {
        return Err(TunnelError::Rendezvous("ciphertext too short".into()));
    }
    let unbound = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|_| TunnelError::Rendezvous("invalid AES key".into()))?;
    let opening = LessSafeKey::new(unbound);

    let (nonce_bytes, encrypted) = data.split_at(NONCE_LENGTH);
    let mut nonce_arr = [0u8; NONCE_LENGTH];
    nonce_arr.copy_from_slice(nonce_bytes);
    let nonce = Nonce::assume_unique_for_key(nonce_arr);

    let mut in_out = encrypted.to_vec();
    let plaintext = opening
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| TunnelError::Rendezvous("decrypt failed (wrong key or tampered)".into()))?;

    serde_json::from_slice(plaintext)
        .map_err(|e| TunnelError::Rendezvous(format!("blob json: {e}")))
}

/// Fetch + decrypt the box's current endpoint from the rendezvous.
///
/// `min_ts` is the publish time of the endpoint we're currently using (0 if
/// none); a blob with an older `ts` is rejected as stale so a replay can't
/// downgrade us to a dead address.
pub fn fetch_endpoint(
    rdv: &RendezvousParams,
    min_ts: i64,
) -> Result<EndpointBlob, TunnelError> {
    let key = decode_key_b64(&rdv.key)?;

    let resp = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build()
        .get(&rdv.url)
        .call()
        .map_err(|e| TunnelError::Rendezvous(format!("GET {}: {e}", rdv.url)))?;

    let mut bytes = Vec::new();
    use std::io::Read;
    resp.into_reader()
        .take(64 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| TunnelError::Rendezvous(format!("read body: {e}")))?;

    let blob = decrypt_endpoint(&key, &bytes)?;
    if blob.ts < min_ts {
        return Err(TunnelError::Rendezvous(format!(
            "stale blob (ts {} < cached {min_ts})",
            blob.ts
        )));
    }
    Ok(blob)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::{SecureRandom, SystemRandom};

    /// Seal exactly like the box (`seal_aes_256_gcm`) so the test proves wire
    /// compatibility, not just self-consistency.
    fn seal(key: &[u8; 32], plaintext: &[u8]) -> Vec<u8> {
        let unbound = UnboundKey::new(&AES_256_GCM, key).unwrap();
        let sealing = LessSafeKey::new(unbound);
        let mut nonce_bytes = [0u8; NONCE_LENGTH];
        SystemRandom::new().fill(&mut nonce_bytes).unwrap();
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut in_out = plaintext.to_vec();
        sealing
            .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
            .unwrap();
        let mut out = nonce_bytes.to_vec();
        out.extend_from_slice(&in_out);
        out
    }

    #[test]
    fn decrypts_box_sealed_blob() {
        let key = [7u8; 32];
        let json = br#"{"v":1,"ip":"2001:db8::1","port":51820,"wg_pub":"abc","ts":1700000000}"#;
        let ct = seal(&key, json);
        let blob = decrypt_endpoint(&key, &ct).unwrap();
        assert_eq!(blob.v, 1);
        assert_eq!(blob.ip, "2001:db8::1");
        assert_eq!(blob.port, 51820);
        assert_eq!(blob.ts, 1_700_000_000);
    }

    #[test]
    fn wrong_key_fails_closed() {
        let json = br#"{"v":1,"ip":"::1","port":1,"wg_pub":"x","ts":1}"#;
        let ct = seal(&[1u8; 32], json);
        assert!(decrypt_endpoint(&[2u8; 32], &ct).is_err());
    }

    #[test]
    fn too_short_fails() {
        assert!(decrypt_endpoint(&[0u8; 32], &[0u8; 4]).is_err());
    }
}
