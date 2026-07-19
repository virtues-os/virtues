//! Plaid investments sync.
//!
//! Cron-driven, per-credential. Calls `/investments/holdings/get` and writes
//! holdings → `data_financial_asset`. Holdings are joined with securities to
//! enrich symbol/name fields.

mod transform;

use anyhow::Result;
use serde_json::{json, Value};
use virtues::storage::lake;
use virtues_helpers::{connect_from_env, output, read_input};

const ACTION: &str = "plaid_investments_sync";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_actions::init_tracing();

    let input = read_input()?;
    let pool = connect_from_env("virtues-action-plaid_investments_sync").await?;

    let access_token = virtues_actions::secret(&input, "access_token")?;

    // Proxied through virtues-api: the box sends only the per-user access_token;
    // the master Plaid secret stays server-side. (A non-2xx — e.g. accounts that
    // don't support investments — surfaces as an error here, as before.)
    let resp: Value = virtues_actions::plaid_proxy(
        &pool,
        "investments/holdings/get",
        &json!({ "access_token": access_token }),
    )
    .await?;

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
