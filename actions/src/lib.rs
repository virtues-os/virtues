//! Shared helpers for the action subprocess binaries.
//!
//! Every action is a `[[bin]]` in this crate; this lib holds the boilerplate
//! they all repeated: tracing init, an HTTP client, `ActionInput`
//! credential/config accessors, and the send→error_for_status→json fetch dance.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::time::Duration;
use virtues_helpers::ActionInput;

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
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .expect("failed to build reqwest client")
}

/// Read `credentials.secrets.<key>` as a string, erroring if absent.
pub fn secret<'a>(input: &'a ActionInput, key: &str) -> Result<&'a str> {
    input
        .credentials
        .as_ref()
        .and_then(|c| c.get("secrets"))
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_str())
        .with_context(|| format!("credentials missing secrets.{key}"))
}

/// Read `config.<key>` as a string, if present.
pub fn config_str<'a>(input: &'a ActionInput, key: &str) -> Option<&'a str> {
    input.config.get(key).and_then(|v| v.as_str())
}

/// Send a request and deserialize a 2xx JSON body, with context on each failure.
pub async fn fetch_json<T: DeserializeOwned>(req: reqwest::RequestBuilder) -> Result<T> {
    let resp = req.send().await.context("request failed")?;
    let resp = resp.error_for_status().context("non-2xx response")?;
    resp.json::<T>().await.context("failed to parse JSON response")
}
