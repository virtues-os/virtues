//! Sync execution logic
//!
//! Executes a pull-stream sync: fetch records → write to storage → chain transforms.
//! Called by both the scheduler (cron) and manual trigger (API).

use crate::error::Result;
use crate::pipeline::{PipelineExecutor, TransformContext};
use crate::sources::base::SyncMode;
use crate::sources::StreamFactory;
use crate::registry;
use sqlx::SqlitePool;
use std::sync::Arc;

/// Execute a sync for a specific stream.
///
/// This is the core sync logic, shared between scheduler dispatch and manual API triggers.
/// The caller is responsible for creating the task_run beforehand.
pub async fn execute_sync(
    db: &SqlitePool,
    executor: &PipelineExecutor,
    context: &Arc<TransformContext>,
    run_id: &str,
    source_id: &str,
    stream_name: &str,
    sync_mode: Option<SyncMode>,
) -> Result<()> {
    // 1. Tier enforcement
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

    let user_tier = virtues_registry::sources::SourceTier::Standard;
    if (registered_stream.descriptor.tier as u32) > (user_tier as u32) {
        return Err(crate::Error::Unauthorized(format!(
            "Tier mismatch: stream '{}' requires {:?} tier, user has {:?} tier",
            stream_name, registered_stream.descriptor.tier, user_tier
        )));
    }

    // 2. Determine sync mode
    let sync_mode = match sync_mode {
        Some(m) => m,
        None => {
            // Default to incremental with cursor from DB
            let cursor = sqlx::query_scalar::<_, Option<String>>(
                "SELECT last_sync_token FROM elt_stream_connections WHERE source_connection_id = $1 AND stream_name = $2",
            )
            .bind(source_id)
            .bind(stream_name)
            .fetch_optional(db)
            .await?
            .flatten();
            SyncMode::incremental(cursor)
        }
    };

    let _cursor_before = match &sync_mode {
        SyncMode::Incremental { cursor } => cursor.clone(),
        _ => None,
    };

    // 3. Create factory and stream instance
    let factory = StreamFactory::new(
        db.clone(),
        context.storage.clone(),
        context.stream_writer.clone(),
    );
    let mut stream_type = factory.create_stream_typed(source_id, stream_name).await?;

    let pull_stream = match stream_type.as_pull_mut() {
        Some(stream) => stream,
        None => {
            return Err(crate::Error::InvalidInput(format!(
                "Cannot sync push stream '{}' via scheduler - push streams are client-initiated",
                stream_name
            )));
        }
    };

    pull_stream.load_config(db, source_id).await?;

    // 4. Execute sync
    let result = pull_stream.sync_pull(sync_mode.clone()).await;

    match result {
        Ok(sync_result) => {
            // Update watermarks and sync status on elt_stream_connections
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
            .bind(source_id)
            .bind(stream_name)
            .execute(db)
            .await?;

            // Extract records for archival and transform chaining
            let has_records = sync_result.records.is_some();
            let records = sync_result.records.clone().unwrap_or_default();

            // Write records to filesystem
            let _storage_key = if !records.is_empty() {
                let key = write_stream_records(
                    db,
                    context.storage.as_ref(),
                    source_id,
                    &source_conn.source,
                    stream_name,
                    &records,
                    sync_result.earliest_record_at,
                    sync_result.latest_record_at,
                )
                .await?;

                tracing::info!(
                    stream_name = %stream_name,
                    storage_key = %key,
                    record_count = records.len(),
                    "Records written to filesystem"
                );
                Some(key)
            } else {
                None
            };

            tracing::info!(
                run_id = %run_id,
                stream_name = %stream_name,
                records_fetched = sync_result.records_fetched,
                records_written = sync_result.records_written,
                duration_ms = sync_result.duration_ms(),
                "Sync completed successfully"
            );

            // Chain to transforms if we have records
            if has_records {
                if let Err(e) = crate::pipeline::create_transform_job_for_stream(
                    db,
                    executor,
                    context,
                    source_id.to_string(),
                    stream_name,
                    Some(records),
                    Some(run_id),
                )
                .await
                {
                    tracing::warn!(
                        error = %e,
                        stream_name = %stream_name,
                        "Failed to create transform job, continuing"
                    );
                }
            }

            Ok(())
        }
        Err(e) => {
            let error_class = classify_sync_error(&e);
            tracing::error!(
                run_id = %run_id,
                stream_name = %stream_name,
                error_class = error_class,
                error = %e,
                "Sync failed"
            );
            Err(e)
        }
    }
}

/// Write stream records directly to filesystem and record metadata
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

    storage.upload_jsonl(&storage_key, records).await?;

    let size_bytes: i64 = records
        .iter()
        .map(|r| serde_json::to_string(r).unwrap_or_default().len() as i64)
        .sum();

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

    Ok(storage_key)
}

/// Classify errors for monitoring
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
            } else if msg_lower.contains("500") || msg_lower.contains("503") {
                "server_error"
            } else if msg_lower.contains("400") || msg_lower.contains("404") {
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
