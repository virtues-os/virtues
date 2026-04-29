//! Plaid accounts → `data_financial_account` transform.
//!
//! Adapted from `core/src/sources/plaid/accounts/transform.rs`.

use anyhow::Result;
use serde_json::Value;
use sqlx::SqlitePool;
use uuid::Uuid;
use virtues_helpers::dedup::{build_batch_insert_query, BATCH_SIZE};

#[allow(clippy::type_complexity)]
type AccountRow = (
    String,         // id
    String,         // account_name
    String,         // account_type
    String,         // institution_name
    i64,            // current_balance (cents)
    Option<i64>,    // available_balance (cents)
    String,         // currency
    Option<String>, // mask
    String,         // source_stream_id
    Value,          // metadata
);

/// `accounts` is the array from Plaid's `/accounts/get` response.
pub async fn write_accounts(
    db: &SqlitePool,
    item_id: &str,
    institution: &str,
    accounts: &[Value],
) -> Result<usize> {
    let mut pending: Vec<AccountRow> = Vec::new();
    let mut written = 0;

    for acct in accounts {
        let plaid_id = acct.get("account_id").and_then(|v| v.as_str()).unwrap_or("");
        if plaid_id.is_empty() {
            continue;
        }
        let name = acct
            .get("official_name")
            .or_else(|| acct.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unnamed Account")
            .to_string();
        let acct_type = acct
            .get("subtype")
            .or_else(|| acct.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("other")
            .to_string();
        let mask = acct.get("mask").and_then(|v| v.as_str()).map(String::from);

        let balances = acct.get("balances");
        let current_dollars = balances
            .and_then(|b| b.get("current"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let available_dollars = balances
            .and_then(|b| b.get("available"))
            .and_then(|v| v.as_f64());
        let currency = balances
            .and_then(|b| b.get("iso_currency_code"))
            .and_then(|v| v.as_str())
            .unwrap_or("USD")
            .to_string();

        let current_cents = (current_dollars * 100.0).round() as i64;
        let available_cents = available_dollars.map(|d| (d * 100.0).round() as i64);

        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("plaid:account:{plaid_id}").as_bytes(),
        )
        .to_string();

        let metadata = serde_json::json!({
            "plaid_account_id": plaid_id,
            "plaid_item_id": item_id,
            "plaid_type": acct.get("type"),
            "plaid_subtype": acct.get("subtype"),
            "limit": balances.and_then(|b| b.get("limit")),
        });

        pending.push((
            id,
            name,
            acct_type,
            institution.to_string(),
            current_cents,
            available_cents,
            currency,
            mask,
            plaid_id.to_string(),
            metadata,
        ));

        if pending.len() >= BATCH_SIZE {
            written += flush(db, &pending).await?;
            pending.clear();
        }
    }
    if !pending.is_empty() {
        written += flush(db, &pending).await?;
    }
    Ok(written)
}

async fn flush(db: &SqlitePool, records: &[AccountRow]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }
    let sql = build_batch_insert_query(
        "data_financial_account",
        &[
            "id",
            "account_name",
            "account_type",
            "institution_name",
            "current_balance",
            "available_balance",
            "currency",
            "mask",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        records.len(),
    );
    let mut q = sqlx::query(&sql);
    for r in records {
        q = q
            .bind(&r.0)
            .bind(&r.1)
            .bind(&r.2)
            .bind(&r.3)
            .bind(r.4)
            .bind(r.5)
            .bind(&r.6)
            .bind(&r.7)
            .bind(&r.8)
            .bind("plaid_accounts")
            .bind("plaid")
            .bind(&r.9);
    }
    Ok(q.execute(db).await?.rows_affected() as usize)
}
