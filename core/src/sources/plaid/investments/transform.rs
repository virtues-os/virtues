//! Plaid investments to financial_asset ontology transformation
//!
//! Transforms raw holdings from stream_plaid_investments into the normalized
//! financial_asset ontology table.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use crate::database::Database;
use crate::error::Result;
use crate::jobs::TransformContext;
use crate::sources::base::{OntologyTransform, TransformRegistration, TransformResult};

/// Batch size for bulk inserts
const BATCH_SIZE: usize = 100;

/// Transform Plaid investments to financial_asset ontology
pub struct PlaidInvestmentTransform;

#[async_trait]
impl OntologyTransform for PlaidInvestmentTransform {
    fn source_table(&self) -> &str {
        "stream_plaid_investments"
    }

    fn target_table(&self) -> &str {
        "financial_asset"
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
            "Starting Plaid investments to financial_asset transformation"
        );

        // Read stream data using data source
        let checkpoint_key = "plaid_investments_to_financial";
        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;
        let batches = data_source
            .read_with_checkpoint(&source_id, "investments", checkpoint_key)
            .await?;

        tracing::info!(
            batch_count = batches.len(),
            source_type = ?data_source.source_type(),
            "Fetched Plaid investment batches from data source"
        );

        // Batch insert configuration
        let mut pending_records: Vec<AssetRecord> = Vec::new();

        for batch in batches {
            tracing::debug!(batch_record_count = batch.records.len(), "Processing batch");

            for record in &batch.records {
                records_read += 1;

                // Extract security_id (required)
                let Some(security_id) = record.get("security_id").and_then(|v| v.as_str()) else {
                    records_failed += 1;
                    continue;
                };

                let stream_id = record
                    .get("id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .unwrap_or_else(Uuid::new_v4);

                // Extract required fields
                let account_id = record
                    .get("account_id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_default();

                let quantity = record
                    .get("quantity")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                let security_name = record
                    .get("security_name")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| "Unknown Security".to_string());

                // Extract optional fields
                let ticker_symbol = record
                    .get("ticker_symbol")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let cusip = record
                    .get("cusip")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let isin = record
                    .get("isin")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let security_type = record
                    .get("security_type")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let cost_basis = record.get("cost_basis").and_then(|v| v.as_f64());

                let institution_value = record.get("institution_value").and_then(|v| v.as_f64());

                let close_price = record.get("close_price").and_then(|v| v.as_f64());

                let currency_code = record
                    .get("iso_currency_code")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| "USD".to_string());

                // Parse as_of_date from institution_price_as_of or close_price_as_of
                let as_of_date = record
                    .get("institution_price_as_of")
                    .or_else(|| record.get("close_price_as_of"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                    .unwrap_or_else(|| Utc::now().date_naive());

                // Get source_connection_id for deterministic ID generation
                let source_connection_id = record
                    .get("source_connection_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Generate deterministic IDs for idempotency
                let id = crate::ids::generate_id("asset", &[&source_connection_id, security_id, &account_id]);
                // Generate account_id that matches the account transform's ID
                let internal_account_id = crate::ids::generate_id("account", &[&source_connection_id, &account_id]);

                // Build metadata
                let metadata = serde_json::json!({
                    "plaid_security_id": security_id,
                    "plaid_account_id": account_id,
                    "cusip": cusip,
                    "isin": isin,
                    "close_price": close_price,
                    "as_of_date": as_of_date.to_string(),
                    "vested_quantity": record.get("vested_quantity"),
                    "vested_value": record.get("vested_value"),
                    "institution_price": record.get("institution_price"),
                    "is_cash_equivalent": record.get("is_cash_equivalent"),
                    "sector": record.get("sector"),
                    "industry": record.get("industry"),
                    "sedol": record.get("sedol"),
                    "synced_at": record.get("synced_at"),
                });

                let timestamp = Utc::now();

                pending_records.push(AssetRecord {
                    id,
                    account_id: internal_account_id,
                    asset_type: security_type.unwrap_or_else(|| "security".to_string()),
                    symbol: ticker_symbol,
                    name: security_name,
                    quantity,
                    cost_basis,
                    current_value: institution_value,
                    currency_code,
                    timestamp,
                    stream_id,
                    source_connection_id,
                    metadata,
                });

                last_processed_id = Some(stream_id.to_string());

                // Execute batch insert when we reach batch size
                if pending_records.len() >= BATCH_SIZE {
                    match execute_asset_batch_insert(db, &pending_records).await {
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
                    .update_checkpoint(&source_id, "investments", checkpoint_key, max_ts)
                    .await?;
            }
        }

        // Insert any remaining records
        if !pending_records.is_empty() {
            match execute_asset_batch_insert(db, &pending_records).await {
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
            "Plaid investments to financial_asset transformation completed"
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

/// Internal struct to hold asset data for batch insert
struct AssetRecord {
    id: String,                    // deterministic ID
    account_id: String,            // internal account ID (generated)
    asset_type: String,            // security_type or "security"
    symbol: Option<String>,        // ticker_symbol
    name: String,                  // security_name
    quantity: f64,
    cost_basis: Option<f64>,
    current_value: Option<f64>,    // institution_value
    currency_code: String,
    timestamp: DateTime<Utc>,
    stream_id: Uuid,
    source_connection_id: String,
    metadata: serde_json::Value,
}

/// Execute batch insert for asset records
async fn execute_asset_batch_insert(db: &Database, records: &[AssetRecord]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    // Build batch insert with ON CONFLICT for upsert behavior
    let query_str = Database::build_batch_insert_query(
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
        // Convert to cents
        let cost_basis_cents = record.cost_basis.map(|b| (b * 100.0) as i64);
        let current_value_cents = record.current_value.map(|v| (v * 100.0) as i64);

        query = query
            .bind(&record.id)
            .bind(&record.account_id)
            .bind(&record.asset_type)
            .bind(&record.symbol)
            .bind(&record.name)
            .bind(record.quantity)
            .bind(cost_basis_cents)
            .bind(current_value_cents)
            .bind(&record.currency_code)
            .bind(record.timestamp)
            .bind(record.stream_id)
            .bind(&record.source_connection_id)
            .bind(&record.metadata)
            .bind("stream_plaid_investments")
            .bind("plaid");
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

// Self-registration
struct PlaidInvestmentTransformRegistration;

impl TransformRegistration for PlaidInvestmentTransformRegistration {
    fn source_table(&self) -> &'static str {
        "stream_plaid_investments"
    }
    fn target_table(&self) -> &'static str {
        "financial_asset"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(PlaidInvestmentTransform))
    }
}

inventory::submit! {
    &PlaidInvestmentTransformRegistration as &dyn TransformRegistration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_metadata() {
        let transform = PlaidInvestmentTransform;
        assert_eq!(transform.source_table(), "stream_plaid_investments");
        assert_eq!(transform.target_table(), "financial_asset");
        assert_eq!(transform.domain(), "financial");
    }
}
