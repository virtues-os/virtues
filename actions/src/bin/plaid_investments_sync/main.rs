//! Plaid investments sync.
//!
//! Cron-driven, per-credential. Calls `/investments/holdings/get` and writes
//! holdings → `data_financial_asset`. Holdings are joined with securities to
//! enrich symbol/name fields.

mod transform;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use virtues_helpers::{connect_from_env, output, read_input};

const PLAID_HOLDINGS: &str = "https://production.plaid.com/investments/holdings/get";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let input = read_input()?;
    let pool = connect_from_env().await?;

    let access_token = input
        .credentials
        .as_ref()
        .and_then(|c| c.get("secrets"))
        .and_then(|s| s.get("access_token"))
        .and_then(|v| v.as_str())
        .context("plaid credentials missing secrets.access_token")?;

    let client_id = std::env::var("PLAID_CLIENT_ID").context("PLAID_CLIENT_ID not set")?;
    let secret = std::env::var("PLAID_SECRET").context("PLAID_SECRET not set")?;

    let resp: Value = reqwest::Client::new()
        .post(PLAID_HOLDINGS)
        .json(&json!({
            "client_id": client_id,
            "secret": secret,
            "access_token": access_token,
        }))
        .send()
        .await
        .context("plaid holdings/get failed")?
        .error_for_status()
        .context("plaid non-2xx (some accounts don't support investments — error is benign)")?
        .json()
        .await
        .context("plaid non-JSON")?;

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

    let written = transform::write_holdings(&pool, &holdings, &securities).await?;
    let summary = format!("synced {written} Plaid holdings");
    output(&summary, &input.config)
}
