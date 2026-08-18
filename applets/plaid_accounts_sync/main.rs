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

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

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
    // Stamped on every account row, so it is the label the user actually reads.
    // The connect flow resolves it from the Link session (falling back to
    // /item/get + /institutions/get_by_id) and stores it on the credential;
    // "Unknown" now means that lookup genuinely failed, not that nobody ever
    // wrote the field.
    let institution = creds
        .get("metadata")
        .and_then(|m| m.get("institution_name"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("Unknown");

    // Proxied through virtues-api: the box sends only the per-user access_token;
    // the master Plaid secret stays server-side.
    let resp: Value = virtues_applets::service_proxy(
        &pool,
        "plaid",
        "accounts/get",
        &json!({ "access_token": access_token }),
    )
    .await?;

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
