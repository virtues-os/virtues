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

/// Bundle-format tolerance contract — READ BEFORE adding/removing a field.
///
/// A field-removal once shipped a hard break: an older client binary required
/// `rendezvous`, the box stopped emitting it, and every paired device failed to
/// decode the bundle (`missing field rendezvous`) — bricking the tunnel until
/// the client was rebuilt. To make that class of skew impossible:
///
///   * ADDING a field is always safe — serde ignores unknown keys (no
///     `deny_unknown_fields` here, deliberately).
///   * REMOVING / churning an *auxiliary* field must never break an older
///     decoder, so every routing/derivable field carries `#[serde(default)]`.
///     A box that drops one just yields the default; the client degrades to a
///     runtime fallback instead of a parse crash.
///   * Only the *core crypto* fields a tunnel cannot function without
///     (`server_public_key`, `preshared_key`, `client_address`,
///     `server_address`, `bearer`) stay required — their absence SHOULD fail
///     loudly, and they're never removed (they're the WG handshake itself).
///
/// Net: a one-version skew between box and client can only ever degrade
/// reachability, never hard-fail decoding.
fn default_http_port() -> u16 {
    8000
}

fn default_internal_host() -> String {
    "virtues.internal".to_string()
}

/// WireGuard parameters for this pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WgParams {
    /// Box (server) WG public key, base64. The device installs the box as its
    /// single peer. CORE: required — no tunnel without it.
    pub server_public_key: String,
    /// Box's current public endpoint, `host:port` (IPv6 in brackets). Baked at
    /// pairing time from the box's detected global address; the device dials it
    /// directly. (A prefix rotation requires re-pairing — no auto re-resolve.)
    ///
    /// This is the *primary* candidate and the back-compat field: older decoders
    /// that predate `server_endpoints` still find a working address here. New
    /// clients prefer `server_endpoints` and treat this as the first fallback.
    /// AUXILIARY: defaults empty — `server_endpoints` is the modern source and
    /// the tunnel skips an empty/unparseable primary ([wg.rs] candidate build).
    #[serde(default)]
    pub server_endpoint: String,
    /// Ordered candidate endpoints (`host:port` each) the device tries, locking
    /// onto whichever completes the WG handshake — the box's LAN address(es) plus
    /// its global IPv6, best-first. Lets a device reach the box by *any* working
    /// path (same-Wi-Fi LAN or off-network global) instead of a single baked
    /// address with no fallback. Additive + back-compatible: old bundles omit it
    /// (defaults empty), in which case the tunnel falls back to `server_endpoint`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub server_endpoints: Vec<String>,
    /// Per-pair pre-shared key, base64 (defense-in-depth on top of Noise IK).
    /// CORE: required.
    pub preshared_key: String,
    /// Address assigned to this device inside the box's ULA space (its `/128`).
    /// CORE: required.
    pub client_address: String,
    /// Box's WG address — the tunnel peer the device talks to, and what
    /// `virtues.internal` resolves to (client-side only). CORE: required.
    pub server_address: String,
    /// AllowedIPs the device routes through the tunnel. Split-tunnel: only the
    /// box's address, so nothing else leaves via the tunnel.
    /// AUXILIARY: defaults empty.
    #[serde(default)]
    pub allowed_ips: Vec<String>,
    /// The device's WG private key, base64 — present ONLY when the box generated
    /// the keypair on the device's behalf (the desktop-relayed `/api/pair/provision`
    /// path, where the new device never speaks to the box directly). For the normal
    /// `/api/pair/consume` path the device generates its own keypair and supplies
    /// only the public key, so this stays `None` and is omitted from the wire.
    /// AUXILIARY: defaults absent. Carries a secret — see the relay flow's on-screen
    /// QR caveats in `docs/wireguard-pairing.md`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_private_key: Option<String>,
}

/// The complete provisioning bundle handed to a device at pairing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingBundle {
    /// The device's server-issued bearer (local auth to the box). Stored in the
    /// device keychain; also returned by `pair_complete` for convenience.
    /// CORE: required — local auth to the box.
    pub bearer: String,
    pub wg: WgParams,
    /// The internal hostname dialed inside the tunnel (never in public DNS).
    /// The daemon sets this as the Host header when proxying browser requests.
    /// AUXILIARY: defaults to `virtues.internal`.
    #[serde(default = "default_internal_host")]
    pub internal_host: String,
    /// What `internal_host` resolves to (client-side only) — equals
    /// `wg.server_address`. AUXILIARY: defaults empty (derivable from
    /// `wg.server_address`).
    #[serde(default)]
    pub internal_ip: String,
    /// HTTP port the device dials at `internal_host` inside the tunnel. The
    /// tunnel itself provides encryption + authentication (Noise IK = SPKI
    /// pinning); the box runs no TLS surface. AUXILIARY: defaults to 8000.
    #[serde(default = "default_http_port")]
    pub http_port: u16,
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
                server_endpoints: vec![
                    "[2001:db8::1]:51820".into(),
                    "192.168.1.50:51820".into(),
                ],
                preshared_key: "cHNr".into(),
                client_address: "fd00:5654::2".into(),
                server_address: "fd00:5654::1".into(),
                allowed_ips: vec!["fd00:5654::1/128".into()],
                client_private_key: None,
            },
            internal_host: "virtues.internal".into(),
            internal_ip: "fd00:5654::1".into(),
            http_port: 8000,
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
        assert!(json["wg"].get("server_endpoint").is_some());
        assert!(json["wg"].get("server_endpoints").is_some());
        assert!(json.get("http_port").is_some());
    }

    #[test]
    fn decodes_bundle_without_server_endpoints() {
        // Back-compat guarantee: a bundle minted by a box that predates the
        // multi-endpoint field (no `server_endpoints` key) must still decode,
        // with the field defaulting to empty so the tunnel falls back to the
        // single `server_endpoint`.
        let json = r#"{
            "bearer": "b",
            "wg": {
                "server_public_key": "k",
                "server_endpoint": "[2001:db8::1]:51820",
                "preshared_key": "p",
                "client_address": "fd00:5654::2",
                "server_address": "fd00:5654::1",
                "allowed_ips": ["fd00:5654::1/128"]
            },
            "internal_host": "virtues.internal",
            "internal_ip": "fd00:5654::1",
            "http_port": 8000
        }"#;
        let b: PairingBundle = serde_json::from_str(json).unwrap();
        assert!(b.wg.server_endpoints.is_empty());
        assert_eq!(b.wg.server_endpoint, "[2001:db8::1]:51820");
    }

    #[test]
    fn client_private_key_round_trips_and_is_omitted_when_absent() {
        // Box-generated (relayed) pairings carry the device's private key in the
        // bundle; normal device-generated pairings don't. Both must round-trip,
        // and the absent case must stay off the wire (`skip_serializing_if`) so
        // the common path's bundle shape is unchanged.
        let mut b = sample();
        assert!(b.wg.client_private_key.is_none());
        let json = serde_json::to_value(&b).unwrap();
        assert!(json["wg"].get("client_private_key").is_none());

        b.wg.client_private_key = Some("ZGV2aWNlLXByaXZrZXk".into());
        let json = serde_json::to_string(&b).unwrap();
        let back: PairingBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
        assert_eq!(back.wg.client_private_key.as_deref(), Some("ZGV2aWNlLXByaXZrZXk"));
    }

    #[test]
    fn empty_server_endpoints_omitted_from_json() {
        // `skip_serializing_if` keeps the wire shape unchanged when the box
        // hasn't populated the list (older box, or single-address case).
        let mut b = sample();
        b.wg.server_endpoints.clear();
        let json = serde_json::to_value(&b).unwrap();
        assert!(json["wg"].get("server_endpoints").is_none());
    }

    #[test]
    fn decodes_bundle_with_only_core_fields() {
        // The rendezvous-class regression guard. A bundle carrying ONLY the core
        // crypto fields — every auxiliary/routing field absent — must still
        // decode. This is what protects an older client from a future box that
        // drops or renames an auxiliary field (the way `rendezvous` was dropped):
        // the parser yields defaults instead of `missing field`, and the tunnel
        // degrades to a runtime fallback rather than failing to come up at all.
        let json = r#"{
            "bearer": "b",
            "wg": {
                "server_public_key": "k",
                "preshared_key": "p",
                "client_address": "fd00:5654::2",
                "server_address": "fd00:5654::1"
            }
        }"#;
        let b: PairingBundle = serde_json::from_str(json).expect("core-only bundle must decode");
        // Auxiliary fields fell back to their defaults.
        assert!(b.wg.server_endpoint.is_empty());
        assert!(b.wg.server_endpoints.is_empty());
        assert!(b.wg.allowed_ips.is_empty());
        assert!(b.internal_ip.is_empty());
        assert_eq!(b.internal_host, "virtues.internal");
        assert_eq!(b.http_port, 8000);
        // Core fields decoded as given.
        assert_eq!(b.wg.server_public_key, "k");
        assert_eq!(b.wg.server_address, "fd00:5654::1");
    }

    #[test]
    fn missing_core_field_still_fails_loudly() {
        // The other half of the contract: dropping a CORE field is NOT silently
        // tolerated — a keyless bundle can't establish a tunnel, so failing to
        // decode is the correct, loud outcome (not a default-to-empty footgun).
        let json = r#"{
            "bearer": "b",
            "wg": {
                "preshared_key": "p",
                "client_address": "fd00:5654::2",
                "server_address": "fd00:5654::1"
            }
        }"#;
        let parsed: Result<PairingBundle, _> = serde_json::from_str(json);
        assert!(parsed.is_err(), "missing server_public_key must fail to decode");
    }
}
