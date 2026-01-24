//! FinanceKit to financial ontology transformations

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::database::Database;
use crate::error::Result;
use crate::jobs::TransformContext;
use crate::sources::base::{OntologyTransform, TransformRegistration, TransformResult};

/// Batch size for bulk inserts
const BATCH_SIZE: usize = 500;

/// Transform FinanceKit account data to financial_account ontology
pub struct IosFinanceAccountTransform;

#[async_trait]
impl OntologyTransform for IosFinanceAccountTransform {
    fn source_table(&self) -> &str {
        "stream_ios_financekit"
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
        context: &crate::jobs::transform_context::TransformContext,
        source_id: String,
    ) -> Result<TransformResult> {
        let mut records_read = 0;
        let mut records_written = 0;
        let records_failed = 0;
        let mut last_processed_id: Option<String> = None;

        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;

        let checkpoint_key = "financekit_to_account";
        let batches = data_source
            .read_with_checkpoint(&source_id, "financekit", checkpoint_key)
            .await?;

        let mut pending_records = Vec::new();

        for batch in batches {
            for record in &batch.records {
                records_read += 1;

                // Extract accounts from the batch record
                let Some(accounts) = record.get("accounts").and_then(|v| v.as_array()) else {
                    continue;
                };

                for account in accounts {
                    let apple_id = account.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let name = account.get("name").and_then(|v| v.as_str()).unwrap_or("Apple Account");
                    let institution = account.get("institutionName").and_then(|v| v.as_str()).unwrap_or("Apple");
                    let acct_type = account.get("type").and_then(|v| v.as_str()).unwrap_or("other");
                    let balance = account.get("currentBalance").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let currency = account.get("currencyCode").and_then(|v| v.as_str()).unwrap_or("USD");

                    // Deterministic ID for account: hash(apple_finance_account + apple_id)
                    let internal_id = Uuid::new_v5(&Uuid::NAMESPACE_DNS, format!("apple_finance_account:{}", apple_id).as_bytes());

                    let metadata = json!({
                        "apple_account_id": apple_id,
                        "raw": account,
                    });

                    pending_records.push((
                        internal_id.to_string(),
                        name.to_string(),
                        acct_type.to_string(),
                        institution.to_string(),
                        (balance * 100.0) as i64,
                        currency.to_string(),
                        apple_id.to_string(),
                        metadata,
                    ));

                    last_processed_id = Some(internal_id.to_string());

                    if pending_records.len() >= BATCH_SIZE {
                        records_written += execute_account_batch_insert(db, &pending_records).await?;
                        pending_records.clear();
                    }
                }
            }

            if let Some(max_ts) = batch.max_timestamp {
                data_source
                    .update_checkpoint(&source_id, "financekit", checkpoint_key, max_ts)
                    .await?;
            }
        }

        if !pending_records.is_empty() {
            records_written += execute_account_batch_insert(db, &pending_records).await?;
        }

        Ok(TransformResult {
            records_read,
            records_written,
            records_failed,
            last_processed_id,
            chained_transforms: vec![],
        })
    }
}

/// Transform FinanceKit transaction data to financial_transaction ontology
pub struct FinanceKitTransactionTransform;

#[async_trait]
impl OntologyTransform for FinanceKitTransactionTransform {
    fn source_table(&self) -> &str {
        "stream_ios_financekit"
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
        context: &crate::jobs::transform_context::TransformContext,
        source_id: String,
    ) -> Result<TransformResult> {
        let mut records_read = 0;
        let mut records_written = 0;
        let records_failed = 0;
        let mut last_processed_id: Option<String> = None;

        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;

        let checkpoint_key = "financekit_to_transaction";
        let batches = data_source
            .read_with_checkpoint(&source_id, "financekit", checkpoint_key)
            .await?;

        let mut pending_records = Vec::new();

        for batch in batches {
            for record in &batch.records {
                records_read += 1;

                // Extract transactions from the batch record
                let Some(transactions) = record.get("transactions").and_then(|v| v.as_array()) else {
                    continue;
                };

                for tx in transactions {
                    let amount = tx.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let timestamp = tx.get("date").and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                        .unwrap_or_else(|| Utc::now());
                    let apple_id = tx.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let apple_account_id = tx.get("accountId").and_then(|v| v.as_str()).unwrap_or("");
                    let merchant_name = tx.get("merchantName").and_then(|v| v.as_str()).map(String::from);
                    let category = tx.get("category").and_then(|v| v.as_str()).map(String::from);
                    let status = tx.get("status").and_then(|v| v.as_str()).unwrap_or("posted");
                    let description = tx.get("description").and_then(|v| v.as_str()).map(String::from);

                    // Deterministic IDs
                    let internal_account_id = Uuid::new_v5(&Uuid::NAMESPACE_DNS, format!("apple_finance_account:{}", apple_account_id).as_bytes());
                    let internal_tx_id = Uuid::new_v5(&Uuid::NAMESPACE_DNS, format!("apple_finance:{}", apple_id).as_bytes());

                    let metadata = json!({
                        "financekit_raw": tx,
                        "apple_transaction_id": apple_id,
                    });

                    pending_records.push((
                        internal_tx_id.to_string(),
                        internal_account_id.to_string(),
                        (amount * 100.0) as i64,
                        merchant_name,
                        category,
                        description,
                        if status == "pending" { 1 } else { 0 },
                        timestamp,
                        apple_id.to_string(),
                        metadata,
                    ));

                    last_processed_id = Some(internal_tx_id.to_string());

                    if pending_records.len() >= BATCH_SIZE {
                        records_written += execute_transaction_batch_insert(db, &pending_records).await?;
                        pending_records.clear();
                    }
                }
            }

            if let Some(max_ts) = batch.max_timestamp {
                data_source
                    .update_checkpoint(&source_id, "financekit", checkpoint_key, max_ts)
                    .await?;
            }
        }

        if !pending_records.is_empty() {
            records_written += execute_transaction_batch_insert(db, &pending_records).await?;
        }

        Ok(TransformResult {
            records_read,
            records_written,
            records_failed,
            last_processed_id,
            chained_transforms: vec![],
        })
    }
}

async fn execute_account_batch_insert(
    db: &Database,
    records: &[(String, String, String, String, i64, String, String, serde_json::Value)],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let query_str = format!(
        "INSERT INTO data_financial_account (
            id, account_name, account_type, institution_name, current_balance, currency, source_stream_id, source_table, source_provider, metadata
        ) VALUES {}
        ON CONFLICT (id) DO UPDATE SET
            account_name = EXCLUDED.account_name,
            current_balance = EXCLUDED.current_balance,
            metadata = EXCLUDED.metadata,
            updated_at = datetime('now')",
        (0..records.len())
            .map(|i| format!("(${}, ${}, ${}, ${}, ${}, ${}, ${}, 'stream_ios_financekit', 'apple_finance', ${})", 
                i * 8 + 1, i * 8 + 2, i * 8 + 3, i * 8 + 4, i * 8 + 5, i * 8 + 6, i * 8 + 7, i * 8 + 8))
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

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

async fn execute_transaction_batch_insert(
    db: &Database,
    records: &[(String, String, i64, Option<String>, Option<String>, Option<String>, i32, DateTime<Utc>, String, serde_json::Value)],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let query_str = format!(
        "INSERT INTO data_financial_transaction (
            id, account_id, amount, merchant_name, category, description, is_pending, timestamp, source_stream_id, source_table, source_provider, metadata
        ) VALUES {}
        ON CONFLICT (id) DO UPDATE SET
            amount = EXCLUDED.amount,
            merchant_name = EXCLUDED.merchant_name,
            category = EXCLUDED.category,
            description = EXCLUDED.description,
            is_pending = EXCLUDED.is_pending,
            metadata = EXCLUDED.metadata,
            updated_at = datetime('now')",
        (0..records.len())
            .map(|i| format!("(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, 'stream_ios_financekit', 'apple_finance', ${})", 
                i * 10 + 1, i * 10 + 2, i * 10 + 3, i * 10 + 4, i * 10 + 5, i * 10 + 6, i * 10 + 7, i * 10 + 8, i * 10 + 9, i * 10 + 10))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let mut query = sqlx::query(&query_str);

    for (id, account_id, amount, merchant, cat, desc, pending, ts, stream_id, meta) in records {
        query = query
            .bind(id)
            .bind(account_id)
            .bind(amount)
            .bind(merchant)
            .bind(cat)
            .bind(desc)
            .bind(pending)
            .bind(ts)
            .bind(stream_id)
            .bind(meta);
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

struct IosFinanceAccountRegistration;
impl TransformRegistration for IosFinanceAccountRegistration {
    fn source_table(&self) -> &'static str { "stream_ios_financekit" }
    fn target_table(&self) -> &'static str { "financial_account" }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(IosFinanceAccountTransform))
    }
}
inventory::submit! { &IosFinanceAccountRegistration as &dyn TransformRegistration }

struct FinanceKitTransactionRegistration;
impl TransformRegistration for FinanceKitTransactionRegistration {
    fn source_table(&self) -> &'static str { "stream_ios_financekit" }
    fn target_table(&self) -> &'static str { "financial_transaction" }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(FinanceKitTransactionTransform))
    }
}
inventory::submit! { &FinanceKitTransactionRegistration as &dyn TransformRegistration }
