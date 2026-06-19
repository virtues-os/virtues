//! Pairing assembly (app side) — turn a `pair_complete` into a [`PairingBundle`].
//!
//! The box-identity, peer-persistence, and reconcile primitives live in the
//! `virtues-wg` crate (re-exported as `super::{peers, reconcile, manager, ula}`);
//! this module is the app-side orchestration that composes them into the bundle
//! handed to a device.
//!
//! Per the 1b split, the app **does not install the kernel peer** — it persists
//! the peer to the DB (`peers::store_peer`) and the `virtues-wireguard` daemon
//! reconciles `wg0` from there. So `assemble_bundle` has no privileged op.
//! `assemble_bundle` is `cfg(linux)` only because it reads the WG server key /
//! mints a PSK via the engine; validated on staging.

use anyhow::Result;
#[cfg(target_os = "linux")]
use anyhow::anyhow;
use sqlx::PgPool;

use super::bundle::PairingBundle;

/// The box's current WG endpoint (`host:port`) to bake into the bundle as the
/// dial target. Resolution order:
///   1. `VIRTUES_WG_ENDPOINT` env override — explicit operator pin.
///   2. The GLOBAL endpoint `virtues-wireguard` detected + wrote to
///      `box_secrets.wg_current_endpoint` — the doctrine's primary path: a
///      real, internet-routable IPv6 a remote device dials directly.
///   3. The box's current LOCAL source address (any non-loopback, incl. LAN) —
///      so a device pairing on the same network reaches the box immediately
///      even before the daemon has detected a global address.
///
/// The endpoint is static once baked: a prefix rotation requires re-pairing
/// (v1 has no rendezvous re-resolution).
///
/// We NEVER bake a wildcard `[::]` — that's an undiallable placeholder. If the
/// box has no detectable address at all (no interfaces), we fall back to its
/// ULA and log loudly; pairing still succeeds so the device can retry later.
/// The full ordered candidate list (`host:port` each) to bake into the bundle.
/// The device tries these and locks onto whichever completes the WG handshake,
/// so it reaches the box by any working path. Best-first:
///   1. `VIRTUES_WG_ENDPOINT` override — most authoritative.
///   2. The daemon-detected GLOBAL endpoint — the off-network path.
///   3. The box's current LAN source address(es), v6 and v4 — the same-network
///      fast path and the fallback when global v6 hairpin is flaky on-LAN.
/// Deduped, order preserved. May be empty only if the box has no detectable
/// address at all (extreme edge); callers fall back to the ULA.
#[cfg(target_os = "linux")]
async fn current_endpoints(db: &PgPool) -> Vec<String> {
    let port = super::manager::wg_listen_port();
    let mut out: Vec<String> = Vec::new();
    let mut push = |ep: String| {
        if !ep.is_empty() && !out.contains(&ep) {
            out.push(ep);
        }
    };

    // 1. Operator override.
    if let Ok(s) = std::env::var("VIRTUES_WG_ENDPOINT") {
        push(s);
    }
    // 2. Daemon-detected global endpoint (the real internet-routable address).
    if let Ok(Some(ep)) = super::endpoint::read_current(db).await {
        push(format!("{}:{}", bracket_host(&ep.ip), ep.port));
    }
    // 3. Current local addresses (LAN is acceptable for same-network pairing).
    for ip in local_best_addrs() {
        push(format!("{}:{port}", bracket_host(&ip.to_string())));
    }
    out
}

/// The single primary endpoint — the first candidate, or the ULA as a
/// last-resort placeholder (reachable once the tunnel is up; never a wildcard).
/// Kept as the bundle's back-compat `server_endpoint` for older decoders.
#[cfg(target_os = "linux")]
async fn current_endpoint(db: &PgPool) -> String {
    if let Some(first) = current_endpoints(db).await.into_iter().next() {
        return first;
    }
    let port = super::manager::wg_listen_port();
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

/// Best-effort: the box's current outbound source addresses (any non-loopback),
/// **both** IPv6 and IPv4 when available. Unlike the daemon's `detect_public_ip`,
/// this accepts LAN addresses — they're same-network candidates, not the
/// published global endpoint. Returning both families gives a device a LAN v4
/// path when on-LAN v6 hairpin is flaky (and vice-versa).
#[cfg(target_os = "linux")]
fn local_best_addrs() -> Vec<std::net::IpAddr> {
    let mut addrs: Vec<std::net::IpAddr> = Vec::new();
    for (dest, bind) in [
        ("[2606:4700:4700::1111]:53", "[::]:0"),
        ("1.1.1.1:53", "0.0.0.0:0"),
    ] {
        if let Ok(sock) = std::net::UdpSocket::bind(bind) {
            if sock.connect(dest).is_ok() {
                if let Ok(local) = sock.local_addr() {
                    let ip = local.ip();
                    if !ip.is_loopback() && !ip.is_unspecified() && !addrs.contains(&ip) {
                        addrs.push(ip);
                    }
                }
            }
        }
    }
    addrs
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
    use super::bundle::WgParams;
    use super::{manager, peers, reconcile, ula, INTERNAL_HOST, INTERNAL_PORT};
    use std::net::Ipv6Addr;

    let server_kp = reconcile::ensure_server_keypair(db).await?;

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

    // Bake every reachable address so the device can lock onto any working path;
    // keep `server_endpoint` = the primary for back-compat with older decoders.
    let endpoints = current_endpoints(db).await;
    let primary = match endpoints.first() {
        Some(first) => first.clone(),
        None => current_endpoint(db).await, // ULA last-resort + warn
    };

    Ok(PairingBundle {
        bearer: bearer.to_string(),
        wg: WgParams {
            server_public_key: server_kp.public_key,
            server_endpoint: primary,
            server_endpoints: endpoints,
            preshared_key: psk,
            client_address: client_addr.to_string(),
            server_address: server_addr.to_string(),
            allowed_ips: vec![format!("{server_addr}/128")],
        },
        internal_host: INTERNAL_HOST.to_string(),
        internal_ip: server_addr.to_string(),
        http_port: INTERNAL_PORT,
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
