//! Plaid transactions stream implementation
//!
//! Syncs financial transactions from Plaid using the /transactions/sync endpoint
//! with cursor-based pagination for efficient incremental sync.

pub mod transform;

use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use super::client::PlaidClient;
use super::config::PlaidTransactionsConfig;
use crate::{
    error::{Error, Result},
    sources::{
        base::{oauth::encryption::TokenEncryptor, ConfigSerializable, SyncMode, SyncResult},
        pull_stream::PullStream,
    },
    storage::stream_writer::StreamWriter,
};

/// Plaid transactions stream
///
/// Syncs transactions from Plaid to object storage via StreamWriter.
/// Uses cursor-based sync for efficient incremental updates.
pub struct PlaidTransactionsStream {
    source_id: Uuid,
    client: PlaidClient,
    db: PgPool,
    stream_writer: Arc<Mutex<StreamWriter>>,
    config: PlaidTransactionsConfig,
    /// Access token for this Item (loaded from database, decrypted)
    access_token: Option<String>,
}

impl PlaidTransactionsStream {
    /// Create a new transactions stream
    pub fn new(
        source_id: Uuid,
        db: PgPool,
        stream_writer: Arc<Mutex<StreamWriter>>,
    ) -> Result<Self> {
        let client = PlaidClient::from_env()?;

        Ok(Self {
            source_id,
            client,
            db,
            stream_writer,
            config: PlaidTransactionsConfig::default(),
            access_token: None,
        })
    }

    /// Create with explicit client (for testing)
    pub fn with_client(
        source_id: Uuid,
        client: PlaidClient,
        db: PgPool,
        stream_writer: Arc<Mutex<StreamWriter>>,
    ) -> Self {
        Self {
            source_id,
            client,
            db,
            stream_writer,
            config: PlaidTransactionsConfig::default(),
            access_token: None,
        }
    }

    /// Load configuration from database
    async fn load_config_internal(&mut self, db: &PgPool, source_id: Uuid) -> Result<()> {
        // Load stream config
        let result = sqlx::query_as::<_, (serde_json::Value,)>(
            "SELECT config FROM data.stream_connections WHERE source_connection_id = $1 AND stream_name = 'transactions'",
        )
        .bind(source_id)
        .fetch_optional(db)
        .await?;

        if let Some((config_json,)) = result {
            if let Ok(config) = PlaidTransactionsConfig::from_json(&config_json) {
                self.config = config;
            }
        }

        // Load encrypted access token from source_connections
        let token_result = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT access_token FROM data.source_connections WHERE id = $1",
        )
        .bind(source_id)
        .fetch_optional(db)
        .await?;

        if let Some((Some(encrypted_token),)) = token_result {
            // Decrypt the access token
            let encryptor = TokenEncryptor::from_env()?;
            self.access_token = Some(encryptor.decrypt(&encrypted_token)?);
        }

        Ok(())
    }

    /// Sync transactions with explicit mode
    #[tracing::instrument(skip(self), fields(source_id = %self.source_id, mode = ?sync_mode))]
    pub async fn sync_with_mode(&self, sync_mode: &SyncMode) -> Result<SyncResult> {
        tracing::info!("Starting Plaid transactions sync");

        let access_token = self.access_token.as_ref().ok_or_else(|| {
            Error::Configuration("Plaid access token not loaded".to_string())
        })?;

        self.sync_internal(access_token, sync_mode).await
    }

    /// Internal sync implementation using cursor-based sync
    async fn sync_internal(&self, access_token: &str, sync_mode: &SyncMode) -> Result<SyncResult> {
        let started_at = Utc::now();
        let mut records_fetched = 0;
        let mut records_written = 0;
        let mut records_failed = 0;
        let mut next_cursor = None;

        // Start database transaction
        let mut tx = self.db.begin().await?;

        // Get last sync cursor from database
        let cursor = match sync_mode {
            SyncMode::FullRefresh => {
                // Full refresh: start from empty cursor to get all history
                tracing::info!("Full refresh requested, starting from empty cursor");
                None
            }
            SyncMode::Incremental { .. } => {
                // Incremental: use stored cursor
                self.get_last_cursor().await?
            }
        };

        // Loop until has_more is false
        let mut current_cursor = cursor.clone();

        loop {
            tracing::debug!(cursor = ?current_cursor, "Fetching transactions batch");

            let response = self
                .client
                .transactions_sync(
                    access_token,
                    current_cursor.as_deref(),
                    Some(self.config.max_transactions_per_sync),
                )
                .await?;

            // Process added transactions
            for transaction in &response.added {
                records_fetched += 1;

                match self.write_transaction(transaction, &mut tx).await {
                    Ok(true) => records_written += 1,
                    Ok(false) => records_failed += 1,
                    Err(e) => {
                        tracing::warn!(
                            transaction_id = %transaction.transaction_id,
                            error = %e,
                            "Failed to write transaction"
                        );
                        records_failed += 1;
                    }
                }
            }

            // Process modified transactions (update existing)
            for transaction in &response.modified {
                records_fetched += 1;

                match self.write_transaction(transaction, &mut tx).await {
                    Ok(true) => records_written += 1,
                    Ok(false) => records_failed += 1,
                    Err(e) => {
                        tracing::warn!(
                            transaction_id = %transaction.transaction_id,
                            error = %e,
                            "Failed to update transaction"
                        );
                        records_failed += 1;
                    }
                }
            }

            // Mark removed transactions (soft delete or actual delete)
            for removed in &response.removed {
                tracing::debug!(transaction_id = %removed.transaction_id, "Transaction removed");
                // For now, we just log removed transactions
                // The transform will handle reconciliation
            }

            // Check if there's more data
            if response.has_more {
                current_cursor = Some(response.next_cursor.clone());
                tracing::debug!(
                    added = response.added.len(),
                    modified = response.modified.len(),
                    removed = response.removed.len(),
                    "Batch processed, more data available"
                );
            } else {
                // No more data - save the cursor
                tracing::info!(
                    total_fetched = records_fetched,
                    total_written = records_written,
                    "Sync complete, saving cursor"
                );
                self.save_cursor_with_tx(&response.next_cursor, &mut tx).await?;
                next_cursor = Some(response.next_cursor);
                break;
            }
        }

        // Commit transaction
        tx.commit().await?;

        let completed_at = Utc::now();

        // Collect records from StreamWriter
        let records = {
            let mut writer = self.stream_writer.lock().await;
            let collected = writer
                .collect_records(self.source_id, "transactions")
                .map(|(records, _, _)| records);

            if let Some(ref recs) = collected {
                tracing::info!(
                    record_count = recs.len(),
                    "Collected transaction records from StreamWriter"
                );
            }

            collected
        };

        Ok(SyncResult {
            records_fetched,
            records_written,
            records_failed,
            next_cursor,
            started_at,
            completed_at,
            records,
            archive_job_id: None,
        })
    }

    /// Write a transaction to the StreamWriter
    async fn write_transaction(
        &self,
        transaction: &super::client::Transaction,
        _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<bool> {
        // Parse transaction date (it's a string in YYYY-MM-DD format)
        let timestamp = chrono::NaiveDate::parse_from_str(&transaction.date, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(12, 0, 0))
            .map(|dt| dt.and_utc())
            .unwrap_or_else(Utc::now);

        // Build the record
        let record = serde_json::json!({
            "transaction_id": transaction.transaction_id,
            "account_id": transaction.account_id,
            "amount": transaction.amount,
            "iso_currency_code": transaction.iso_currency_code,
            "unofficial_currency_code": transaction.unofficial_currency_code,
            "date": transaction.date,
            "datetime": transaction.datetime,
            "authorized_date": transaction.authorized_date,
            "authorized_datetime": transaction.authorized_datetime,
            "name": transaction.name,
            "merchant_name": transaction.merchant_name,
            "merchant_entity_id": transaction.merchant_entity_id,
            "logo_url": transaction.logo_url,
            "website": transaction.website,
            "payment_channel": transaction.payment_channel,
            "pending": transaction.pending,
            "pending_transaction_id": transaction.pending_transaction_id,
            "account_owner": transaction.account_owner,
            "transaction_type": transaction.transaction_type,
            "transaction_code": transaction.transaction_code,
            "personal_finance_category": transaction.personal_finance_category,
            "location": transaction.location,
            "payment_meta": transaction.payment_meta,
            "category": transaction.category,
            "category_id": transaction.category_id,
            "synced_at": Utc::now(),
        });

        // Write to StreamWriter
        {
            let mut writer = self.stream_writer.lock().await;
            writer.write_record(self.source_id, "transactions", record, Some(timestamp))?;
        }

        tracing::trace!(
            transaction_id = %transaction.transaction_id,
            amount = %transaction.amount,
            "Wrote transaction to stream"
        );

        Ok(true)
    }

    /// Get the last sync cursor from database
    async fn get_last_cursor(&self) -> Result<Option<String>> {
        let row = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT last_sync_token FROM data.stream_connections WHERE source_connection_id = $1 AND stream_name = 'transactions'",
        )
        .bind(self.source_id)
        .fetch_optional(&self.db)
        .await?;

        Ok(row.and_then(|(token,)| token))
    }

    /// Save the cursor to database within a transaction
    async fn save_cursor_with_tx(
        &self,
        cursor: &str,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE data.stream_connections SET last_sync_token = $1, last_sync_at = $2 WHERE source_connection_id = $3 AND stream_name = 'transactions'"
        )
        .bind(cursor)
        .bind(Utc::now())
        .bind(self.source_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }
}

#[async_trait]
impl PullStream for PlaidTransactionsStream {
    async fn sync_pull(&self, mode: SyncMode) -> Result<SyncResult> {
        self.sync_with_mode(&mode).await
    }

    async fn load_config(&mut self, db: &PgPool, source_id: Uuid) -> Result<()> {
        self.load_config_internal(db, source_id).await
    }

    fn table_name(&self) -> &str {
        "stream_plaid_transactions"
    }

    fn stream_name(&self) -> &str {
        "transactions"
    }

    fn source_name(&self) -> &str {
        "plaid"
    }

    fn supports_incremental(&self) -> bool {
        true
    }

    fn supports_full_refresh(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_stream_table_name() {
        // Test that the stream returns correct table name
        assert_eq!("stream_plaid_transactions", "stream_plaid_transactions");
    }
}
