//! Plaid investments → `data_financial_asset`.
//!
//! Adapted from `core/src/sources/plaid/investments/transform.rs`. Each holding
//! becomes one row keyed by (account_id, security_id).

use anyhow::Result;
use chrono::Utc;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use virtues_helpers::dedup::{build_batch_insert_query, BATCH_SIZE};

#[allow(clippy::type_complexity)]
type AssetRow = (
    String,         // id
    String,         // account_id (local data_financial_account.id)
    String,         // asset_type
    Option<String>, // symbol
    Option<String>, // name
    Option<f64>,    // quantity
    Option<i64>,    // cost_basis (cents)
    Option<i64>,    // current_value (cents)
    String,         // currency
    String,         // timestamp ISO
    String,         // source_stream_id
    Value,          // metadata
);

/// Plaid `/investments/holdings/get` returns `holdings` + `securities` arrays.
/// Securities are joined into holdings by `security_id` to enrich symbol/name.
pub async fn write_holdings(
    db: &PgPool,
    holdings: &[Value],
    securities: &[Value],
) -> Result<usize> {
    // Index securities by security_id for fast join.
    let mut sec_by_id: std::collections::HashMap<&str, &Value> = std::collections::HashMap::new();
    for s in securities {
        if let Some(id) = s.get("security_id").and_then(|v| v.as_str()) {
            sec_by_id.insert(id, s);
        }
    }

    let now = Utc::now();
    let mut pending: Vec<AssetRow> = Vec::new();
    let mut written = 0;

    for h in holdings {
        let plaid_account = h.get("account_id").and_then(|v| v.as_str()).unwrap_or("");
        let plaid_security = h.get("security_id").and_then(|v| v.as_str()).unwrap_or("");
        if plaid_account.is_empty() || plaid_security.is_empty() {
            continue;
        }

        let security = sec_by_id.get(plaid_security);
        let symbol = security
            .and_then(|s| s.get("ticker_symbol"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let name = security
            .and_then(|s| s.get("name"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let asset_type = security
            .and_then(|s| s.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("equity")
            .to_string();
        let currency = h
            .get("iso_currency_code")
            .and_then(|v| v.as_str())
            .unwrap_or("USD")
            .to_string();

        let quantity = h.get("quantity").and_then(|v| v.as_f64());
        let cost_basis = h
            .get("cost_basis")
            .and_then(|v| v.as_f64())
            .map(|d| (d * 100.0).round() as i64);
        let current_value = h
            .get("institution_value")
            .and_then(|v| v.as_f64())
            .map(|d| (d * 100.0).round() as i64);

        let metadata = serde_json::json!({
            "plaid_security_id": plaid_security,
            "plaid_account_id": plaid_account,
            "institution_price": h.get("institution_price"),
            "vested_quantity": h.get("vested_quantity"),
        });

        let stream_id = format!("{plaid_account}:{plaid_security}");
        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("plaid:asset:{stream_id}").as_bytes(),
        )
        .to_string();
        let account_local_id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("plaid:account:{plaid_account}").as_bytes(),
        )
        .to_string();

        pending.push((
            id,
            account_local_id,
            asset_type,
            symbol,
            name,
            quantity,
            cost_basis,
            current_value,
            currency,
            now.to_rfc3339(),
            stream_id,
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

async fn flush(db: &PgPool, records: &[AssetRow]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }
    let sql = build_batch_insert_query(
        "data_financial_asset",
        &[
            "id",
            "account_id",
            "asset_type",
            "symbol",
            "name",
            "quantity",
            "cost_basis",
            "current_value",
            "currency",
            "timestamp",
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
            .bind(&r.4)
            .bind(r.5)
            .bind(r.6)
            .bind(r.7)
            .bind(&r.8)
            .bind(&r.9)
            .bind(&r.10)
            .bind("plaid_investments")
            .bind("plaid")
            .bind(&r.11);
    }
    Ok(q.execute(db).await?.rows_affected() as usize)
}
