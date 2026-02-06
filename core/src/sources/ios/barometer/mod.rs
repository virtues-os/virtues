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

pub mod transform;

pub struct IosBarometerStream {
    _db: SqlitePool,
    stream_writer: Arc<Mutex<StreamWriter>>,
}

impl IosBarometerStream {
    pub fn new(db: SqlitePool, stream_writer: Arc<Mutex<StreamWriter>>) -> Self {
        Self {
            _db: db,
            stream_writer,
        }
    }

    pub fn descriptor() -> RegisteredStream {
        RegisteredStream::new("barometer")
            .config_schema(serde_json::json!({
                "type": "object",
                "properties": {}
            }))
            .build()
    }
}

#[async_trait]
impl PushStream for IosBarometerStream {
    async fn receive_push(&self, source_id: &str, payload: IngestPayload) -> Result<PushResult> {
        let mut result = PushResult::new(payload.records.len());

        for record in &payload.records {
            let timestamp_dt =
                if let Some(ts_str) = record.get("timestamp").and_then(|v| v.as_str()) {
                    chrono::DateTime::parse_from_rfc3339(ts_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or(payload.timestamp)
                } else {
                    payload.timestamp
                };

            {
                let mut writer = self.stream_writer.lock().await;
                writer.write_record(source_id, "barometer", record.clone(), Some(timestamp_dt))?;
            }
            result.records_written += 1;
        }

        tracing::info!(
            "Processed {} Barometer records from device {}",
            result.records_written,
            payload.device_id
        );

        Ok(result)
    }

    fn table_name(&self) -> &str {
        "stream_ios_barometer"
    }

    fn stream_name(&self) -> &str {
        "barometer"
    }

    fn source_name(&self) -> &str {
        "ios"
    }
}
