//! Box-side device-link client.
//!
//! Connects this box to a paid subscription via Atlas's device-authorization
//! flow ([RFC 8628] shape) — the box never holds a Stripe key and never sees a
//! checkout page. `start` kicks off a link and stashes the secret `device_code`
//! in `box_secrets`, so a later `poll` (possibly a separate HTTP request from
//! the web UI) can resume. `poll` returns the status; on `ready` it stores the
//! billing token and mints the first bearer, so AI works immediately.
//!
//! [RFC 8628]: https://www.rfc-editor.org/rfc/rfc8628

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use sqlx::PgPool;

use crate::wireguard::box_secrets;

/// box_secrets key holding the in-flight link's secret device_code + metadata.
const INFLIGHT_KEY: &str = "billing_link_inflight";

/// What the box shows the user to complete the link. The secret `device_code`
/// is intentionally absent — it stays box-side (sealed in `box_secrets`) and is
/// never exposed to the browser.
#[derive(Debug, Clone, Serialize)]
pub struct LinkStart {
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub interval: u64,
    pub expires_in: i64,
}

/// Outcome of a poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkStatus {
    /// Checkout not completed yet — keep polling.
    Pending,
    /// Linked: billing token stored + first bearer minted.
    Ready,
    /// The link expired or was denied — start over.
    Expired,
    /// No link is in flight on this box.
    None,
}

/// Start a device link: ask Atlas for a device/user code pair, stash the secret
/// device_code, and return the user-facing bits.
pub async fn start(db: &PgPool, http: &reqwest::Client, atlas_url: &str) -> Result<LinkStart> {
    let resp = http
        .post(format!("{}/init/start", atlas_url.trim_end_matches('/')))
        .send()
        .await
        .context("POST /init/start")?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(anyhow!("link start failed: {s} — {b}"));
    }
    let v: serde_json::Value = resp.json().await?;
    let device_code = v["device_code"]
        .as_str()
        .ok_or_else(|| anyhow!("link start response missing device_code"))?
        .to_string();
    let user_code = v["user_code"].as_str().unwrap_or("").to_string();
    let verification_uri = v["verification_uri"].as_str().unwrap_or("").to_string();
    let verification_uri_complete =
        v["verification_uri_complete"].as_str().unwrap_or("").to_string();
    let interval = v["interval"].as_u64().unwrap_or(5);
    let expires_in = v["expires_in"].as_i64().unwrap_or(900);

    // Stash the secret device_code (sealed) + public bits for a later poll.
    let meta = serde_json::json!({
        "user_code": user_code,
        "verification_uri_complete": verification_uri_complete,
        "interval": interval,
    });
    box_secrets::put(db, INFLIGHT_KEY, &device_code, &meta).await?;

    Ok(LinkStart {
        user_code,
        verification_uri,
        verification_uri_complete,
        interval,
        expires_in,
    })
}

/// Poll the in-flight link. On `ready`, store the billing token and eagerly
/// mint the first bearer (best-effort — lazy renew retries on the first AI
/// call). Clears the in-flight state on any terminal outcome.
pub async fn poll(
    db: &PgPool,
    http: &reqwest::Client,
    atlas_url: &str,
    api_url: &str,
) -> Result<LinkStatus> {
    let Some((device_code, _meta)) = box_secrets::get(db, INFLIGHT_KEY).await? else {
        return Ok(LinkStatus::None);
    };

    let resp = http
        .post(format!("{}/init/poll", atlas_url.trim_end_matches('/')))
        .json(&serde_json::json!({ "device_code": device_code }))
        .send()
        .await
        .context("POST /init/poll")?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(anyhow!("link poll failed: {s} — {b}"));
    }

    let v: serde_json::Value = resp.json().await?;
    match v["status"].as_str().unwrap_or("pending") {
        "ready" => {
            let token = v["billing_token"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow!("link ready but no billing_token"))?;
            super::renew::store_billing_token(db, token).await?;
            if let Err(e) = super::renew::renew(db, http, atlas_url, api_url).await {
                tracing::warn!("eager bearer mint after link failed (lazy retry later): {e}");
            }
            clear_inflight(db).await;
            Ok(LinkStatus::Ready)
        }
        // Already retrieved on a prior poll (e.g. crash before clearing) — the
        // token is stored; just converge.
        "claimed" => {
            clear_inflight(db).await;
            Ok(LinkStatus::Ready)
        }
        "expired" | "denied" => {
            clear_inflight(db).await;
            Ok(LinkStatus::Expired)
        }
        _ => Ok(LinkStatus::Pending),
    }
}

async fn clear_inflight(db: &PgPool) {
    if let Err(e) = sqlx::query("DELETE FROM box_secrets WHERE key = $1")
        .bind(INFLIGHT_KEY)
        .execute(db)
        .await
    {
        tracing::warn!("failed to clear in-flight billing link: {e}");
    }
}
