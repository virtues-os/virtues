//! HTTP calls to the OAuth proxy (the `oauth` routes in `services/virtues-api`,
//! WS-4 — formerly the standalone Node `apps/oauth-proxy`) for token exchange
//! and refresh.
//!
//! The proxy is the only component that holds third-party OAuth client_id /
//! client_secret. Self-hosted Virtues never sees provider secrets — it only
//! sees the proxy's normalized `{secrets, metadata, expires_in, scopes}`
//! response.
//!
//! Proxy URL: defaults to `https://auth.virtues.com`. Override with the
//! `VIRTUES_OAUTH_PROXY_URL` env var to point at virtues-api (or self-host).

use serde::{Deserialize, Serialize};

use crate::auth::error::{AuthError, Result};

const DEFAULT_PROXY_URL: &str = "https://auth.virtues.com";

/// Client for proxy calls, self-sufficient about the rustls CryptoProvider:
/// reqwest here is `rustls-tls-no-provider`, and this crate is linked into
/// standalone applet binaries whose `main` installs no provider — a bare
/// `Client::new()` panics "No provider set" there. Install ring ourselves
/// (idempotent; a second install errors harmlessly) instead of relying on
/// the host process having done it.
///
/// The timeout matters beyond the cron: `ensure_fresh` is also the
/// just-in-time path the action runner takes before every dispatch, so a
/// hung proxy call without one would wedge a sync forever.
fn client() -> reqwest::Client {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("failed to build reqwest client")
}

/// Resolve the OAuth proxy base URL. Reads `VIRTUES_OAUTH_PROXY_URL` if set
/// (with any trailing slash trimmed), otherwise falls back to the first-party
/// proxy at `https://auth.virtues.com`.
///
/// Both the core HTTP handlers (`oauth_start` building the redirect URL) and
/// the helpers crate (`proxy_exchange` / `proxy_refresh` doing the
/// server-to-server callback POST) call this — keep it here so the two paths
/// can never drift to different domains.
pub fn proxy_url() -> String {
    let raw = std::env::var("VIRTUES_OAUTH_PROXY_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_PROXY_URL.to_string());
    raw.trim_end_matches('/').to_string()
}

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
///
/// `api_key` is the box's Virtues key, sent as `X-Virtues-Api-Key` so the
/// proxy can tell which box is asking. Optional today: the proxy accepts calls
/// without it while boxes upgrade, and logs them; it flips to required once
/// the fleet has moved. `None` on an unlinked box is an ordinary state.
pub async fn proxy_exchange(
    source_id: &str,
    exchange_token: &str,
    api_key: Option<&str>,
) -> Result<ProxyExchangeResponse> {
    let url = format!("{}/{source_id}/exchange/{exchange_token}", proxy_url());
    let resp = with_box_key(client().post(&url), api_key)
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
///
/// `api_key`: as for [`proxy_exchange`]. This is the call the proxy will
/// require it on first — unauthenticated, `/refresh` performs the refresh
/// grant with the proxy's own client secret for anyone holding a refresh
/// token.
pub async fn proxy_refresh(
    source_id: &str,
    refresh_token: &str,
    api_key: Option<&str>,
) -> Result<ProxyExchangeResponse> {
    let url = format!("{}/{source_id}/refresh", proxy_url());
    let resp = with_box_key(client().post(&url), api_key)
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

/// Attach the box's key when it has one. The header name is the contract with
/// `services/virtues-api/src/routes/oauth.rs::caller_api_key`.
fn with_box_key(req: reqwest::RequestBuilder, api_key: Option<&str>) -> reqwest::RequestBuilder {
    match api_key {
        Some(k) => req.header("X-Virtues-Api-Key", k),
        None => req,
    }
}
