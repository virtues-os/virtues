//! Sync job execution logic

use crate::error::Result;
use crate::jobs::models::Job;
use crate::jobs::{JobExecutor, TransformContext};
use crate::sources::base::SyncMode;
use crate::sources::StreamFactory;
use crate::registry;
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::Arc;

/// Execute a sync job
///
/// This function is called by the job executor to perform the actual sync work.
/// It updates the job status in the database as it progresses.
pub async fn execute_sync_job(
    db: &SqlitePool,
    executor: &JobExecutor,
    context: &Arc<TransformContext>,
    job: &Job,
) -> Result<()> {
    let source_id = job
        .source_connection_id
        .clone()
        .ok_or_else(|| crate::Error::InvalidInput("Sync job missing source_id".to_string()))?;

    let stream_name = job
        .stream_name
        .as_ref()
        .ok_or_else(|| crate::Error::InvalidInput("Sync job missing stream_name".to_string()))?;

    let sync_mode_str = job
        .sync_mode
        .as_ref()
        .ok_or_else(|| crate::Error::InvalidInput("Sync job missing sync_mode".to_string()))?;

    // 1. Tier Enforcement
    // Get source and stream info from registry
    let source_conn = sqlx::query!(
        "SELECT source FROM elt_source_connections WHERE id = $1",
        source_id
    )
    .fetch_one(db)
    .await
    .map_err(|e| crate::Error::Database(format!("Failed to fetch source connection: {}", e)))?;

    let registered_stream = registry::get_stream(&source_conn.source, stream_name).ok_or_else(|| {
        crate::Error::InvalidInput(format!(
            "Stream '{}' not found for source '{}'",
            stream_name, source_conn.source
        ))
    })?;

    // Get user tier (mocked for now, should come from profile/subscription)
    // TODO: Implement actual subscription check
    let user_tier = virtues_registry::sources::SourceTier::Standard;

    if (registered_stream.descriptor.tier as u32) > (user_tier as u32) {
        let error_msg = format!(
            "Tier mismatch: stream '{}' requires {:?} tier, user has {:?} tier",
            stream_name, registered_stream.descriptor.tier, user_tier
        );
        tracing::warn!(error_msg);

        sqlx::query(
            "UPDATE elt_jobs SET status = 'failed', error_message = $1, error_class = 'tier_error' WHERE id = $2",
        )
        .bind(&error_msg)
        .bind(&job.id)
        .execute(db)
        .await?;

        return Err(crate::Error::Unauthorized(error_msg));
    }

    // Extract cursor from metadata if present
    let cursor_before = job
        .metadata
        .get("cursor_before")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Convert sync mode string to SyncMode enum
    let sync_mode = match sync_mode_str.as_str() {
        "full_refresh" => SyncMode::FullRefresh,
        "incremental" => SyncMode::incremental(cursor_before.clone()),
        "backfill" => {
            let start = job
                .metadata
                .get("start_date")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| crate::Error::InvalidInput("Backfill missing start_date".into()))?;
            let end = job
                .metadata
                .get("end_date")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| crate::Error::InvalidInput("Backfill missing end_date".into()))?;
            SyncMode::backfill(start, end)
        }
        _ => {
            return Err(crate::Error::InvalidInput(format!(
                "Invalid sync mode: {}",
                sync_mode_str
            )))
        }
    };

    // Create factory and stream instance using new PullStream API
    let factory = StreamFactory::new(
        db.clone(),
        context.storage.clone(),
        context.stream_writer.clone(),
    );
    let mut stream_type = factory.create_stream_typed(&source_id, stream_name).await?;

    // Ensure we got a PullStream (sync jobs should only work with pull streams)
    let pull_stream = match stream_type.as_pull_mut() {
        Some(stream) => stream,
        None => {
            return Err(crate::Error::InvalidInput(format!(
                "Cannot sync push stream '{}' via scheduler - push streams are client-initiated",
                stream_name
            )));
        }
    };

    // Load configuration
    pull_stream.load_config(db, &source_id).await?;

    // Execute sync using PullStream API
    let result = pull_stream.sync_pull(sync_mode.clone()).await;

    match result {
        Ok(sync_result) => {
            // Update watermarks and sync status
            sqlx::query(
                r#"
                UPDATE elt_stream_connections
                SET last_sync_at = $1, 
                    last_sync_token = $2, 
                    earliest_record_at = COALESCE(earliest_record_at, $3),
                    latest_record_at = $4,
                    sync_status = $5,
                    updated_at = datetime('now')
                WHERE source_connection_id = $6 AND stream_name = $7
                "#,
            )
            .bind(sync_result.completed_at)
            .bind(&sync_result.next_cursor)
            .bind(sync_result.earliest_record_at)
            .bind(sync_result.latest_record_at)
            .bind(match sync_mode {
                SyncMode::FullRefresh => "initial",
                SyncMode::Incremental { .. } => "incremental",
                SyncMode::Backfill { .. } => "backfilling",
            })
            .bind(&source_id)
            .bind(stream_name)
            .execute(db)
            .await?;

            // Extract records for direct transform and archival
            let has_records = sync_result.records.is_some();
            let records = sync_result.records.clone().unwrap_or_default();

            tracing::info!(
                stream_name = %stream_name,
                has_records = has_records,
                record_count = records.len(),
                "Sync completed, checking for records to archive"
            );

            // Write records directly to filesystem (sync, no async job queue)
            let storage_key = if !records.is_empty() {
                tracing::info!(
                    stream_name = %stream_name,
                    record_count = records.len(),
                    "Writing records to filesystem"
                );

                let storage_key = write_stream_records(
                    db,
                    context.storage.as_ref(),
                    &source_id,
                    &source_conn.source,
                    stream_name,
                    &records,
                    sync_result.earliest_record_at,
                    sync_result.latest_record_at,
                )
                .await?;

                tracing::info!(
                    stream_name = %stream_name,
                    storage_key = %storage_key,
                    "Records written to filesystem successfully"
                );

                Some(storage_key)
            } else {
                tracing::warn!(
                    stream_name = %stream_name,
                    "No records collected from sync, skipping archival"
                );
                None
            };

            // Build metadata with detailed sync info
            let metadata = json!({
                "cursor_before": cursor_before,
                "cursor_after": sync_result.next_cursor,
                "records_fetched": sync_result.records_fetched,
                "records_written": sync_result.records_written,
                "records_failed": sync_result.records_failed,
                "earliest_record_at": sync_result.earliest_record_at,
                "latest_record_at": sync_result.latest_record_at,
                "duration_ms": sync_result.duration_ms(),
                "direct_transform_enabled": has_records,
                "storage_key": storage_key
            });

            // Update job with final stats and metadata
            sqlx::query(
                r#"
                UPDATE elt_jobs
                SET status = 'succeeded',
                    completed_at = datetime('now'),
                    records_processed = $1,
                    metadata = $2
                WHERE id = $3
                "#,
            )
            .bind(sync_result.records_written as i64)
            .bind(metadata)
            .bind(&job.id)
            .execute(db)
            .await?;

            tracing::info!(
                job_id = %job.id,
                stream_name = %stream_name,
                records_fetched = sync_result.records_fetched,
                records_written = sync_result.records_written,
                duration_ms = sync_result.duration_ms(),
                direct_transform = has_records,
                storage_key = ?storage_key,
                "Sync job completed successfully"
            );

            // Create transform job with optional memory data source (direct transform)
            // Only create transform job if we actually have records to transform
            if has_records {
                if let Err(e) = crate::jobs::create_transform_job_for_stream(
                    db,
                    executor,
                    context,
                    source_id.clone(),
                    stream_name,
                    Some(records),
                )
                .await
                .map(|_job_id| ())
                {
                    tracing::warn!(
                        error = %e,
                        stream_name = %stream_name,
                        "Failed to create transform job, continuing"
                    );
                }
            } else {
                tracing::debug!(
                    stream_name = %stream_name,
                    "No records to transform, skipping transform job creation"
                );
            }

            Ok(())
        }
        Err(e) => {
            // Classify error for monitoring
            let error_class = classify_sync_error(&e);

            // Build metadata with error details
            let metadata = json!({
                "cursor_before": cursor_before,
                "error_class": error_class
            });

            // Update job with error
            sqlx::query(
                r#"
                UPDATE elt_jobs
                SET status = 'failed',
                    completed_at = datetime('now'),
                    error_message = $1,
                    error_class = $2,
                    metadata = $3
                WHERE id = $4
                "#,
            )
            .bind(e.to_string())
            .bind(error_class)
            .bind(metadata)
            .bind(&job.id)
            .execute(db)
            .await?;

            // Handle rate limiting backoff
            if error_class == "rate_limit" {
                tracing::info!(
                    job_id = %job.id,
                    stream_name = %stream_name,
                    "Rate limit detected, scheduling backoff"
                );
                // In a real system, we would reschedule the job with a delay
                // For now, we just log it as a rate_limit error
            }

            tracing::error!(
                job_id = %job.id,
                stream_name = %stream_name,
                error_class = error_class,
                error = %e,
                "Sync job failed"
            );

            Err(e)
        }
    }
}

/// Write stream records directly to filesystem and record metadata
///
/// This replaces the async archive job system with synchronous filesystem writes.
async fn write_stream_records(
    db: &SqlitePool,
    storage: &crate::storage::Storage,
    source_id: &str,
    source_type: &str,
    stream_name: &str,
    records: &[serde_json::Value],
    min_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    max_timestamp: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<String> {
    use chrono::Utc;
    use crate::storage::models::StreamKeyBuilder;

    let date = Utc::now().date_naive();
    let key_builder = StreamKeyBuilder::new(None, source_type, source_id, stream_name, date)
        .map_err(|e| crate::Error::Other(format!("Invalid stream key: {}", e)))?;
    let storage_key = key_builder.build();

    // Write JSONL to filesystem
    storage.upload_jsonl(&storage_key, records).await?;

    // Calculate size
    let size_bytes: i64 = records
        .iter()
        .map(|r| serde_json::to_string(r).unwrap_or_default().len() as i64)
        .sum();

    // Record metadata in elt_stream_objects
    let stream_object_id = crate::ids::generate_id(crate::ids::STREAM_OBJECT_PREFIX, &[&storage_key]);
    sqlx::query(
        "INSERT INTO elt_stream_objects
         (id, source_connection_id, stream_name, storage_key, record_count, size_bytes,
          min_timestamp, max_timestamp, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, datetime('now'))",
    )
    .bind(&stream_object_id)
    .bind(source_id)
    .bind(stream_name)
    .bind(&storage_key)
    .bind(records.len() as i32)
    .bind(size_bytes)
    .bind(min_timestamp)
    .bind(max_timestamp)
    .execute(db)
    .await?;

    tracing::info!(
        stream_object_id = %stream_object_id,
        storage_key = %storage_key,
        record_count = records.len(),
        "Stream object metadata recorded"
    );

    Ok(storage_key)
}

/// Classify errors for monitoring and alerting
fn classify_sync_error(error: &crate::error::Error) -> &'static str {
    use crate::error::Error;

    match error {
        Error::Http(msg) => {
            let msg_lower = msg.to_lowercase();
            if msg_lower.contains("401") || msg_lower.contains("unauthorized") {
                "auth_error"
            } else if msg_lower.contains("429") || msg_lower.contains("rate limit") {
                "rate_limit"
            } else if msg_lower.contains("sync token") {
                "sync_token_error"
            } else if msg_lower.contains("5")
                && (msg_lower.contains("500") || msg_lower.contains("503"))
            {
                "server_error"
            } else if msg_lower.contains("4")
                && (msg_lower.contains("400") || msg_lower.contains("404"))
            {
                "client_error"
            } else {
                "network_error"
            }
        }
        Error::Source(_) => "sync_token_error",
        Error::Database(_) => "database_error",
        Error::Storage(_) => "storage_error",
        Error::Authentication(_) | Error::Unauthorized(_) => "auth_error",
        Error::Serialization(_) => "serialization_error",
        Error::Configuration(_) => "config_error",
        _ => "unknown_error",
    }
}
