//! Plaid transactions → `data_financial_transaction`.
//!
//! Adapted from `core/src/sources/plaid/transactions/transform.rs`.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use virtues_helpers::dedup::{build_batch_insert_query, BATCH_SIZE};

#[allow(clippy::type_complexity)]
type TxRow = (
    String,         // id
    String,         // account_id (FK = our local account row id)
    String,         // transaction_id (Plaid's id)
    i64,            // amount cents (positive=expense, negative=credit per Plaid convention)
    String,         // currency
    Option<String>, // merchant_name
    Option<String>, // merchant_category
    Option<String>, // description
    Value,          // category (JSONB)
    bool,           // is_pending
    Option<String>, // transaction_type
    Option<String>, // payment_channel
    DateTime<Utc>,  // timestamp
    Option<DateTime<Utc>>, // authorized_timestamp
    String,         // source_stream_id
    Value,          // metadata
);

/// `transactions` is the Plaid `/transactions/sync` `added` array.
/// `account_id_map` maps Plaid's `account_id` to our deterministic
/// `data_financial_account.id` (computed via `Uuid::v5(NAMESPACE_OID, "plaid:account:{plaid_id}")`).
pub async fn write_transactions(
    db: &PgPool,
    transactions: &[Value],
) -> Result<usize> {
    let mut pending: Vec<TxRow> = Vec::new();
    let mut written = 0;

    for tx in transactions {
        let plaid_tx_id = tx
            .get("transaction_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if plaid_tx_id.is_empty() {
            continue;
        }
        let plaid_account_id = tx
            .get("account_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if plaid_account_id.is_empty() {
            continue;
        }

        let amount_dollars = tx.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let amount_cents = (amount_dollars * 100.0).round() as i64;

        let currency = tx
            .get("iso_currency_code")
            .and_then(|v| v.as_str())
            .unwrap_or("USD")
            .to_string();

        let merchant_name = tx
            .get("merchant_name")
            .and_then(|v| v.as_str())
            .map(String::from);
        let merchant_category = tx
            .get("personal_finance_category")
            .and_then(|c| c.get("primary"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let description = tx.get("name").and_then(|v| v.as_str()).map(String::from);

        let categories: Vec<String> = tx
            .get("category")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let is_pending = tx
            .get("pending")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let transaction_type = tx
            .get("transaction_type")
            .and_then(|v| v.as_str())
            .map(String::from);
        let payment_channel = tx
            .get("payment_channel")
            .and_then(|v| v.as_str())
            .map(String::from);

        let timestamp = tx
            .get("date")
            .and_then(|v| v.as_str())
            .map(|s| format!("{s}T00:00:00Z"))
            .and_then(|s| s.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now);

        let authorized_timestamp = tx
            .get("authorized_date")
            .and_then(|v| v.as_str())
            .map(|s| format!("{s}T00:00:00Z"))
            .and_then(|s| s.parse::<DateTime<Utc>>().ok());

        let metadata = serde_json::json!({
            "plaid_transaction_id": plaid_tx_id,
            "plaid_account_id": plaid_account_id,
            "personal_finance_category": tx.get("personal_finance_category"),
            "location": tx.get("location"),
            "logo_url": tx.get("logo_url"),
        });

        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("plaid:tx:{plaid_tx_id}").as_bytes(),
        )
        .to_string();
        let account_local_id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("plaid:account:{plaid_account_id}").as_bytes(),
        )
        .to_string();

        pending.push((
            id,
            account_local_id,
            plaid_tx_id.to_string(),
            amount_cents,
            currency,
            merchant_name,
            merchant_category,
            description,
            serde_json::json!(categories),
            is_pending,
            transaction_type,
            payment_channel,
            timestamp,
            authorized_timestamp,
            plaid_tx_id.to_string(),
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

async fn flush(db: &PgPool, records: &[TxRow]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }
    let sql = build_batch_insert_query(
        "data_financial_transaction",
        &[
            "id",
            "account_id",
            "transaction_id",
            "amount",
            "currency",
            "merchant_name",
            "merchant_category",
            "description",
            "category",
            "is_pending",
            "transaction_type",
            "payment_channel",
            "timestamp",
            "authorized_timestamp",
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
            .bind(r.3)
            .bind(&r.4)
            .bind(&r.5)
            .bind(&r.6)
            .bind(&r.7)
            .bind(&r.8)
            .bind(r.9)
            .bind(&r.10)
            .bind(&r.11)
            .bind(r.12)
            .bind(r.13)
            .bind(&r.14)
            .bind("plaid_transactions")
            .bind("plaid")
            .bind(&r.15);
    }
    Ok(q.execute(db).await?.rows_affected() as usize)
}
