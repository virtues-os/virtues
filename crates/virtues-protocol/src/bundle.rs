//! The pairing bundle — everything a device needs to reach the box, provisioned
//! in a single exchange at `/api/pair/consume`.
//!
//! The box assembles this and the device (iOS / desktop daemon / web client)
//! consumes it. Serialized to JSON; every client implementation mirrors this
//! shape exactly — the Swift `PairingBundle` in `apps/ios`, the Rust client in
//! `apps/client`, and any future Android / ESP32 firmware all decode against
//! these field names.
//!
//! The QR / pair URL itself stays tiny (a one-time pair token + box endpoint);
//! this full bundle is pulled over the initial connection. See
//! `docs/wireguard-pairing.md` §4.

use serde::{Deserialize, Serialize};

/// WireGuard parameters for this pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WgParams {
    /// Box (server) WG public key, base64. The device installs the box as its
    /// single peer.
    pub server_public_key: String,
    /// Box's current public endpoint, `host:port` (IPv6 in brackets). The device
    /// dials this first; on handshake failure it re-resolves via the rendezvous.
    pub server_endpoint: String,
    /// Per-pair pre-shared key, base64 (defense-in-depth on top of Noise IK).
    pub preshared_key: String,
    /// Address assigned to this device inside the box's ULA space (its `/128`).
    pub client_address: String,
    /// Box's WG address — the tunnel peer the device talks to, and what
    /// `virtues.internal` resolves to (client-side only).
    pub server_address: String,
    /// AllowedIPs the device routes through the tunnel. Split-tunnel: only the
    /// box's address, so nothing else leaves via the tunnel.
    pub allowed_ips: Vec<String>,
}

/// What the device needs to relearn the box's endpoint after an ISP prefix
/// rotation (the blind rendezvous).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RendezvousParams {
    /// Opaque capability to GET the box's current (encrypted) endpoint.
    pub publish_id: String,
    /// Per-box key K, base64 — decrypts the rendezvous blob. Never leaves the
    /// box + its paired devices.
    pub key: String,
    /// Full GET URL for the rendezvous, e.g.
    /// `https://api.virtues.com/v1/rendezvous/<publish_id>`.
    pub url: String,
}

/// The complete provisioning bundle handed to a device at pairing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingBundle {
    /// The device's server-issued bearer (local auth to the box). Stored in the
    /// device keychain; also returned by `pair_complete` for convenience.
    pub bearer: String,
    pub wg: WgParams,
    /// The internal hostname dialed inside the tunnel (never in public DNS).
    /// The daemon sets this as the Host header when proxying browser requests.
    pub internal_host: String,
    /// What `internal_host` resolves to (client-side only) — equals
    /// `wg.server_address`.
    pub internal_ip: String,
    /// HTTP port the device dials at `internal_host` inside the tunnel. The
    /// tunnel itself provides encryption + authentication (Noise IK = SPKI
    /// pinning); the box runs no TLS surface.
    pub http_port: u16,
    pub rendezvous: RendezvousParams,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PairingBundle {
        PairingBundle {
            bearer: "ZGV2aWNlLWJlYXJlcg".into(),
            wg: WgParams {
                server_public_key: "c2VydmVycHVia2V5".into(),
                server_endpoint: "[2001:db8::1]:51820".into(),
                preshared_key: "cHNr".into(),
                client_address: "fd00:5654::2".into(),
                server_address: "fd00:5654::1".into(),
                allowed_ips: vec!["fd00:5654::1/128".into()],
            },
            internal_host: "virtues.internal".into(),
            internal_ip: "fd00:5654::1".into(),
            http_port: 8000,
            rendezvous: RendezvousParams {
                publish_id: "abc123".into(),
                key: "a2V5".into(),
                url: "https://api.example/v1/rendezvous/abc123".into(),
            },
        }
    }

    #[test]
    fn json_round_trip() {
        let b = sample();
        let json = serde_json::to_string(&b).unwrap();
        let back: PairingBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn field_names_are_snake_case_stable() {
        // Swift, TypeScript, and ESP32-C decoders all key on these exact names —
        // any rename here is a wire-protocol break across every client.
        let json = serde_json::to_value(sample()).unwrap();
        assert!(json.get("bearer").is_some());
        assert!(json["wg"].get("server_public_key").is_some());
        assert!(json["wg"].get("client_address").is_some());
        assert!(json.get("http_port").is_some());
        assert!(json["rendezvous"].get("publish_id").is_some());
        assert!(json["rendezvous"].get("key").is_some());
    }
}
