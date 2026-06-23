//! HTTP client for the atlas → virtues-api internal surface.
//!
//! Two calls, both carrying only an opaque `account_id` + amounts — never a
//! Stripe id, email, or device key:
//!   - `register_device` — bind (or rotate) a device api key to an account.
//!   - `credit` — fund an account on renewal (`set`) or top-up (`add`).

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::Serialize;

const INTERNAL_SECRET_HEADER: &str = "X-Internal-Secret";

#[derive(Clone)]
pub struct VirtuesApiClient {
    http: Client,
    base_url: String,
    internal_secret: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterDevice {
    /// Lowercase hex of SHA-256(api_key).
    pub api_key_hash: String,
    /// Opaque per-customer account id.
    pub account_id: String,
    /// The customer's user-tunable daily spend ceiling.
    pub daily_cap_micros: i64,
}

#[derive(Debug, Serialize)]
pub struct Credit {
    pub account_id: String,
    pub amount_micros: i64,
    /// "set" = subscription renewal (overwrite). "add" = top-up (increment).
    pub mode: &'static str,
    pub daily_cap_micros: i64,
    /// Optional ledger reference (e.g. a Stripe invoice/PI id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

impl VirtuesApiClient {
    pub fn new(base_url: String, internal_secret: String) -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("reqwest client"),
            base_url,
            internal_secret,
        }
    }

    async fn post<T: Serialize>(&self, path: &str, payload: &T) -> Result<()> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let resp = self
            .http
            .post(&url)
            .header(INTERNAL_SECRET_HEADER, &self.internal_secret)
            .json(payload)
            .send()
            .await
            .with_context(|| format!("posting {path}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("{path} failed: {status} — {body}"));
        }
        Ok(())
    }

    /// Bind (or rotate) a device api key to an account. Idempotent recovery
    /// primitive — re-pointing an account never touches its balance.
    pub async fn register_device(&self, payload: &RegisterDevice) -> Result<()> {
        self.post("/internal/device", payload).await
    }

    /// Credit an account: `set` (renewal overwrite) or `add` (top-up).
    pub async fn credit(&self, payload: &Credit) -> Result<()> {
        self.post("/internal/credit", payload).await
    }
}
