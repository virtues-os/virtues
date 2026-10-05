//! Plaid investments sync.
//!
//! Cron-driven, per-credential. Calls `/investments/holdings/get` and writes
//! holdings → `data_financial_asset`. Holdings are joined with securities to
//! enrich symbol/name fields. Items that don't carry the investments product
//! return 400 from Plaid, which we treat as benign (`written = 0`).

mod transform;

use anyhow::Result;
use serde_json::json;
use virtues::storage::lake;
use virtues_helpers::{connect_from_env, output, read_input};

const ACTION: &str = "plaid_investments_sync";

/// Plaid codes that mean the Item simply lacks this product. Kept in step with
/// the same list in `plaid_liabilities_sync`.
const NO_PRODUCT_CODES: &[&str] = &[
    "INVALID_PRODUCT",
    "PRODUCTS_NOT_SUPPORTED",
    "PRODUCT_NOT_READY",
    "NO_INVESTMENT_ACCOUNTS",
    "NO_INVESTMENT_AUTH",
];

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let input = read_input()?;
    let pool = connect_from_env("virtues-action-plaid_investments_sync").await?;

    let access_token = virtues_applets::secret(&input, "access_token")?;

    // Proxied through virtues-api: the box sends only the per-user access_token;
    // the master Plaid secret stays server-side. Use the raw variant so a benign
    // non-2xx doesn't error: Items are linked with `transactions` only, so every
    // run here returns PRODUCTS_NOT_SUPPORTED until the Plaid account is enabled
    // for investments. On a 12-hour cron that would have painted the whole Plaid
    // source permanently red for a condition that isn't a fault.
    let (status, resp) = virtues_applets::service_proxy_raw(
        &pool,
        "plaid",
        "investments/holdings/get",
        &json!({ "access_token": access_token }),
    )
    .await?;

    if !(200..300).contains(&status) {
        // Same split as plaid_liabilities_sync: only "this Item has no
        // investments product" is benign (record 0). Any other Plaid code —
        // ITEM_LOGIN_REQUIRED above all — fails the run, and so does our own
        // proxy's `{"error":{"code":...}}` (wallet_empty, service_not_configured, …).
        if let Some(code) = resp.get("error_code").and_then(|v| v.as_str()) {
            if NO_PRODUCT_CODES.contains(&code) {
                let summary = format!("no investments for this credential (plaid {code})");
                return output(&summary, &input.config);
            }
            anyhow::bail!("plaid investments error {code} ({status}): {resp}");
        }
        anyhow::bail!("plaid investments proxy error {status}: {resp}");
    }

    let holdings = resp
        .get("holdings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let securities = resp
        .get("securities")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let storage = lake::storage_from_env()?;
    lake::archive_cloud(&pool, &storage, "plaid", ACTION, "investments", &[resp.clone()]).await?;

    let written = transform::write_holdings(&pool, &holdings, &securities).await?;
    let summary = format!("synced {written} Plaid holdings");
    output(&summary, &input.config)
}
