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
        .timeout(PROXY_TIMEOUT)
        .build()
        .expect("failed to build reqwest client")
}

/// The longest one proxy call may take. `ensure_fresh` sizes its wait for
/// another process's refresh from it.
pub(crate) const PROXY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

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
        .map_err(|e| transport_error("unreachable", e))?;
    read_response(resp).await
}

/// Refresh an OAuth access token via the proxy.
///
/// POSTs to `{proxy}/{source_id}/refresh` with `{refresh_token}` body.
/// Returns the same shape as `proxy_exchange`. Its one caller is
/// `ensure_fresh`, which the action runner and the `credential_refresh`
/// applet both go through. [`is_refused_grant`] reads its errors.
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
        .map_err(|e| transport_error("unreachable", e))?;
    read_response(resp).await
}

/// The normalized answer, or the proxy's error as [`upstream_error`] words it.
/// The proxy's error bodies contain messages and codes, never a token.
async fn read_response(resp: reqwest::Response) -> Result<ProxyExchangeResponse> {
    let status = resp.status();
    if !status.is_success() {
        // An unreadable error body still leaves the status to act on.
        let body = resp.text().await.unwrap_or_default();
        return Err(upstream_error(status.as_u16(), &body));
    }
    resp.json::<ProxyExchangeResponse>()
        .await
        .map_err(|e| transport_error("invalid response", e))
}

/// A call that got no usable answer. reqwest puts the request URL in its
/// errors, and the exchange call carries its one-time token in the path, so
/// the URL is dropped: this message reaches the journal and the HTTP answer
/// to the browser.
fn transport_error(what: &str, e: reqwest::Error) -> AuthError {
    AuthError::Proxy(format!("{what}: {}", e.without_url()))
}

const UPSTREAM_PREFIX: &str = "upstream ";

/// The error for a non-2xx answer from the proxy. `credential_refresh`
/// matches "upstream 4" in it, and [`is_refused_grant`] parses it back, so
/// its wording changes only together with both.
fn upstream_error(status: u16, body: &str) -> AuthError {
    AuthError::Proxy(format!("{UPSTREAM_PREFIX}{status}: {body}"))
}

/// The proxy's status and body from an [`upstream_error`] message.
fn parse_upstream(msg: &str) -> Option<(u16, &str)> {
    let (status, body) = msg.strip_prefix(UPSTREAM_PREFIX)?.split_once(": ")?;
    Some((status.parse().ok()?, body))
}

/// The field of a proxy error answer that says what failed. A proxy older
/// than the field does not send it.
#[derive(Deserialize)]
struct ProxyErrorBody {
    code: Option<String>,
}

/// True when a refresh failed because the provider refused the grant: the
/// owner revoked access, or the grant expired. Only a reconnect fixes that,
/// so `ensure_fresh` marks the credential `reauth_required`.
///
/// When the proxy sends `code`, only `invalid_grant` is a refused grant. The
/// proxy's own 4xx answers, such as an unknown provider, are not, because a
/// reconnect would not fix them. A proxy without `code` gets the rule boxes
/// used before it: any 4xx answer. That proxy answers a refused grant with a
/// 502 and no detail, so against it this cannot see one at all.
pub fn is_refused_grant(err: &AuthError) -> bool {
    let AuthError::Proxy(msg) = err else {
        return false;
    };
    let Some((status, body)) = parse_upstream(msg) else {
        return false;
    };
    match serde_json::from_str::<ProxyErrorBody>(body) {
        Ok(ProxyErrorBody { code: Some(code) }) => code == "invalid_grant",
        _ => (400..500).contains(&status),
    }
}

/// Attach the box's key when it has one. The header name is the contract with
/// `services/virtues-api/src/routes/oauth.rs::caller_api_key`.
fn with_box_key(req: reqwest::RequestBuilder, api_key: Option<&str>) -> reqwest::RequestBuilder {
    match api_key {
        Some(k) => req.header("X-Virtues-Api-Key", k),
        None => req,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proxy_with_code_is_believed() {
        let refused = r#"{"error":"refresh failed: upstream status 400 (invalid_grant)","code":"invalid_grant","upstream_status":400}"#;
        assert!(is_refused_grant(&upstream_error(401, refused)));

        // The proxy's own 4xx answers: a reconnect would not fix them.
        let unknown =
            r#"{"error":"unknown provider","code":"unknown_provider","upstream_status":null}"#;
        assert!(!is_refused_grant(&upstream_error(404, unknown)));
        let missing = r#"{"error":"missing refresh_token","code":"missing_refresh_token","upstream_status":null}"#;
        assert!(!is_refused_grant(&upstream_error(400, missing)));

        let client = r#"{"error":"refresh failed: upstream status 401 (invalid_client)","code":"invalid_client","upstream_status":401}"#;
        assert!(!is_refused_grant(&upstream_error(502, client)));
    }

    #[test]
    fn a_proxy_without_code_gets_the_old_rule() {
        // An older proxy answers a refused grant this way, with no detail.
        assert!(!is_refused_grant(&upstream_error(
            502,
            r#"{"error":"refresh failed: upstream status 400"}"#
        )));
        // Its own 4xx answers were, and stay, a reason to reconnect.
        assert!(is_refused_grant(&upstream_error(
            404,
            r#"{"error":"unknown provider"}"#
        )));
        assert!(is_refused_grant(&upstream_error(400, "")));
        assert!(is_refused_grant(&upstream_error(
            422,
            "Failed to deserialize the JSON body"
        )));
    }

    #[test]
    fn other_errors_are_not_a_refused_grant() {
        assert!(!is_refused_grant(&AuthError::Proxy(
            "unreachable: error sending request".into()
        )));
        assert!(!is_refused_grant(&AuthError::Proxy(
            "invalid response: error decoding response body".into()
        )));
        assert!(!is_refused_grant(&AuthError::NotFound("cred_1".into())));
    }

    #[test]
    fn the_error_parses_back() {
        let AuthError::Proxy(msg) = upstream_error(401, r#"{"code":"a: b"}"#) else {
            panic!("upstream_error must build a Proxy error");
        };
        assert_eq!(parse_upstream(&msg), Some((401, r#"{"code":"a: b"}"#)));
        // The substring the credential_refresh applet matches today.
        assert!(msg.contains("upstream 4"));
        assert_eq!(parse_upstream("unreachable: error sending request"), None);
    }

    #[tokio::test]
    async fn an_unreachable_proxy_error_does_not_carry_the_url() {
        // A port that was free a moment ago, so the connect is refused.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let token = "xt-test-0123456789";
        let e = client()
            .post(format!("http://127.0.0.1:{port}/google/exchange/{token}"))
            .send()
            .await
            .expect_err("nothing listens on the port");
        assert!(
            e.to_string().contains(token),
            "reqwest puts the URL in its errors: {e}"
        );

        let msg = transport_error("unreachable", e).to_string();
        assert!(!msg.contains(token), "{msg}");
        assert!(msg.starts_with("proxy error: unreachable: "), "{msg}");
    }
}
