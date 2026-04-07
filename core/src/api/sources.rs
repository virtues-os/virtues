//! Source management API - CRUD operations for data sources

use sqlx::SqlitePool;

use super::types::{SourceConnection, SourceConnectionStatus};
use crate::error::{Error, Result};

/// List all configured sources
///
/// Returns all sources in the database with stream counts and last sync time.
pub async fn list_sources(db: &SqlitePool) -> Result<Vec<SourceConnection>> {
    let sources = sqlx::query_as::<_, SourceConnection>(
        r#"
        SELECT
            s.id,
            s.source,
            s.name,
            s.auth_type,
            s.is_active,
            s.is_internal,
            s.error_message,
            s.created_at,
            s.updated_at,
            MAX(st.last_sync_at) as last_sync_at,
            COALESCE(COUNT(DISTINCT CASE WHEN st.is_enabled THEN st.stream_name END), 0) as enabled_streams_count,
            COALESCE(COUNT(DISTINCT st.stream_name), 0) as total_streams_count
        FROM elt_source_connections s
        LEFT JOIN elt_stream_connections st ON s.id = st.source_connection_id
        WHERE NOT (s.auth_type = 'device' AND s.pairing_status IS NULL)
        GROUP BY s.id, s.source, s.name, s.auth_type, s.is_active, s.is_internal, s.error_message, s.created_at, s.updated_at
        ORDER BY s.created_at DESC
        "#,
    )
    .fetch_all(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to list sources: {e}")))?;

    Ok(sources)
}

/// Get a specific source by ID
pub async fn get_source(db: &SqlitePool, source_id: String) -> Result<SourceConnection> {
    let source_id_str = source_id;
    let source = sqlx::query_as::<_, SourceConnection>(
        r#"
        SELECT
            s.id,
            s.source,
            s.name,
            s.auth_type,
            s.is_active,
            s.is_internal,
            s.error_message,
            s.created_at,
            s.updated_at,
            MAX(st.last_sync_at) as last_sync_at,
            COALESCE(COUNT(DISTINCT CASE WHEN st.is_enabled THEN st.stream_name END), 0) as enabled_streams_count,
            COALESCE(COUNT(DISTINCT st.stream_name), 0) as total_streams_count
        FROM elt_source_connections s
        LEFT JOIN elt_stream_connections st ON s.id = st.source_connection_id
        WHERE s.id = $1
        GROUP BY s.id, s.source, s.name, s.auth_type, s.is_active, s.is_internal, s.error_message, s.created_at, s.updated_at
        "#,
    )
    .bind(&source_id_str)
    .fetch_one(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to get source: {e}")))?;

    Ok(source)
}

/// Pause a source by setting is_active to false
///
/// This prevents scheduled syncs from running but keeps the source configured.
pub async fn pause_source(db: &SqlitePool, source_id: String) -> Result<SourceConnection> {
    let source_id_str = &source_id;
    sqlx::query("UPDATE elt_source_connections SET is_active = false, updated_at = datetime('now') WHERE id = $1")
        .bind(source_id_str)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("Failed to pause source: {e}")))?;

    get_source(db, source_id).await
}

/// Resume a source by setting is_active to true
///
/// This re-enables scheduled syncs for the source.
pub async fn resume_source(db: &SqlitePool, source_id: String) -> Result<SourceConnection> {
    let source_id_str = &source_id;
    sqlx::query("UPDATE elt_source_connections SET is_active = true, updated_at = datetime('now') WHERE id = $1")
        .bind(source_id_str)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("Failed to resume source: {e}")))?;

    get_source(db, source_id).await
}

/// Delete a source by ID
///
/// This will cascade delete all associated data in stream tables.
pub async fn delete_source(db: &SqlitePool, source_id: String) -> Result<()> {
    let source_id_str = &source_id;
    sqlx::query("DELETE FROM elt_source_connections WHERE id = $1")
        .bind(source_id_str)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete source: {e}")))?;

    Ok(())
}

/// Get source status with sync statistics
///
/// Returns detailed status including sync history and success rates.
pub async fn get_source_status(db: &SqlitePool, source_id: String) -> Result<SourceConnectionStatus> {
    let source_id_str = &source_id;
    let status = sqlx::query_as::<_, SourceConnectionStatus>(
        r#"
        SELECT
            s.id,
            s.name,
            s.source,
            s.is_active,
            s.is_internal,
            (SELECT MAX(r.completed_at) FROM app_action_runs r
             JOIN app_actions t ON r.action_id = t.id
             WHERE t.action_type = 'sync'
               AND json_extract(t.config, '$.source_connection_id') = s.id
               AND r.status = 'success') as last_sync_at,
            s.error_message,
            COALESCE((SELECT COUNT(*) FROM app_action_runs r
             JOIN app_actions t ON r.action_id = t.id
             WHERE t.action_type = 'sync'
               AND json_extract(t.config, '$.source_connection_id') = s.id), 0) as total_syncs,
            COALESCE((SELECT SUM(CASE WHEN r.status = 'success' THEN 1 ELSE 0 END) FROM app_action_runs r
             JOIN app_actions t ON r.action_id = t.id
             WHERE t.action_type = 'sync'
               AND json_extract(t.config, '$.source_connection_id') = s.id), 0) as successful_syncs,
            COALESCE((SELECT SUM(CASE WHEN r.status = 'error' THEN 1 ELSE 0 END) FROM app_action_runs r
             JOIN app_actions t ON r.action_id = t.id
             WHERE t.action_type = 'sync'
               AND json_extract(t.config, '$.source_connection_id') = s.id), 0) as failed_syncs,
            (SELECT r.status FROM app_action_runs r
             JOIN app_actions t ON r.action_id = t.id
             WHERE t.action_type = 'sync'
               AND json_extract(t.config, '$.source_connection_id') = s.id
             ORDER BY r.started_at DESC LIMIT 1) as last_sync_status,
            (SELECT CAST((julianday(r.completed_at) - julianday(r.started_at)) * 86400000 AS INTEGER)
             FROM app_action_runs r
             JOIN app_actions t ON r.action_id = t.id
             WHERE t.action_type = 'sync'
               AND json_extract(t.config, '$.source_connection_id') = s.id
               AND r.completed_at IS NOT NULL
             ORDER BY r.started_at DESC LIMIT 1) as last_sync_duration_ms
        FROM elt_source_connections s
        WHERE s.id = $1
        "#
    )
    .bind(&source_id_str)
    .fetch_one(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to get source status: {e}")))?;

    Ok(status)
}
