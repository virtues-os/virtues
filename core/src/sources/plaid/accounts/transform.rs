//! Plaid accounts to financial_account ontology transformation
//!
//! Transforms raw accounts from stream_plaid_accounts into the normalized
//! financial_account ontology table.

use async_trait::async_trait;
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
        source_id: String,
    ) -> Result<TransformResult> {
        let mut records_read = 0;
        let mut records_written = 0;
        let mut records_failed = 0;
        let mut last_processed_id: Option<String> = None;

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
            .read_with_checkpoint(&source_id, "accounts", checkpoint_key)
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

                // Use source_id (passed to transform) as source_connection_id
                // This is the actual source connection that synced this data
                let source_connection_id = source_id.clone();

                // Generate deterministic ID for idempotency
                let id = crate::ids::generate_id(crate::ids::MONEY_ACCOUNT_PREFIX, &[&source_connection_id, account_id]);

                // Build metadata
                let metadata = serde_json::json!({
                    "plaid_account_id": account_id,
                    "official_name": official_name,
                    "account_subtype": account_subtype,
                    "synced_at": record.get("synced_at"),
                });

                pending_records.push(AccountRecord {
                    id,
                    account_name,
                    account_type,
                    mask,
                    current_balance,
                    available_balance,
                    credit_limit,
                    currency_code,
                    institution_id,
                    institution_name,
                    stream_id,
                    source_connection_id,
                    metadata,
                });

                last_processed_id = Some(stream_id.to_string());

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
                    .update_checkpoint(&source_id, "accounts", checkpoint_key, max_ts)
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
    id: String,              // deterministic ID
    account_name: String,
    account_type: String,
    mask: Option<String>,
    current_balance: Option<f64>,
    available_balance: Option<f64>,
    credit_limit: Option<f64>,
    currency_code: String,
    institution_id: Option<String>,
    institution_name: Option<String>,
    stream_id: Uuid,
    source_connection_id: String,
    metadata: serde_json::Value,
}

/// Execute batch insert for account records with fallback to individual inserts
async fn execute_account_batch_insert(db: &Database, records: &[AccountRecord]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    // Build batch insert with ON CONFLICT for upsert behavior
    let query_str = Database::build_batch_insert_query(
        "data_financial_account",
        &[
            "id",
            "account_name",
            "account_type",
            "mask",
            "currency",
            "current_balance",
            "available_balance",
            "credit_limit",
            "institution_id",
            "institution_name",
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
        // Convert balances to cents (INTEGER in schema)
        let current_balance_cents = record.current_balance.map(|b| (b * 100.0) as i64);
        let available_balance_cents = record.available_balance.map(|b| (b * 100.0) as i64);
        let credit_limit_cents = record.credit_limit.map(|b| (b * 100.0) as i64);

        query = query
            .bind(&record.id)
            .bind(&record.account_name)
            .bind(&record.account_type)
            .bind(&record.mask)
            .bind(&record.currency_code)
            .bind(current_balance_cents)
            .bind(available_balance_cents)
            .bind(credit_limit_cents)
            .bind(&record.institution_id)
            .bind(&record.institution_name)
            .bind(record.stream_id)
            .bind(&record.source_connection_id)
            .bind(&record.metadata)
            .bind("stream_plaid_accounts")
            .bind("plaid");
    }

    match query.execute(db.pool()).await {
        Ok(result) => Ok(result.rows_affected() as usize),
        Err(batch_err) => {
            // Batch failed - fall back to individual inserts
            tracing::warn!(
                batch_size = records.len(),
                error = %batch_err,
                "Account batch insert failed, falling back to individual inserts"
            );
            execute_account_individual_inserts(db, records).await
        }
    }
}

/// Fallback: insert account records one by one when batch fails
async fn execute_account_individual_inserts(
    db: &Database,
    records: &[AccountRecord],
) -> Result<usize> {
    let mut written = 0;

    for record in records {
        let current_balance_cents = record.current_balance.map(|b| (b * 100.0) as i64);
        let available_balance_cents = record.available_balance.map(|b| (b * 100.0) as i64);
        let credit_limit_cents = record.credit_limit.map(|b| (b * 100.0) as i64);

        let result = sqlx::query(
            r#"
            INSERT INTO data_financial_account (
                id, account_name, account_type, mask, currency, current_balance,
                available_balance, credit_limit, institution_id, institution_name,
                source_stream_id, source_connection_id, metadata, source_table, source_provider
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            ON CONFLICT (id) DO UPDATE SET
                account_name = excluded.account_name,
                current_balance = excluded.current_balance,
                available_balance = excluded.available_balance,
                credit_limit = excluded.credit_limit,
                metadata = excluded.metadata,
                updated_at = datetime('now')
            "#,
        )
        .bind(&record.id)
        .bind(&record.account_name)
        .bind(&record.account_type)
        .bind(&record.mask)
        .bind(&record.currency_code)
        .bind(current_balance_cents)
        .bind(available_balance_cents)
        .bind(credit_limit_cents)
        .bind(&record.institution_id)
        .bind(&record.institution_name)
        .bind(record.stream_id)
        .bind(&record.source_connection_id)
        .bind(&record.metadata)
        .bind("stream_plaid_accounts")
        .bind("plaid")
        .execute(db.pool())
        .await;

        match result {
            Ok(_) => written += 1,
            Err(e) => {
                tracing::warn!(
                    account_id = %record.id,
                    account_name = %record.account_name,
                    error = %e,
                    "Failed to insert individual account"
                );
            }
        }
    }

    Ok(written)
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
