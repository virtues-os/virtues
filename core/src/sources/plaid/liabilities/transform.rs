//! Plaid liabilities to financial_liability ontology transformation
//!
//! Transforms raw liabilities from stream_plaid_liabilities into the normalized
//! financial_liability ontology table.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use crate::database::Database;
use crate::error::Result;
use crate::jobs::TransformContext;
use crate::sources::base::{OntologyTransform, TransformRegistration, TransformResult};

/// Batch size for bulk inserts
const BATCH_SIZE: usize = 100;

/// Transform Plaid liabilities to financial_liability ontology
pub struct PlaidLiabilityTransform;

#[async_trait]
impl OntologyTransform for PlaidLiabilityTransform {
    fn source_table(&self) -> &str {
        "stream_plaid_liabilities"
    }

    fn target_table(&self) -> &str {
        "financial_liability"
    }

    fn domain(&self) -> &str {
        "financial"
    }

    #[tracing::instrument(skip(self, db, context), fields(source_table = %self.source_table(), target_table = %self.target_table()))]
    async fn transform(
        &self,
        db: &Database,
        context: &TransformContext,
        source_id: Uuid,
    ) -> Result<TransformResult> {
        let mut records_read = 0;
        let mut records_written = 0;
        let mut records_failed = 0;
        let mut last_processed_id: Option<Uuid> = None;

        let transform_start = std::time::Instant::now();

        tracing::info!(
            source_id = %source_id,
            "Starting Plaid liabilities to financial_liability transformation"
        );

        // Read stream data using data source
        let checkpoint_key = "plaid_liabilities_to_financial";
        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;
        let batches = data_source
            .read_with_checkpoint(source_id, "liabilities", checkpoint_key)
            .await?;

        tracing::info!(
            batch_count = batches.len(),
            source_type = ?data_source.source_type(),
            "Fetched Plaid liability batches from data source"
        );

        // Batch insert configuration
        let mut pending_records: Vec<LiabilityRecord> = Vec::new();

        for batch in batches {
            tracing::debug!(batch_record_count = batch.records.len(), "Processing batch");

            for record in &batch.records {
                records_read += 1;

                // Extract account_id (required)
                let Some(account_id) = record.get("account_id").and_then(|v| v.as_str()) else {
                    records_failed += 1;
                    continue;
                };

                let stream_id = record
                    .get("id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .unwrap_or_else(Uuid::new_v4);

                // Extract liability type
                let liability_type = record
                    .get("liability_type")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| "other".to_string());

                // Extract APR/interest rate
                let apr_percentage = record.get("apr_percentage").and_then(|v| v.as_f64());

                let apr_type = record
                    .get("apr_type")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let interest_rate_percentage = record
                    .get("interest_rate_percentage")
                    .and_then(|v| v.as_f64());

                // Extract payment info
                let minimum_payment = record.get("minimum_payment").and_then(|v| v.as_f64());

                let last_payment_amount =
                    record.get("last_payment_amount").and_then(|v| v.as_f64());

                let last_payment_date = record
                    .get("last_payment_date")
                    .and_then(|v| v.as_str())
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

                let next_payment_due_date = record
                    .get("next_payment_due_date")
                    .and_then(|v| v.as_str())
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

                let next_payment_amount =
                    record.get("next_payment_amount").and_then(|v| v.as_f64());

                // Extract loan specifics
                let original_loan_amount =
                    record.get("original_loan_amount").and_then(|v| v.as_f64());

                let outstanding_balance = record
                    .get("last_statement_balance")
                    .or_else(|| record.get("outstanding_interest_amount"))
                    .and_then(|v| v.as_f64());

                // Parse loan term from string (e.g., "30 year") to months
                let loan_term_months = record
                    .get("loan_term")
                    .and_then(|v| v.as_str())
                    .and_then(|s| parse_loan_term_months(s));

                let origination_date = record
                    .get("origination_date")
                    .and_then(|v| v.as_str())
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

                let maturity_date = record
                    .get("maturity_date")
                    .or_else(|| record.get("expected_payoff_date"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

                // Extract mortgage property address
                let property_address = record
                    .get("property_address")
                    .and_then(|v| v.get("street"))
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let property_city = record
                    .get("property_address")
                    .and_then(|v| v.get("city"))
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let property_region = record
                    .get("property_address")
                    .and_then(|v| v.get("region"))
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let property_postal_code = record
                    .get("property_address")
                    .and_then(|v| v.get("postal_code"))
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let escrow_balance = record.get("escrow_balance").and_then(|v| v.as_f64());

                // Build metadata with all extra fields
                let metadata = serde_json::json!({
                    "plaid_account_id": account_id,
                    "is_overdue": record.get("is_overdue"),
                    "aprs": record.get("aprs"),
                    "loan_name": record.get("loan_name"),
                    "guarantor": record.get("guarantor"),
                    "loan_status": record.get("loan_status"),
                    "repayment_plan": record.get("repayment_plan"),
                    "pslf_status": record.get("pslf_status"),
                    "has_pmi": record.get("has_pmi"),
                    "has_prepayment_penalty": record.get("has_prepayment_penalty"),
                    "loan_type_description": record.get("loan_type_description"),
                    "past_due_amount": record.get("past_due_amount"),
                    "ytd_interest_paid": record.get("ytd_interest_paid"),
                    "ytd_principal_paid": record.get("ytd_principal_paid"),
                    "synced_at": record.get("synced_at"),
                });

                let timestamp = Utc::now();

                pending_records.push(LiabilityRecord {
                    account_id_external: format!("plaid:{}", account_id),
                    liability_type,
                    apr_percentage,
                    apr_type,
                    interest_rate_percentage,
                    minimum_payment,
                    last_payment_amount,
                    last_payment_date,
                    next_payment_due_date,
                    next_payment_amount,
                    original_loan_amount,
                    outstanding_balance,
                    loan_term_months,
                    origination_date,
                    maturity_date,
                    property_address,
                    property_city,
                    property_region,
                    property_postal_code,
                    escrow_balance,
                    timestamp,
                    stream_id,
                    metadata,
                });

                last_processed_id = Some(stream_id);

                // Execute batch insert when we reach batch size
                if pending_records.len() >= BATCH_SIZE {
                    match execute_liability_batch_insert(db, &pending_records).await {
                        Ok(written) => records_written += written,
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                batch_size = pending_records.len(),
                                "Batch insert failed"
                            );
                            records_failed += pending_records.len();
                        }
                    }
                    pending_records.clear();
                }
            }

            // Update checkpoint after processing batch
            if let Some(max_ts) = batch.max_timestamp {
                data_source
                    .update_checkpoint(source_id, "liabilities", checkpoint_key, max_ts)
                    .await?;
            }
        }

        // Insert any remaining records
        if !pending_records.is_empty() {
            match execute_liability_batch_insert(db, &pending_records).await {
                Ok(written) => records_written += written,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        batch_size = pending_records.len(),
                        "Final batch insert failed"
                    );
                    records_failed += pending_records.len();
                }
            }
        }

        let total_duration = transform_start.elapsed();

        tracing::info!(
            source_id = %source_id,
            records_read,
            records_written,
            records_failed,
            total_duration_ms = total_duration.as_millis(),
            "Plaid liabilities to financial_liability transformation completed"
        );

        Ok(TransformResult {
            records_read,
            records_written,
            records_failed,
            last_processed_id,
            chained_transforms: vec![],
        })
    }
}

/// Parse loan term string (e.g., "30 year", "15 year") to months
fn parse_loan_term_months(term: &str) -> Option<i32> {
    let term_lower = term.to_lowercase();
    if term_lower.contains("year") {
        // Extract number before "year"
        let years: i32 = term_lower.split_whitespace().next()?.parse().ok()?;
        Some(years * 12)
    } else if term_lower.contains("month") {
        term_lower.split_whitespace().next()?.parse().ok()
    } else {
        None
    }
}

/// Internal struct to hold liability data for batch insert
struct LiabilityRecord {
    account_id_external: String,
    liability_type: String,
    apr_percentage: Option<f64>,
    apr_type: Option<String>,
    interest_rate_percentage: Option<f64>,
    minimum_payment: Option<f64>,
    last_payment_amount: Option<f64>,
    last_payment_date: Option<NaiveDate>,
    next_payment_due_date: Option<NaiveDate>,
    next_payment_amount: Option<f64>,
    original_loan_amount: Option<f64>,
    outstanding_balance: Option<f64>,
    loan_term_months: Option<i32>,
    origination_date: Option<NaiveDate>,
    maturity_date: Option<NaiveDate>,
    property_address: Option<String>,
    property_city: Option<String>,
    property_region: Option<String>,
    property_postal_code: Option<String>,
    escrow_balance: Option<f64>,
    timestamp: DateTime<Utc>,
    stream_id: Uuid,
    metadata: serde_json::Value,
}

/// Execute batch insert for liability records
async fn execute_liability_batch_insert(
    db: &Database,
    records: &[LiabilityRecord],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    // Build batch insert with ON CONFLICT for upsert behavior
    let query_str = Database::build_batch_insert_query(
        "data_financial_liability",
        &[
            "account_id_external",
            "liability_type",
            "apr_percentage",
            "apr_type",
            "interest_rate_percentage",
            "minimum_payment",
            "last_payment_amount",
            "last_payment_date",
            "next_payment_due_date",
            "next_payment_amount",
            "original_loan_amount",
            "outstanding_balance",
            "loan_term_months",
            "origination_date",
            "maturity_date",
            "property_address",
            "property_city",
            "property_region",
            "property_postal_code",
            "escrow_balance",
            "timestamp",
            "source_stream_id",
            "metadata",
            "source_table",
            "source_provider",
        ],
        "account_id_external",
        records.len(),
    );

    let mut query = sqlx::query(&query_str);

    for record in records {
        query = query
            .bind(&record.account_id_external)
            .bind(&record.liability_type)
            .bind(record.apr_percentage)
            .bind(&record.apr_type)
            .bind(record.interest_rate_percentage)
            .bind(record.minimum_payment)
            .bind(record.last_payment_amount)
            .bind(record.last_payment_date)
            .bind(record.next_payment_due_date)
            .bind(record.next_payment_amount)
            .bind(record.original_loan_amount)
            .bind(record.outstanding_balance)
            .bind(record.loan_term_months)
            .bind(record.origination_date)
            .bind(record.maturity_date)
            .bind(&record.property_address)
            .bind(&record.property_city)
            .bind(&record.property_region)
            .bind(&record.property_postal_code)
            .bind(record.escrow_balance)
            .bind(record.timestamp)
            .bind(record.stream_id)
            .bind(&record.metadata)
            .bind("stream_plaid_liabilities")
            .bind("plaid");
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

// Self-registration
struct PlaidLiabilityTransformRegistration;

impl TransformRegistration for PlaidLiabilityTransformRegistration {
    fn source_table(&self) -> &'static str {
        "stream_plaid_liabilities"
    }
    fn target_table(&self) -> &'static str {
        "financial_liability"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(PlaidLiabilityTransform))
    }
}

inventory::submit! {
    &PlaidLiabilityTransformRegistration as &dyn TransformRegistration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_metadata() {
        let transform = PlaidLiabilityTransform;
        assert_eq!(transform.source_table(), "stream_plaid_liabilities");
        assert_eq!(transform.target_table(), "financial_liability");
        assert_eq!(transform.domain(), "financial");
    }

    #[test]
    fn test_parse_loan_term() {
        assert_eq!(parse_loan_term_months("30 year"), Some(360));
        assert_eq!(parse_loan_term_months("15 year"), Some(180));
        assert_eq!(parse_loan_term_months("12 months"), Some(12));
        assert_eq!(parse_loan_term_months("invalid"), None);
    }
}
