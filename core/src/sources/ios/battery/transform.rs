//! Battery stream to device_battery ontology transformation

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::database::Database;
use crate::error::Result;
use crate::jobs::TransformContext;
use crate::sources::base::{OntologyTransform, TransformResult};

/// Batch size for bulk inserts
const BATCH_SIZE: usize = 500;

/// Transform iOS battery data to device_battery ontology
pub struct IosBatteryTransform;

#[async_trait]
impl OntologyTransform for IosBatteryTransform {
    fn source_table(&self) -> &str {
        "stream_ios_battery"
    }

    fn target_table(&self) -> &str {
        "device_battery"
    }

    fn domain(&self) -> &str {
        "device"
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

        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;

        let checkpoint_key = "ios_battery_to_device_battery";
        let batches = data_source
            .read_with_checkpoint(&source_id, "battery", checkpoint_key)
            .await?;

        let mut pending_records: Vec<(String, f64, String, bool, DateTime<Utc>, String, serde_json::Value)> = Vec::new();

        for batch in batches {
            for record in &batch.records {
                records_read += 1;

                // iOS sends: level (0.0-1.0), state, isLowPowerMode
                let battery_level = record.get("level").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let battery_state = record.get("state").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                let is_low_power_mode = record.get("isLowPowerMode").and_then(|v| v.as_bool()).unwrap_or(false);
                
                let timestamp = record
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                    .unwrap_or_else(|| Utc::now());

                let stream_id = record
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| Uuid::new_v4().to_string());

                let metadata = record.get("metadata").cloned().unwrap_or(serde_json::json!({}));

                last_processed_id = Some(stream_id.clone());

                pending_records.push((
                    Uuid::new_v4().to_string(),
                    battery_level,
                    battery_state,
                    is_low_power_mode,
                    timestamp,
                    stream_id,
                    metadata,
                ));

                if pending_records.len() >= BATCH_SIZE {
                    let written = execute_battery_batch_insert(db, &pending_records).await?;
                    records_written += written;
                    pending_records.clear();
                }
            }

            if let Some(max_ts) = batch.max_timestamp {
                data_source
                    .update_checkpoint(&source_id, "battery", checkpoint_key, max_ts)
                    .await?;
            }
        }

        if !pending_records.is_empty() {
            let written = execute_battery_batch_insert(db, &pending_records).await?;
            records_written += written;
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

async fn execute_battery_batch_insert(
    db: &Database,
    records: &[(String, f64, String, bool, DateTime<Utc>, String, serde_json::Value)],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let query_str = Database::build_batch_insert_query(
        "data_device_battery",
        &[
            "id",
            "battery_level",
            "battery_state",
            "is_low_power_mode",
            "timestamp",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut query = sqlx::query(&query_str);

    for (id, level, state, low_power, ts, stream_id, meta) in records {
        query = query
            .bind(id)
            .bind(level)
            .bind(state)
            .bind(if *low_power { 1 } else { 0 })
            .bind(ts)
            .bind(stream_id)
            .bind("stream_ios_battery")
            .bind("ios")
            .bind(meta);
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

// Self-registration
struct IosBatteryRegistration;
impl crate::sources::base::TransformRegistration for IosBatteryRegistration {
    fn source_table(&self) -> &'static str {
        "stream_ios_battery"
    }
    fn target_table(&self) -> &'static str {
        "device_battery"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(IosBatteryTransform))
    }
}
inventory::submit! { &IosBatteryRegistration as &dyn crate::sources::base::TransformRegistration }
