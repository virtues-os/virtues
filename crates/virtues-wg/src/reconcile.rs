//! Box WG identity + interface reconcile — the daemon's core. **Linux-only.**
//!
//! `ensure_server_keypair` mints/loads the box's singleton WG keypair (sealed in
//! `box_secrets`). `rebuild_interface` makes `wg0` match the durable peer set —
//! it's both the boot path and the daemon's reconcile primitive (the interface
//! is the ephemeral projection of the DB).

use anyhow::{anyhow, Result};
use sqlx::PgPool;
use std::net::IpAddr;

use crate::{box_secrets, manager, peers, ula};

/// `box_secrets.key` for the box's own WG keypair.
const SERVER_KEYPAIR_SECRET_KEY: &str = "wg_server_keypair";

/// Load the box's WG keypair, minting + sealing one on first call (singleton).
///
/// Race-safe: the app (bundle) and the daemon (reconcile) both call this, so a
/// naive get-then-put could double-mint and disagree (app advertises pubkey A,
/// daemon brings up wg0 with B). Instead we mint, **insert-if-absent**, then
/// **re-read** — the first writer wins and both converge on the same keypair.
pub async fn ensure_server_keypair(db: &PgPool) -> Result<manager::KeyPair> {
    if let Some(kp) = load_server_keypair(db).await? {
        return Ok(kp);
    }
    // Mint a candidate and try to claim the slot; another process may win.
    let candidate = manager::generate_keypair();
    let meta = serde_json::json!({ "public_key": candidate.public_key });
    box_secrets::put_if_absent(db, SERVER_KEYPAIR_SECRET_KEY, &candidate.private_key, &meta).await?;
    // Re-read: returns whoever actually landed in the row (us or the racer).
    load_server_keypair(db)
        .await?
        .ok_or_else(|| anyhow!("wg server keypair missing after insert"))
}

async fn load_server_keypair(db: &PgPool) -> Result<Option<manager::KeyPair>> {
    let Some((private_key, meta)) = box_secrets::get(db, SERVER_KEYPAIR_SECRET_KEY).await? else {
        return Ok(None);
    };
    let public_key = meta
        .get("public_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("wg server keypair missing public_key"))?
        .to_string();
    Ok(Some(manager::KeyPair {
        private_key,
        public_key,
    }))
}

/// Reconcile `wg0` to the durable peer store: bring the interface up with the
/// box's keypair and the current active peers. Idempotent — safe to call at boot
/// and on every change (a new pairing, a revoke).
pub async fn rebuild_interface(db: &PgPool) -> Result<()> {
    let server_kp = ensure_server_keypair(db).await?;
    let peers = peers::load_all_peers(db).await?;
    let configs: Vec<manager::PeerConfig> = peers
        .iter()
        .filter_map(|p| {
            p.client_address
                .parse::<IpAddr>()
                .ok()
                .map(|ip| manager::PeerConfig {
                    public_key: p.device_public_key.clone(),
                    preshared_key: p.preshared_key.clone(),
                    allowed_ip: ip,
                })
        })
        .collect();
    manager::bring_up(
        &server_kp.private_key,
        IpAddr::V6(ula::server_address()),
        &configs,
    )
}
