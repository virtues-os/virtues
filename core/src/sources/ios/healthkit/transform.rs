//! HealthKit to health ontology transformations
//!
//! Transforms raw HealthKit data from stream_ios_healthkit into normalized
//! health ontology tables (heart_rate, hrv, steps, sleep, workout).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::database::Database;
use crate::error::Result;
use crate::jobs::TransformContext;
use crate::sources::base::{OntologyTransform, TransformRegistration, TransformResult};

/// Batch size for bulk inserts
const BATCH_SIZE: usize = 500;

/// Transform HealthKit heart rate data to health_heart_rate ontology
pub struct HealthKitHeartRateTransform;

#[async_trait]
impl OntologyTransform for HealthKitHeartRateTransform {
    fn source_table(&self) -> &str {
        "stream_ios_healthkit"
    }

    fn target_table(&self) -> &str {
        "health_heart_rate"
    }

    fn domain(&self) -> &str {
        "health"
    }

    #[tracing::instrument(skip(self, db, context), fields(source_table = %self.source_table(), target_table = %self.target_table()))]
    async fn transform(
        &self,
        db: &Database,
        context: &crate::jobs::transform_context::TransformContext,
        source_id: String,
    ) -> Result<TransformResult> {
        let mut records_read = 0;
        let mut records_written = 0;
        let mut records_failed = 0;
        let mut last_processed_id: Option<String> = None;

        let transform_start = std::time::Instant::now();

        tracing::info!(
            source_id = %source_id,
            "Starting HealthKit to health_heart_rate transformation"
        );

        // Read stream data from data source using checkpoint
        let checkpoint_key = "healthkit_to_heart_rate";
        let read_start = std::time::Instant::now();
        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;
        let batches = data_source
            .read_with_checkpoint(&source_id, "healthkit", checkpoint_key)
            .await?;
        let read_duration = read_start.elapsed();

        tracing::info!(
            batch_count = batches.len(),
            read_duration_ms = read_duration.as_millis(),
            source_type = ?data_source.source_type(),
            "Fetched HealthKit batches from data source"
        );

        // Batch insert configuration
        let mut pending_records: Vec<(
            String,
            i32,
            DateTime<Utc>,
            String,
            serde_json::Value,
        )> = Vec::new();
        let mut batch_insert_total_ms = 0u128;
        let mut batch_insert_count = 0;

        let processing_start = std::time::Instant::now();

        for batch in batches {
            for record in &batch.records {
                records_read += 1;

                // Extract fields from JSONL record
                let metric_type = record.get("metric_type").and_then(|v| v.as_str()).unwrap_or("");
                if metric_type != "heart_rate" && metric_type != "resting_heart_rate" {
                    continue; // Skip non-heart-rate records
                }
                // Try iOS format first (value field with metric_type), then legacy format
                let Some(heart_rate) = record.get("value").and_then(|v| v.as_f64())
                    .or_else(|| record.get("heart_rate").and_then(|v| v.as_f64())) else {
                    continue;
                };

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

                let raw_data = record.get("raw_data").cloned();

                // Determine measurement context from raw_data or time of day
                let measurement_context = raw_data
                    .as_ref()
                    .and_then(|d| d.get("context"))
                    .and_then(|c| c.as_str())
                    .map(String::from);

                // Build metadata
                let metadata = serde_json::json!({
                    "healthkit_raw": raw_data,
                    "measurement_context": measurement_context,
                });

                last_processed_id = Some(stream_id.clone());

                // Add to pending batch
                pending_records.push((
                    Uuid::new_v4().to_string(),
                    heart_rate as i32,
                    timestamp,
                    stream_id,
                    metadata,
                ));

                // Execute batch insert when we reach batch size
                if pending_records.len() >= BATCH_SIZE {
                    let insert_start = std::time::Instant::now();
                    let batch_result = execute_heart_rate_batch_insert(db, &pending_records).await;
                    let insert_duration = insert_start.elapsed();
                    batch_insert_total_ms += insert_duration.as_millis();
                    batch_insert_count += 1;

                    tracing::info!(
                        batch_size = pending_records.len(),
                        insert_duration_ms = insert_duration.as_millis(),
                        "Executed batch insert"
                    );

                    match batch_result {
                        Ok(written) => {
                            records_written += written;
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                batch_size = pending_records.len(),
                                "Batch insert failed"
                            );
                            records_failed += pending_records.len();
                        }
                    }
                    pending_records.clear();
                }
            }

            // Update checkpoint after processing batch
            if let Some(max_ts) = batch.max_timestamp {
                data_source
                    .update_checkpoint(&source_id, "healthkit", checkpoint_key, max_ts)
                    .await?;
            }
        }

        // Insert any remaining records
        if !pending_records.is_empty() {
            let insert_start = std::time::Instant::now();
            let batch_result = execute_heart_rate_batch_insert(db, &pending_records).await;
            let insert_duration = insert_start.elapsed();
            batch_insert_total_ms += insert_duration.as_millis();
            batch_insert_count += 1;

            tracing::info!(
                batch_size = pending_records.len(),
                insert_duration_ms = insert_duration.as_millis(),
                "Executed final batch insert"
            );

            match batch_result {
                Ok(written) => {
                    records_written += written;
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        batch_size = pending_records.len(),
                        "Final batch insert failed"
                    );
                    records_failed += pending_records.len();
                }
            }
        }

        let processing_duration = processing_start.elapsed();
        let total_duration = transform_start.elapsed();

        tracing::info!(
            source_id = %source_id,
            records_read,
            records_written,
            records_failed,
            total_duration_ms = total_duration.as_millis(),
            processing_duration_ms = processing_duration.as_millis(),
            read_duration_ms = read_duration.as_millis(),
            batch_insert_total_ms,
            batch_insert_count,
            avg_batch_insert_ms = if batch_insert_count > 0 { batch_insert_total_ms / batch_insert_count as u128 } else { 0 },
            "HealthKit to health_heart_rate transformation completed"
        );

        Ok(TransformResult {
            records_read,
            records_written,
            records_failed,
            last_processed_id,
            chained_transforms: vec![],
        })
    }
}

/// Transform HealthKit HRV data to health_hrv ontology
pub struct HealthKitHRVTransform;

#[async_trait]
impl OntologyTransform for HealthKitHRVTransform {
    fn source_table(&self) -> &str {
        "stream_ios_healthkit"
    }

    fn target_table(&self) -> &str {
        "health_hrv"
    }

    fn domain(&self) -> &str {
        "health"
    }

    #[tracing::instrument(skip(self, db, context), fields(source_table = %self.source_table(), target_table = %self.target_table()))]
    async fn transform(
        &self,
        db: &Database,
        context: &crate::jobs::transform_context::TransformContext,
        source_id: String,
    ) -> Result<TransformResult> {
        let mut records_read = 0;
        let mut records_written = 0;
        let mut records_failed = 0;
        let mut last_processed_id: Option<String> = None;

        let transform_start = std::time::Instant::now();

        tracing::info!(
            source_id = %source_id,
            "Starting HealthKit to health_hrv transformation"
        );

        // Read stream data from data source using checkpoint
        let checkpoint_key = "healthkit_to_hrv";
        let read_start = std::time::Instant::now();
        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;
        let batches = data_source
            .read_with_checkpoint(&source_id, "healthkit", checkpoint_key)
            .await?;
        let read_duration = read_start.elapsed();

        tracing::info!(
            batch_count = batches.len(),
            read_duration_ms = read_duration.as_millis(),
            source_type = ?data_source.source_type(),
            "Fetched HealthKit batches from data source"
        );

        // Batch insert configuration
        let mut pending_records: Vec<(String, f64, DateTime<Utc>, String, serde_json::Value)> =
            Vec::new();
        let mut batch_insert_total_ms = 0u128;
        let mut batch_insert_count = 0;

        let processing_start = std::time::Instant::now();

        for batch in batches {
            for record in &batch.records {
                records_read += 1;

                // Extract fields from JSONL record
                let metric_type = record.get("metric_type").and_then(|v| v.as_str()).unwrap_or("");
                if metric_type != "heart_rate_variability" {
                    continue; // Skip non-HRV records
                }
                let Some(hrv) = record.get("value").and_then(|v| v.as_f64())
                    .or_else(|| record.get("hrv").and_then(|v| v.as_f64())) else {
                    continue;
                };

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

                let raw_data = record.get("raw_data").cloned();

                // Determine HRV measurement type (default to RMSSD for Apple Watch)
                let measurement_type = raw_data
                    .as_ref()
                    .and_then(|d| d.get("hrv_type"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("rmssd");

                let metadata = serde_json::json!({
                    "healthkit_raw": raw_data,
                    "measurement_type": measurement_type,
                });

                last_processed_id = Some(stream_id.clone());

                // Add to pending batch
                pending_records.push((
                    Uuid::new_v4().to_string(),
                    hrv,
                    timestamp,
                    stream_id,
                    metadata,
                ));

                // Execute batch insert when we reach batch size
                if pending_records.len() >= BATCH_SIZE {
                    let insert_start = std::time::Instant::now();
                    let batch_result = execute_hrv_batch_insert(db, &pending_records).await;
                    let insert_duration = insert_start.elapsed();
                    batch_insert_total_ms += insert_duration.as_millis();
                    batch_insert_count += 1;

                    tracing::info!(
                        batch_size = pending_records.len(),
                        insert_duration_ms = insert_duration.as_millis(),
                        "Executed batch insert"
                    );

                    match batch_result {
                        Ok(written) => {
                            records_written += written;
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                batch_size = pending_records.len(),
                                "Batch insert failed"
                            );
                            records_failed += pending_records.len();
                        }
                    }
                    pending_records.clear();
                }
            }

            // Update checkpoint after processing batch
            if let Some(max_ts) = batch.max_timestamp {
                data_source
                    .update_checkpoint(&source_id, "healthkit", checkpoint_key, max_ts)
                    .await?;
            }
        }

        // Insert any remaining records
        if !pending_records.is_empty() {
            let insert_start = std::time::Instant::now();
            let batch_result = execute_hrv_batch_insert(db, &pending_records).await;
            let insert_duration = insert_start.elapsed();
            batch_insert_total_ms += insert_duration.as_millis();
            batch_insert_count += 1;

            tracing::info!(
                batch_size = pending_records.len(),
                insert_duration_ms = insert_duration.as_millis(),
                "Executed final batch insert"
            );

            match batch_result {
                Ok(written) => {
                    records_written += written;
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        batch_size = pending_records.len(),
                        "Final batch insert failed"
                    );
                    records_failed += pending_records.len();
                }
            }
        }

        let processing_duration = processing_start.elapsed();
        let total_duration = transform_start.elapsed();

        tracing::info!(
            source_id = %source_id,
            records_read,
            records_written,
            records_failed,
            total_duration_ms = total_duration.as_millis(),
            processing_duration_ms = processing_duration.as_millis(),
            read_duration_ms = read_duration.as_millis(),
            batch_insert_total_ms,
            batch_insert_count,
            avg_batch_insert_ms = if batch_insert_count > 0 { batch_insert_total_ms / batch_insert_count as u128 } else { 0 },
            "HealthKit to health_hrv transformation completed"
        );

        Ok(TransformResult {
            records_read,
            records_written,
            records_failed,
            last_processed_id,
            chained_transforms: vec![],
        })
    }
}

/// Transform HealthKit steps data to health_steps ontology
pub struct HealthKitStepsTransform;

#[async_trait]
impl OntologyTransform for HealthKitStepsTransform {
    fn source_table(&self) -> &str {
        "stream_ios_healthkit"
    }

    fn target_table(&self) -> &str {
        "health_steps"
    }

    fn domain(&self) -> &str {
        "health"
    }

    #[tracing::instrument(skip(self, db, context), fields(source_table = %self.source_table(), target_table = %self.target_table()))]
    async fn transform(
        &self,
        db: &Database,
        context: &crate::jobs::transform_context::TransformContext,
        source_id: String,
    ) -> Result<TransformResult> {
        let mut records_read = 0;
        let mut records_written = 0;
        let mut records_failed = 0;
        let mut last_processed_id: Option<String> = None;

        let transform_start = std::time::Instant::now();

        tracing::info!(
            source_id = %source_id,
            "Starting HealthKit to health_steps transformation"
        );

        // Read stream data from data source using checkpoint
        let checkpoint_key = "healthkit_to_steps";
        let read_start = std::time::Instant::now();
        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;
        let batches = data_source
            .read_with_checkpoint(&source_id, "healthkit", checkpoint_key)
            .await?;
        let read_duration = read_start.elapsed();

        tracing::info!(
            batch_count = batches.len(),
            read_duration_ms = read_duration.as_millis(),
            source_type = ?data_source.source_type(),
            "Fetched HealthKit batches from data source"
        );

        // Batch insert configuration
        let mut pending_records: Vec<(String, i32, DateTime<Utc>, String, serde_json::Value)> = Vec::new();
        let mut batch_insert_total_ms = 0u128;
        let mut batch_insert_count = 0;

        let processing_start = std::time::Instant::now();

        for batch in batches {
            for record in &batch.records {
                records_read += 1;

                // Extract fields from JSONL record
                let metric_type = record.get("metric_type").and_then(|v| v.as_str()).unwrap_or("");
                if metric_type != "steps" {
                    continue; // Skip non-steps records
                }
                let Some(steps) = record.get("value").and_then(|v| v.as_i64())
                    .or_else(|| record.get("steps").and_then(|v| v.as_i64())) else {
                    continue;
                };

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

                let raw_data = record.get("raw_data").cloned();

                let metadata = serde_json::json!({
                    "healthkit_raw": raw_data,
                });

                last_processed_id = Some(stream_id.clone());

                // Add to pending batch
                pending_records.push((Uuid::new_v4().to_string(), steps as i32, timestamp, stream_id, metadata));

                // Execute batch insert when we reach batch size
                if pending_records.len() >= BATCH_SIZE {
                    let insert_start = std::time::Instant::now();
                    let batch_result = execute_steps_batch_insert(db, &pending_records).await;
                    let insert_duration = insert_start.elapsed();
                    batch_insert_total_ms += insert_duration.as_millis();
                    batch_insert_count += 1;

                    tracing::info!(
                        batch_size = pending_records.len(),
                        insert_duration_ms = insert_duration.as_millis(),
                        "Executed batch insert"
                    );

                    match batch_result {
                        Ok(written) => {
                            records_written += written;
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                batch_size = pending_records.len(),
                                "Batch insert failed"
                            );
                            records_failed += pending_records.len();
                        }
                    }
                    pending_records.clear();
                }
            }

            // Update checkpoint after processing batch
            if let Some(max_ts) = batch.max_timestamp {
                data_source
                    .update_checkpoint(&source_id, "healthkit", checkpoint_key, max_ts)
                    .await?;
            }
        }

        // Insert any remaining records
        if !pending_records.is_empty() {
            let insert_start = std::time::Instant::now();
            let batch_result = execute_steps_batch_insert(db, &pending_records).await;
            let insert_duration = insert_start.elapsed();
            batch_insert_total_ms += insert_duration.as_millis();
            batch_insert_count += 1;

            tracing::info!(
                batch_size = pending_records.len(),
                insert_duration_ms = insert_duration.as_millis(),
                "Executed final batch insert"
            );

            match batch_result {
                Ok(written) => {
                    records_written += written;
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        batch_size = pending_records.len(),
                        "Final batch insert failed"
                    );
                    records_failed += pending_records.len();
                }
            }
        }

        let processing_duration = processing_start.elapsed();
        let total_duration = transform_start.elapsed();

        tracing::info!(
            source_id = %source_id,
            records_read,
            records_written,
            records_failed,
            total_duration_ms = total_duration.as_millis(),
            processing_duration_ms = processing_duration.as_millis(),
            read_duration_ms = read_duration.as_millis(),
            batch_insert_total_ms,
            batch_insert_count,
            avg_batch_insert_ms = if batch_insert_count > 0 { batch_insert_total_ms / batch_insert_count as u128 } else { 0 },
            "HealthKit to health_steps transformation completed"
        );

        Ok(TransformResult {
            records_read,
            records_written,
            records_failed,
            last_processed_id,
            chained_transforms: vec![],
        })
    }
}

/// Transform HealthKit sleep data to health_sleep ontology
pub struct HealthKitSleepTransform;

#[async_trait]
impl OntologyTransform for HealthKitSleepTransform {
    fn source_table(&self) -> &str {
        "stream_ios_healthkit"
    }

    fn target_table(&self) -> &str {
        "health_sleep"
    }

    fn domain(&self) -> &str {
        "health"
    }

    #[tracing::instrument(skip(self, db, context), fields(source_table = %self.source_table(), target_table = %self.target_table()))]
    async fn transform(
        &self,
        db: &Database,
        context: &crate::jobs::transform_context::TransformContext,
        source_id: String,
    ) -> Result<TransformResult> {
        let mut records_read = 0;
        let mut records_written = 0;
        let mut records_failed = 0;
        let mut last_processed_id: Option<String> = None;

        let transform_start = std::time::Instant::now();

        tracing::info!(
            source_id = %source_id,
            "Starting HealthKit to health_sleep transformation"
        );

        // Read stream data from data source using checkpoint
        let checkpoint_key = "healthkit_to_sleep";
        let read_start = std::time::Instant::now();
        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;
        let batches = data_source
            .read_with_checkpoint(&source_id, "healthkit", checkpoint_key)
            .await?;
        let read_duration = read_start.elapsed();

        tracing::info!(
            batch_count = batches.len(),
            read_duration_ms = read_duration.as_millis(),
            source_type = ?data_source.source_type(),
            "Fetched HealthKit batches from data source"
        );

        // Batch insert configuration
        let mut pending_records: Vec<(
            String,
            Option<serde_json::Value>,
            i32,
            Option<f64>,
            DateTime<Utc>,
            DateTime<Utc>,
            String,
            serde_json::Value,
        )> = Vec::new();
        let mut batch_insert_total_ms = 0u128;
        let mut batch_insert_count = 0;

        let processing_start = std::time::Instant::now();

        for batch in batches {
            for record in &batch.records {
                records_read += 1;

                // Extract fields from JSONL record
                let metric_type = record.get("metric_type").and_then(|v| v.as_str()).unwrap_or("");
                if metric_type != "sleep" {
                    continue; // Skip non-sleep records
                }
                // iOS sends duration in metadata.duration_minutes for sleep
                let sleep_duration = record.get("metadata")
                    .and_then(|m| m.get("duration_minutes"))
                    .and_then(|v| v.as_i64())
                    .or_else(|| record.get("sleep_duration").and_then(|v| v.as_i64()));
                let Some(sleep_duration) = sleep_duration else {
                    continue;
                };

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

                let sleep_stage = record
                    .get("sleep_stage")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let raw_data = record.get("raw_data").cloned();

                // Build sleep_stages JSON from raw_data if available
                let sleep_stages = raw_data
                    .as_ref()
                    .and_then(|d| d.get("stages"))
                    .cloned()
                    .or_else(|| {
                        sleep_stage.as_ref().map(|stage| {
                            serde_json::json!([{
                                "stage": stage,
                                "duration_minutes": sleep_duration
                            }])
                        })
                    });

                // Calculate end_time from timestamp + duration
                let end_time = timestamp + chrono::Duration::minutes(sleep_duration);

                let metadata = serde_json::json!({
                    "healthkit_raw": raw_data,
                });

                last_processed_id = Some(stream_id.clone());

                // Add to pending batch
                pending_records.push((
                    Uuid::new_v4().to_string(),
                    sleep_stages,
                    sleep_duration as i32,
                    None, // sleep_quality_score not available in basic data
                    timestamp,
                    end_time,
                    stream_id,
                    metadata,
                ));

                // Execute batch insert when we reach batch size
                if pending_records.len() >= BATCH_SIZE {
                    let insert_start = std::time::Instant::now();
                    let batch_result = execute_sleep_batch_insert(db, &pending_records).await;
                    let insert_duration = insert_start.elapsed();
                    batch_insert_total_ms += insert_duration.as_millis();
                    batch_insert_count += 1;

                    tracing::info!(
                        batch_size = pending_records.len(),
                        insert_duration_ms = insert_duration.as_millis(),
                        "Executed batch insert"
                    );

                    match batch_result {
                        Ok(written) => {
                            records_written += written;
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                batch_size = pending_records.len(),
                                "Batch insert failed"
                            );
                            records_failed += pending_records.len();
                        }
                    }
                    pending_records.clear();
                }
            }

            // Update checkpoint after processing batch
            if let Some(max_ts) = batch.max_timestamp {
                data_source
                    .update_checkpoint(&source_id, "healthkit", checkpoint_key, max_ts)
                    .await?;
            }
        }

        // Insert any remaining records
        if !pending_records.is_empty() {
            let insert_start = std::time::Instant::now();
            let batch_result = execute_sleep_batch_insert(db, &pending_records).await;
            let insert_duration = insert_start.elapsed();
            batch_insert_total_ms += insert_duration.as_millis();
            batch_insert_count += 1;

            tracing::info!(
                batch_size = pending_records.len(),
                insert_duration_ms = insert_duration.as_millis(),
                "Executed final batch insert"
            );

            match batch_result {
                Ok(written) => {
                    records_written += written;
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        batch_size = pending_records.len(),
                        "Final batch insert failed"
                    );
                    records_failed += pending_records.len();
                }
            }
        }

        let processing_duration = processing_start.elapsed();
        let total_duration = transform_start.elapsed();

        tracing::info!(
            source_id = %source_id,
            records_read,
            records_written,
            records_failed,
            total_duration_ms = total_duration.as_millis(),
            processing_duration_ms = processing_duration.as_millis(),
            read_duration_ms = read_duration.as_millis(),
            batch_insert_total_ms,
            batch_insert_count,
            avg_batch_insert_ms = if batch_insert_count > 0 { batch_insert_total_ms / batch_insert_count as u128 } else { 0 },
            "HealthKit to health_sleep transformation completed"
        );

        Ok(TransformResult {
            records_read,
            records_written,
            records_failed,
            last_processed_id,
            chained_transforms: vec![],
        })
    }
}

/// Transform HealthKit workout data to health_workout ontology
pub struct HealthKitWorkoutTransform;

#[async_trait]
impl OntologyTransform for HealthKitWorkoutTransform {
    fn source_table(&self) -> &str {
        "stream_ios_healthkit"
    }

    fn target_table(&self) -> &str {
        "health_workout"
    }

    fn domain(&self) -> &str {
        "health"
    }

    #[tracing::instrument(skip(self, db, context), fields(source_table = %self.source_table(), target_table = %self.target_table()))]
    async fn transform(
        &self,
        db: &Database,
        context: &crate::jobs::transform_context::TransformContext,
        source_id: String,
    ) -> Result<TransformResult> {
        let mut records_read = 0;
        let mut records_written = 0;
        let mut records_failed = 0;
        let mut last_processed_id: Option<String> = None;

        let transform_start = std::time::Instant::now();

        tracing::info!(
            source_id = %source_id,
            "Starting HealthKit to health_workout transformation"
        );

        // Read stream data from data source using checkpoint
        let checkpoint_key = "healthkit_to_workout";
        let read_start = std::time::Instant::now();
        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;
        let batches = data_source
            .read_with_checkpoint(&source_id, "healthkit", checkpoint_key)
            .await?;
        let read_duration = read_start.elapsed();

        tracing::info!(
            batch_count = batches.len(),
            read_duration_ms = read_duration.as_millis(),
            source_type = ?data_source.source_type(),
            "Fetched HealthKit batches from data source"
        );

        // Batch insert configuration
        let mut pending_records: Vec<(
            String,       // id
            String,       // workout_type
            Option<i32>,  // duration_minutes
            Option<i32>,  // calories_burned
            Option<i32>,  // avg_heart_rate
            Option<i32>,  // max_heart_rate
            Option<f64>,  // distance_km
            Option<String>, // place_id
            DateTime<Utc>,  // start_time
            DateTime<Utc>,  // end_time
            String,         // stream_id
            serde_json::Value, // metadata
        )> = Vec::new();
        let mut batch_insert_total_ms = 0u128;
        let mut batch_insert_count = 0;

        let processing_start = std::time::Instant::now();

        for batch in batches {
            for record in &batch.records {
                records_read += 1;

                // Extract fields from JSONL record
                let metric_type = record.get("metric_type").and_then(|v| v.as_str()).unwrap_or("");
                if metric_type != "workout" {
                    continue; // Skip non-workout records
                }
                let Some(workout_type) = record.get("workout_type").and_then(|v| v.as_str())
                    .or_else(|| record.get("value").and_then(|v| v.as_str())) else {
                    continue;
                };

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

                let workout_duration = record
                    .get("workout_duration")
                    .and_then(|v| v.as_i64())
                    .map(|d| d as i32);

                let active_energy = record.get("active_energy").and_then(|v| v.as_f64());
                let distance = record.get("distance").and_then(|v| v.as_f64());
                let heart_rate = record.get("heart_rate").and_then(|v| v.as_f64());
                let raw_data = record.get("raw_data").cloned();

                // Calculate end_time from timestamp + duration
                let duration_minutes = workout_duration.unwrap_or(0);
                let end_time = timestamp + chrono::Duration::minutes(duration_minutes as i64);

                // Extract additional workout details from raw_data
                let max_heart_rate = raw_data
                    .as_ref()
                    .and_then(|d| d.get("max_heart_rate"))
                    .and_then(|h| h.as_f64())
                    .map(|h| h as i32);

                let intensity = raw_data
                    .as_ref()
                    .and_then(|d| d.get("intensity"))
                    .and_then(|i| i.as_str())
                    .map(String::from);

                let metadata = serde_json::json!({
                    "healthkit_raw": raw_data,
                    "intensity": intensity,
                });

                last_processed_id = Some(stream_id.clone());

                // Add to pending batch
                pending_records.push((
                    Uuid::new_v4().to_string(),
                    workout_type.to_string(),
                    workout_duration,                    // duration_minutes
                    active_energy.map(|e| e as i32),     // calories_burned
                    heart_rate.map(|h| h as i32),        // avg_heart_rate
                    max_heart_rate,                       // max_heart_rate
                    distance.map(|d| d / 1000.0),        // distance_km (convert m to km)
                    None,                                 // place_id
                    timestamp,
                    end_time,
                    stream_id,
                    metadata,
                ));

                // Execute batch insert when we reach batch size
                if pending_records.len() >= BATCH_SIZE {
                    let insert_start = std::time::Instant::now();
                    let batch_result = execute_workout_batch_insert(db, &pending_records).await;
                    let insert_duration = insert_start.elapsed();
                    batch_insert_total_ms += insert_duration.as_millis();
                    batch_insert_count += 1;

                    tracing::info!(
                        batch_size = pending_records.len(),
                        insert_duration_ms = insert_duration.as_millis(),
                        "Executed batch insert"
                    );

                    match batch_result {
                        Ok(written) => {
                            records_written += written;
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                batch_size = pending_records.len(),
                                "Batch insert failed"
                            );
                            records_failed += pending_records.len();
                        }
                    }
                    pending_records.clear();
                }
            }

            // Update checkpoint after processing batch
            if let Some(max_ts) = batch.max_timestamp {
                data_source
                    .update_checkpoint(&source_id, "healthkit", checkpoint_key, max_ts)
                    .await?;
            }
        }

        // Insert any remaining records
        if !pending_records.is_empty() {
            let insert_start = std::time::Instant::now();
            let batch_result = execute_workout_batch_insert(db, &pending_records).await;
            let insert_duration = insert_start.elapsed();
            batch_insert_total_ms += insert_duration.as_millis();
            batch_insert_count += 1;

            tracing::info!(
                batch_size = pending_records.len(),
                insert_duration_ms = insert_duration.as_millis(),
                "Executed final batch insert"
            );

            match batch_result {
                Ok(written) => {
                    records_written += written;
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        batch_size = pending_records.len(),
                        "Final batch insert failed"
                    );
                    records_failed += pending_records.len();
                }
            }
        }

        let processing_duration = processing_start.elapsed();
        let total_duration = transform_start.elapsed();

        tracing::info!(
            source_id = %source_id,
            records_read,
            records_written,
            records_failed,
            total_duration_ms = total_duration.as_millis(),
            processing_duration_ms = processing_duration.as_millis(),
            read_duration_ms = read_duration.as_millis(),
            batch_insert_total_ms,
            batch_insert_count,
            avg_batch_insert_ms = if batch_insert_count > 0 { batch_insert_total_ms / batch_insert_count as u128 } else { 0 },
            "HealthKit to health_workout transformation completed"
        );

        Ok(TransformResult {
            records_read,
            records_written,
            records_failed,
            last_processed_id,
            chained_transforms: vec![],
        })
    }
}

/// Execute batch insert for heart rate records
///
/// Builds and executes a multi-row INSERT statement for efficient bulk insertion.
async fn execute_heart_rate_batch_insert(
    db: &Database,
    records: &[(String, i32, DateTime<Utc>, String, serde_json::Value)],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let query_str = Database::build_batch_insert_query(
        "data_health_heart_rate",
        &[
            "id",
            "bpm",
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

    // Bind all parameters row by row
    for (id, bpm, timestamp, stream_id, metadata) in records {
        query = query
            .bind(id)
            .bind(bpm)
            .bind(timestamp)
            .bind(stream_id)
            .bind("stream_ios_healthkit")
            .bind("ios")
            .bind(metadata);
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

/// Execute batch insert for HRV records
///
/// Builds and executes a multi-row INSERT statement for efficient bulk insertion.
async fn execute_hrv_batch_insert(
    db: &Database,
    records: &[(String, f64, DateTime<Utc>, String, serde_json::Value)],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let query_str = Database::build_batch_insert_query(
        "data_health_hrv",
        &[
            "id",
            "hrv_ms",
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

    // Bind all parameters row by row
    for (id, hrv_ms, timestamp, stream_id, metadata) in records {
        query = query
            .bind(id)
            .bind(hrv_ms)
            .bind(timestamp)
            .bind(stream_id)
            .bind("stream_ios_healthkit")
            .bind("ios")
            .bind(metadata);
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

/// Execute batch insert for steps records
///
/// Builds and executes a multi-row INSERT statement for efficient bulk insertion.
async fn execute_steps_batch_insert(
    db: &Database,
    records: &[(String, i32, DateTime<Utc>, String, serde_json::Value)],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let query_str = Database::build_batch_insert_query(
        "data_health_steps",
        &[
            "id",
            "step_count",
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

    // Bind all parameters row by row
    for (id, step_count, timestamp, stream_id, metadata) in records {
        query = query
            .bind(id)
            .bind(step_count)
            .bind(timestamp)
            .bind(stream_id)
            .bind("stream_ios_healthkit")
            .bind("ios")
            .bind(metadata);
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

/// Execute batch insert for sleep records
///
/// Builds and executes a multi-row INSERT statement for efficient bulk insertion.
async fn execute_sleep_batch_insert(
    db: &Database,
    records: &[(
        String,
        Option<serde_json::Value>,
        i32,
        Option<f64>,
        DateTime<Utc>,
        DateTime<Utc>,
        String,
        serde_json::Value,
    )],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let query_str = Database::build_batch_insert_query(
        "data_health_sleep",
        &[
            "id",
            "sleep_stages",
            "duration_minutes",
            "sleep_quality_score",
            "start_time",
            "end_time",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut query = sqlx::query(&query_str);

    // Bind all parameters row by row
    for (
        id,
        sleep_stages,
        total_duration_minutes,
        sleep_quality_score,
        start_time,
        end_time,
        stream_id,
        metadata,
    ) in records
    {
        query = query
            .bind(id)
            .bind(sleep_stages)
            .bind(total_duration_minutes)
            .bind(sleep_quality_score)
            .bind(start_time)
            .bind(end_time)
            .bind(stream_id)
            .bind("stream_ios_healthkit")
            .bind("ios")
            .bind(metadata);
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

/// Execute batch insert for workout records
///
/// Builds and executes a multi-row INSERT statement for efficient bulk insertion.
async fn execute_workout_batch_insert(
    db: &Database,
    records: &[(
        String,       // id
        String,       // workout_type
        Option<i32>,  // duration_minutes
        Option<i32>,  // calories_burned
        Option<i32>,  // avg_heart_rate
        Option<i32>,  // max_heart_rate
        Option<f64>,  // distance_km
        Option<String>, // place_id
        DateTime<Utc>,  // start_time
        DateTime<Utc>,  // end_time
        String,         // stream_id
        serde_json::Value, // metadata
    )],
) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let query_str = Database::build_batch_insert_query(
        "data_health_workout",
        &[
            "id",
            "workout_type",
            "duration_minutes",
            "calories_burned",
            "avg_heart_rate",
            "max_heart_rate",
            "distance_km",
            "place_id",
            "start_time",
            "end_time",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut query = sqlx::query(&query_str);

    // Bind all parameters row by row
    for (
        id,
        workout_type,
        duration_minutes,
        calories_burned,
        avg_heart_rate,
        max_heart_rate,
        distance_km,
        place_id,
        start_time,
        end_time,
        stream_id,
        metadata,
    ) in records
    {
        query = query
            .bind(id)
            .bind(workout_type)
            .bind(duration_minutes)
            .bind(calories_burned)
            .bind(avg_heart_rate)
            .bind(max_heart_rate)
            .bind(distance_km)
            .bind(place_id)
            .bind(start_time)
            .bind(end_time)
            .bind(stream_id)
            .bind("stream_ios_healthkit")
            .bind("ios")
            .bind(metadata);
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

// Self-registrations for all HealthKit transforms

struct HealthKitHeartRateRegistration;
impl TransformRegistration for HealthKitHeartRateRegistration {
    fn source_table(&self) -> &'static str {
        "stream_ios_healthkit"
    }
    fn target_table(&self) -> &'static str {
        "health_heart_rate"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(HealthKitHeartRateTransform))
    }
}
inventory::submit! { &HealthKitHeartRateRegistration as &dyn TransformRegistration }

struct HealthKitHRVRegistration;
impl TransformRegistration for HealthKitHRVRegistration {
    fn source_table(&self) -> &'static str {
        "stream_ios_healthkit"
    }
    fn target_table(&self) -> &'static str {
        "health_hrv"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(HealthKitHRVTransform))
    }
}
inventory::submit! { &HealthKitHRVRegistration as &dyn TransformRegistration }

struct HealthKitStepsRegistration;
impl TransformRegistration for HealthKitStepsRegistration {
    fn source_table(&self) -> &'static str {
        "stream_ios_healthkit"
    }
    fn target_table(&self) -> &'static str {
        "health_steps"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(HealthKitStepsTransform))
    }
}
inventory::submit! { &HealthKitStepsRegistration as &dyn TransformRegistration }

struct HealthKitSleepRegistration;
impl TransformRegistration for HealthKitSleepRegistration {
    fn source_table(&self) -> &'static str {
        "stream_ios_healthkit"
    }
    fn target_table(&self) -> &'static str {
        "health_sleep"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(HealthKitSleepTransform))
    }
}
inventory::submit! { &HealthKitSleepRegistration as &dyn TransformRegistration }

struct HealthKitWorkoutRegistration;
impl TransformRegistration for HealthKitWorkoutRegistration {
    fn source_table(&self) -> &'static str {
        "stream_ios_healthkit"
    }
    fn target_table(&self) -> &'static str {
        "health_workout"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(HealthKitWorkoutTransform))
    }
}
inventory::submit! { &HealthKitWorkoutRegistration as &dyn TransformRegistration }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heart_rate_transform_metadata() {
        let transform = HealthKitHeartRateTransform;
        assert_eq!(transform.source_table(), "stream_ios_healthkit");
        assert_eq!(transform.target_table(), "health_heart_rate");
        assert_eq!(transform.domain(), "health");
    }

    #[test]
    fn test_hrv_transform_metadata() {
        let transform = HealthKitHRVTransform;
        assert_eq!(transform.source_table(), "stream_ios_healthkit");
        assert_eq!(transform.target_table(), "health_hrv");
        assert_eq!(transform.domain(), "health");
    }

    #[test]
    fn test_steps_transform_metadata() {
        let transform = HealthKitStepsTransform;
        assert_eq!(transform.source_table(), "stream_ios_healthkit");
        assert_eq!(transform.target_table(), "health_steps");
        assert_eq!(transform.domain(), "health");
    }

    #[test]
    fn test_sleep_transform_metadata() {
        let transform = HealthKitSleepTransform;
        assert_eq!(transform.source_table(), "stream_ios_healthkit");
        assert_eq!(transform.target_table(), "health_sleep");
        assert_eq!(transform.domain(), "health");
    }

    #[test]
    fn test_workout_transform_metadata() {
        let transform = HealthKitWorkoutTransform;
        assert_eq!(transform.source_table(), "stream_ios_healthkit");
        assert_eq!(transform.target_table(), "health_workout");
        assert_eq!(transform.domain(), "health");
    }
}
