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
        source_id: String,
    ) -> Result<TransformResult> {
        let mut records_read = 0;
        let mut records_written = 0;
        let mut records_failed = 0;
        let mut last_processed_id: Option<String> = None;

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
            .read_with_checkpoint(&source_id, "transactions", checkpoint_key)
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
                let amount = record.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);

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

                let plaid_account_id = record
                    .get("account_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                let pending = record
                    .get("pending")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let iso_currency_code = record
                    .get("iso_currency_code")
                    .and_then(|v| v.as_str())
                    .unwrap_or("USD")
                    .to_string();

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

                // Use source_id (passed to transform) as source_connection_id
                // This is the actual source connection that synced this data
                let source_connection_id = source_id.clone();

                // Generate deterministic IDs for idempotency
                let id = crate::ids::generate_id(crate::ids::MONEY_TRANSACTION_PREFIX, &[&source_connection_id, transaction_id]);
                // Generate account_id that matches the account transform's ID
                let account_id = crate::ids::generate_id(crate::ids::MONEY_ACCOUNT_PREFIX, &[&source_connection_id, plaid_account_id]);

                // Build metadata with Plaid-specific fields
                let metadata = serde_json::json!({
                    "plaid_transaction_id": transaction_id,
                    "plaid_account_id": plaid_account_id,
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
                    id,
                    account_id,
                    transaction_id: transaction_id.to_string(),
                    amount,
                    merchant_name,
                    description: name,
                    category,
                    pending,
                    currency_code: iso_currency_code,
                    timestamp,
                    stream_id,
                    source_connection_id,
                    metadata,
                });

                last_processed_id = Some(stream_id.to_string());

                // Execute batch insert when we reach batch size
                if pending_records.len() >= BATCH_SIZE {
                    let insert_start = std::time::Instant::now();
                    let batch_result = execute_transaction_batch_insert(db, &pending_records).await;
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
                    .update_checkpoint(&source_id, "transactions", checkpoint_key, max_ts)
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
    id: String,                     // deterministic ID
    account_id: String,             // internal account ID (generated)
    transaction_id: String,         // external transaction ID
    amount: f64,
    merchant_name: Option<String>,
    description: String,            // from name field
    category: Option<String>,
    pending: bool,
    currency_code: String,
    timestamp: DateTime<Utc>,
    stream_id: Uuid,
    source_connection_id: String,
    metadata: serde_json::Value,
}

/// Execute batch insert for transaction records with fallback to individual inserts
async fn execute_transaction_batch_insert(
    db: &Database,
    records: &[TransactionRecord],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let query_str = Database::build_batch_insert_query(
        "data_financial_transaction",
        &[
            "id",
            "account_id",
            "transaction_id",
            "amount",
            "currency",
            "merchant_name",
            "description",
            "category",
            "is_pending",
            "timestamp",
            "source_stream_id",
            "source_connection_id",
            "metadata",
            "source_table",
            "source_provider",
        ],
        "id",
        records.len(),
    );

    let mut query = sqlx::query(&query_str);

    for record in records {
        // Convert amount to cents (INTEGER in schema)
        let amount_cents = (record.amount * 100.0) as i64;

        query = query
            .bind(&record.id)
            .bind(&record.account_id)
            .bind(&record.transaction_id)
            .bind(amount_cents)
            .bind(&record.currency_code)
            .bind(&record.merchant_name)
            .bind(&record.description)
            .bind(&record.category)
            .bind(record.pending)
            .bind(record.timestamp)
            .bind(record.stream_id)
            .bind(&record.source_connection_id)
            .bind(&record.metadata)
            .bind("stream_plaid_transactions")
            .bind("plaid");
    }

    match query.execute(db.pool()).await {
        Ok(result) => Ok(result.rows_affected() as usize),
        Err(batch_err) => {
            // Batch failed - fall back to individual inserts
            tracing::warn!(
                batch_size = records.len(),
                error = %batch_err,
                "Batch insert failed, falling back to individual inserts"
            );
            execute_transaction_individual_inserts(db, records).await
        }
    }
}

/// Fallback: insert records one by one when batch fails
/// This allows partial success and identifies problematic records.
/// Implements "lazy hydration" - if FK constraint fails (missing account),
/// creates a stub account and retries.
async fn execute_transaction_individual_inserts(
    db: &Database,
    records: &[TransactionRecord],
) -> Result<usize> {
    let mut written = 0;

    for record in records {
        match insert_single_transaction(db, record).await {
            Ok(_) => written += 1,
            Err(e) => {
                // Check if this is a foreign key constraint error (code 787 in SQLite)
                let error_str = e.to_string();
                if error_str.contains("FOREIGN KEY constraint failed") || error_str.contains("787") {
                    // Lazy hydration: create a stub account and retry
                    tracing::info!(
                        account_id = %record.account_id,
                        transaction_id = %record.transaction_id,
                        "Account not found, creating stub for lazy hydration"
                    );

                    match create_stub_account(db, record).await {
                        Ok(_) => {
                            // Retry the transaction insert
                            match insert_single_transaction(db, record).await {
                                Ok(_) => {
                                    written += 1;
                                    tracing::debug!(
                                        transaction_id = %record.transaction_id,
                                        "Transaction inserted after stub account creation"
                                    );
                                }
                                Err(retry_err) => {
                                    tracing::warn!(
                                        transaction_id = %record.transaction_id,
                                        account_id = %record.account_id,
                                        error = %retry_err,
                                        "Failed to insert transaction even after stub account creation"
                                    );
                                }
                            }
                        }
                        Err(stub_err) => {
                            tracing::warn!(
                                account_id = %record.account_id,
                                error = %stub_err,
                                "Failed to create stub account"
                            );
                        }
                    }
                } else {
                    // Not an FK error, log and continue
                    tracing::warn!(
                        transaction_id = %record.transaction_id,
                        account_id = %record.account_id,
                        error = %e,
                        "Failed to insert individual transaction"
                    );
                }
            }
        }
    }

    Ok(written)
}

/// Insert a single transaction record
async fn insert_single_transaction(
    db: &Database,
    record: &TransactionRecord,
) -> std::result::Result<(), sqlx::Error> {
    let amount_cents = (record.amount * 100.0) as i64;

    sqlx::query(
        r#"
        INSERT INTO data_financial_transaction (
            id, account_id, transaction_id, amount, currency, merchant_name,
            description, category, is_pending, timestamp, source_stream_id,
            source_connection_id, metadata, source_table, source_provider
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        ON CONFLICT (id) DO UPDATE SET
            amount = excluded.amount,
            currency = excluded.currency,
            merchant_name = excluded.merchant_name,
            description = excluded.description,
            category = excluded.category,
            is_pending = excluded.is_pending,
            metadata = excluded.metadata,
            updated_at = datetime('now')
        "#,
    )
    .bind(&record.id)
    .bind(&record.account_id)
    .bind(&record.transaction_id)
    .bind(amount_cents)
    .bind(&record.currency_code)
    .bind(&record.merchant_name)
    .bind(&record.description)
    .bind(&record.category)
    .bind(record.pending)
    .bind(record.timestamp)
    .bind(record.stream_id)
    .bind(&record.source_connection_id)
    .bind(&record.metadata)
    .bind("stream_plaid_transactions")
    .bind("plaid")
    .execute(db.pool())
    .await?;

    Ok(())
}

/// Create a stub account for lazy hydration
/// This creates a minimal account record that will be "hydrated" with full details
/// when the accounts sync runs.
async fn create_stub_account(
    db: &Database,
    transaction: &TransactionRecord,
) -> std::result::Result<(), sqlx::Error> {
    // Extract the Plaid account ID from metadata for the stub name
    let plaid_account_id = transaction.metadata
        .get("plaid_account_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let stub_metadata = serde_json::json!({
        "is_stub": true,
        "stub_created_at": chrono::Utc::now().to_rfc3339(),
        "plaid_account_id": plaid_account_id,
        "stub_reason": "Created during transaction sync (lazy hydration)"
    });

    sqlx::query(
        r#"
        INSERT INTO data_financial_account (
            id, account_name, account_type, currency, source_stream_id,
            source_connection_id, metadata, source_table, source_provider
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(&transaction.account_id)
    .bind(format!("Pending Account ({})", &plaid_account_id[..plaid_account_id.len().min(8)]))
    .bind("unknown")
    .bind(&transaction.currency_code)
    .bind(transaction.stream_id)
    .bind(&transaction.source_connection_id)
    .bind(stub_metadata)
    .bind("stub")
    .bind("plaid")
    .execute(db.pool())
    .await?;

    tracing::info!(
        account_id = %transaction.account_id,
        "Created stub account for lazy hydration"
    );

    Ok(())
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
