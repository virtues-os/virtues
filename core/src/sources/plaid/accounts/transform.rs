//! Plaid accounts to financial_account ontology transformation
//!
//! Transforms raw accounts from stream_plaid_accounts into the normalized
//! financial_account ontology table.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::database::Database;
use crate::error::Result;
use crate::jobs::TransformContext;
use crate::sources::base::{OntologyTransform, TransformRegistration, TransformResult};

/// Batch size for bulk inserts
const BATCH_SIZE: usize = 100;

/// Transform Plaid accounts to financial_account ontology
pub struct PlaidAccountTransform;

#[async_trait]
impl OntologyTransform for PlaidAccountTransform {
    fn source_table(&self) -> &str {
        "stream_plaid_accounts"
    }

    fn target_table(&self) -> &str {
        "financial_account"
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
            "Starting Plaid accounts to financial_account transformation"
        );

        // Read stream data using data source
        let checkpoint_key = "plaid_accounts_to_financial";
        let read_start = std::time::Instant::now();
        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;
        let batches = data_source
            .read_with_checkpoint(source_id, "accounts", checkpoint_key)
            .await?;
        let read_duration = read_start.elapsed();

        tracing::info!(
            batch_count = batches.len(),
            read_duration_ms = read_duration.as_millis(),
            source_type = ?data_source.source_type(),
            "Fetched Plaid account batches from data source"
        );

        // Batch insert configuration
        let mut pending_records: Vec<AccountRecord> = Vec::new();

        for batch in batches {
            tracing::debug!(batch_record_count = batch.records.len(), "Processing batch");

            for record in &batch.records {
                records_read += 1;

                // Extract account_id
                let Some(account_id) = record.get("account_id").and_then(|v| v.as_str()) else {
                    records_failed += 1;
                    continue;
                };

                let stream_id = record
                    .get("id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .unwrap_or_else(Uuid::new_v4);

                // Extract required fields
                let account_name = record
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| "Unknown Account".to_string());

                let official_name = record
                    .get("official_name")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let account_type = record
                    .get("type")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| "other".to_string());

                let account_subtype = record
                    .get("subtype")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let mask = record
                    .get("mask")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                // Extract balances
                let balances = record.get("balances");
                let current_balance = balances
                    .and_then(|b| b.get("current"))
                    .and_then(|v| v.as_f64());
                let available_balance = balances
                    .and_then(|b| b.get("available"))
                    .and_then(|v| v.as_f64());
                let credit_limit = balances
                    .and_then(|b| b.get("limit"))
                    .and_then(|v| v.as_f64());
                let currency_code = balances
                    .and_then(|b| b.get("iso_currency_code"))
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| "USD".to_string());

                // Extract institution info
                let institution_id = record
                    .get("institution_id")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let institution_name = record
                    .get("institution_name")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                // Build metadata
                let metadata = serde_json::json!({
                    "plaid_account_id": account_id,
                    "synced_at": record.get("synced_at"),
                });

                let timestamp = Utc::now();

                pending_records.push(AccountRecord {
                    account_id_external: format!("plaid:{}", account_id),
                    account_name,
                    official_name,
                    account_type,
                    account_subtype,
                    mask,
                    current_balance,
                    available_balance,
                    credit_limit,
                    currency_code,
                    institution_id,
                    institution_name,
                    timestamp,
                    stream_id,
                    metadata,
                });

                last_processed_id = Some(stream_id);

                // Execute batch insert when we reach batch size
                if pending_records.len() >= BATCH_SIZE {
                    match execute_account_batch_insert(db, &pending_records).await {
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
                    .update_checkpoint(source_id, "accounts", checkpoint_key, max_ts)
                    .await?;
            }
        }

        // Insert any remaining records
        if !pending_records.is_empty() {
            match execute_account_batch_insert(db, &pending_records).await {
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
            "Plaid accounts to financial_account transformation completed"
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

/// Internal struct to hold account data for batch insert
struct AccountRecord {
    account_id_external: String,
    account_name: String,
    official_name: Option<String>,
    account_type: String,
    account_subtype: Option<String>,
    mask: Option<String>,
    current_balance: Option<f64>,
    available_balance: Option<f64>,
    credit_limit: Option<f64>,
    currency_code: String,
    institution_id: Option<String>,
    institution_name: Option<String>,
    timestamp: DateTime<Utc>,
    stream_id: Uuid,
    metadata: serde_json::Value,
}

/// Execute batch insert for account records
async fn execute_account_batch_insert(
    db: &Database,
    records: &[AccountRecord],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    // Build batch insert with ON CONFLICT for upsert behavior
    let query_str = Database::build_batch_insert_query(
        "data.financial_account",
        &[
            "account_id_external",
            "account_name",
            "official_name",
            "account_type",
            "account_subtype",
            "mask",
            "current_balance",
            "available_balance",
            "credit_limit",
            "currency_code",
            "institution_id",
            "institution_name",
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
            .bind(&record.account_name)
            .bind(&record.official_name)
            .bind(&record.account_type)
            .bind(&record.account_subtype)
            .bind(&record.mask)
            .bind(record.current_balance)
            .bind(record.available_balance)
            .bind(record.credit_limit)
            .bind(&record.currency_code)
            .bind(&record.institution_id)
            .bind(&record.institution_name)
            .bind(record.timestamp)
            .bind(record.stream_id)
            .bind(&record.metadata)
            .bind("stream_plaid_accounts")
            .bind("plaid");
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

// Self-registration
struct PlaidAccountTransformRegistration;

impl TransformRegistration for PlaidAccountTransformRegistration {
    fn source_table(&self) -> &'static str {
        "stream_plaid_accounts"
    }
    fn target_table(&self) -> &'static str {
        "financial_account"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(PlaidAccountTransform))
    }
}

inventory::submit! {
    &PlaidAccountTransformRegistration as &dyn TransformRegistration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_metadata() {
        let transform = PlaidAccountTransform;
        assert_eq!(transform.source_table(), "stream_plaid_accounts");
        assert_eq!(transform.target_table(), "financial_account");
        assert_eq!(transform.domain(), "financial");
    }
}
