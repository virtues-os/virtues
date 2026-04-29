//! Plaid transactions sync.
//!
//! Cron-driven, per-credential. Uses Plaid's `/transactions/sync` endpoint
//! which is incremental — we pass the cursor from our last sync, Plaid returns
//! `added`, `modified`, `removed`, and a new cursor.
//!
//! Cursor stored in `app_actions.config.plaid_cursor`. First run: empty string
//! (Plaid interprets as "give me everything").

mod transform;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use virtues_helpers::{connect_from_env, output, read_input};

const PLAID_TX_SYNC: &str = "https://production.plaid.com/transactions/sync";
const MAX_CHUNKS: u32 = 20;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let mut input = read_input()?;
    let pool = connect_from_env().await?;

    let access_token = input
        .credentials
        .as_ref()
        .and_then(|c| c.get("secrets"))
        .and_then(|s| s.get("access_token"))
        .and_then(|v| v.as_str())
        .context("plaid credentials missing secrets.access_token")?
        .to_string();

    let client_id = std::env::var("PLAID_CLIENT_ID").context("PLAID_CLIENT_ID not set")?;
    let secret = std::env::var("PLAID_SECRET").context("PLAID_SECRET not set")?;

    let mut cursor = input
        .config
        .get("plaid_cursor")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let client = reqwest::Client::new();
    let mut total_written = 0usize;

    for _ in 0..MAX_CHUNKS {
        let resp: Value = client
            .post(PLAID_TX_SYNC)
            .json(&json!({
                "client_id": client_id,
                "secret": secret,
                "access_token": access_token,
                "cursor": cursor,
            }))
            .send()
            .await
            .context("plaid /transactions/sync failed")?
            .error_for_status()
            .context("plaid non-2xx")?
            .json()
            .await
            .context("plaid non-JSON")?;

        let added = resp
            .get("added")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        // For now, treat "modified" as upserts via the same insert path.
        let modified = resp
            .get("modified")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut combined = added;
        combined.extend(modified);

        let written = transform::write_transactions(&pool, &combined).await?;
        total_written += written;

        if let Some(next) = resp.get("next_cursor").and_then(|v| v.as_str()) {
            cursor = next.to_string();
        }
        let has_more = resp.get("has_more").and_then(|v| v.as_bool()).unwrap_or(false);
        if !has_more {
            break;
        }
    }

    input.config["plaid_cursor"] = Value::String(cursor);
    let summary = format!("synced {total_written} Plaid transactions");
    output(&summary, &input.config)
}
