//! Plaid liabilities → `data_financial_liability`.
//!
//! Adapted from `core/src/sources/plaid/liabilities/transform.rs`. Plaid
//! returns `liabilities.credit[]`, `liabilities.mortgage[]`, `liabilities.student[]`,
//! each with a different schema. We normalize to a common row shape.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use virtues_helpers::dedup::{build_batch_insert_query, BATCH_SIZE};

#[allow(clippy::type_complexity)]
type LiabilityRow = (
    String,         // id
    String,         // account_id (local)
    String,         // liability_type ('credit' | 'mortgage' | 'student')
    Option<i64>,    // principal cents
    Option<f64>,    // interest_rate
    Option<i64>,    // minimum_payment cents
    Option<NaiveDate>, // next_payment_due_date
    Option<NaiveDate>, // origination_date
    Option<NaiveDate>, // maturity_date
    String,         // currency
    DateTime<Utc>,  // timestamp
    String,         // source_stream_id
    Value,          // metadata
);

pub async fn write_liabilities(db: &PgPool, liabilities: &Value) -> Result<usize> {
    let mut pending: Vec<LiabilityRow> = Vec::new();
    let mut written = 0;
    let now = Utc::now();

    let kinds = [
        ("credit", liabilities.get("credit")),
        ("mortgage", liabilities.get("mortgage")),
        ("student", liabilities.get("student")),
    ];

    for (kind, list) in kinds {
        let Some(arr) = list.and_then(|v| v.as_array()) else {
            continue;
        };
        for item in arr {
            let plaid_account = item
                .get("account_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if plaid_account.is_empty() {
                continue;
            }

            let principal_dollars = item
                .get("origination_principal_amount")
                .or_else(|| item.get("last_statement_balance"))
                .and_then(|v| v.as_f64());
            let principal = principal_dollars.map(|d| (d * 100.0).round() as i64);

            let interest_rate = match kind {
                "credit" => item
                    .get("aprs")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.first())
                    .and_then(|a| a.get("apr_percentage"))
                    .and_then(|v| v.as_f64()),
                _ => item
                    .get("interest_rate")
                    .and_then(|r| r.get("percentage"))
                    .and_then(|v| v.as_f64()),
            };

            let min_payment_dollars = item
                .get("minimum_payment_amount")
                .or_else(|| item.get("next_monthly_payment"))
                .and_then(|v| v.as_f64());
            let min_payment = min_payment_dollars.map(|d| (d * 100.0).round() as i64);

            // Plaid sends dates as ISO `YYYY-MM-DD` strings → DATE columns.
            let parse_date = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
            let next_payment = item
                .get("next_payment_due_date")
                .and_then(|v| v.as_str())
                .and_then(parse_date);
            let origination = item
                .get("origination_date")
                .and_then(|v| v.as_str())
                .and_then(parse_date);
            let maturity = item
                .get("expected_payoff_date")
                .or_else(|| item.get("maturity_date"))
                .and_then(|v| v.as_str())
                .and_then(parse_date);

            let stream_id = format!("{kind}:{plaid_account}");
            let id = Uuid::new_v5(
                &Uuid::NAMESPACE_OID,
                format!("plaid:liability:{stream_id}").as_bytes(),
            )
            .to_string();
            let account_local_id = Uuid::new_v5(
                &Uuid::NAMESPACE_OID,
                format!("plaid:account:{plaid_account}").as_bytes(),
            )
            .to_string();

            let metadata = serde_json::json!({
                "plaid_kind": kind,
                "plaid_account_id": plaid_account,
                "raw": item,
            });

            pending.push((
                id,
                account_local_id,
                kind.to_string(),
                principal,
                interest_rate,
                min_payment,
                next_payment,
                origination,
                maturity,
                "USD".to_string(),
                now,
                stream_id,
                metadata,
            ));

            if pending.len() >= BATCH_SIZE {
                written += flush(db, &pending).await?;
                pending.clear();
            }
        }
    }
    if !pending.is_empty() {
        written += flush(db, &pending).await?;
    }
    Ok(written)
}

async fn flush(db: &PgPool, records: &[LiabilityRow]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }
    let sql = build_batch_insert_query(
        "data_financial_liability",
        &[
            "id",
            "account_id",
            "liability_type",
            "principal",
            "interest_rate",
            "minimum_payment",
            "next_payment_due_date",
            "origination_date",
            "maturity_date",
            "currency",
            "occurred_at",
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
            .bind(r.4)
            .bind(r.5)
            .bind(&r.6)
            .bind(&r.7)
            .bind(&r.8)
            .bind(&r.9)
            .bind(&r.10)
            .bind(&r.11)
            .bind("plaid_liabilities")
            .bind("plaid")
            .bind(&r.12);
    }
    Ok(q.execute(db).await?.rows_affected() as usize)
}
