//! Stream management and configuration API
//!
//! Merges shared metadata from virtues-registry with user-specific state from SQLite.

use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::plaid::PlaidSourceMetadata;
use super::sources::get_source;
use crate::error::{Error, Result};
use crate::storage::stream_writer::StreamWriter;
use crate::types::Timestamp;

/// A user's stream connection
/// Merges RegisteredStream (from registry) with user state (from DB).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StreamConnection {
    pub stream_name: String,
    pub display_name: String,
    pub description: String,
    pub table_name: String,
    pub is_enabled: bool,
    pub cron_schedule: Option<String>,
    pub config: serde_json::Value,
    pub last_sync_at: Option<Timestamp>,
    pub supports_incremental: bool,
    pub supports_full_refresh: bool,
    pub config_schema: serde_json::Value,
    pub config_example: serde_json::Value,
    pub default_cron_schedule: Option<String>,
}

/// Request for enabling a stream
#[derive(Debug, serde::Deserialize)]
pub struct EnableStreamRequest {
    pub config: Option<serde_json::Value>,
}

/// Request for updating stream configuration
#[derive(Debug, serde::Deserialize)]
pub struct UpdateStreamConfigRequest {
    pub config: serde_json::Value,
}

/// Request for updating stream schedule
#[derive(Debug, serde::Deserialize)]
pub struct UpdateStreamScheduleRequest {
    pub cron_schedule: Option<String>,
}

/// List all streams for a source with their connection status
pub async fn list_source_streams(
    db: &SqlitePool,
    source_id: String,
) -> Result<Vec<StreamConnection>> {
    // Get source to determine type
    let source = get_source(db, source_id.clone()).await?;
    let provider = &source.source;
 
    // Get source descriptor from registry
    let source_reg = crate::registry::get_source(provider)
        .ok_or_else(|| Error::Other(format!("Unknown provider: {provider}")))?;
 
    // Get enabled streams from database
    let source_id_str = &source_id;
    let enabled_streams: Vec<(
        String,
        bool,
        Option<String>,
        serde_json::Value,
        Option<Timestamp>,
    )> = sqlx::query_as(
        r#"
            SELECT stream_name, is_enabled, cron_schedule, config, last_sync_at
            FROM elt_stream_connections
            WHERE source_connection_id = $1
            "#,
    )
    .bind(&source_id_str)
    .fetch_all(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to query streams: {e}")))?;

    // Build response by combining registry metadata with database state
    let mut result = Vec::new();
    for stream_reg in &source_reg.streams {
        let stream_desc = &stream_reg.descriptor;

        // Skip disabled streams in the system
        if !stream_desc.enabled {
            continue;
        }

        // Find matching database record
        let db_record = enabled_streams
            .iter()
            .find(|(name, _, _, _, _)| name == stream_desc.name);

        let (is_enabled, cron_schedule, config, last_sync_at) = if let Some(record) = db_record {
            (record.1, record.2.clone(), record.3.clone(), record.4.clone())
        } else {
            (false, None, serde_json::json!({}), None)
        };

        result.push(StreamConnection {
            stream_name: stream_desc.name.to_string(),
            display_name: stream_desc.display_name.to_string(),
            description: stream_desc.description.to_string(),
            table_name: stream_desc.table_name.to_string(),
            is_enabled,
            cron_schedule,
            config,
            last_sync_at,
            supports_incremental: stream_desc.supports_incremental,
            supports_full_refresh: stream_desc.supports_full_refresh,
            config_schema: stream_reg.config_schema.clone(),
            config_example: stream_reg.config_example.clone(),
            default_cron_schedule: stream_desc.default_cron_schedule.map(|s| s.to_string()),
        });
    }

    // For Plaid sources, filter streams based on connected account types
    if provider == "plaid" {
        let metadata_row: Option<(Option<serde_json::Value>,)> =
            sqlx::query_as("SELECT metadata FROM elt_source_connections WHERE id = $1")
                .bind(&source_id_str)
                .fetch_optional(db)
                .await
                .ok()
                .flatten();

        if let Some((Some(metadata),)) = metadata_row {
            if let Ok(plaid_meta) = serde_json::from_value::<PlaidSourceMetadata>(metadata) {
                let account_types = &plaid_meta.connected_account_types;

                if !account_types.is_empty() {
                    result.retain(|stream| {
                        match stream.stream_name.as_str() {
                            "transactions" | "accounts" => true,
                            "investments" => account_types
                                .iter()
                                .any(|t| t == "investment" || t == "brokerage"),
                            "liabilities" => {
                                account_types.iter().any(|t| t == "credit" || t == "loan")
                            }
                            _ => true,
                        }
                    });
                }
            }
        }
    }

    Ok(result)
}

/// Get details for a specific stream
pub async fn get_stream_info(
    db: &SqlitePool,
    source_id: String,
    stream_name: &str,
) -> Result<StreamConnection> {
    let streams = list_source_streams(db, source_id).await?;
    streams
        .into_iter()
        .find(|s| s.stream_name == stream_name)
        .ok_or_else(|| Error::Other(format!("Stream not found: {stream_name}")))
}

/// Enable a stream for a source
pub async fn enable_stream(
    db: &SqlitePool,
    storage: &crate::storage::Storage,
    stream_writer: Arc<Mutex<StreamWriter>>,
    source_id: String,
    stream_name: &str,
    config: Option<serde_json::Value>,
) -> Result<StreamConnection> {
    // Get source to determine type
    let source = get_source(db, source_id.clone()).await?;

    // Validate stream exists in registry
    let stream_reg = crate::registry::get_stream(&source.source, stream_name)
        .ok_or_else(|| Error::Other(format!("Stream not found: {stream_name}")))?;
    let stream_desc = &stream_reg.descriptor;

    // Use provided config or empty object
    let config = config.unwrap_or_else(|| serde_json::json!({}));

    // Get default cron schedule from registry
    let default_schedule = stream_desc.default_cron_schedule;

    // Insert or update streams table
    sqlx::query(
        r#"
        INSERT INTO elt_stream_connections (id, source_connection_id, stream_name, table_name, is_enabled, config, cron_schedule, created_at, updated_at)
        VALUES ($1, $2, $3, $4, true, $5, $6, datetime('now'), datetime('now'))
        ON CONFLICT (source_connection_id, stream_name)
        DO UPDATE SET
            is_enabled = true,
            config = EXCLUDED.config,
            cron_schedule = COALESCE(elt_stream_connections.cron_schedule, EXCLUDED.cron_schedule),
            updated_at = datetime('now')
        "#
    )
    .bind(crate::ids::generate_id(crate::ids::STREAM_PREFIX, &[&source_id, stream_name]))
    .bind(&source_id)
    .bind(stream_name)
    .bind(stream_desc.table_name)
    .bind(&config)
    .bind(default_schedule)
    .execute(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to enable stream: {e}")))?;
 
    // Trigger initial sync for pull-based sources
    if source.auth_type == "oauth2" || source.auth_type == "plaid" {
        let db_clone = db.clone();
        let storage_clone = storage.clone();
        let stream_writer_clone = stream_writer.clone();
        let stream_name_clone = stream_name.to_string();
        let source_id_clone = source_id.clone();
        tokio::spawn(async move {
            match crate::api::jobs::trigger_stream_sync(
                &db_clone,
                &storage_clone,
                stream_writer_clone,
                source_id_clone,
                &stream_name_clone,
                None,
            )
            .await
            {
                Ok(response) => {
                    tracing::info!(
                        "Initial sync job created for {}: job_id={}, status={}",
                        stream_name_clone,
                        response.job_id,
                        response.status
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to create initial sync job for {}: {}",
                        stream_name_clone,
                        e
                    );
                }
            }
        });
    }

    // Return updated stream info
    get_stream_info(db, source_id, stream_name).await
}

/// Disable a stream for a source
pub async fn disable_stream(db: &SqlitePool, source_id: String, stream_name: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE elt_stream_connections
        SET is_enabled = false, updated_at = datetime('now')
        WHERE source_connection_id = $1 AND stream_name = $2
        "#,
    )
    .bind(&source_id)
    .bind(stream_name)
    .execute(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to disable stream: {e}")))?;

    Ok(())
}

/// Update stream configuration
pub async fn update_stream_config(
    db: &SqlitePool,
    source_id: String,
    stream_name: &str,
    config: serde_json::Value,
) -> Result<StreamConnection> {
    // Validate stream exists
    get_stream_info(db, source_id.clone(), stream_name).await?;

    // Update config
    sqlx::query(
        r#"
        UPDATE elt_stream_connections
        SET config = $1, updated_at = datetime('now')
        WHERE source_connection_id = $2 AND stream_name = $3
        "#,
    )
    .bind(&config)
    .bind(&source_id)
    .bind(stream_name)
    .execute(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to update stream config: {e}")))?;
 
    // Return updated stream info
    get_stream_info(db, source_id, stream_name).await
}

/// Update stream cron schedule
pub async fn update_stream_schedule(
    db: &SqlitePool,
    source_id: String,
    stream_name: &str,
    cron_schedule: Option<String>,
) -> Result<StreamConnection> {
    // Validate stream exists
    get_stream_info(db, source_id.clone(), stream_name).await?;
 
    // Update schedule
    sqlx::query(
        r#"
        UPDATE elt_stream_connections
        SET cron_schedule = $1, updated_at = datetime('now')
        WHERE source_connection_id = $2 AND stream_name = $3
        "#,
    )
    .bind(&cron_schedule)
    .bind(&source_id)
    .bind(stream_name)
    .execute(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to update stream schedule: {e}")))?;
 
    // Return updated stream info
    get_stream_info(db, source_id, stream_name).await
}

/// Enable default streams for a newly created source (internal helper)
pub async fn enable_default_streams(
    db: &SqlitePool,
    source_id: String,
    provider: &str,
) -> Result<()> {
    let source_reg = crate::registry::get_source(provider)
        .ok_or_else(|| Error::Other(format!("Unknown provider: {provider}")))?;
 
    for stream_reg in &source_reg.streams {
        let stream_desc = &stream_reg.descriptor;
        if !stream_desc.enabled {
            continue;
        }

        sqlx::query(
            r#"
            INSERT INTO elt_stream_connections (id, source_connection_id, stream_name, table_name, is_enabled, config, cron_schedule, created_at, updated_at)
            VALUES ($1, $2, $3, $4, true, '{}', $5, datetime('now'), datetime('now'))
            ON CONFLICT (source_connection_id, stream_name) DO NOTHING
            "#
        )
        .bind(crate::ids::generate_id(crate::ids::STREAM_PREFIX, &[&source_id, stream_desc.name]))
        .bind(&source_id)
        .bind(stream_desc.name)
        .bind(stream_desc.table_name)
        .bind(stream_desc.default_cron_schedule)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("Failed to enable stream {}: {e}", stream_desc.name)))?;
    }

    Ok(())
}

/// Request for bulk updating multiple streams at once
#[derive(Debug, serde::Deserialize)]
pub struct BulkUpdateStreamsRequest {
    pub streams: Vec<StreamUpdate>,
}

/// Update for a single stream in a bulk operation
#[derive(Debug, serde::Deserialize)]
pub struct StreamUpdate {
    pub stream_name: String,
    pub is_enabled: bool,
    pub config: Option<serde_json::Value>,
}

/// Response for bulk stream update
#[derive(Debug, serde::Serialize)]
pub struct BulkUpdateStreamsResponse {
    pub updated_count: usize,
    pub streams: Vec<StreamConnection>,
}

/// Bulk update multiple streams for a source
pub async fn bulk_update_streams(
    db: &SqlitePool,
    storage: &crate::storage::Storage,
    stream_writer: Arc<Mutex<StreamWriter>>,
    source_id: String,
    updates: Vec<StreamUpdate>,
) -> Result<BulkUpdateStreamsResponse> {
    let source = get_source(db, source_id.clone()).await?;
    let provider = &source.source;

    let source_reg = crate::registry::get_source(provider)
        .ok_or_else(|| Error::Other(format!("Unknown provider: {provider}")))?;

    let mut updated_count = 0;

    for update in &updates {
        let stream_reg = source_reg
            .streams
            .iter()
            .find(|s| s.descriptor.name == update.stream_name && s.descriptor.enabled)
            .ok_or_else(|| {
                Error::Other(format!(
                    "Stream not found or disabled: {}",
                    update.stream_name
                ))
            })?;
        let stream_desc = &stream_reg.descriptor;

        let config = update.config.clone().unwrap_or_else(|| serde_json::json!({}));

        if update.is_enabled {
            sqlx::query(
                r#"
                INSERT INTO elt_stream_connections (id, source_connection_id, stream_name, table_name, is_enabled, config, cron_schedule, created_at, updated_at)
                VALUES ($1, $2, $3, $4, true, $5, $6, datetime('now'), datetime('now'))
                ON CONFLICT (source_connection_id, stream_name)
                DO UPDATE SET
                    is_enabled = true,
                    config = EXCLUDED.config,
                    cron_schedule = COALESCE(elt_stream_connections.cron_schedule, EXCLUDED.cron_schedule),
                    updated_at = datetime('now')
                "#,
            )
            .bind(crate::ids::generate_id(
                crate::ids::STREAM_PREFIX,
                &[&source_id, &update.stream_name],
            ))
            .bind(&source_id)
            .bind(&update.stream_name)
            .bind(stream_desc.table_name)
            .bind(&config)
            .bind(stream_desc.default_cron_schedule)
            .execute(db)
            .await
            .map_err(|e| Error::Database(format!("Failed to enable stream: {e}")))?;

            if source.auth_type == "oauth2" || source.auth_type == "plaid" {
                let db_clone = db.clone();
                let storage_clone = storage.clone();
                let stream_writer_clone = stream_writer.clone();
                let stream_name_clone = update.stream_name.clone();
                let source_id_clone = source_id.clone();
                tokio::spawn(async move {
                    if let Err(e) = crate::api::jobs::trigger_stream_sync(
                        &db_clone,
                        &storage_clone,
                        stream_writer_clone,
                        source_id_clone,
                        &stream_name_clone,
                        None,
                    )
                    .await
                    {
                        tracing::error!(
                            "Failed to create initial sync job for {}: {}",
                            stream_name_clone,
                            e
                        );
                    }
                });
            }
        } else {
            sqlx::query(
                r#"
                UPDATE elt_stream_connections
                SET is_enabled = false, updated_at = datetime('now')
                WHERE source_connection_id = $1 AND stream_name = $2
                "#,
            )
            .bind(&source_id)
            .bind(&update.stream_name)
            .execute(db)
            .await
            .map_err(|e| Error::Database(format!("Failed to disable stream: {e}")))?;
        }

        updated_count += 1;
    }

    let streams = list_source_streams(db, source_id).await?;

    Ok(BulkUpdateStreamsResponse {
        updated_count,
        streams,
    })
}
