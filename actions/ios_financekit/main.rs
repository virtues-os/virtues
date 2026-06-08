//! iOS FinanceKit action.
//!
//! Receives account and transaction batches from the iPhone via `/ingest`.
//! Writes to `data_financial_account` and `data_financial_transaction`.
//! Each push payload can contain both `accounts[]` and `transactions[]` inside
//! one wrapper record.

mod transform;

use anyhow::Result;
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_actions::init_tracing();

    let input = read_input()?;
    let db = connect_from_env("virtues-action-ios_financekit").await?;

    let records = input
        .payload
        .as_ref()
        .and_then(|p| {
            p.get("records")
                .and_then(|r| r.as_array())
                .or_else(|| p.as_array())
        })
        .ok_or_else(|| anyhow::anyhow!("ios_financekit requires `records` array in payload"))?;

    let accounts_written = transform::write_accounts(&db, records).await?;
    let transactions_written = transform::write_transactions(&db, records).await?;

    let summary = format!(
        "accounts: {}, transactions: {}",
        accounts_written, transactions_written
    );

    output(&summary, &input.config)?;
    Ok(())
}
