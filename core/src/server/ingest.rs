//! Ingestion API for receiving data from all sources

use axum::{
    extract::{rejection::JsonRejection, Json, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    api::chat::ChatCancellationState,
    database::Database,
    error::{Error, Result},
    sources::{push_stream::IngestPayload, stream_type::StreamType, StreamFactory},
    storage::{stream_writer::StreamWriter, Storage},
};

/// Ingestion request from any source
#[derive(Debug, Deserialize)]
pub struct IngestRequest {
    /// Source identifier (e.g., "ios.healthkit", "mac.imessage")
    pub source: String,

    /// Stream within the source (e.g., "heart_rate", "messages")
    pub stream: String,

    /// Device or instance ID
    /// Currently unused but kept for future device-specific features
    #[allow(dead_code)]
    pub device_id: String,

    /// Actual data records
    pub records: Vec<Value>,

    /// Optional checkpoint for incremental sync
    /// Currently unused but kept for future checkpoint tracking
    #[allow(dead_code)]
    pub checkpoint: Option<String>,

    /// Timestamp of this batch
    #[allow(dead_code)]
    pub timestamp: DateTime<Utc>,
}

/// Response after successful ingestion
#[derive(Debug, Serialize)]
pub struct IngestResponse {
    /// Number of records accepted
    pub accepted: usize,

    /// Number of records rejected
    pub rejected: usize,

    /// Next checkpoint for incremental sync
    pub next_checkpoint: Option<String>,

    /// Pipeline activity ID for tracking
    pub activity_id: String,
}

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub storage: Arc<Storage>,
    pub drive_config: crate::api::DriveConfig,
    pub stream_writer: Arc<Mutex<StreamWriter>>,
    pub tool_executor: Option<Arc<crate::tools::ToolExecutor>>,
    pub yjs_state: super::yjs::YjsState,
    pub chat_cancel_state: ChatCancellationState,
}

/// Enable extracting SqlitePool from AppState for auth middleware
impl axum::extract::FromRef<AppState> for sqlx::SqlitePool {
    fn from_ref(state: &AppState) -> Self {
        state.db.pool().clone()
    }
}

/// Enable extracting YjsState from AppState for WebSocket handlers
impl axum::extract::FromRef<AppState> for super::yjs::YjsState {
    fn from_ref(state: &AppState) -> Self {
        state.yjs_state.clone()
    }
}

/// Enable extracting ChatCancellationState from AppState for chat handlers
impl axum::extract::FromRef<AppState> for ChatCancellationState {
    fn from_ref(state: &AppState) -> Self {
        state.chat_cancel_state.clone()
    }
}

/// Main ingestion handler
pub async fn ingest(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: std::result::Result<Json<IngestRequest>, JsonRejection>,
) -> Response {
    let payload = match payload {
        Ok(Json(payload)) => payload,
        Err(rejection) => {
            tracing::warn!(error = %rejection, "Failed to parse ingest payload");
            return rejection.into_response();
        }
    };
    // Extract and validate device token from Authorization header
    let device_token = match extract_device_token(&headers) {
        Some(token) => token,
        None => {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({
                "error": "Missing device token",
                "hint": "Include 'Authorization: Bearer <device_token>' or 'X-Device-Token: <device_token>' header"
            }))).into_response();
        }
    };

    // Validate device token and get source_id
    let source_id = match crate::api::validate_device_token(state.db.pool(), &device_token).await {
        Ok(id) => id,
        Err(e) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Invalid or revoked device token",
                    "message": e.to_string()
                })),
            )
                .into_response();
        }
    };

    // Update last_seen timestamp
    if let Err(e) = crate::api::update_last_seen(state.db.pool(), &source_id).await {
        tracing::warn!("Failed to update last_seen: {}", e);
    }

    // Validate source and stream exist
    if let Err(e) = validate_source_stream(&state.db, &payload.source, &payload.stream).await {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response();
    }

    // Process records using PushStream trait
    let (accepted, rejected) = match process_batch(
        &state,
        &source_id,
        &payload.source,
        &payload.stream,
        &payload.records,
        &payload.device_id,
        payload.timestamp,
    )
    .await
    {
        Ok(counts) => counts,
        Err(e) => {
            tracing::error!("Failed to process records: {}", e);
            (0, payload.records.len())
        }
    };

    // Trigger transforms if records were successfully processed (hot path like cloud syncs)
    if accepted > 0 {
        if let Err(e) = trigger_transforms_for_batch(&state, &source_id, &payload.stream).await {
            tracing::error!(
                error = %e,
                error_debug = ?e,
                source_id = %source_id,
                stream = %payload.stream,
                device_id = %payload.device_id,
                accepted = accepted,
                rejected = rejected,
                "CRITICAL: Failed to trigger transforms for device batch - data will NOT be archived or transformed!"
            );
        }
    }

    (
        StatusCode::OK,
        Json(IngestResponse {
            accepted,
            rejected,
            next_checkpoint: None, // Checkpoint management will be implemented later if needed
            activity_id: uuid::Uuid::new_v4().to_string(), // Generate ID for debugging/tracing
        }),
    )
        .into_response()
}

/// Extract device token from Authorization header
fn extract_device_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .or_else(|| {
            headers
                .get("x-device-token")
                .and_then(|h| h.to_str().ok())
                .map(|s| s.to_string())
        })
}

/// Validate source and stream configuration exists
///
/// This validation is now minimal - we accept any source/stream combination and let
/// the downstream processors and transform registry determine if it's valid. This
/// avoids maintaining a hardcoded list that can get out of sync with the rest of the system.
async fn validate_source_stream(_db: &Database, _source: &str, _stream: &str) -> Result<()> {
    // Basic validation - just ensure strings are not empty
    // Downstream processors will handle unknown source/stream combinations
    Ok(())
}

/// Process batch of records using PushStream trait
async fn process_batch(
    state: &AppState,
    source_id: &str,
    source: &str,
    stream: &str,
    records: &[Value],
    device_id: &str,
    timestamp: DateTime<Utc>,
) -> Result<(usize, usize)> {
    // Create factory and get stream instance
    let factory = StreamFactory::new(
        state.db.pool().clone(),
        state.storage.clone(),
        state.stream_writer.clone(),
    );

    // Create the stream instance using the new StreamType pattern
    let stream_instance = factory.create_stream_typed(source_id, stream).await?;

    // Ensure we got a PushStream (device data should always be push)
    let push_stream = match stream_instance {
        StreamType::Push(push_stream) => push_stream,
        StreamType::Pull(_) => {
            // This shouldn't happen - pull streams (Google, Notion) don't use the ingest endpoint
            return Err(Error::Other(format!(
                "Unexpected pull stream at ingest endpoint: {}",
                stream
            )));
        }
    };

    // Build IngestPayload for the push stream
    let payload = IngestPayload {
        source: source.to_string(),
        stream: stream.to_string(),
        device_id: device_id.to_string(),
        records: records.to_vec(),
        timestamp,
    };

    // Call receive_push with source_id and payload
    // source_id is created once in handler - single source of truth
    let result = push_stream.receive_push(source_id, payload).await?;

    // Return (accepted, rejected) counts
    // Note: PushResult only tracks received/written, so we calculate rejected as difference
    let accepted = result.records_written;
    let rejected = result
        .records_received
        .saturating_sub(result.records_written);

    Ok((accepted, rejected))
}

/// Trigger transforms for device batch (hot path - unified with cloud syncs)
///
/// After device records are processed and buffered in StreamWriter,
/// this function collects them, writes to filesystem, and triggers transform jobs.
/// This fully unifies the push (device) and pull (cloud) data pipelines.
async fn trigger_transforms_for_batch(
    state: &AppState,
    source_id: &str,
    stream_name: &str,
) -> Result<()> {
    // Collect buffered records from StreamWriter
    let (records, min_timestamp, max_timestamp) = {
        let mut writer = state.stream_writer.lock().await;
        match writer.collect_records(source_id, stream_name) {
            Some((records, min_ts, max_ts)) => {
                tracing::info!(
                    source_id = %source_id,
                    stream_name,
                    record_count = records.len(),
                    "Collected records from device batch for direct transform (hot path)"
                );
                (records, min_ts, max_ts)
            }
            None => {
                tracing::debug!(
                    source_id = %source_id,
                    stream_name,
                    "No buffered records to transform"
                );
                return Ok(());
            }
        }
    };

    // Write records directly to filesystem
    let _storage_key = if !records.is_empty() {
        match write_stream_records(
            state.db.pool(),
            state.storage.as_ref(),
            source_id,
            stream_name,
            &records,
            min_timestamp,
            max_timestamp,
        )
        .await
        {
            Ok(key) => {
                tracing::info!(
                    storage_key = %key,
                    source_id = %source_id,
                    stream_name = %stream_name,
                    record_count = records.len(),
                    "Records written to filesystem successfully"
                );

                // Update watermarks on elt_stream_connections for push streams
                if let Err(e) = sqlx::query(
                    r#"
                    UPDATE elt_stream_connections
                    SET earliest_record_at = COALESCE(earliest_record_at, $1),
                        latest_record_at = $2,
                        last_sync_at = datetime('now'),
                        sync_status = 'incremental',
                        updated_at = datetime('now')
                    WHERE source_connection_id = $3 AND stream_name = $4
                    "#,
                )
                .bind(min_timestamp)
                .bind(max_timestamp)
                .bind(source_id)
                .bind(stream_name)
                .execute(state.db.pool())
                .await
                {
                    tracing::warn!(
                        error = %e,
                        source_id = %source_id,
                        stream_name = %stream_name,
                        "Failed to update stream connection watermarks"
                    );
                }

                Some(key)
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    error_debug = ?e,
                    source_id = %source_id,
                    stream_name = %stream_name,
                    record_count = records.len(),
                    "Failed to write records to filesystem!"
                );
                return Err(e);
            }
        }
    } else {
        None
    };

    // Create context without data source for transform triggering
    // The create_transform_job_for_stream function will create a new context
    // with the actual MemoryDataSource when executing the transform
    let api_keys = crate::jobs::ApiKeys::from_env();

    let context = Arc::new(crate::jobs::TransformContext::new(
        Arc::clone(&state.storage),
        state.stream_writer.clone(),
        api_keys,
    ));

    let executor = crate::jobs::JobExecutor::new(state.db.pool().clone(), (*context).clone());

    // Create and execute transform job with in-memory records (hot path)
    let _job_id = crate::jobs::create_transform_job_for_stream(
        state.db.pool(),
        &executor,
        &context,
        source_id.to_string(),
        stream_name,
        Some(records), // Pass collected records for direct transform
    )
    .await?;

    Ok(())
}

/// Write stream records directly to filesystem and record metadata
async fn write_stream_records(
    db: &sqlx::SqlitePool,
    storage: &Storage,
    source_id: &str,
    stream_name: &str,
    records: &[Value],
    min_timestamp: Option<DateTime<Utc>>,
    max_timestamp: Option<DateTime<Utc>>,
) -> Result<String> {
    use crate::storage::models::StreamKeyBuilder;

    // Get source type from source connection
    let source_type: String =
        sqlx::query_scalar("SELECT source FROM elt_source_connections WHERE id = $1")
            .bind(source_id)
            .fetch_one(db)
            .await?;

    let date = Utc::now().date_naive();
    let key_builder = StreamKeyBuilder::new(None, &source_type, source_id, stream_name, date)
        .map_err(|e| Error::Other(format!("Invalid stream key: {}", e)))?;
    let storage_key = key_builder.build();

    // Write JSONL to filesystem
    storage.upload_jsonl(&storage_key, records).await?;

    // Calculate size
    let size_bytes: i64 = records
        .iter()
        .map(|r| serde_json::to_string(r).unwrap_or_default().len() as i64)
        .sum();

    // Record metadata in elt_stream_objects
    let stream_object_id =
        crate::ids::generate_id(crate::ids::STREAM_OBJECT_PREFIX, &[&storage_key]);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ingest_request_deserialize() {
        let json = r#"{
            "source": "ios",
            "stream": "healthkit",
            "device_id": "iPhone-123",
            "records": [{"value": 72, "unit": "bpm"}],
            "timestamp": "2024-01-01T00:00:00Z"
        }"#;

        let request: IngestRequest = serde_json::from_str(json).expect("test JSON should be valid");
        assert_eq!(request.source, "ios");
        assert_eq!(request.stream, "healthkit");
        assert_eq!(request.records.len(), 1);
    }
}
