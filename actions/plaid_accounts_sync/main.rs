//! Plaid accounts sync.
//!
//! Cron-driven, per-credential. Fetches `/accounts/get` from Plaid using the
//! item's access_token (the only secret). Writes to `data_financial_account`.

mod transform;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use virtues::storage::lake;
use virtues_helpers::{connect_from_env, output, read_input};

const ACTION: &str = "plaid_accounts_sync";
const PLAID_ACCOUNTS_GET: &str = "https://production.plaid.com/accounts/get";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_actions::init_tracing();

    let input = read_input()?;
    let pool = connect_from_env("virtues-action-plaid_accounts_sync").await?;

    let creds = input
        .credentials
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("plaid credentials missing"))?;
    let access_token = creds
        .get("secrets")
        .and_then(|s| s.get("access_token"))
        .and_then(|v| v.as_str())
        .context("plaid credentials missing secrets.access_token")?;

    let item_id = creds
        .get("metadata")
        .and_then(|m| m.get("item_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let institution = creds
        .get("metadata")
        .and_then(|m| m.get("institution_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");

    let client_id = std::env::var("PLAID_CLIENT_ID").context("PLAID_CLIENT_ID not set")?;
    let secret = std::env::var("PLAID_SECRET").context("PLAID_SECRET not set")?;

    let resp: Value = reqwest::Client::new()
        .post(PLAID_ACCOUNTS_GET)
        .json(&json!({
            "client_id": client_id,
            "secret": secret,
            "access_token": access_token,
        }))
        .send()
        .await
        .context("plaid accounts/get failed")?
        .error_for_status()
        .context("plaid non-2xx")?
        .json()
        .await
        .context("plaid response non-JSON")?;

    let accounts = resp
        .get("accounts")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let storage = lake::storage_from_env()?;
    lake::archive_cloud(&pool, &storage, "plaid", ACTION, "accounts", &[resp.clone()]).await?;

    let written = transform::write_accounts(&pool, item_id, institution, &accounts).await?;
    let summary = format!("synced {written} Plaid accounts");
    output(&summary, &input.config)
}
