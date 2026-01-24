//! iOS FinanceKit data processor and transforms

pub mod transform;

use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    error::{Error, Result},
    sources::{
        base::validate_timestamp_reasonable,
        push_stream::{IngestPayload, PushResult, PushStream},
    },
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
        self.validate_payload(&payload)?;

        let mut result = PushResult::new(payload.records.len());

        for record in &payload.records {
            let timestamp = record
                .get("date")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::Other("Missing date in record".into()))?;

            let timestamp_dt = chrono::DateTime::parse_from_rfc3339(timestamp)
                .map_err(|e| Error::Other(format!("Invalid date format: {e}")))?
                .with_timezone(&Utc);
            validate_timestamp_reasonable(timestamp_dt)?;

            // Write to object storage via StreamWriter
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
