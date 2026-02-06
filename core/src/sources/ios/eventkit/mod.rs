//! iOS EventKit data source
//!
//! Processor for EventKit data (calendar events and reminders) pushed from iOS devices.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    error::Result,
    registry::RegisteredStream,
    sources::push_stream::{IngestPayload, PushResult, PushStream},
    storage::stream_writer::StreamWriter,
};

pub mod transform;

pub struct IosEventKitStream {
    _db: SqlitePool,
    stream_writer: Arc<Mutex<StreamWriter>>,
}

impl IosEventKitStream {
    pub fn new(db: SqlitePool, stream_writer: Arc<Mutex<StreamWriter>>) -> Self {
        Self {
            _db: db,
            stream_writer,
        }
    }

    pub fn descriptor() -> RegisteredStream {
        RegisteredStream::new("eventkit")
            .config_schema(serde_json::json!({
                "type": "object",
                "properties": {}
            }))
            .build()
    }
}

#[async_trait]
impl PushStream for IosEventKitStream {
    async fn receive_push(&self, source_id: &str, payload: IngestPayload) -> Result<PushResult> {
        let mut result = PushResult::new(payload.records.len());

        for record in &payload.records {
            // Handle calendar events
            if let Some(events) = record.get("events").and_then(|v| v.as_array()) {
                for event in events {
                    let timestamp_dt =
                        if let Some(ts_str) = event.get("startDate").and_then(|v| v.as_str()) {
                            chrono::DateTime::parse_from_rfc3339(ts_str)
                                .map(|dt| dt.with_timezone(&Utc))
                                .unwrap_or(payload.timestamp)
                        } else {
                            payload.timestamp
                        };

                    // Add record_type to distinguish events from reminders
                    let mut event_record = event.clone();
                    if let Some(obj) = event_record.as_object_mut() {
                        obj.insert("record_type".to_string(), serde_json::json!("event"));
                    }

                    {
                        let mut writer = self.stream_writer.lock().await;
                        writer.write_record(source_id, "eventkit", event_record, Some(timestamp_dt))?;
                    }
                    result.records_written += 1;
                }
            }

            // Handle reminders
            if let Some(reminders) = record.get("reminders").and_then(|v| v.as_array()) {
                for reminder in reminders {
                    let timestamp_dt =
                        if let Some(ts_str) = reminder.get("dueDate").and_then(|v| v.as_str()) {
                            chrono::DateTime::parse_from_rfc3339(ts_str)
                                .map(|dt| dt.with_timezone(&Utc))
                                .unwrap_or(payload.timestamp)
                        } else {
                            payload.timestamp
                        };

                    // Add record_type to distinguish reminders from events
                    let mut reminder_record = reminder.clone();
                    if let Some(obj) = reminder_record.as_object_mut() {
                        obj.insert("record_type".to_string(), serde_json::json!("reminder"));
                    }

                    {
                        let mut writer = self.stream_writer.lock().await;
                        writer.write_record(source_id, "eventkit", reminder_record, Some(timestamp_dt))?;
                    }
                    result.records_written += 1;
                }
            }
        }

        tracing::info!(
            "Processed {} EventKit records from device {}",
            result.records_written,
            payload.device_id
        );

        Ok(result)
    }

    fn table_name(&self) -> &str {
        "stream_ios_eventkit"
    }

    fn stream_name(&self) -> &str {
        "eventkit"
    }

    fn source_name(&self) -> &str {
        "ios"
    }
}
