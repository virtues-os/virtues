//! Stream management and configuration API
//!
//! Merges shared metadata from virtues-registry with user-specific state from SQLite.
//! Cron schedules live on `app_actions`, not on `elt_stream_connections`.

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

    // Get enabled streams from database, joining with app_actions for cron_schedule
    let source_id_str = &source_id;
    let enabled_streams: Vec<(
        String,
        bool,
        serde_json::Value,
        Option<Timestamp>,
        Option<String>,
    )> = sqlx::query_as(
        r#"
            SELECT sc.stream_name, sc.is_enabled, sc.config, sc.last_sync_at,
                   st.cron_schedule
            FROM elt_stream_connections sc
            LEFT JOIN app_actions st ON st.id = 'task_sync_' || sc.id
            WHERE sc.source_connection_id = $1
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

        let (is_enabled, config, last_sync_at, cron_schedule) = if let Some(record) = db_record {
            (record.1, record.2.clone(), record.3.clone(), record.4.clone())
        } else {
            (false, serde_json::json!({}), None, None)
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

    // Insert or update stream connection (no cron_schedule column)
    let stream_connection_id = crate::ids::generate_id(crate::ids::STREAM_PREFIX, &[&source_id, stream_name]);
    sqlx::query(
        r#"
        INSERT INTO elt_stream_connections (id, source_connection_id, stream_name, table_name, is_enabled, config, created_at, updated_at)
        VALUES ($1, $2, $3, $4, true, $5, datetime('now'), datetime('now'))
        ON CONFLICT (source_connection_id, stream_name)
        DO UPDATE SET
            is_enabled = true,
            config = EXCLUDED.config,
            updated_at = datetime('now')
        "#
    )
    .bind(&stream_connection_id)
    .bind(&source_id)
    .bind(stream_name)
    .bind(stream_desc.table_name)
    .bind(&config)
    .execute(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to enable stream: {e}")))?;

    // Upsert the scheduled task for this stream
    let action_id = format!("task_sync_{}", stream_connection_id);
    let task_name = format!("{} / {}", source.name, stream_name);
    let task_config = serde_json::json!({
        "source_connection_id": source_id,
        "stream_name": stream_name,
    });
    sqlx::query(
        r#"
        INSERT INTO app_actions (id, action_type, name, cron_schedule, enabled, config, created_at, updated_at)
        VALUES ($1, 'sync', $2, $3, 1, $4, datetime('now'), datetime('now'))
        ON CONFLICT (id) DO UPDATE SET
            enabled = 1,
            cron_schedule = COALESCE(app_actions.cron_schedule, EXCLUDED.cron_schedule),
            updated_at = datetime('now')
        "#
    )
    .bind(&action_id)
    .bind(&task_name)
    .bind(default_schedule)
    .bind(&task_config)
    .execute(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to create sync task: {e}")))?;

    // Trigger initial sync for pull-based sources
    if source.auth_type == "oauth2" || source.auth_type == "plaid" {
        let db_clone = db.clone();
        let storage_clone = storage.clone();
        let stream_writer_clone = stream_writer.clone();
        let stream_name_clone = stream_name.to_string();
        let source_id_clone = source_id.clone();
        tokio::spawn(async move {
            match crate::api::actions::trigger_stream_sync(
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
                        "Initial sync run created for {}: run_id={}, status={}",
                        stream_name_clone,
                        response.run_id,
                        response.status
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to create initial sync run for {}: {}",
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
    // Disable the stream connection
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

    // Also disable the corresponding scheduled task
    sqlx::query(
        r#"
        UPDATE app_actions SET enabled = 0, updated_at = datetime('now')
        WHERE action_type = 'sync'
          AND json_extract(config, '$.source_connection_id') = $1
          AND json_extract(config, '$.stream_name') = $2
        "#,
    )
    .bind(&source_id)
    .bind(stream_name)
    .execute(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to disable sync task: {e}")))?;

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

/// Update stream cron schedule (writes to app_actions)
pub async fn update_stream_schedule(
    db: &SqlitePool,
    source_id: String,
    stream_name: &str,
    cron_schedule: Option<String>,
) -> Result<StreamConnection> {
    // Validate stream exists
    get_stream_info(db, source_id.clone(), stream_name).await?;

    // Update schedule on the scheduled task
    sqlx::query(
        r#"
        UPDATE app_actions
        SET cron_schedule = $1, updated_at = datetime('now')
        WHERE action_type = 'sync'
          AND json_extract(config, '$.source_connection_id') = $2
          AND json_extract(config, '$.stream_name') = $3
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

    // Get source name for task naming
    let source = get_source(db, source_id.clone()).await?;

    for stream_reg in &source_reg.streams {
        let stream_desc = &stream_reg.descriptor;
        if !stream_desc.enabled {
            continue;
        }

        let stream_connection_id = crate::ids::generate_id(crate::ids::STREAM_PREFIX, &[&source_id, stream_desc.name]);

        // Create stream connection (no cron_schedule column)
        sqlx::query(
            r#"
            INSERT INTO elt_stream_connections (id, source_connection_id, stream_name, table_name, is_enabled, config, created_at, updated_at)
            VALUES ($1, $2, $3, $4, true, '{}', datetime('now'), datetime('now'))
            ON CONFLICT (source_connection_id, stream_name) DO NOTHING
            "#
        )
        .bind(&stream_connection_id)
        .bind(&source_id)
        .bind(stream_desc.name)
        .bind(stream_desc.table_name)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("Failed to enable stream {}: {e}", stream_desc.name)))?;

        // Create scheduled task for this stream
        let action_id = format!("task_sync_{}", stream_connection_id);
        let task_config = serde_json::json!({
            "source_connection_id": source_id,
            "stream_name": stream_desc.name,
        });
        sqlx::query(
            r#"
            INSERT INTO app_actions (id, action_type, name, cron_schedule, enabled, config, created_at, updated_at)
            VALUES ($1, 'sync', $2, $3, 1, $4, datetime('now'), datetime('now'))
            ON CONFLICT (id) DO NOTHING
            "#
        )
        .bind(&action_id)
        .bind(format!("{} / {}", source.name, stream_desc.name))
        .bind(stream_desc.default_cron_schedule)
        .bind(&task_config)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("Failed to create sync task for {}: {e}", stream_desc.name)))?;
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
            let stream_connection_id = crate::ids::generate_id(
                crate::ids::STREAM_PREFIX,
                &[&source_id, &update.stream_name],
            );

            // Insert/update stream connection
            sqlx::query(
                r#"
                INSERT INTO elt_stream_connections (id, source_connection_id, stream_name, table_name, is_enabled, config, created_at, updated_at)
                VALUES ($1, $2, $3, $4, true, $5, datetime('now'), datetime('now'))
                ON CONFLICT (source_connection_id, stream_name)
                DO UPDATE SET
                    is_enabled = true,
                    config = EXCLUDED.config,
                    updated_at = datetime('now')
                "#,
            )
            .bind(&stream_connection_id)
            .bind(&source_id)
            .bind(&update.stream_name)
            .bind(stream_desc.table_name)
            .bind(&config)
            .execute(db)
            .await
            .map_err(|e| Error::Database(format!("Failed to enable stream: {e}")))?;

            // Upsert scheduled task
            let action_id = format!("task_sync_{}", stream_connection_id);
            let task_config = serde_json::json!({
                "source_connection_id": source_id,
                "stream_name": update.stream_name,
            });
            sqlx::query(
                r#"
                INSERT INTO app_actions (id, action_type, name, cron_schedule, enabled, config, created_at, updated_at)
                VALUES ($1, 'sync', $2, $3, 1, $4, datetime('now'), datetime('now'))
                ON CONFLICT (id) DO UPDATE SET
                    enabled = 1,
                    cron_schedule = COALESCE(app_actions.cron_schedule, EXCLUDED.cron_schedule),
                    updated_at = datetime('now')
                "#,
            )
            .bind(&action_id)
            .bind(format!("{} / {}", source.name, update.stream_name))
            .bind(stream_desc.default_cron_schedule)
            .bind(&task_config)
            .execute(db)
            .await
            .map_err(|e| Error::Database(format!("Failed to create sync task: {e}")))?;

            if source.auth_type == "oauth2" || source.auth_type == "plaid" {
                let db_clone = db.clone();
                let storage_clone = storage.clone();
                let stream_writer_clone = stream_writer.clone();
                let stream_name_clone = update.stream_name.clone();
                let source_id_clone = source_id.clone();
                tokio::spawn(async move {
                    if let Err(e) = crate::api::actions::trigger_stream_sync(
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
                            "Failed to create initial sync run for {}: {}",
                            stream_name_clone,
                            e
                        );
                    }
                });
            }
        } else {
            // Disable stream connection
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

            // Disable scheduled task
            sqlx::query(
                r#"
                UPDATE app_actions SET enabled = 0, updated_at = datetime('now')
                WHERE action_type = 'sync'
                  AND json_extract(config, '$.source_connection_id') = $1
                  AND json_extract(config, '$.stream_name') = $2
                "#,
            )
            .bind(&source_id)
            .bind(&update.stream_name)
            .execute(db)
            .await
            .map_err(|e| Error::Database(format!("Failed to disable sync task: {e}")))?;
        }

        updated_count += 1;
    }

    let streams = list_source_streams(db, source_id).await?;

    Ok(BulkUpdateStreamsResponse {
        updated_count,
        streams,
    })
}
