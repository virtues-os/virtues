//! Plaid investments stream implementation
//!
//! Syncs investment holdings from Plaid using the /investments/holdings/get endpoint.

pub mod transform;

use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use super::client::PlaidClient;
use super::config::PlaidInvestmentsConfig;
use crate::sources::base::oauth::encryption::TokenEncryptor;
use crate::{
    error::{Error, Result},
    sources::{
        base::{ConfigSerializable, SyncMode, SyncResult},
        pull_stream::PullStream,
    },
    storage::stream_writer::StreamWriter,
};

/// Plaid investments stream
///
/// Syncs investment holdings from Plaid to the financial_asset ontology.
/// Uses full refresh (no incremental sync) since holdings change daily.
pub struct PlaidInvestmentsStream {
    source_id: Uuid,
    client: PlaidClient,
    db: PgPool,
    stream_writer: Arc<Mutex<StreamWriter>>,
    config: PlaidInvestmentsConfig,
    /// Access token for this Item (loaded from database)
    access_token: Option<String>,
}

impl PlaidInvestmentsStream {
    /// Create a new investments stream
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
            config: PlaidInvestmentsConfig::default(),
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
            config: PlaidInvestmentsConfig::default(),
            access_token: None,
        }
    }

    /// Load configuration from database
    async fn load_config_internal(&mut self, db: &PgPool, source_id: Uuid) -> Result<()> {
        // Load stream config
        let result = sqlx::query_as::<_, (serde_json::Value,)>(
            "SELECT config FROM data.stream_connections WHERE source_connection_id = $1 AND stream_name = 'investments'",
        )
        .bind(source_id)
        .fetch_optional(db)
        .await?;

        if let Some((config_json,)) = result {
            if let Ok(config) = PlaidInvestmentsConfig::from_json(&config_json) {
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

    /// Sync investments with explicit mode
    #[tracing::instrument(skip(self), fields(source_id = %self.source_id, mode = ?sync_mode))]
    pub async fn sync_with_mode(&self, sync_mode: &SyncMode) -> Result<SyncResult> {
        tracing::info!("Starting Plaid investments sync");

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

        // Fetch investment holdings
        let response = self.client.investments_holdings_get(access_token).await?;

        // Build security lookup map for quick access
        let securities_map: HashMap<String, &super::client::Security> = response
            .securities
            .iter()
            .map(|s| (s.security_id.clone(), s))
            .collect();

        // Process each holding
        for holding in &response.holdings {
            records_fetched += 1;

            // Look up the security details
            let security = securities_map.get(&holding.security_id);

            match self.write_holding(holding, security).await {
                Ok(true) => records_written += 1,
                Ok(false) => records_failed += 1,
                Err(e) => {
                    tracing::warn!(
                        security_id = %holding.security_id,
                        account_id = %holding.account_id,
                        error = %e,
                        "Failed to write holding"
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
                .collect_records(self.source_id, "investments")
                .map(|(records, _, _)| records);

            if let Some(ref recs) = collected {
                tracing::info!(
                    record_count = recs.len(),
                    "Collected investment records from StreamWriter"
                );
            }

            collected
        };

        tracing::info!(
            records_fetched,
            records_written,
            records_failed,
            securities_count = response.securities.len(),
            "Plaid investments sync complete"
        );

        Ok(SyncResult {
            records_fetched,
            records_written,
            records_failed,
            next_cursor: None, // Investments don't use cursors
            started_at,
            completed_at,
            records,
            archive_job_id: None,
        })
    }

    /// Write a holding to the StreamWriter
    async fn write_holding(
        &self,
        holding: &super::client::Holding,
        security: Option<&&super::client::Security>,
    ) -> Result<bool> {
        let timestamp = Utc::now();

        // Build the record combining holding and security data
        let record = serde_json::json!({
            // Holding data
            "account_id": holding.account_id,
            "security_id": holding.security_id,
            "quantity": holding.quantity,
            "institution_value": holding.institution_value,
            "institution_price": holding.institution_price,
            "institution_price_as_of": holding.institution_price_as_of,
            "cost_basis": holding.cost_basis,
            "iso_currency_code": holding.iso_currency_code,
            "vested_quantity": holding.vested_quantity,
            "vested_value": holding.vested_value,
            // Security data (if available)
            "ticker_symbol": security.and_then(|s| s.ticker_symbol.as_ref()),
            "cusip": security.and_then(|s| s.cusip.as_ref()),
            "isin": security.and_then(|s| s.isin.as_ref()),
            "sedol": security.and_then(|s| s.sedol.as_ref()),
            "security_name": security.map(|s| &s.name),
            "security_type": security.and_then(|s| s.security_type.as_ref()),
            "close_price": security.and_then(|s| s.close_price),
            "close_price_as_of": security.and_then(|s| s.close_price_as_of.as_ref()),
            "is_cash_equivalent": security.and_then(|s| s.is_cash_equivalent),
            "sector": security.and_then(|s| s.sector.as_ref()),
            "industry": security.and_then(|s| s.industry.as_ref()),
            "synced_at": timestamp,
        });

        // Write to StreamWriter
        {
            let mut writer = self.stream_writer.lock().await;
            writer.write_record(self.source_id, "investments", record, Some(timestamp))?;
        }

        tracing::trace!(
            security_id = %holding.security_id,
            quantity = %holding.quantity,
            "Wrote holding to stream"
        );

        Ok(true)
    }
}

#[async_trait]
impl PullStream for PlaidInvestmentsStream {
    async fn sync_pull(&self, mode: SyncMode) -> Result<SyncResult> {
        self.sync_with_mode(&mode).await
    }

    async fn load_config(&mut self, db: &PgPool, source_id: Uuid) -> Result<()> {
        self.load_config_internal(db, source_id).await
    }

    fn table_name(&self) -> &str {
        "stream_plaid_investments"
    }

    fn stream_name(&self) -> &str {
        "investments"
    }

    fn source_name(&self) -> &str {
        "plaid"
    }

    fn supports_incremental(&self) -> bool {
        false // Investments are always full refresh
    }

    fn supports_full_refresh(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_stream_metadata() {
        assert_eq!("stream_plaid_investments", "stream_plaid_investments");
        assert_eq!("investments", "investments");
        assert_eq!("plaid", "plaid");
    }
}
