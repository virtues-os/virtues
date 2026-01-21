//! Storage API - List and view stored stream objects

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::storage::encryption::{derive_stream_key, encode_key_base64, parse_master_key_hex};
use crate::storage::models::StreamKeyParser;
use crate::storage::{EncryptionKey, Storage};

/// Summary of a stream object for listing
/// Note: UUIDs are stored as TEXT in SQLite
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StreamObjectSummary {
    pub id: String,
    pub source_connection_id: String,
    pub source_name: String,
    pub source_type: String,
    pub stream_name: String,
    pub s3_key: String,
    pub record_count: i32,
    pub size_bytes: i64,
    pub min_timestamp: Option<String>,
    pub max_timestamp: Option<String>,
    pub created_at: String,
}

/// Content of a stream object after decryption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectContent {
    pub id: String,
    pub s3_key: String,
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
            so.s3_key,
            so.record_count,
            so.size_bytes,
            so.min_timestamp,
            so.max_timestamp,
            so.created_at
        FROM data_stream_objects so
        JOIN data_source_connections sc ON so.source_connection_id = sc.id
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
    source_connection_id: String,
    stream_name: String,
    s3_key: String,
}

/// Get decrypted content of a stream object
///
/// Fetches the object from S3, decrypts it using the derived key,
/// and parses the JSONL content into a vector of JSON values.
pub async fn get_object_content(
    pool: &SqlitePool,
    storage: &Storage,
    object_id: Uuid,
) -> Result<ObjectContent> {
    let object_id_str = object_id.to_string();

    // 1. Get object metadata from database
    let metadata = sqlx::query_as::<_, StreamObjectMetadata>(
        r#"
        SELECT id, source_connection_id, stream_name, s3_key
        FROM data_stream_objects
        WHERE id = $1
        "#,
    )
    .bind(&object_id_str)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to query stream object: {e}")))?
    .ok_or_else(|| Error::NotFound(format!("Stream object not found: {object_id}")))?;

    // 2. Parse date from S3 key for encryption key derivation
    let date = StreamKeyParser::parse_date_from_key(&metadata.s3_key)?;

    // 3. Get master encryption key from environment
    let master_key_hex = std::env::var("STREAM_ENCRYPTION_MASTER_KEY").map_err(|_| {
        Error::Other("Encryption not configured: STREAM_ENCRYPTION_MASTER_KEY not set".to_string())
    })?;
    let master_key = parse_master_key_hex(&master_key_hex)?;

    // 4. Derive encryption key for this specific object
    // Parse source_connection_id back to Uuid for key derivation
    let source_conn_uuid = Uuid::parse_str(&metadata.source_connection_id)
        .map_err(|e| Error::Database(format!("Invalid source_connection_id UUID: {e}")))?;
    let derived_key = derive_stream_key(
        &master_key,
        source_conn_uuid,
        &metadata.stream_name,
        date,
    )?;
    let encryption_key = EncryptionKey {
        key_base64: encode_key_base64(&derived_key),
    };

    // 5. Download and decrypt from S3
    let data = storage
        .download_encrypted(&metadata.s3_key, &encryption_key)
        .await
        .map_err(|e| Error::Other(format!("Failed to download object from S3: {e}")))?;

    // 6. Parse JSONL content (newline-delimited JSON)
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
        s3_key: metadata.s3_key,
        record_count: records.len(),
        records,
    })
}
