//! iOS FinanceKit → financial ontology transforms.
//!
//! Ported from `core/src/sources/ios/financekit/transform.rs`. The iOS app sends
//! wrapper records containing `accounts[]` and `transactions[]` arrays. Each array
//! is flattened and inserted into the respective ontology table.
//!
//! Uses deterministic UUIDv5 IDs keyed on Apple's finance account/transaction IDs
//! so upserts are idempotent. Amounts are stored as cents (integer) to avoid
//! floating-point precision issues.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::SqlitePool;
use uuid::Uuid;
use virtues_action_helpers::dedup::BATCH_SIZE;

// ─────────────────────────────────────────────────────────────────────────────
// Accounts
// ─────────────────────────────────────────────────────────────────────────────

#[allow(clippy::type_complexity)]
type AccountRow = (
    String, // id (deterministic UUIDv5)
    String, // account_name
    String, // account_type
    String, // institution_name
    i64,    // current_balance (cents)
    String, // currency
    String, // source_stream_id (apple_id)
    Value,  // metadata
);

pub async fn write_accounts(db: &SqlitePool, wrapper_records: &[Value]) -> Result<usize> {
    let mut pending: Vec<AccountRow> = Vec::new();
    let mut written = 0;

    for wrapper in wrapper_records {
        let Some(accounts) = wrapper.get("accounts").and_then(|v| v.as_array()) else {
            continue;
        };

        for account in accounts {
            let apple_id = account.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if apple_id.is_empty() {
                continue;
            }

            let name = account
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Apple Account");
            let institution = account
                .get("institutionName")
                .and_then(|v| v.as_str())
                .unwrap_or("Apple");
            let acct_type = account.get("type").and_then(|v| v.as_str()).unwrap_or("other");
            let balance = account
                .get("currentBalance")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let currency = account
                .get("currencyCode")
                .and_then(|v| v.as_str())
                .unwrap_or("USD");

            let internal_id = Uuid::new_v5(
                &Uuid::NAMESPACE_DNS,
                format!("apple_finance_account:{}", apple_id).as_bytes(),
            );

            let metadata = serde_json::json!({
                "apple_account_id": apple_id,
                "raw": account,
            });

            pending.push((
                internal_id.to_string(),
                name.to_string(),
                acct_type.to_string(),
                institution.to_string(),
                (balance * 100.0) as i64,
                currency.to_string(),
                apple_id.to_string(),
                metadata,
            ));

            if pending.len() >= BATCH_SIZE {
                written += flush_accounts(db, &pending).await?;
                pending.clear();
            }
        }
    }

    if !pending.is_empty() {
        written += flush_accounts(db, &pending).await?;
    }

    Ok(written)
}

async fn flush_accounts(db: &SqlitePool, records: &[AccountRow]) -> Result<usize> {
    // FinanceKit uses UPSERT (not INSERT OR IGNORE) because balances update frequently.
    let query_str = format!(
        "INSERT INTO data_financial_account (
            id, account_name, account_type, institution_name, current_balance, currency,
            source_stream_id, source_table, source_provider, metadata
        ) VALUES {}
        ON CONFLICT (id) DO UPDATE SET
            account_name = EXCLUDED.account_name,
            current_balance = EXCLUDED.current_balance,
            metadata = EXCLUDED.metadata,
            updated_at = datetime('now')",
        (0..records.len())
            .map(|i| format!(
                "(${}, ${}, ${}, ${}, ${}, ${}, ${}, 'stream_ios_financekit', 'apple_finance', ${})",
                i * 8 + 1,
                i * 8 + 2,
                i * 8 + 3,
                i * 8 + 4,
                i * 8 + 5,
                i * 8 + 6,
                i * 8 + 7,
                i * 8 + 8
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let mut query = sqlx::query(&query_str);
    for (id, name, acct_type, inst, balance, currency, stream_id, meta) in records {
        query = query
            .bind(id)
            .bind(name)
            .bind(acct_type)
            .bind(inst)
            .bind(balance)
            .bind(currency)
            .bind(stream_id)
            .bind(meta);
    }

    let result = query.execute(db).await?;
    Ok(result.rows_affected() as usize)
}

// ─────────────────────────────────────────────────────────────────────────────
// Transactions
// ─────────────────────────────────────────────────────────────────────────────

#[allow(clippy::type_complexity)]
type TransactionRow = (
    String,         // id (deterministic UUIDv5)
    String,         // account_id (deterministic UUIDv5)
    String,         // transaction_id (apple_id)
    i64,            // amount (cents)
    Option<String>, // merchant_name
    Option<String>, // category
    Option<String>, // description
    i32,            // is_pending
    DateTime<Utc>,  // timestamp
    String,         // source_stream_id (apple_id)
    Value,          // metadata
);

pub async fn write_transactions(db: &SqlitePool, wrapper_records: &[Value]) -> Result<usize> {
    let mut pending: Vec<TransactionRow> = Vec::new();
    let mut written = 0;

    for wrapper in wrapper_records {
        let Some(transactions) = wrapper.get("transactions").and_then(|v| v.as_array()) else {
            continue;
        };

        for tx in transactions {
            let apple_id = tx.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if apple_id.is_empty() {
                continue;
            }

            let amount = tx.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let timestamp = tx
                .get("date")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                .unwrap_or_else(Utc::now);

            let apple_account_id = tx.get("accountId").and_then(|v| v.as_str()).unwrap_or("");
            let merchant_name = tx
                .get("merchantName")
                .and_then(|v| v.as_str())
                .map(String::from);
            let category = tx.get("category").and_then(|v| v.as_str()).map(String::from);
            let status = tx.get("status").and_then(|v| v.as_str()).unwrap_or("posted");
            let description = tx
                .get("description")
                .and_then(|v| v.as_str())
                .map(String::from);

            let internal_account_id = Uuid::new_v5(
                &Uuid::NAMESPACE_DNS,
                format!("apple_finance_account:{}", apple_account_id).as_bytes(),
            );
            let internal_tx_id = Uuid::new_v5(
                &Uuid::NAMESPACE_DNS,
                format!("apple_finance:{}", apple_id).as_bytes(),
            );

            let metadata = serde_json::json!({
                "financekit_raw": tx,
                "apple_transaction_id": apple_id,
            });

            pending.push((
                internal_tx_id.to_string(),
                internal_account_id.to_string(),
                apple_id.to_string(),
                (amount * 100.0) as i64,
                merchant_name,
                category,
                description,
                if status == "pending" { 1 } else { 0 },
                timestamp,
                apple_id.to_string(),
                metadata,
            ));

            if pending.len() >= BATCH_SIZE {
                written += flush_transactions(db, &pending).await?;
                pending.clear();
            }
        }
    }

    if !pending.is_empty() {
        written += flush_transactions(db, &pending).await?;
    }

    Ok(written)
}

async fn flush_transactions(db: &SqlitePool, records: &[TransactionRow]) -> Result<usize> {
    let query_str = format!(
        "INSERT INTO data_financial_transaction (
            id, account_id, transaction_id, amount, merchant_name, category, description,
            is_pending, timestamp, source_stream_id, source_table, source_provider, metadata
        ) VALUES {}
        ON CONFLICT (id) DO UPDATE SET
            amount = EXCLUDED.amount,
            merchant_name = EXCLUDED.merchant_name,
            category = EXCLUDED.category,
            description = EXCLUDED.description,
            is_pending = EXCLUDED.is_pending,
            metadata = EXCLUDED.metadata,
            updated_at = datetime('now')",
        (0..records.len())
            .map(|i| format!(
                "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, 'stream_ios_financekit', 'apple_finance', ${})",
                i * 11 + 1,
                i * 11 + 2,
                i * 11 + 3,
                i * 11 + 4,
                i * 11 + 5,
                i * 11 + 6,
                i * 11 + 7,
                i * 11 + 8,
                i * 11 + 9,
                i * 11 + 10,
                i * 11 + 11
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let mut query = sqlx::query(&query_str);
    for (id, account_id, transaction_id, amount, merchant, cat, desc, pending, ts, stream_id, meta) in records {
        query = query
            .bind(id)
            .bind(account_id)
            .bind(transaction_id)
            .bind(amount)
            .bind(merchant)
            .bind(cat)
            .bind(desc)
            .bind(pending)
            .bind(ts)
            .bind(stream_id)
            .bind(meta);
    }

    let result = query.execute(db).await?;
    Ok(result.rows_affected() as usize)
}
