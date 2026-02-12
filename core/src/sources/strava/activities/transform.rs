//! Strava activities to health_workout ontology transformation
//!
//! Transforms raw Strava activity data from stream_strava_activities into the
//! normalized health_workout ontology table.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::database::Database;
use crate::error::Result;
use crate::jobs::TransformContext;
use crate::sources::base::{OntologyTransform, TransformRegistration, TransformResult};

/// Batch size for bulk inserts
const BATCH_SIZE: usize = 500;

/// Transform Strava activities to health_workout ontology
///
/// This transform is registered with the stream in the unified registry,
/// so the standalone inventory registration is kept for backward compatibility.
pub struct StravaWorkoutTransform;

#[async_trait]
impl OntologyTransform for StravaWorkoutTransform {
    fn source_table(&self) -> &str {
        "stream_strava_activities"
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
            "Starting Strava activities to health_workout transformation"
        );

        // Read stream data from data source using checkpoint
        let checkpoint_key = "strava_activities_to_health_workout";
        let read_start = std::time::Instant::now();
        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;
        let batches = data_source
            .read_with_checkpoint(&source_id, "activities", checkpoint_key)
            .await?;
        let read_duration = read_start.elapsed();

        tracing::info!(
            batch_count = batches.len(),
            read_duration_ms = read_duration.as_millis(),
            source_type = ?data_source.source_type(),
            "Fetched Strava activity batches from data source"
        );

        // Batch insert configuration
        let mut pending_records: Vec<(
            String,         // id (deterministic)
            String,         // workout_type
            Option<i32>,    // duration_minutes
            Option<i32>,    // calories_burned
            Option<i32>,    // avg_heart_rate
            Option<i32>,    // max_heart_rate
            Option<f64>,    // distance_km
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
            tracing::debug!(batch_record_count = batch.records.len(), "Processing batch");

            for record in &batch.records {
                records_read += 1;

                // Extract the Strava activity ID
                let Some(activity_id) = record.get("activity_id").and_then(|v| v.as_i64()) else {
                    records_failed += 1;
                    continue;
                };

                let stream_id = record
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| Uuid::new_v4().to_string());

                // sport_type -> workout_type (direct string mapping)
                let workout_type = record
                    .get("sport_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();

                // start_date -> start_time (ISO 8601 parse)
                let start_time = record
                    .get("start_date")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                    .unwrap_or_else(|| Utc::now());

                // elapsed_time -> duration_minutes (seconds / 60)
                let elapsed_time = record
                    .get("elapsed_time")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                let duration_minutes = Some((elapsed_time as f64 / 60.0).round() as i32);

                // start_date + elapsed_time -> end_time
                let end_time = start_time + Duration::seconds(elapsed_time);

                // kilojoules -> calories_burned (kJ * 0.239 = kcal)
                let calories_burned = record
                    .get("kilojoules")
                    .and_then(|v| v.as_f64())
                    .map(|kj| (kj * 0.239).round() as i32);

                // distance -> distance_km (meters / 1000)
                let distance_km = record
                    .get("distance")
                    .and_then(|v| v.as_f64())
                    .map(|m| m / 1000.0);

                // average_heartrate -> avg_heart_rate (round to int)
                let avg_heart_rate = record
                    .get("average_heartrate")
                    .and_then(|v| v.as_f64())
                    .map(|hr| hr.round() as i32);

                // max_heartrate -> max_heart_rate (round to int)
                let max_heart_rate = record
                    .get("max_heartrate")
                    .and_then(|v| v.as_f64())
                    .map(|hr| hr.round() as i32);

                // Build metadata with Strava-specific fields
                let metadata = serde_json::json!({
                    "strava_activity_id": activity_id,
                    "name": record.get("name"),
                    "activity_type": record.get("activity_type"),
                    "total_elevation_gain": record.get("total_elevation_gain"),
                    "average_speed": record.get("average_speed"),
                    "max_speed": record.get("max_speed"),
                    "suffer_score": record.get("suffer_score"),
                    "gear_id": record.get("gear_id"),
                    "summary_polyline": record.get("map")
                        .and_then(|m| m.get("summary_polyline")),
                    "source_connection_id": source_id,
                });

                // Generate deterministic ID for idempotency
                let id = crate::ids::generate_id(
                    "health_workout",
                    &[&source_id, &activity_id.to_string()],
                );

                last_processed_id = Some(stream_id.clone());

                // Add to pending batch
                pending_records.push((
                    id,
                    workout_type,
                    duration_minutes,
                    calories_burned,
                    avg_heart_rate,
                    max_heart_rate,
                    distance_km,
                    None, // place_id
                    start_time,
                    end_time,
                    stream_id,
                    metadata,
                ));

                // Execute batch insert when we reach batch size
                if pending_records.len() >= BATCH_SIZE {
                    let insert_start = std::time::Instant::now();
                    let batch_result =
                        execute_workout_batch_insert(db, &source_id, &pending_records).await;
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
                    .update_checkpoint(&source_id, "activities", checkpoint_key, max_ts)
                    .await?;
            }
        }

        // Insert any remaining records
        if !pending_records.is_empty() {
            let insert_start = std::time::Instant::now();
            let batch_result =
                execute_workout_batch_insert(db, &source_id, &pending_records).await;
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
            "Strava activities to health_workout transformation completed"
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

/// Execute batch insert for workout records from Strava
///
/// Builds and executes a multi-row INSERT statement for efficient bulk insertion.
async fn execute_workout_batch_insert(
    db: &Database,
    source_connection_id: &str,
    records: &[(
        String,         // id (deterministic)
        String,         // workout_type
        Option<i32>,    // duration_minutes
        Option<i32>,    // calories_burned
        Option<i32>,    // avg_heart_rate
        Option<i32>,    // max_heart_rate
        Option<f64>,    // distance_km
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
            "source_connection_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "id",
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
            .bind(source_connection_id)
            .bind("stream_strava_activities")
            .bind("strava")
            .bind(metadata);
    }

    let result = query.execute(db.pool()).await?;
    Ok(result.rows_affected() as usize)
}

// Self-registration
struct StravaWorkoutTransformRegistration;

impl TransformRegistration for StravaWorkoutTransformRegistration {
    fn source_table(&self) -> &'static str {
        "stream_strava_activities"
    }
    fn target_table(&self) -> &'static str {
        "health_workout"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(StravaWorkoutTransform))
    }
}

inventory::submit! {
    &StravaWorkoutTransformRegistration as &dyn TransformRegistration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_metadata() {
        let transform = StravaWorkoutTransform;
        assert_eq!(transform.source_table(), "stream_strava_activities");
        assert_eq!(transform.target_table(), "health_workout");
        assert_eq!(transform.domain(), "health");
    }

    #[test]
    fn test_kilojoules_to_calories_conversion() {
        // 1000 kJ * 0.239 = 239 kcal
        let kj = 1000.0_f64;
        let kcal = (kj * 0.239).round() as i32;
        assert_eq!(kcal, 239);
    }

    #[test]
    fn test_meters_to_km_conversion() {
        // 5000 meters = 5.0 km
        let meters = 5000.0_f64;
        let km = meters / 1000.0;
        assert!((km - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_seconds_to_minutes_conversion() {
        // 3600 seconds = 60 minutes
        let seconds = 3600_i64;
        let minutes = (seconds as f64 / 60.0).round() as i32;
        assert_eq!(minutes, 60);
    }
}
