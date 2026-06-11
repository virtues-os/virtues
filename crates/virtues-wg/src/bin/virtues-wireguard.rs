//! `virtues-wireguard` — the minimal, privileged WireGuard daemon.
//!
//! Runs rootful (`NET_ADMIN` + `/dev/net/tun` + host networking) as its own
//! Quadlet/systemd unit so the main app stays rootless. It does three things and
//! nothing else — no web, no HTTP client, no bearer, no internet egress:
//!
//!   1. **Reconcile** `wg0` to the durable peer set (`reconcile::rebuild_interface`).
//!   2. **Detect** the box's current public endpoint and record it in the DB
//!      (`endpoint::write_current`) for the app to publish to the rendezvous.
//!   3. Repeat on a tick (later: netlink-event-driven + `LISTEN/NOTIFY`).
//!
//! Env: `DATABASE_URL` (the box's local Postgres) + `VIRTUES_ENCRYPTION_KEY`
//! (to unseal the WG server key from `box_secrets`).

#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use std::time::Duration;
    use virtues_wg::{endpoint, manager, reconcile};

    let db_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))?;
    let db = sqlx::PgPool::connect(&db_url).await?;
    eprintln!("[virtues-wireguard] started; reconciling wg0 from the peer store");

    // Poll loop. The reconcile + endpoint-record are idempotent, so a steady tick
    // is safe; a netlink RTM_NEWADDR watch + Postgres LISTEN/NOTIFY replace the
    // poll later (staging) for prompt reaction to prefix rotation / new pairings.
    loop {
        if let Err(e) = reconcile::rebuild_interface(&db).await {
            eprintln!("[virtues-wireguard] reconcile failed: {e:#}");
        }

        match detect_public_ip() {
            Some(ip) => match reconcile::ensure_server_keypair(&db).await {
                Ok(kp) => {
                    let ep = endpoint::Endpoint {
                        ip,
                        port: manager::WG_LISTEN_PORT,
                        wg_pub: kp.public_key,
                    };
                    if let Err(e) = endpoint::write_current(&db, &ep).await {
                        eprintln!("[virtues-wireguard] endpoint record failed: {e:#}");
                    }
                }
                Err(e) => eprintln!("[virtues-wireguard] server key load failed: {e:#}"),
            },
            None => { /* no public IP yet; try again next tick */ }
        }

        tokio::time::sleep(Duration::from_secs(15)).await;
    }
}

/// Detect the box's current routable IP — what a peer dialing the box would
/// see as its source-of-truth destination.
///
/// Resolution order:
///   1. `VIRTUES_WG_PUBLIC_IP` env override — explicit, takes priority. Used
///      when the box sits behind a router with port-forwarding and the
///      operator knows the WAN address out-of-band.
///   2. The "outbound socket trick": open a UDP socket, `connect()` it to a
///      public address (no packets sent, just kernel route lookup), read back
///      the local address the kernel picked. That's the IP on whichever
///      interface owns the default route — i.e. the LAN-routable IP that a
///      peer on the same LAN would dial. For LAN-only E2E this is exactly
///      right; cross-NAT is the punch coordinator's job.
///
/// Returns `None` only when both fail (no default route, no interfaces). In
/// that case the rendezvous won't be updated this tick and we retry.
#[cfg(target_os = "linux")]
fn detect_public_ip() -> Option<String> {
    if let Ok(s) = std::env::var("VIRTUES_WG_PUBLIC_IP") {
        if !s.is_empty() {
            return Some(s);
        }
    }

    // Two probes — once for v4, once for v6. The kernel chooses the source
    // address based on the destination's family + the default route. We
    // prefer v6 if present (matches our ULA-by-default design) but accept
    // either; whichever resolves is what the box has working egress on.
    if let Some(ip) = probe_outbound_addr("[2606:4700:4700::1111]:53", "[::]:0") {
        return Some(ip);
    }
    probe_outbound_addr("1.1.1.1:53", "0.0.0.0:0")
}

#[cfg(target_os = "linux")]
fn probe_outbound_addr(dest: &str, bind: &str) -> Option<String> {
    let sock = std::net::UdpSocket::bind(bind).ok()?;
    sock.connect(dest).ok()?;
    let local = sock.local_addr().ok()?;
    let ip = local.ip();
    // Loopback / unspecified mean the kernel didn't pick a real address —
    // treat as "no answer" so we retry next tick rather than publishing junk.
    if ip.is_loopback() || ip.is_unspecified() {
        return None;
    }
    Some(ip.to_string())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("virtues-wireguard runs only on Linux (the appliance).");
    std::process::exit(1);
}
