//! Barometer stream to environment_pressure ontology transformation

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::database::Database;
use crate::error::Result;
use crate::jobs::TransformContext;
use crate::sources::base::{OntologyTransform, TransformResult};

/// Batch size for bulk inserts
const BATCH_SIZE: usize = 500;

/// Transform iOS barometer data to environment_pressure ontology
pub struct IosBarometerTransform;

#[async_trait]
impl OntologyTransform for IosBarometerTransform {
    fn source_table(&self) -> &str {
        "stream_ios_barometer"
    }

    fn target_table(&self) -> &str {
        "environment_pressure"
    }

    fn domain(&self) -> &str {
        "environment"
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

        let checkpoint_key = "ios_barometer_to_environment_pressure";
        let batches = data_source
            .read_with_checkpoint(&source_id, "barometer", checkpoint_key)
            .await?;

        let mut pending_records: Vec<(f64, Option<f64>, DateTime<Utc>, Uuid, serde_json::Value)> = Vec::new();

        for batch in batches {
            for record in &batch.records {
                records_read += 1;

                let pressure_hpa = record.get("pressure_hpa").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let relative_altitude = record.get("relative_altitude").and_then(|v| v.as_f64());
                
                let timestamp = record
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                    .unwrap_or_else(|| Utc::now());

                let stream_id = record
                    .get("id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .unwrap_or_else(|| Uuid::new_v4());

                let metadata = record.get("metadata").cloned().unwrap_or(serde_json::json!({}));

                pending_records.push((
                    pressure_hpa,
                    relative_altitude,
                    timestamp,
                    stream_id,
                    metadata,
                ));

                last_processed_id = Some(stream_id.to_string());

                if pending_records.len() >= BATCH_SIZE {
                    let written = execute_pressure_batch_insert(db, &pending_records).await?;
                    records_written += written;
                    pending_records.clear();
                }
            }

            if let Some(max_ts) = batch.max_timestamp {
                data_source
                    .update_checkpoint(&source_id, "barometer", checkpoint_key, max_ts)
                    .await?;
            }
        }

        if !pending_records.is_empty() {
            let written = execute_pressure_batch_insert(db, &pending_records).await?;
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

async fn execute_pressure_batch_insert(
    db: &Database,
    records: &[(f64, Option<f64>, DateTime<Utc>, Uuid, serde_json::Value)],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let query_str = Database::build_batch_insert_query(
        "data_environment_pressure",
        &[
            "pressure_hpa",
            "relative_altitude_change",
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

    for (pressure, altitude, ts, stream_id, meta) in records {
        query = query
            .bind(pressure)
            .bind(altitude)
            .bind(ts)
            .bind(stream_id)
            .bind("stream_ios_barometer")
            .bind("ios")
            .bind(meta);
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

// Self-registration
struct IosBarometerRegistration;
impl crate::sources::base::TransformRegistration for IosBarometerRegistration {
    fn source_table(&self) -> &'static str {
        "stream_ios_barometer"
    }
    fn target_table(&self) -> &'static str {
        "environment_pressure"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(IosBarometerTransform))
    }
}
inventory::submit! { &IosBarometerRegistration as &dyn crate::sources::base::TransformRegistration }
