//! Hole-punch coordinator types.
//!
//! Wire shapes for the stateless punch helper on atlas (`/v1/rendezvous/punch/*`)
//! that mediates NAT traversal when direct WG dial fails. See
//! `[[remote-access-decision]]` in MEMORY.md for the architectural commitment.
//!
//! ## Flow
//!
//! 1. Daemon tries direct WG handshake via the rendezvous-published endpoint.
//! 2. On timeout (~3s), daemon `POST`s a [`PunchAnnounce`] to atlas with its
//!    own reflected address (`my_role = Device`).
//! 3. Box (on its periodic rendezvous publish loop) also announces with
//!    `my_role = Box`.
//! 4. Either side `GET`s `/v1/rendezvous/punch/peer/<publish_id>?my_role=...`.
//!    When both announcements are present, atlas returns a [`PunchPeerResponse`]
//!    containing the other peer's reflected address and a `fire_time` ~250ms
//!    in the future.
//! 5. At `fire_time`, both peers fire a UDP packet at the other's reflected
//!    address simultaneously. Outbound packets open NAT mappings; the
//!    in-flight packets from the other side arrive through them.
//! 6. WG handshake completes. Daemon `POST`s [`PunchCompleteRequest`] for
//!    telemetry + immediate cleanup.
//!
//! All state on atlas is ephemeral (≤30s TTL). Atlas sees opaque `publish_id`s
//! and public reflected addresses; no identity, no traffic, no content. See
//! `[[network-topology-star]]`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Which side of the pair this caller is. The coordinator needs to know so it
/// can match a `Device` to a `Box` (and never device-to-device).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PunchRole {
    /// The box (server side of the WG tunnel).
    Box,
    /// A paired device trying to reach the box.
    Device,
}

/// `POST /v1/rendezvous/punch/announce` body.
///
/// "I want to NAT-punch with `publish_id`." Atlas observes the announcer's
/// reflected `ip:port` from the request socket peer (via `ConnectInfo`); the
/// caller doesn't assert it, because (a) accepting client-asserted addresses
/// would let an attacker poison legitimate punches by spamming the
/// `publish_id` with bogus addresses, and (b) it would turn atlas into a UDP
/// reflector — anyone could provoke atlas-coordinated peers to send UDP at
/// arbitrary victim IPs.
///
/// Atlas holds the resulting slot in an ephemeral row (≤30 s TTL) until the
/// other peer announces or the row expires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PunchAnnounce {
    pub publish_id: String,
    pub my_role: PunchRole,
}

/// `GET /v1/rendezvous/punch/peer/<publish_id>?my_role=device` 200 body.
///
/// 404 means the other peer hasn't announced yet — caller retries with backoff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PunchPeerResponse {
    /// `ip:port` of the matched peer (the other side of the star).
    pub peer_reflected_addr: String,
    /// Both peers fire their UDP punch packet at this UTC instant. Atlas picks
    /// ~250ms in the future so both sides have time to receive this response.
    pub fire_time: DateTime<Utc>,
}

/// `POST /v1/rendezvous/punch/complete` body. Telemetry + cleanup signal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PunchCompleteRequest {
    pub publish_id: String,
    /// Did the WG handshake actually come up after the punch? Aggregated to
    /// tune the fire-time offset and surface NAT classes we don't handle.
    pub success: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announce_json_round_trip() {
        let a = PunchAnnounce {
            publish_id: "abc123".into(),
            my_role: PunchRole::Device,
        };
        let j = serde_json::to_string(&a).unwrap();
        let back: PunchAnnounce = serde_json::from_str(&j).unwrap();
        assert_eq!(a, back);
        // Snake-case enum so it matches the Swift / TS decoder convention.
        assert!(j.contains("\"device\""));
    }

    #[test]
    fn announce_rejects_legacy_reflected_addr_field() {
        // Ensure no one accidentally re-adds the client-asserted address
        // field. If this fails, someone added a `reflected_addr` field;
        // before re-introducing it, audit the atlas-side handler.
        let j = serde_json::to_string(&PunchAnnounce {
            publish_id: "x".into(),
            my_role: PunchRole::Box,
        })
        .unwrap();
        assert!(!j.contains("reflected_addr"), "legacy field crept back in: {j}");
    }

    #[test]
    fn role_serializes_box_lowercase() {
        let j = serde_json::to_string(&PunchRole::Box).unwrap();
        assert_eq!(j, "\"box\"");
    }

    #[test]
    fn peer_response_field_names_are_stable() {
        let r = PunchPeerResponse {
            peer_reflected_addr: "198.51.100.7:51820".into(),
            fire_time: chrono::DateTime::<Utc>::from_timestamp(1_780_000_000, 0).unwrap(),
        };
        let v = serde_json::to_value(&r).unwrap();
        assert!(v.get("peer_reflected_addr").is_some());
        assert!(v.get("fire_time").is_some());
    }
}
