//! Shared helpers for the action subprocess binaries.
//!
//! Every action is a `[[bin]]` in this crate; this lib holds the boilerplate
//! they all repeated: tracing init, an HTTP client, `AppletInput`
//! credential/config accessors, and the send→error_for_status→json fetch dance.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::time::Duration;
use virtues_helpers::AppletInput;

/// Initialize stderr tracing with an `info` default (overridable via `RUST_LOG`),
/// and load the box environment — the same files, in the same order, as the
/// server (`main.rs`).
///
/// Actions used to load NOTHING. On a box that was invisible, because systemd
/// hands them an `EnvironmentFile`; in dev they simply ran with different
/// configuration than the server, and nobody noticed because nothing important
/// depended on it.
///
/// Something does now. The embedding model's width and prompt formats live in the
/// environment (they are facts about a model, not about Virtues), so an action
/// that cannot see them computes a different vector geometry than the server —
/// and the index guard, correctly, refuses to let it write. The config drift was
/// always there; embeddings are just the first thing sharp enough to cut on it.
pub fn init_tracing() {
    let _ = dotenv::from_path("/var/lib/virtues/virtues.env");
    if dotenv::dotenv().is_err() {
        let _ = dotenv::from_path("../.env");
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
}

/// A reqwest client with a generous default timeout for action HTTP calls
/// (the bare `Client::new()` they used had no timeout at all).
pub fn http_client() -> reqwest::Client {
    ensure_crypto_provider();
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .expect("failed to build reqwest client")
}

/// Install the ring crypto provider once (reqwest here is rustls-tls-no-provider,
/// so direct external HTTPS panics with "No provider set" without this).
fn ensure_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Read `credentials.secrets.<key>` as a string, erroring if absent.
pub fn secret<'a>(input: &'a AppletInput, key: &str) -> Result<&'a str> {
    input
        .credentials
        .as_ref()
        .and_then(|c| c.get("secrets"))
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_str())
        .with_context(|| format!("credentials missing secrets.{key}"))
}

/// Read `config.<key>` as a string, if present.
pub fn config_str<'a>(input: &'a AppletInput, key: &str) -> Option<&'a str> {
    input.config.get(key).and_then(|v| v.as_str())
}

/// Send a request and deserialize a 2xx JSON body, with context on each failure.
pub async fn fetch_json<T: DeserializeOwned>(req: reqwest::RequestBuilder) -> Result<T> {
    let resp = req.send().await.context("request failed")?;
    let resp = resp.error_for_status().context("non-2xx response")?;
    resp.json::<T>().await.context("failed to parse JSON response")
}

/// Call a third-party data endpoint through virtues-api's `/v1/services/*`
/// proxy.
///
/// The box holds only the per-user token; virtues-api injects the master
/// application credentials server-side and forwards upstream, so the vendor's
/// app secret never lives on the box. `service` names the proxied vendor as
/// virtues-api routes it (`"plaid"`); `path` is the endpoint tail (e.g.
/// `"transactions/sync"`); `body` carries the per-user token and any params —
/// never the master credentials. Returns the vendor's response JSON verbatim,
/// so callers archive and transform it exactly as they would a direct call.
///
/// **Takes the service rather than naming one.** This was `plaid_proxy`, with
/// the vendor and its route welded into a library shared by every applet — so
/// the generic layer knew one integration by name, and the second vendor to use
/// the same proxy would have arrived as a copy of it. `/v1/services/{service}/`
/// is already a namespace on the virtues-api side; this now matches it.
pub async fn service_proxy(
    pool: &sqlx::PgPool,
    service: &str,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value> {
    let (status, resp_body) = service_proxy_raw(pool, service, path, body).await?;
    if !(200..300).contains(&status) {
        anyhow::bail!("{service} proxy /v1/services/{service}/{path} returned {status}: {resp_body}");
    }
    Ok(resp_body)
}

/// Like [`service_proxy`] but returns the raw `(status, body)` without erroring
/// on a non-2xx. For endpoints where a non-2xx is expected and benign (Plaid's
/// `liabilities/get` returns 400 for accounts that do not carry that product).
pub async fn service_proxy_raw(
    pool: &sqlx::PgPool,
    service: &str,
    path: &str,
    body: &serde_json::Value,
) -> Result<(u16, serde_json::Value)> {
    use virtues::virtues_api::client::{BearerClient, Purpose};

    let route = format!("/v1/services/{service}/{path}");
    let resp = BearerClient::from_env(pool.clone())
        .with_purpose(Purpose::System)
        .post_json(&route, body)
        .await
        .with_context(|| format!("{service} proxy call to {route} failed"))?;
    Ok((resp.status, resp.body))
}
