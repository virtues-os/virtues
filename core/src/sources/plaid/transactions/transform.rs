//! Plaid transactions to financial_transaction ontology transformation
//!
//! Transforms raw transactions from stream_plaid_transactions into the normalized
//! financial_transaction ontology table.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use crate::database::Database;
use crate::error::Result;
use crate::jobs::TransformContext;
use crate::sources::base::{OntologyTransform, TransformRegistration, TransformResult};

/// Batch size for bulk inserts
const BATCH_SIZE: usize = 500;

/// Transform Plaid transactions to financial_transaction ontology
pub struct PlaidTransactionTransform;

#[async_trait]
impl OntologyTransform for PlaidTransactionTransform {
    fn source_table(&self) -> &str {
        "stream_plaid_transactions"
    }

    fn target_table(&self) -> &str {
        "financial_transaction"
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
            "Starting Plaid transactions to financial_transaction transformation"
        );

        // Read stream data using data source
        let checkpoint_key = "plaid_transactions_to_financial";
        let read_start = std::time::Instant::now();
        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;
        let batches = data_source
            .read_with_checkpoint(source_id, "transactions", checkpoint_key)
            .await?;
        let read_duration = read_start.elapsed();

        tracing::info!(
            batch_count = batches.len(),
            read_duration_ms = read_duration.as_millis(),
            source_type = ?data_source.source_type(),
            "Fetched Plaid transaction batches from data source"
        );

        // Batch insert configuration
        let mut pending_records: Vec<TransactionRecord> = Vec::new();
        let mut batch_insert_total_ms = 0u128;
        let mut batch_insert_count = 0;

        let processing_start = std::time::Instant::now();

        for batch in batches {
            tracing::debug!(batch_record_count = batch.records.len(), "Processing batch");

            for record in &batch.records {
                records_read += 1;

                // Extract transaction_id
                let Some(transaction_id) = record.get("transaction_id").and_then(|v| v.as_str())
                else {
                    records_failed += 1;
                    continue;
                };

                let stream_id = record
                    .get("id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .unwrap_or_else(Uuid::new_v4);

                // Extract required fields
                let amount = record
                    .get("amount")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                let transaction_date = record
                    .get("date")
                    .and_then(|v| v.as_str())
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                    .unwrap_or_else(|| Utc::now().date_naive());

                let name = record
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_default();

                // Extract optional fields
                let merchant_name = record
                    .get("merchant_name")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let account_id = record
                    .get("account_id")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let pending = record
                    .get("pending")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let iso_currency_code = record
                    .get("iso_currency_code")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let payment_channel = record
                    .get("payment_channel")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                // Extract Plaid category (primary category)
                let category = record
                    .get("personal_finance_category")
                    .and_then(|v| v.get("primary"))
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .or_else(|| {
                        record
                            .get("category")
                            .and_then(|v| v.as_array())
                            .and_then(|arr| arr.first())
                            .and_then(|v| v.as_str())
                            .map(String::from)
                    });

                // Extract location if available
                let location = record.get("location").cloned();

                // Build metadata with Plaid-specific fields
                let metadata = serde_json::json!({
                    "plaid_transaction_id": transaction_id,
                    "plaid_account_id": account_id,
                    "plaid_category": record.get("category"),
                    "plaid_category_id": record.get("category_id"),
                    "personal_finance_category": record.get("personal_finance_category"),
                    "payment_channel": payment_channel,
                    "payment_meta": record.get("payment_meta"),
                    "location": location,
                    "counterparties": record.get("counterparties"),
                    "merchant_entity_id": record.get("merchant_entity_id"),
                    "logo_url": record.get("logo_url"),
                    "website": record.get("website"),
                });

                // Create timestamp from transaction date
                let timestamp = transaction_date
                    .and_hms_opt(12, 0, 0)
                    .map(|dt| dt.and_utc())
                    .unwrap_or_else(Utc::now);

                pending_records.push(TransactionRecord {
                    transaction_id: format!("plaid:{}", transaction_id),
                    account_id,
                    amount,
                    transaction_date,
                    name,
                    merchant_name,
                    category,
                    pending,
                    currency_code: iso_currency_code,
                    timestamp,
                    stream_id,
                    metadata,
                });

                last_processed_id = Some(stream_id);

                // Execute batch insert when we reach batch size
                if pending_records.len() >= BATCH_SIZE {
                    let insert_start = std::time::Instant::now();
                    let batch_result =
                        execute_transaction_batch_insert(db, &pending_records).await;
                    let insert_duration = insert_start.elapsed();
                    batch_insert_total_ms += insert_duration.as_millis();
                    batch_insert_count += 1;

                    tracing::debug!(
                        batch_size = pending_records.len(),
                        insert_duration_ms = insert_duration.as_millis(),
                        "Executed batch insert"
                    );

                    match batch_result {
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
                    .update_checkpoint(source_id, "transactions", checkpoint_key, max_ts)
                    .await?;
            }
        }

        // Insert any remaining records
        if !pending_records.is_empty() {
            let insert_start = std::time::Instant::now();
            let batch_result = execute_transaction_batch_insert(db, &pending_records).await;
            let insert_duration = insert_start.elapsed();
            batch_insert_total_ms += insert_duration.as_millis();
            batch_insert_count += 1;

            tracing::debug!(
                batch_size = pending_records.len(),
                insert_duration_ms = insert_duration.as_millis(),
                "Executed final batch insert"
            );

            match batch_result {
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

        let processing_duration = processing_start.elapsed();
        let total_duration = transform_start.elapsed();

        tracing::info!(
            source_id = %source_id,
            records_read,
            records_written,
            records_failed,
            total_duration_ms = total_duration.as_millis(),
            processing_duration_ms = processing_duration.as_millis(),
            read_duration_ms = read_duration.as_millis(),
            batch_insert_total_ms,
            batch_insert_count,
            "Plaid transactions to financial_transaction transformation completed"
        );

        Ok(TransformResult {
            records_read,
            records_written,
            records_failed,
            last_processed_id,
            chained_transforms: vec![], // No chained transforms for financial data
        })
    }
}

/// Internal struct to hold transaction data for batch insert
struct TransactionRecord {
    transaction_id: String,
    account_id: Option<String>,
    amount: f64,
    transaction_date: NaiveDate,
    name: String,
    merchant_name: Option<String>,
    category: Option<String>,
    pending: bool,
    currency_code: Option<String>,
    timestamp: DateTime<Utc>,
    stream_id: Uuid,
    metadata: serde_json::Value,
}

/// Execute batch insert for transaction records
async fn execute_transaction_batch_insert(
    db: &Database,
    records: &[TransactionRecord],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let query_str = Database::build_batch_insert_query(
        "data.financial_transaction",
        &[
            "transaction_id_external",
            "account_id_external",
            "amount",
            "transaction_date",
            "name",
            "merchant_name",
            "category",
            "is_pending",
            "currency_code",
            "timestamp",
            "source_stream_id",
            "metadata",
            "source_table",
            "source_provider",
        ],
        "transaction_id_external",
        records.len(),
    );

    let mut query = sqlx::query(&query_str);

    for record in records {
        query = query
            .bind(&record.transaction_id)
            .bind(&record.account_id)
            .bind(record.amount)
            .bind(record.transaction_date)
            .bind(&record.name)
            .bind(&record.merchant_name)
            .bind(&record.category)
            .bind(record.pending)
            .bind(&record.currency_code)
            .bind(record.timestamp)
            .bind(record.stream_id)
            .bind(&record.metadata)
            .bind("stream_plaid_transactions")
            .bind("plaid");
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

// Self-registration
struct PlaidTransactionTransformRegistration;

impl TransformRegistration for PlaidTransactionTransformRegistration {
    fn source_table(&self) -> &'static str {
        "stream_plaid_transactions"
    }
    fn target_table(&self) -> &'static str {
        "financial_transaction"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(PlaidTransactionTransform))
    }
}

inventory::submit! {
    &PlaidTransactionTransformRegistration as &dyn TransformRegistration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_metadata() {
        let transform = PlaidTransactionTransform;
        assert_eq!(transform.source_table(), "stream_plaid_transactions");
        assert_eq!(transform.target_table(), "financial_transaction");
        assert_eq!(transform.domain(), "financial");
    }
}
