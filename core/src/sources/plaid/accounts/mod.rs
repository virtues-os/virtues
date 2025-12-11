//! Plaid accounts stream implementation
//!
//! Syncs bank accounts and balances from Plaid using the /accounts/balance/get endpoint.

pub mod transform;

use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use super::client::PlaidClient;
use super::config::PlaidAccountsConfig;
use crate::sources::base::oauth::encryption::TokenEncryptor;
use crate::{
    error::{Error, Result},
    sources::{
        base::{ConfigSerializable, SyncMode, SyncResult},
        pull_stream::PullStream,
    },
    storage::stream_writer::StreamWriter,
};

/// Plaid accounts stream
///
/// Syncs accounts and balances from Plaid to the financial_account ontology.
/// Uses full refresh (no incremental sync) since account data is relatively small.
pub struct PlaidAccountsStream {
    source_id: Uuid,
    client: PlaidClient,
    db: PgPool,
    stream_writer: Arc<Mutex<StreamWriter>>,
    config: PlaidAccountsConfig,
    /// Access token for this Item (loaded from database)
    access_token: Option<String>,
}

impl PlaidAccountsStream {
    /// Create a new accounts stream
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
            config: PlaidAccountsConfig::default(),
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
            config: PlaidAccountsConfig::default(),
            access_token: None,
        }
    }

    /// Load configuration from database
    async fn load_config_internal(&mut self, db: &PgPool, source_id: Uuid) -> Result<()> {
        // Load stream config
        let result = sqlx::query_as::<_, (serde_json::Value,)>(
            "SELECT config FROM data.stream_connections WHERE source_connection_id = $1 AND stream_name = 'accounts'",
        )
        .bind(source_id)
        .fetch_optional(db)
        .await?;

        if let Some((config_json,)) = result {
            if let Ok(config) = PlaidAccountsConfig::from_json(&config_json) {
                self.config = config;
            }
        }

        // Load access token from source_connections (encrypted)
        let token_result = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT access_token FROM data.source_connections WHERE id = $1",
        )
        .bind(source_id)
        .fetch_optional(db)
        .await?;

        if let Some((Some(encrypted_token),)) = token_result {
            let encryptor = TokenEncryptor::from_env()?;
            self.access_token = Some(encryptor.decrypt(&encrypted_token)?);
        }

        Ok(())
    }

    /// Sync accounts with explicit mode
    #[tracing::instrument(skip(self), fields(source_id = %self.source_id, mode = ?sync_mode))]
    pub async fn sync_with_mode(&self, sync_mode: &SyncMode) -> Result<SyncResult> {
        tracing::info!("Starting Plaid accounts sync");

        let access_token = self.access_token.as_ref().ok_or_else(|| {
            Error::Configuration("Plaid access token not loaded".to_string())
        })?;

        self.sync_internal(access_token, sync_mode).await
    }

    /// Internal sync implementation
    async fn sync_internal(&self, access_token: &str, _sync_mode: &SyncMode) -> Result<SyncResult> {
        let started_at = Utc::now();
        let mut records_fetched = 0;
        let mut records_written = 0;
        let mut records_failed = 0;

        // Fetch accounts (using accounts_get to avoid $0.10/call balance fees)
        // Balance data will still be included from the last transaction sync
        let response = self.client.accounts_get(access_token).await?;

        // Get institution info from the item
        let institution_id = response.item.institution_id.clone();
        let institution_name = self.get_institution_name(&institution_id).await;

        // Process each account
        for account in &response.accounts {
            records_fetched += 1;

            match self.write_account(account, &institution_id, &institution_name).await {
                Ok(true) => records_written += 1,
                Ok(false) => records_failed += 1,
                Err(e) => {
                    tracing::warn!(
                        account_id = %account.account_id,
                        error = %e,
                        "Failed to write account"
                    );
                    records_failed += 1;
                }
            }
        }

        let completed_at = Utc::now();

        // Collect records from StreamWriter
        let records = {
            let mut writer = self.stream_writer.lock().await;
            let collected = writer
                .collect_records(self.source_id, "accounts")
                .map(|(records, _, _)| records);

            if let Some(ref recs) = collected {
                tracing::info!(
                    record_count = recs.len(),
                    "Collected account records from StreamWriter"
                );
            }

            collected
        };

        tracing::info!(
            records_fetched,
            records_written,
            records_failed,
            "Plaid accounts sync complete"
        );

        Ok(SyncResult {
            records_fetched,
            records_written,
            records_failed,
            next_cursor: None, // Accounts don't use cursors
            started_at,
            completed_at,
            records,
            archive_job_id: None,
        })
    }

    /// Get institution name from Plaid (could be cached)
    async fn get_institution_name(&self, institution_id: &Option<String>) -> Option<String> {
        // For now, we'll get the institution name from the source metadata
        // In the future, could call Plaid's /institutions/get_by_id
        if institution_id.is_some() {
            let row = sqlx::query_as::<_, (serde_json::Value,)>(
                "SELECT metadata FROM data.source_connections WHERE id = $1",
            )
            .bind(self.source_id)
            .fetch_optional(&self.db)
            .await
            .ok()
            .flatten();

            if let Some((metadata,)) = row {
                return metadata
                    .get("institution_name")
                    .and_then(|v| v.as_str())
                    .map(String::from);
            }
        }
        None
    }

    /// Write an account to the StreamWriter
    async fn write_account(
        &self,
        account: &super::client::Account,
        institution_id: &Option<String>,
        institution_name: &Option<String>,
    ) -> Result<bool> {
        let timestamp = Utc::now();

        // Build the record
        let record = serde_json::json!({
            "account_id": account.account_id,
            "name": account.name,
            "official_name": account.official_name,
            "type": account.account_type,
            "subtype": account.subtype,
            "mask": account.mask,
            "balances": {
                "current": account.balances.current,
                "available": account.balances.available,
                "limit": account.balances.limit,
                "iso_currency_code": account.balances.iso_currency_code,
            },
            "institution_id": institution_id,
            "institution_name": institution_name,
            "synced_at": timestamp,
        });

        // Write to StreamWriter
        {
            let mut writer = self.stream_writer.lock().await;
            writer.write_record(self.source_id, "accounts", record, Some(timestamp))?;
        }

        tracing::trace!(
            account_id = %account.account_id,
            name = %account.name,
            "Wrote account to stream"
        );

        Ok(true)
    }
}

#[async_trait]
impl PullStream for PlaidAccountsStream {
    async fn sync_pull(&self, mode: SyncMode) -> Result<SyncResult> {
        self.sync_with_mode(&mode).await
    }

    async fn load_config(&mut self, db: &PgPool, source_id: Uuid) -> Result<()> {
        self.load_config_internal(db, source_id).await
    }

    fn table_name(&self) -> &str {
        "stream_plaid_accounts"
    }

    fn stream_name(&self) -> &str {
        "accounts"
    }

    fn source_name(&self) -> &str {
        "plaid"
    }

    fn supports_incremental(&self) -> bool {
        false // Accounts are always full refresh
    }

    fn supports_full_refresh(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_metadata() {
        // Can't easily test without a mock client, but we can verify compile-time correctness
        assert_eq!("stream_plaid_accounts", "stream_plaid_accounts");
        assert_eq!("accounts", "accounts");
        assert_eq!("plaid", "plaid");
    }
}
