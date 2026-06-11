//! Pairing assembly (app side) — turn a `pair_complete` into a [`PairingBundle`].
//!
//! The box-identity, peer-persistence, and reconcile primitives live in the
//! `virtues-wg` crate (re-exported as `super::{peers, reconcile, manager, ula}`);
//! this module is the app-side orchestration that composes them + the CA + the
//! rendezvous identity into the bundle handed to a device.
//!
//! Per the 1b split, the app **does not install the kernel peer** — it persists
//! the peer to the DB (`peers::store_peer`) and the `virtues-wireguard` daemon
//! reconciles `wg0` from there. So `assemble_bundle` has no privileged op.
//! `assemble_bundle` is `cfg(linux)` only because it reads the WG server key /
//! mints a PSK via the engine; validated on staging.

use anyhow::{anyhow, Result};
use base64::Engine;
use sqlx::PgPool;

use super::box_secrets;
use super::bundle::PairingBundle;

/// `box_secrets.key` for the per-box rendezvous identity (publish_id + K).
const RENDEZVOUS_SECRET_KEY: &str = "rendezvous_identity";

/// The box's rendezvous identity: an opaque publish capability + the key K that
/// decrypts the published endpoint. Per-box singleton, shared by all the box's
/// paired devices (they all resolve the same box).
#[derive(Debug, Clone)]
pub struct RendezvousIdentity {
    pub publish_id: String,
    /// Base64 of the 32-byte key K.
    pub key_b64: String,
}

/// Load the box's rendezvous identity, minting + sealing one on first call.
pub async fn ensure_rendezvous_identity(db: &PgPool) -> Result<RendezvousIdentity> {
    if let Some((key_b64, meta)) = box_secrets::get(db, RENDEZVOUS_SECRET_KEY).await? {
        let publish_id = meta
            .get("publish_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("rendezvous identity missing publish_id"))?
            .to_string();
        return Ok(RendezvousIdentity { publish_id, key_b64 });
    }
    let key = crate::virtues_api::rendezvous::generate_key();
    let key_b64 = base64::engine::general_purpose::STANDARD.encode(key);
    let publish_id = crate::virtues_api::rendezvous::generate_publish_id();
    let meta = serde_json::json!({ "publish_id": publish_id });
    box_secrets::put(db, RENDEZVOUS_SECRET_KEY, &key_b64, &meta).await?;
    Ok(RendezvousIdentity { publish_id, key_b64 })
}

/// The box's current WG endpoint (`host:port`) to bake into the bundle as the
/// initial dial target. Resolution order:
///   1. `VIRTUES_WG_ENDPOINT` env override — explicit operator pin.
///   2. The GLOBAL endpoint `virtues-wireguard` detected + wrote to
///      `box_secrets.wg_current_endpoint` — the doctrine's primary path: a
///      real, internet-routable IPv6 a remote device dials directly.
///   3. The box's current LOCAL source address (any non-loopback, incl. LAN) —
///      so a device pairing on the same network reaches the box immediately
///      even before the daemon has detected a global address. Remote
///      reachability then depends on (2) landing; the device re-resolves via
///      the rendezvous when the prefix rotates.
///
/// We NEVER bake a wildcard `[::]` — that's an undiallable placeholder. If the
/// box has no detectable address at all (no interfaces), we fall back to its
/// ULA and log loudly; pairing still succeeds so the device can retry later.
#[cfg(target_os = "linux")]
async fn current_endpoint(db: &PgPool) -> String {
    let port = super::manager::wg_listen_port();

    // 1. Operator override.
    if let Ok(s) = std::env::var("VIRTUES_WG_ENDPOINT") {
        if !s.is_empty() {
            return s;
        }
    }
    // 2. Daemon-detected global endpoint (the real internet-routable address).
    if let Ok(Some(ep)) = super::endpoint::read_current(db).await {
        return format!("{}:{}", bracket_host(&ep.ip), ep.port);
    }
    // 3. Current local address (LAN is acceptable for same-network pairing).
    if let Some(ip) = local_best_addr() {
        return format!("{}:{port}", bracket_host(&ip.to_string()));
    }
    // 4. No address at all — extreme edge. Bake the ULA (reachable once the
    //    tunnel is up) and warn; never a wildcard.
    tracing::warn!(
        "current_endpoint: box has no detectable address; baking ULA only — \
         this device won't reach the box until it has a real address"
    );
    format!("[{}]:{port}", super::ula::server_address())
}

/// Bracket an IPv6 literal so `parse::<SocketAddr>()` on the daemon accepts it.
#[cfg(target_os = "linux")]
fn bracket_host(ip: &str) -> String {
    if ip.contains(':') && !ip.starts_with('[') {
        format!("[{ip}]")
    } else {
        ip.to_string()
    }
}

/// Best-effort: the box's current outbound source address (any non-loopback).
/// Unlike the daemon's `detect_public_ip`, this accepts LAN addresses — it's
/// only the *initial* bundle target for same-network pairing, not the
/// published global endpoint.
#[cfg(target_os = "linux")]
fn local_best_addr() -> Option<std::net::IpAddr> {
    for (dest, bind) in [
        ("[2606:4700:4700::1111]:53", "[::]:0"),
        ("1.1.1.1:53", "0.0.0.0:0"),
    ] {
        if let Ok(sock) = std::net::UdpSocket::bind(bind) {
            if sock.connect(dest).is_ok() {
                if let Ok(local) = sock.local_addr() {
                    let ip = local.ip();
                    if !ip.is_loopback() && !ip.is_unspecified() {
                        return Some(ip);
                    }
                }
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn virtues_api_base() -> String {
    std::env::var("VIRTUES_API_URL").unwrap_or_else(|_| "http://localhost:9002".into())
}

/// Assemble the full pairing bundle: mint/load the box identity, allocate this
/// device's address, **persist** it as a peer (the daemon installs it), and
/// return everything the device needs. The device generated its own WG keypair
/// and supplies only its public key (`device_wg_pubkey`, base64).
#[cfg(target_os = "linux")]
pub async fn assemble_bundle(
    db: &PgPool,
    credential_id: &str,
    bearer: &str,
    device_wg_pubkey: &str,
) -> Result<PairingBundle> {
    use super::bundle::{RendezvousParams, WgParams};
    use super::{manager, peers, reconcile, ula, INTERNAL_HOST, INTERNAL_PORT};
    use std::net::Ipv6Addr;

    let server_kp = reconcile::ensure_server_keypair(db).await?;
    let rdv = ensure_rendezvous_identity(db).await?;

    // Allocate the next free ULA /128, skipping addresses already handed out.
    let existing: Vec<Ipv6Addr> = peers::load_all_peers(db)
        .await?
        .iter()
        .filter_map(|p| p.client_address.parse::<Ipv6Addr>().ok())
        .collect();
    let client_addr = ula::allocate(&existing).ok_or_else(|| anyhow!("ULA pool exhausted"))?;
    let server_addr = ula::server_address();
    let psk = manager::generate_psk();

    // Persist the peer; the virtues-wireguard daemon reconciles wg0 from the DB.
    // (No in-process kernel install here — the app stays unprivileged.)
    let peer = peers::PeerRecord {
        device_public_key: device_wg_pubkey.to_string(),
        preshared_key: psk.clone(),
        client_address: client_addr.to_string(),
    };
    peers::store_peer(db, credential_id, &peer).await?;

    let rdv_url = format!("{}/v1/rendezvous/{}", virtues_api_base(), rdv.publish_id);
    Ok(PairingBundle {
        bearer: bearer.to_string(),
        wg: WgParams {
            server_public_key: server_kp.public_key,
            server_endpoint: current_endpoint(db).await,
            preshared_key: psk,
            client_address: client_addr.to_string(),
            server_address: server_addr.to_string(),
            allowed_ips: vec![format!("{server_addr}/128")],
        },
        internal_host: INTERNAL_HOST.to_string(),
        internal_ip: server_addr.to_string(),
        http_port: INTERNAL_PORT,
        rendezvous: RendezvousParams {
            publish_id: rdv.publish_id,
            key: rdv.key_b64,
            url: rdv_url,
        },
    })
}

/// Non-Linux stub: WG pairing only runs on the Linux appliance.
#[cfg(not(target_os = "linux"))]
pub async fn assemble_bundle(
    _db: &PgPool,
    _credential_id: &str,
    _bearer: &str,
    _device_wg_pubkey: &str,
) -> Result<PairingBundle> {
    anyhow::bail!("WireGuard pairing is only supported on the Linux appliance")
}
