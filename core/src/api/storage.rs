//! Storage API - List and view stored stream objects

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::{Error, Result};
use crate::storage::Storage;
use crate::types::Timestamp;

/// Summary of a stream object for listing
/// Note: UUIDs are stored as TEXT in SQLite
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StreamObjectSummary {
    pub id: String,
    pub source_connection_id: String,
    pub source_name: String,
    pub source_type: String,
    pub stream_name: String,
    pub storage_key: String,
    pub record_count: i32,
    pub size_bytes: i64,
    pub min_timestamp: Option<Timestamp>,
    pub max_timestamp: Option<Timestamp>,
    pub created_at: Timestamp,
}

/// Content of a stream object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectContent {
    pub id: String,
    pub storage_key: String,
    pub records: Vec<serde_json::Value>,
    pub record_count: usize,
}

/// List recent stream objects with source metadata
///
/// Returns the most recently created stream objects, joining with source_connections
/// to get human-readable source names.
pub async fn list_recent_objects(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<StreamObjectSummary>> {
    let objects = sqlx::query_as::<_, StreamObjectSummary>(
        r#"
        SELECT
            so.id,
            so.source_connection_id,
            sc.name as source_name,
            sc.source as source_type,
            so.stream_name,
            so.storage_key,
            so.record_count,
            so.size_bytes,
            so.min_timestamp,
            so.max_timestamp,
            so.created_at
        FROM elt_stream_objects so
        JOIN elt_source_connections sc ON so.source_connection_id = sc.id
        ORDER BY so.created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list stream objects: {e}")))?;

    Ok(objects)
}

/// Internal struct for querying stream object metadata
#[derive(Debug, sqlx::FromRow)]
struct StreamObjectMetadata {
    id: String,
    storage_key: String,
}

/// Get content of a stream object
///
/// Fetches the object from storage and parses the JSONL content into a vector of JSON values.
pub async fn get_object_content(
    pool: &SqlitePool,
    storage: &Storage,
    object_id: String,
) -> Result<ObjectContent> {
    let object_id_str = object_id;

    // 1. Get object metadata from database
    let metadata = sqlx::query_as::<_, StreamObjectMetadata>(
        r#"
        SELECT id, storage_key
        FROM elt_stream_objects
        WHERE id = $1
        "#,
    )
    .bind(&object_id_str)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to query stream object: {e}")))?
    .ok_or_else(|| Error::NotFound(format!("Stream object not found: {object_id_str}")))?;

    // 2. Download from storage
    let data = storage
        .download(&metadata.storage_key)
        .await
        .map_err(|e| Error::Other(format!("Failed to download object from storage: {e}")))?;

    // 3. Parse JSONL content (newline-delimited JSON)
    let content = String::from_utf8(data)
        .map_err(|e| Error::Other(format!("Invalid UTF-8 in object content: {e}")))?;

    let records: Vec<serde_json::Value> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            serde_json::from_str(line)
                .map_err(|e| {
                    tracing::warn!("Failed to parse JSONL line: {e}");
                    e
                })
                .ok()
        })
        .collect();

    Ok(ObjectContent {
        id: metadata.id,
        storage_key: metadata.storage_key,
        record_count: records.len(),
        records,
    })
}
