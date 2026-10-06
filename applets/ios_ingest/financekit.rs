//! iOS FinanceKit → financial ontology transforms.
//!
//! Ported from `core/src/sources/ios/financekit/transform.rs`. The iOS app sends
//! wrapper records containing `accounts[]` and `transactions[]` arrays. Each array
//! is flattened and inserted into the respective ontology table.
//!
//! Uses deterministic UUIDv5 IDs keyed on Apple's finance account/transaction IDs
//! so upserts are idempotent. Amounts are stored as cents (integer) to avoid
//! floating-point precision issues, signed the way the table signs every
//! provider: positive is money leaving. See `signed_cents`.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use virtues_helpers::dedup::{dedup_refs_keep_last, BATCH_SIZE};

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

pub async fn write_accounts(db: &PgPool, wrapper_records: &[Value]) -> Result<usize> {
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
            let acct_type = account
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("other");
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
                (balance * 100.0).round() as i64,
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

async fn flush_accounts(db: &PgPool, records: &[AccountRow]) -> Result<usize> {
    // FinanceKit uses UPSERT (not INSERT OR IGNORE) because balances update frequently.
    // Collapse repeats of the conflict key (id) within the batch, or the whole
    // ON CONFLICT DO UPDATE aborts — FinanceKit re-sends an account in one payload.
    let records = dedup_refs_keep_last(records, |r| &r.0);
    let query_str = format!(
        "INSERT INTO data_financial_account (
            id, account_name, account_type, institution_name, current_balance, currency,
            source_stream_id, source_table, source_provider, metadata
        ) VALUES {}
        ON CONFLICT (id) DO UPDATE SET
            account_name = EXCLUDED.account_name,
            current_balance = EXCLUDED.current_balance,
            metadata = EXCLUDED.metadata,
            updated_at = now()",
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
    i64,            // amount (cents, positive = money leaving)
    Option<String>, // transaction_type (FinanceKit's; NULL = direction unknown)
    Option<String>, // merchant_name
    Value,          // category (jsonb array)
    Option<String>, // description
    bool,           // is_pending
    DateTime<Utc>,  // timestamp
    String,         // source_stream_id (apple_id)
    Value,          // metadata
);

pub async fn write_transactions(db: &PgPool, wrapper_records: &[Value]) -> Result<usize> {
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

            let (amount_cents, transaction_type) = signed_cents(tx);
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
            // `category` is a JSONB array column. iOS FinanceKit doesn't send a
            // category, so default to `[]`; wrap a bare string in an array if present.
            // (Binding an Option<String> here failed: TEXT/NULL vs `NOT NULL JSONB`.)
            let category = tx
                .get("category")
                .and_then(|v| v.as_str())
                .map(|s| serde_json::json!([s]))
                .unwrap_or_else(|| serde_json::json!([]));
            let status = tx
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("posted");
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
                amount_cents,
                transaction_type,
                merchant_name,
                category,
                description,
                status == "pending",
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

/// Cents signed the way the table signs every provider (positive = money
/// leaving), and FinanceKit's own name for what the transaction was.
///
/// FinanceKit reports `amount` as a magnitude and the direction separately, as
/// `creditDebitIndicator`. A phone that predates sending it gets the amount as
/// received and no type: a NULL `transaction_type` on an `apple_finance` row
/// is what tells a reader its direction is unknown, so the type is only ever
/// written beside a sign. The phone re-sends its history on every sync, and
/// the upsert overwrites both together.
fn signed_cents(tx: &Value) -> (i64, Option<String>) {
    let amount = tx.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let sign = match tx.get("creditDebitIndicator").and_then(|v| v.as_str()) {
        Some("debit") => 1,
        Some("credit") => -1,
        _ => return ((amount * 100.0).round() as i64, None),
    };
    // `unknown` is FinanceKit's own case name, for a phone that sent a
    // direction without a type.
    let kind = tx
        .get("transactionType")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    (sign * (amount.abs() * 100.0).round() as i64, Some(kind.to_string()))
}

async fn flush_transactions(db: &PgPool, records: &[TransactionRow]) -> Result<usize> {
    // Collapse repeats of the conflict key (id) within the batch — FinanceKit ships a
    // transaction as both pending and posted — or the ON CONFLICT DO UPDATE aborts.
    let records = dedup_refs_keep_last(records, |r| &r.0);
    let query_str = format!(
        "INSERT INTO data_financial_transaction (
            id, account_id, transaction_id, amount, transaction_type, merchant_name, category,
            description, is_pending, occurred_at, source_stream_id, source_table, source_provider,
            metadata
        ) VALUES {}
        ON CONFLICT (id) DO UPDATE SET
            amount = EXCLUDED.amount,
            transaction_type = EXCLUDED.transaction_type,
            merchant_name = EXCLUDED.merchant_name,
            category = EXCLUDED.category,
            description = EXCLUDED.description,
            is_pending = EXCLUDED.is_pending,
            metadata = EXCLUDED.metadata,
            updated_at = now()",
        (0..records.len())
            .map(|i| format!(
                "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, 'stream_ios_financekit', 'apple_finance', ${})",
                i * 12 + 1,
                i * 12 + 2,
                i * 12 + 3,
                i * 12 + 4,
                i * 12 + 5,
                i * 12 + 6,
                i * 12 + 7,
                i * 12 + 8,
                i * 12 + 9,
                i * 12 + 10,
                i * 12 + 11,
                i * 12 + 12
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let mut query = sqlx::query(&query_str);
    for (
        id,
        account_id,
        transaction_id,
        amount,
        transaction_type,
        merchant,
        cat,
        desc,
        pending,
        ts,
        stream_id,
        meta,
    ) in records
    {
        query = query
            .bind(id)
            .bind(account_id)
            .bind(transaction_id)
            .bind(amount)
            .bind(transaction_type)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The sign is the whole point: an unsigned deposit reads as spending.
    #[test]
    fn credits_are_negative_and_debits_positive() {
        let purchase = json!({"amount": 12.5, "creditDebitIndicator": "debit",
                              "transactionType": "pointOfSale"});
        assert_eq!(signed_cents(&purchase), (1250, Some("pointOfSale".into())));

        let deposit = json!({"amount": 3.0, "creditDebitIndicator": "credit",
                             "transactionType": "deposit"});
        assert_eq!(signed_cents(&deposit), (-300, Some("deposit".into())));
    }

    /// FinanceKit sends a magnitude, but the direction must come from the
    /// indicator alone, never from a sign that happens to be on the number.
    #[test]
    fn the_indicator_decides_even_if_the_amount_is_signed() {
        let tx = json!({"amount": -40.0, "creditDebitIndicator": "debit",
                        "transactionType": "fee"});
        assert_eq!(signed_cents(&tx).0, 4000);
    }

    /// An older phone sends no indicator. Its direction is unknown, and the
    /// NULL type is what marks that; it must not get a type without a sign.
    #[test]
    fn no_indicator_means_no_type() {
        let legacy = json!({"amount": 7.25, "description": "Deposit"});
        assert_eq!(signed_cents(&legacy), (725, None));

        let unrecognized = json!({"amount": 7.25, "creditDebitIndicator": "sideways",
                                  "transactionType": "deposit"});
        assert_eq!(signed_cents(&unrecognized), (725, None));
    }

    /// `(0.29 * 100.0) as i64` is 28. Truncation lost a cent on about one
    /// FinanceKit row in eleven.
    #[test]
    fn cents_round_rather_than_truncate() {
        let tx = json!({"amount": 0.29, "creditDebitIndicator": "debit",
                        "transactionType": "pointOfSale"});
        assert_eq!(signed_cents(&tx).0, 29);
        assert_eq!(signed_cents(&json!({"amount": 0.29})).0, 29);
    }
}
