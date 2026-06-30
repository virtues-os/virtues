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

/// Derive a box's per-SNI registration token: `hex(HMAC-SHA256(secret, sni))`.
///
/// The relay holds the `secret` and derives the expected token for the `sni` a
/// box claims at `Register`; box provisioning mints the same value. Because the
/// token is bound to the SNI, a box (or a leaked single token) can register only
/// its **own** name — it can't compute a valid token for another tenant's SNI
/// without the secret, which closes the cross-tenant-hijack hole that a flat
/// shared bearer leaves open. Compare the result in constant time.
pub fn derive_token(secret: &str, sni: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(sni.as_bytes());
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
    fn derive_token_is_sni_bound_and_stable() {
        let secret = "relay-secret";
        let a = derive_token(secret, "a.boxes.virtues.com");
        let b = derive_token(secret, "b.boxes.virtues.com");
        // Same inputs → same token (provisioning and relay must agree).
        assert_eq!(a, derive_token(secret, "a.boxes.virtues.com"));
        // Different SNI → different token (can't reuse one box's token for another).
        assert_ne!(a, b);
        // Hex-encoded SHA-256 HMAC is 64 chars.
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn pong_roundtrip() {
        let line = serde_json::to_string(&BoxMsg::Pong).unwrap();
        assert_eq!(line, r#"{"type":"pong"}"#);
        assert_eq!(serde_json::from_str::<BoxMsg>(&line).unwrap(), BoxMsg::Pong);
    }
}
