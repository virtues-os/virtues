//! Relay reachability config, provisioned by atlas.
//!
//! At claim/link atlas returns `{ relay_url }` (the iroh relay this box homes
//! on). The box persists it in `box_secrets` and the iroh reach subsystem
//! ([`crate::relay`]) reads it to build its endpoint's relay map. `relay_url` is
//! public (not a secret); it's stored in the secret slot purely for reuse of the
//! `box_secrets` key/value store.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use sqlx::PgPool;

/// `box_secrets` key for the provisioned relay config.
pub const BOX_SECRET_KEY: &str = "relay_config";

/// Relay reachability config for this box.
#[derive(Debug, Clone)]
pub struct RelayConfig {
    /// The iroh relay URL this box homes on, e.g. `https://relay.virtues.ch`.
    pub relay_url: String,
}

#[derive(Deserialize)]
struct RelayConfigResponse {
    relay_url: String,
}

/// Fetch this box's relay config from atlas (authenticated by `api_key`) and
/// persist it. Best-effort: the caller should log and continue on error — the
/// box still works on LAN (and via `VIRTUES_RELAY_URL`) without this.
pub async fn fetch_and_store(
    db: &PgPool,
    http: &reqwest::Client,
    atlas_url: &str,
    api_key: &str,
) -> Result<()> {
    let resp = http
        .post(format!("{}/relay/config", atlas_url.trim_end_matches('/')))
        .json(&serde_json::json!({ "api_key": api_key }))
        .send()
        .await
        .context("atlas /relay/config request")?;
    let status = resp.status();
    if !status.is_success() {
        // 503 = relay not enabled on this deployment (expected on LAN-only/dev).
        return Err(anyhow!("atlas /relay/config returned {status}"));
    }
    let body: RelayConfigResponse = resp.json().await.context("parse relay config")?;
    store(db, &RelayConfig { relay_url: body.relay_url }).await
}

/// Persist the relay config.
pub async fn store(db: &PgPool, cfg: &RelayConfig) -> Result<()> {
    crate::box_secrets::put(db, BOX_SECRET_KEY, &cfg.relay_url, &serde_json::json!({})).await
}

/// Load the provisioned relay config from `box_secrets`, if present.
pub async fn load(db: &PgPool) -> Result<Option<RelayConfig>> {
    let Some((relay_url, _metadata)) = crate::box_secrets::get(db, BOX_SECRET_KEY).await? else {
        return Ok(None);
    };
    if relay_url.is_empty() {
        return Ok(None);
    }
    Ok(Some(RelayConfig { relay_url }))
}
