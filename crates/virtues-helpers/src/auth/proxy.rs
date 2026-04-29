//! HTTP calls to `apps/oauth-proxy` for token exchange and refresh.
//!
//! The proxy is the only component that holds third-party OAuth client_id /
//! client_secret. Self-hosted Virtues never sees provider secrets — it only
//! sees the proxy's normalized `{secrets, metadata, expires_in, scopes}`
//! response.
//!
//! Proxy URL: hardcoded to `https://auth.virtues.com` for v1. Self-hosted
//! proxy override (`VIRTUES_OAUTH_PROXY_URL`) is a deferred feature.

use serde::{Deserialize, Serialize};

use crate::auth::error::{AuthError, Result};

/// First-party proxy URL. v1: hardcoded.
const PROXY_URL: &str = "https://auth.virtues.com";

/// Normalized response from the proxy after a successful exchange or refresh.
/// Same shape for both calls — the proxy abstracts away provider differences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyExchangeResponse {
    /// Encrypted-on-write secrets payload. Shape per source kind:
    /// - via_proxy (OAuth): `{ access_token, refresh_token, expires_at }`
    /// - via_proxy (Plaid): `{ access_token }` (no expiry, no refresh)
    pub secrets: serde_json::Value,

    /// Plaintext non-secret context (e.g. Plaid `item_id`, OAuth `email`).
    #[serde(default)]
    pub metadata: serde_json::Value,

    /// Seconds until access token expires. `None` for non-refreshing kinds
    /// like Plaid Hosted Link.
    #[serde(default)]
    pub expires_in: Option<i64>,

    /// OAuth scopes granted by the user. `None` for non-OAuth kinds.
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
}

/// Exchange a one-time `exchange_token` (issued by the proxy after the user
/// completes the OAuth dance) for the actual `{secrets, metadata, ...}` to
/// store in the Vault.
///
/// POSTs to `{proxy}/{source_id}/exchange/{exchange_token}`. The proxy holds
/// the provider's `client_id` / `client_secret` and performs the real token
/// exchange before responding to us.
pub async fn proxy_exchange(
    source_id: &str,
    exchange_token: &str,
) -> Result<ProxyExchangeResponse> {
    let url = format!("{PROXY_URL}/{source_id}/exchange/{exchange_token}");
    let resp = reqwest::Client::new()
        .post(&url)
        .send()
        .await
        .map_err(|e| AuthError::Proxy(format!("unreachable: {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AuthError::Proxy(format!(
            "upstream {}: {}",
            status.as_u16(),
            body
        )));
    }

    resp.json::<ProxyExchangeResponse>()
        .await
        .map_err(|e| AuthError::Proxy(format!("invalid response: {e}")))
}

/// Refresh an OAuth access token via the proxy.
///
/// POSTs to `{proxy}/{source_id}/refresh` with `{refresh_token}` body.
/// Returns the same shape as `proxy_exchange`. Used by the
/// `credential_refresh` cron action.
pub async fn proxy_refresh(
    source_id: &str,
    refresh_token: &str,
) -> Result<ProxyExchangeResponse> {
    let url = format!("{PROXY_URL}/{source_id}/refresh");
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .await
        .map_err(|e| AuthError::Proxy(format!("unreachable: {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AuthError::Proxy(format!(
            "upstream {}: {}",
            status.as_u16(),
            body
        )));
    }

    resp.json::<ProxyExchangeResponse>()
        .await
        .map_err(|e| AuthError::Proxy(format!("invalid response: {e}")))
}
