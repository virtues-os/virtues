//! Wire protocol between a box and the blind relay.
//!
//! Framing is **newline-delimited JSON**: each message is one `serde_json` value
//! on its own line. The box opens connections *outbound* to the relay; the first
//! line on any connection is a [`BoxHello`] declaring whether it's the persistent
//! **control** connection or an ephemeral **work** connection (1:1 passthrough —
//! one work connection per inbound client, no multiplexer in v1).
//!
//! After a `BoxHello::Register`, the control connection carries [`RelayMsg`]
//! (relay→box) and [`BoxMsg`] (box→relay). A work connection carries only the
//! `BoxHello::Work` line, after which the relay splices raw ciphertext over it.
//!
//! This module is pure data + serde — no I/O. The relay and the box-side client
//! both depend on it so the shape can't drift.

use serde::{Deserialize, Serialize};

/// Seconds per revocation bucket (24h). Tokens are scoped to a bucket so the
/// relay can expire a box's access **without holding per-box state**: it accepts
/// only the current or previous bucket, and a box must re-fetch its token each
/// bucket from atlas — which stops minting for a revoked/lapsed account. See
/// `docs/relay-control-plane.md` → Revocation.
pub const BUCKET_SECS: u64 = 86_400;

/// The revocation bucket index for a unix timestamp (seconds).
pub fn bucket_at(unix_secs: u64) -> u64 {
    unix_secs / BUCKET_SECS
}

/// The current revocation bucket from the system clock.
pub fn current_bucket() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    bucket_at(now)
}

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

/// The canonical message signed for a box's per-SNI, per-bucket registration.
/// Both the signer (atlas) and the verifier (relay) must construct it identically.
fn token_message(sni: &str, bucket: u64) -> String {
    format!("{sni}:{bucket}")
}

/// Sign a box's per-SNI, per-bucket registration token with atlas's Ed25519
/// **private** key. Returns `hex(signature)` (128 hex chars).
///
/// **Asymmetric by design:** only atlas holds the signing key. The relay holds
/// only the matching **public** key and *verifies* ([`verify_token`]) — it can
/// never mint, so a relay compromise leaks nothing forgeable (unlike a shared
/// HMAC secret, where the verifier could also mint). Binding to the SNI means a
/// leaked token authorizes only its own name; binding to the bucket means it
/// expires after ~2 buckets unless atlas keeps re-minting — stateless revocation.
/// See `docs/relay-control-plane.md`.
pub fn sign_token(signing_key: &SigningKey, sni: &str, bucket: u64) -> String {
    let sig = signing_key.sign(token_message(sni, bucket).as_bytes());
    hex::encode(sig.to_bytes())
}

/// Verify a `hex(signature)` registration token against atlas's Ed25519 **public**
/// key for the given `sni`+`bucket`. No secret is involved (a public key is not
/// secret), so the relay holds nothing that can forge a token. Uses strict
/// verification (rejects malleable / non-canonical signatures). Returns `false`
/// on any malformed input rather than erroring — never panics on network data.
pub fn verify_token(verifying_key: &VerifyingKey, sni: &str, bucket: u64, token: &str) -> bool {
    let Ok(bytes) = hex::decode(token) else {
        return false;
    };
    let Ok(sig) = Signature::from_slice(&bytes) else {
        return false;
    };
    verifying_key
        .verify_strict(token_message(sni, bucket).as_bytes(), &sig)
        .is_ok()
}

/// Parse a hex-encoded 32-byte Ed25519 **signing** (private) key — held by atlas
/// only. `None` if the hex is malformed or not exactly 32 bytes.
pub fn parse_signing_key(hex_key: &str) -> Option<SigningKey> {
    let bytes = hex::decode(hex_key.trim()).ok()?;
    let arr: [u8; 32] = bytes.try_into().ok()?;
    Some(SigningKey::from_bytes(&arr))
}

/// Parse a hex-encoded 32-byte Ed25519 **verifying** (public) key — held by the
/// relay (non-secret). `None` if malformed, wrong length, or not a valid point.
pub fn parse_verifying_key(hex_key: &str) -> Option<VerifyingKey> {
    let bytes = hex::decode(hex_key.trim()).ok()?;
    let arr: [u8; 32] = bytes.try_into().ok()?;
    VerifyingKey::from_bytes(&arr).ok()
}

/// First line on any box→relay connection: declares its purpose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BoxHello {
    /// Open the persistent control connection for `sni`, authenticated by `token`
    /// (a simple shared bearer in v1; blinded tokens in P3).
    Register { sni: String, token: String },
    /// A work connection answering a [`RelayMsg::OpenConn`] for `conn_id`.
    Work { conn_id: String },
}

/// Relay → box, over the control connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayMsg {
    /// Registration accepted (confirm-by-echo, not assumption).
    Registered,
    /// Registration refused.
    Rejected { reason: String },
    /// Asks the box to dial a fresh work connection for an inbound client.
    OpenConn { conn_id: String },
    /// Liveness probe; the box must answer [`BoxMsg::Pong`].
    Ping,
}

/// Box → relay, over the control connection (after `Register`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BoxMsg {
    /// Liveness response to [`RelayMsg::Ping`].
    Pong,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_hello_register_roundtrip() {
        let m = BoxHello::Register {
            sni: "abc.boxes.virtues.com".into(),
            token: "secret".into(),
        };
        let line = serde_json::to_string(&m).unwrap();
        assert_eq!(
            line,
            r#"{"type":"register","sni":"abc.boxes.virtues.com","token":"secret"}"#
        );
        assert_eq!(serde_json::from_str::<BoxHello>(&line).unwrap(), m);
    }

    #[test]
    fn box_hello_work_roundtrip() {
        let m = BoxHello::Work {
            conn_id: "11111111-1111-1111-1111-111111111111".into(),
        };
        let line = serde_json::to_string(&m).unwrap();
        assert_eq!(serde_json::from_str::<BoxHello>(&line).unwrap(), m);
    }

    #[test]
    fn relay_msg_open_conn_roundtrip() {
        let m = RelayMsg::OpenConn {
            conn_id: "deadbeef".into(),
        };
        let line = serde_json::to_string(&m).unwrap();
        assert_eq!(line, r#"{"type":"open_conn","conn_id":"deadbeef"}"#);
        assert_eq!(serde_json::from_str::<RelayMsg>(&line).unwrap(), m);
    }

    #[test]
    fn token_signs_verifies_and_is_sni_bucket_bound() {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let vk = sk.verifying_key();
        let bucket = 20_000;
        let t = sign_token(&sk, "a.virtues.ch", bucket);

        // Correct sni+bucket under the matching public key verifies.
        assert!(verify_token(&vk, "a.virtues.ch", bucket, &t));
        // Wrong SNI fails (a leaked token can't be reused for another box).
        assert!(!verify_token(&vk, "b.virtues.ch", bucket, &t));
        // Wrong bucket fails (this is what makes it expire).
        assert!(!verify_token(&vk, "a.virtues.ch", bucket + 1, &t));
        // A DIFFERENT key can't verify — the relay only accepts atlas-signed
        // tokens, and holding the public key does NOT let it mint.
        let other_vk = SigningKey::from_bytes(&[9u8; 32]).verifying_key();
        assert!(!verify_token(&other_vk, "a.virtues.ch", bucket, &t));
        // Ed25519 signing is deterministic → atlas and any re-mint agree.
        assert_eq!(t, sign_token(&sk, "a.virtues.ch", bucket));
        // hex(64-byte signature) = 128 chars.
        assert_eq!(t.len(), 128);
        // Malformed tokens return false, never panic (network-facing).
        assert!(!verify_token(&vk, "a.virtues.ch", bucket, "not-hex"));
        assert!(!verify_token(&vk, "a.virtues.ch", bucket, "deadbeef"));
        assert!(!verify_token(&vk, "a.virtues.ch", bucket, ""));
    }

    #[test]
    fn key_parsers_roundtrip_and_reject_garbage() {
        let sk = SigningKey::from_bytes(&[3u8; 32]);
        let vk = sk.verifying_key();
        let sk2 = parse_signing_key(&hex::encode(sk.to_bytes())).unwrap();
        let vk2 = parse_verifying_key(&hex::encode(vk.to_bytes())).unwrap();
        // A token signed by the parsed key verifies under the parsed public key.
        let t = sign_token(&sk2, "x.virtues.ch", 1);
        assert!(verify_token(&vk2, "x.virtues.ch", 1, &t));
        // Garbage / wrong-length hex is rejected, not panicked.
        assert!(parse_signing_key("xyz").is_none());
        assert!(parse_signing_key("dead").is_none()); // too short
        assert!(parse_verifying_key("xyz").is_none());
    }

    #[test]
    fn bucket_at_is_day_quantized() {
        assert_eq!(bucket_at(0), 0);
        assert_eq!(bucket_at(BUCKET_SECS - 1), 0);
        assert_eq!(bucket_at(BUCKET_SECS), 1);
        assert_eq!(bucket_at(BUCKET_SECS * 3 + 5), 3);
    }

    #[test]
    fn pong_roundtrip() {
        let line = serde_json::to_string(&BoxMsg::Pong).unwrap();
        assert_eq!(line, r#"{"type":"pong"}"#);
        assert_eq!(serde_json::from_str::<BoxMsg>(&line).unwrap(), BoxMsg::Pong);
    }
}
