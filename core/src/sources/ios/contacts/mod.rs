use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;


use crate::{
    error::{Error, Result},
    registry::RegisteredStream,
    sources::push_stream::{IngestPayload, PushResult, PushStream},
    storage::stream_writer::StreamWriter,
};

pub struct IosContactsStream {
    _db: SqlitePool,
    stream_writer: Arc<Mutex<StreamWriter>>,
}

impl IosContactsStream {
    pub fn new(db: SqlitePool, stream_writer: Arc<Mutex<StreamWriter>>) -> Self {
        Self {
            _db: db,
            stream_writer,
        }
    }

    pub fn descriptor() -> RegisteredStream {
        RegisteredStream::new("contacts")
            .config_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "sync_photos": { "type": "boolean", "default": false }
                }
            }))
            .build()
    }
}

#[async_trait]
impl PushStream for IosContactsStream {
    async fn receive_push(&self, source_id: &str, payload: IngestPayload) -> Result<PushResult> {
        let mut result = PushResult::new(payload.records.len());

        for record in &payload.records {
            // We expect contacts to maybe NOT have a timestamp in the record itself?
            // Or maybe they do (last_updated).
            // If missing, default to batch timestamp.
            let timestamp_dt =
                if let Some(ts_str) = record.get("last_updated").and_then(|v| v.as_str()) {
                    chrono::DateTime::parse_from_rfc3339(ts_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or(payload.timestamp)
                } else {
                    payload.timestamp
                };

            // Write to writer
            {
                let mut writer = self.stream_writer.lock().await;
                writer.write_record(source_id, "contacts", record.clone(), Some(timestamp_dt))?;
            }
            result.records_written += 1;
        }

        tracing::info!(
            "Processed {} Contacts records from device {}",
            result.records_written,
            payload.device_id
        );

        Ok(result)
    }

    fn table_name(&self) -> &str {
        "stream_ios_contacts"
    }

    fn stream_name(&self) -> &str {
        "contacts"
    }

    fn source_name(&self) -> &str {
        "ios"
    }
}
