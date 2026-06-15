//! HTTP client for registering vouchers with virtues-api.
//!
//! Per WS-6a/7, this is the ONLY thing Atlas sends across the wall, and
//! it carries only a voucher's *value* — its code hash, budget, validity.
//! No customer, no bearer. virtues-api never calls back.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Serialize;

const INTERNAL_SECRET_HEADER: &str = "X-Internal-Secret";

#[derive(Clone)]
pub struct VirtuesApiClient {
    http: Client,
    base_url: String,
    internal_secret: String,
}

/// v3 single-amount voucher payload. Atlas mints a single value;
/// virtues-api stores it as the `wallet_micros` refill on redeem.
/// - Sub renewal: `amount_micros = $15`, `is_renewal = true` (overwrite).
/// - Top-up (manual or auto): `amount_micros = $10–$50`, `is_renewal = false` (add).
#[derive(Debug, Serialize)]
pub struct RegisterVoucher {
    /// Lowercase hex of SHA-256(voucher_code).
    pub voucher_code_hash: String,
    /// Voucher amount in micros USD.
    pub amount_micros: i64,
    /// Whether this voucher overwrites the wallet (sub renewal) or adds to it (top-up).
    pub is_renewal: bool,
    pub voucher_expires_at: DateTime<Utc>,
    /// The customer's user-tunable daily spend ceiling (`customers.daily_cap_micros`),
    /// carried across the wall so virtues-api can enforce it per-bearer without
    /// ever learning the customer. Lands on the entitlement at redeem; a cap
    /// change takes effect at the customer's next voucher / top-up.
    pub daily_cap_micros: i64,
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

    /// Register a freshly minted voucher with virtues-api so it can be
    /// redeemed by the device.
    pub async fn register_voucher(&self, payload: &RegisterVoucher) -> Result<()> {
        let url = format!("{}/internal/voucher", self.base_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .header(INTERNAL_SECRET_HEADER, &self.internal_secret)
            .json(payload)
            .send()
            .await
            .context("posting register voucher")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("register voucher failed: {status} — {body}"));
        }
        Ok(())
    }
}
