//! Plaid liabilities sync.
//!
//! Cron-driven, per-credential. Calls `/liabilities/get` and writes credit /
//! mortgage / student loan rows → `data_financial_liability`. Some accounts
//! don't support liabilities; the API returns 400/404 in that case which we
//! treat as benign (`written = 0`).

mod transform;

use anyhow::Result;
use serde_json::{json, Value};
use virtues::storage::lake;
use virtues_helpers::{connect_from_env, output, read_input};

const ACTION: &str = "plaid_liabilities_sync";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let input = read_input()?;
    let pool = connect_from_env("virtues-action-plaid_liabilities_sync").await?;

    let access_token = virtues_applets::secret(&input, "access_token")?;

    // Proxied through virtues-api: the box sends only the per-user access_token;
    // the master Plaid secret stays server-side. Use the raw variant so a benign
    // non-2xx (accounts that don't support liabilities → 400) doesn't error.
    let (status, body) = virtues_applets::plaid_proxy_raw(
        &pool,
        "liabilities/get",
        &json!({ "access_token": access_token }),
    )
    .await?;

    if !(200..300).contains(&status) {
        // Distinguish a genuine Plaid response from a proxy-layer failure.
        // Plaid errors carry a top-level `error_code` (e.g. PRODUCTS_NOT_SUPPORTED
        // for accounts that don't support liabilities → benign, record 0).
        // Proxy errors are shaped `{"error":{"code":...}}` (wallet_empty,
        // service_not_configured, unknown_key, upstream_error) — those are real
        // failures and must NOT be silently reported as "no liabilities".
        if let Some(code) = body.get("error_code").and_then(|v| v.as_str()) {
            let summary = format!("no liabilities for this credential (plaid {code})");
            return output(&summary, &input.config);
        }
        anyhow::bail!("plaid liabilities proxy error {status}: {body}");
    }

    let liabilities = body
        .get("liabilities")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));

    let storage = lake::storage_from_env()?;
    // `body`, not `liabilities` — the response also carries the `accounts` array and
    // item metadata that the transform below never looks at.
    lake::archive_cloud(&pool, &storage, "plaid", ACTION, "liabilities", &[body.clone()]).await?;

    let written = transform::write_liabilities(&pool, &liabilities).await?;
    let summary = format!("synced {written} Plaid liabilities");
    output(&summary, &input.config)
}
