//! Plaid transactions → `data_financial_transaction`.
//!
//! Adapted from `core/src/sources/plaid/transactions/transform.rs`.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use virtues_helpers::dedup::{build_batch_upsert_query, dedup_refs_keep_last, BATCH_SIZE};

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
    // `/transactions/sync` can return the same txn in both `added` and `modified`,
    // colliding on the conflict key (source_stream_id, r.14). Keep the last so the
    // ON CONFLICT DO UPDATE doesn't abort ("cannot affect row a second time").
    let records = dedup_refs_keep_last(records, |r| &r.14);
    let columns: &[&str] = &[
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
        "occurred_at",
        "authorized_timestamp",
        "source_stream_id",
        "source_table",
        "source_provider",
        "metadata",
    ];
    // UPSERT. Plaid's `/transactions/sync` returns `added` AND `modified`, and both
    // arrive here — `modified` being Plaid correcting something it already told us.
    // Under ON CONFLICT DO NOTHING every one of those corrections was discarded on
    // arrival, so a pending $50 pre-auth that settled at $43.17 kept the $50, and kept
    // its pending flag, forever. The correction is only visible in a diff Plaid will
    // never send again (the cursor is consumed), so this is not a "fix it on the next
    // sync" situation — it is the only chance.
    //
    // Every listed column is a pure function of the Plaid transaction object, so
    // re-deriving one can only make an existing row more right. `id`, `transaction_id`
    // and `source_stream_id` are identity and stay out of it.
    let sql = build_batch_upsert_query(
        "data_financial_transaction",
        columns,
        "source_stream_id",
        &[
            "amount",
            "currency",
            "merchant_name",
            "merchant_category",
            "description",
            "category",
            "is_pending",
            "transaction_type",
            "payment_channel",
            "occurred_at",
            "authorized_timestamp",
            "metadata",
        ],
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
    Ok(q.fetch_all(db).await?.len())
}

/// Mark transactions Plaid says are gone.
///
/// `/transactions/sync` returns a `removed` list — a transaction was reversed, voided,
/// or deleted at the bank. Nothing read it. The message was discarded on arrival and
/// the transaction stayed in the user's finances permanently, which means a reversed
/// charge quietly kept counting against them.
///
/// A tombstone, not a DELETE. "The bank took this back" is itself a fact worth keeping:
/// deleting the row would leave a hole where a transaction used to be, and no way to
/// tell that hole apart from one we never saw. `deleted_at_source` already exists on
/// every ontology table for exactly this.
///
/// Plaid sends only `{"transaction_id": "..."}` here, so the join is on that.
pub async fn tombstone_removed(db: &PgPool, removed: &[Value]) -> Result<usize> {
    let ids: Vec<String> = removed
        .iter()
        .filter_map(|r| r.get("transaction_id").and_then(|v| v.as_str()))
        .map(String::from)
        .collect();
    if ids.is_empty() {
        return Ok(0);
    }

    let affected = sqlx::query(
        "UPDATE data_financial_transaction
         SET deleted_at_source = now(), updated_at = now()
         WHERE transaction_id = ANY($1) AND deleted_at_source IS NULL",
    )
    .bind(&ids)
    .execute(db)
    .await?
    .rows_affected();

    if affected > 0 {
        tracing::info!(
            removed = affected,
            "Plaid reported transactions removed upstream — tombstoned"
        );
    }
    Ok(affected as usize)
}
