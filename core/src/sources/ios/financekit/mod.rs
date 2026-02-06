//! iOS FinanceKit data processor and transforms

pub mod transform;

use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    error::Result,
    sources::push_stream::{IngestPayload, PushResult, PushStream},
    storage::stream_writer::StreamWriter,
};

pub use transform::FinanceKitTransactionTransform;

/// iOS FinanceKit stream implementing PushStream trait
///
/// Receives financial data pushed from iOS devices via /ingest endpoint.
pub struct IosFinanceKitStream {
    _db: SqlitePool,
    stream_writer: Arc<Mutex<StreamWriter>>,
}

impl IosFinanceKitStream {
    /// Create a new IosFinanceKitStream
    pub fn new(db: SqlitePool, stream_writer: Arc<Mutex<StreamWriter>>) -> Self {
        Self {
            _db: db,
            stream_writer,
        }
    }
}

#[async_trait]
impl PushStream for IosFinanceKitStream {
    async fn receive_push(&self, source_id: &str, payload: IngestPayload) -> Result<PushResult> {
        let mut result = PushResult::new(payload.records.len());

        for record in &payload.records {
            // Each record is a wrapper: { accounts: [...], transactions: [...] }
            // Derive timestamp from the first transaction's date, or fall back to payload timestamp
            let timestamp_dt = record
                .get("transactions")
                .and_then(|v| v.as_array())
                .and_then(|txs| txs.first())
                .and_then(|tx| tx.get("date"))
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or(payload.timestamp);

            // Write the full wrapper record to the lake — transforms know
            // how to read the accounts and transactions arrays from it
            {
                let mut writer = self.stream_writer.lock().await;
                writer.write_record(source_id, "financekit", record.clone(), Some(timestamp_dt))?;
            }

            result.records_written += 1;
        }

        tracing::info!(
            "Processed {} FinanceKit records from device {}",
            result.records_written,
            payload.device_id
        );

        Ok(result)
    }

    fn table_name(&self) -> &str {
        "stream_ios_financekit"
    }

    fn stream_name(&self) -> &str {
        "financekit"
    }

    fn source_name(&self) -> &str {
        "ios"
    }
}
