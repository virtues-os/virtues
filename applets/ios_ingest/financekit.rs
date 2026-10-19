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
    Option<String>, // merchant_category (from the MCC; Plaid's vocabulary)
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
            let merchant_category = tx
                .get("merchantCategoryCode")
                .and_then(|v| v.as_i64())
                .and_then(mcc_category)
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
                merchant_category,
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

/// A merchant category code (ISO 18245) as the primary category Plaid's
/// `personal_finance_category` uses, so `merchant_category` reads the same
/// across providers. Only codes whose meaning is unambiguous are mapped; any
/// other code leaves the category NULL, and the code itself stays in
/// `financekit_raw`.
fn mcc_category(code: i64) -> Option<&'static str> {
    Some(match code {
        3000..=3299 | 3351..=3441 | 3501..=3999 | 4411 | 4511 | 4722 | 7011..=7033 | 7512 => {
            "TRAVEL"
        }
        4011 | 4111 | 4112 | 4121 | 4131 | 4784 | 4789 | 5541 | 5542 | 5983 | 7523 => {
            "TRANSPORTATION"
        }
        4814 | 4899 | 4900 => "RENT_AND_UTILITIES",
        4829 | 6010 | 6011 => "TRANSFER_OUT",
        5200..=5299 | 5712..=5719 => "HOME_IMPROVEMENT",
        5411 | 5422 | 5441 | 5451 | 5462 | 5499 | 5811..=5814 => "FOOD_AND_DRINK",
        5912 | 5975 | 5976 | 8011..=8099 => "MEDICAL",
        7210..=7299 | 7997 => "PERSONAL_CARE",
        5815 | 5816 | 7832 | 7841 | 7900..=7999 => "ENTERTAINMENT",
        5300..=5399 | 5600..=5699 | 5720..=5799 | 5900..=5999 => "GENERAL_MERCHANDISE",
        6300 | 7311..=7399 | 7531..=7549 | 8111 | 8211..=8299 | 8931 => "GENERAL_SERVICES",
        8398 | 8641 | 8651 | 8661 | 9211..=9405 => "GOVERNMENT_AND_NON_PROFIT",
        _ => return None,
    })
}

async fn flush_transactions(db: &PgPool, records: &[TransactionRow]) -> Result<usize> {
    // Collapse repeats of the conflict key (id) within the batch — FinanceKit ships a
    // transaction as both pending and posted — or the ON CONFLICT DO UPDATE aborts.
    let records = dedup_refs_keep_last(records, |r| &r.0);
    let query_str = format!(
        "INSERT INTO data_financial_transaction (
            id, account_id, transaction_id, amount, transaction_type, merchant_name,
            merchant_category, category, description, is_pending, occurred_at, source_stream_id,
            source_table, source_provider, metadata
        ) VALUES {}
        ON CONFLICT (id) DO UPDATE SET
            amount = EXCLUDED.amount,
            transaction_type = EXCLUDED.transaction_type,
            merchant_name = EXCLUDED.merchant_name,
            merchant_category = EXCLUDED.merchant_category,
            category = EXCLUDED.category,
            description = EXCLUDED.description,
            is_pending = EXCLUDED.is_pending,
            metadata = EXCLUDED.metadata,
            updated_at = now()",
        (0..records.len())
            .map(|i| format!(
                "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, 'stream_ios_financekit', 'apple_finance', ${})",
                i * 13 + 1,
                i * 13 + 2,
                i * 13 + 3,
                i * 13 + 4,
                i * 13 + 5,
                i * 13 + 6,
                i * 13 + 7,
                i * 13 + 8,
                i * 13 + 9,
                i * 13 + 10,
                i * 13 + 11,
                i * 13 + 12,
                i * 13 + 13
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
        merchant_category,
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
            .bind(merchant_category)
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

    /// The re-sync is what signs history: a row an older phone wrote unsigned
    /// and untyped must come back signed and typed when a newer phone re-sends
    /// it, both in the same upsert.
    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn a_resync_signs_a_row_an_older_phone_wrote(pool: PgPool) {
        let tx = |extra: Value| {
            let mut t = json!({"id": "fk-tx-1", "accountId": "fk-acct-1", "amount": 25.0,
                               "date": "2026-09-23T15:00:00Z", "status": "booked",
                               "description": "Deposit"});
            t.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            json!({"transactions": [t]})
        };
        let accounts = json!({"accounts": [{"id": "fk-acct-1", "name": "Everyday Checking"}]});
        write_accounts(&pool, &[accounts]).await.unwrap();

        let read = || async {
            sqlx::query_as::<_, (i64, Option<String>)>(
                "SELECT amount, transaction_type FROM data_financial_transaction",
            )
            .fetch_one(&pool)
            .await
            .unwrap()
        };

        write_transactions(&pool, &[tx(json!({}))]).await.unwrap();
        assert_eq!(read().await, (2500, None));

        let signed = tx(json!({"creditDebitIndicator": "credit", "transactionType": "deposit"}));
        write_transactions(&pool, &[signed]).await.unwrap();
        assert_eq!(read().await, (-2500, Some("deposit".into())));
    }

    /// Specific codes win over the ranges they sit in (a gym inside the
    /// entertainment block, a pharmacy inside retail), and an unmapped code
    /// is no category rather than a guess.
    #[test]
    fn merchant_category_codes_map_to_plaids_vocabulary() {
        assert_eq!(mcc_category(5812), Some("FOOD_AND_DRINK"));
        assert_eq!(mcc_category(5411), Some("FOOD_AND_DRINK"));
        assert_eq!(mcc_category(5912), Some("MEDICAL"));
        assert_eq!(mcc_category(5999), Some("GENERAL_MERCHANDISE"));
        assert_eq!(mcc_category(7997), Some("PERSONAL_CARE"));
        assert_eq!(mcc_category(7996), Some("ENTERTAINMENT"));
        assert_eq!(mcc_category(3058), Some("TRAVEL"));
        assert_eq!(mcc_category(5542), Some("TRANSPORTATION"));
        assert_eq!(mcc_category(5817), None);
        assert_eq!(mcc_category(1520), None);
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
