//! Plaid liabilities sync.
//!
//! Cron-driven, per-credential. Calls `/liabilities/get` and writes credit /
//! mortgage / student loan rows → `data_financial_liability`. Some accounts
//! don't support liabilities; the API returns 400/404 in that case which we
//! treat as benign (`written = 0`).

mod transform;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use virtues_helpers::{connect_from_env, output, read_input};

const PLAID_LIABILITIES: &str = "https://production.plaid.com/liabilities/get";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_actions::init_tracing();

    let input = read_input()?;
    let pool = connect_from_env("virtues-action-plaid_liabilities_sync").await?;

    let access_token = virtues_actions::secret(&input, "access_token")?;

    let client_id = std::env::var("PLAID_CLIENT_ID").context("PLAID_CLIENT_ID not set")?;
    let secret = std::env::var("PLAID_SECRET").context("PLAID_SECRET not set")?;

    let resp = reqwest::Client::new()
        .post(PLAID_LIABILITIES)
        .json(&json!({
            "client_id": client_id,
            "secret": secret,
            "access_token": access_token,
        }))
        .send()
        .await
        .context("plaid liabilities/get failed")?;

    if !resp.status().is_success() {
        // Many Plaid accounts (checking, savings, etc.) don't support
        // liabilities — that returns 400. Don't error the run; just record 0.
        let summary = format!(
            "no liabilities for this credential (plaid {})",
            resp.status().as_u16()
        );
        return output(&summary, &input.config);
    }

    let body: Value = resp.json().await.context("plaid non-JSON")?;
    let liabilities = body
        .get("liabilities")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));

    let written = transform::write_liabilities(&pool, &liabilities).await?;
    let summary = format!("synced {written} Plaid liabilities");
    output(&summary, &input.config)
}
