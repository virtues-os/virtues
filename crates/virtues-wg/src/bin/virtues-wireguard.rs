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

/// Detect the box's current public IP. **Placeholder** — reads an env override
/// the box's network config can set. The real implementation is a netlink
/// `RTM_NEWADDR`/`RTM_DELADDR` watch (or `getifaddrs` poll) for the global IPv6
/// on the WAN interface; written + validated on staging where real netlink runs.
#[cfg(target_os = "linux")]
fn detect_public_ip() -> Option<String> {
    std::env::var("VIRTUES_WG_PUBLIC_IP").ok().filter(|s| !s.is_empty())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("virtues-wireguard runs only on Linux (the appliance).");
    std::process::exit(1);
}
