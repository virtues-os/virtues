//! Device WG peer config, persisted in credential metadata.
//!
//! The DB is the app↔daemon interface: the app (`store_peer`) writes a peer when
//! a device pairs; the daemon (`load_all_peers` → reconcile) makes `wg0` match.
//! Cross-platform (DB + serde only).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// A device's WireGuard peer config, stored under `credentials.metadata.wg` so
/// `wg0` can be rebuilt from the durable store on boot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRecord {
    /// Device WG public key, base64.
    pub device_public_key: String,
    /// Per-pair pre-shared key, base64.
    pub preshared_key: String,
    /// Address assigned to the device in the box's ULA space.
    pub client_address: String,
}

/// Persist a device's WG peer config into its credential row (`metadata.wg`).
pub async fn store_peer(db: &PgPool, credential_id: &str, peer: &PeerRecord) -> Result<()> {
    let wg = serde_json::to_value(peer)?;
    sqlx::query(
        "UPDATE credentials
            SET metadata = jsonb_set(COALESCE(metadata, '{}'::jsonb), '{wg}', $1, true),
                updated_at = now()
          WHERE id = $2",
    )
    .bind(&wg)
    .bind(credential_id)
    .execute(db)
    .await
    .context("store wg peer")?;
    Ok(())
}

/// Load every active device's WG peer config — the source of truth the daemon
/// reconciles `wg0` against.
pub async fn load_all_peers(db: &PgPool) -> Result<Vec<PeerRecord>> {
    // `expires_at` is the provision claim deadline (NULL for every normal
    // credential). Exclude a provisioned-but-never-claimed credential whose
    // deadline lapsed so its peer is reconciled out of `wg0` — it can no longer
    // authenticate anyway (see `credentials::validate_device_token`).
    let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
        "SELECT metadata->'wg' FROM credentials
          WHERE (metadata->'wg') IS NOT NULL AND status = 'active'
            AND (expires_at IS NULL OR expires_at > now())",
    )
    .fetch_all(db)
    .await
    .context("load wg peers")?;
    Ok(rows
        .into_iter()
        .filter_map(|(v,)| serde_json::from_value::<PeerRecord>(v).ok())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_record_round_trip() {
        let p = PeerRecord {
            device_public_key: "ZGV2cHVi".into(),
            preshared_key: "cHNr".into(),
            client_address: "fd00:5654::2".into(),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["device_public_key"], "ZGV2cHVi");
        let back: PeerRecord = serde_json::from_value(v).unwrap();
        assert_eq!(p, back);
    }
}
