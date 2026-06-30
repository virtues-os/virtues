//! Relay reachability config, provisioned by atlas (Option A control plane).
//!
//! The box does **not** hold the relay master secret. At link, atlas mints this
//! box's per-SNI registration token (`derive_token(RELAY_SECRET, sni, bucket)`) and
//! returns `{relay_addr, sni, token}`; the box persists it in `box_secrets` and
//! the relay subsystem ([`crate::relay`]) reads it at startup. The token is
//! sealed (it's a bearer); `relay_addr`/`sni` are public, stored in metadata.
//! See `docs/relay-control-plane.md`.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use sqlx::PgPool;

/// `box_secrets` key for the provisioned relay config.
pub const BOX_SECRET_KEY: &str = "relay_config";

/// Relay reachability config for this box.
#[derive(Debug, Clone)]
pub struct RelayConfig {
    /// Relay control address the box dials out to (host:port).
    pub relay_addr: String,
    /// This box's SNI, e.g. `abc123.virtues.ch`.
    pub sni: String,
    /// Per-SNI registration token (HMAC, minted by atlas).
    pub token: String,
}

#[derive(Deserialize)]
struct RelayConfigResponse {
    relay_addr: String,
    sni: String,
    token: String,
}

/// Fetch this box's relay config from atlas (authenticated by `api_key`) and
/// persist it in `box_secrets`. Best-effort: the caller should log and continue
/// on error — the box still works on LAN without relay reach.
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
    store(
        db,
        &RelayConfig {
            relay_addr: body.relay_addr,
            sni: body.sni,
            token: body.token,
        },
    )
    .await
}

/// Persist relay config: token sealed as the secret, addr+sni in public metadata.
pub async fn store(db: &PgPool, cfg: &RelayConfig) -> Result<()> {
    let metadata = serde_json::json!({ "relay_addr": cfg.relay_addr, "sni": cfg.sni });
    crate::box_secrets::put(db, BOX_SECRET_KEY, &cfg.token, &metadata).await
}

/// Load the provisioned relay config from `box_secrets`, if complete.
pub async fn load(db: &PgPool) -> Result<Option<RelayConfig>> {
    let Some((token, metadata)) = crate::box_secrets::get(db, BOX_SECRET_KEY).await? else {
        return Ok(None);
    };
    let relay_addr = metadata["relay_addr"].as_str().unwrap_or_default().to_string();
    let sni = metadata["sni"].as_str().unwrap_or_default().to_string();
    if relay_addr.is_empty() || sni.is_empty() {
        return Ok(None);
    }
    Ok(Some(RelayConfig {
        relay_addr,
        sni,
        token,
    }))
}

/// The box's provisioned relay SNI, if any — for advertising `box_url` at pairing.
pub async fn sni(db: &PgPool) -> Option<String> {
    load(db).await.ok().flatten().map(|c| c.sni)
}
