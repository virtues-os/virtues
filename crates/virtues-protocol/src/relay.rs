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

/// Derive a box's per-SNI, per-bucket registration token:
/// `hex(HMAC-SHA256(secret, "<sni>:<bucket>"))`.
///
/// The relay holds the `secret` and derives the expected token for the `sni` a
/// box claims at `Register` (checking the current and previous `bucket`); atlas
/// mints the same value for the current bucket. Binding to the SNI means a box
/// (or a leaked token) can register only its **own** name — it can't compute a
/// valid token for another tenant's SNI without the secret. Binding to the
/// `bucket` means a token naturally expires after ~2 buckets unless atlas keeps
/// re-minting it, which is how a stateless relay supports revocation. Compare in
/// constant time.
pub fn derive_token(secret: &str, sni: &str, bucket: u64) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(sni.as_bytes());
    mac.update(b":");
    mac.update(bucket.to_string().as_bytes());
    hex::encode(mac.finalize().into_bytes())
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
    fn derive_token_is_sni_bucket_bound_and_stable() {
        let secret = "relay-secret";
        let bucket = 20_000;
        let a = derive_token(secret, "a.virtues.ch", bucket);
        let b = derive_token(secret, "b.virtues.ch", bucket);
        // Same inputs → same token (atlas and relay must agree).
        assert_eq!(a, derive_token(secret, "a.virtues.ch", bucket));
        // Different SNI → different token (can't reuse one box's token for another).
        assert_ne!(a, b);
        // Different bucket → different token (this is what makes it expire).
        assert_ne!(a, derive_token(secret, "a.virtues.ch", bucket + 1));
        // Hex-encoded SHA-256 HMAC is 64 chars.
        assert_eq!(a.len(), 64);
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
