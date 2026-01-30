//! Lake API - Summary and stream listing for the immutable data archive

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::{Error, Result};
use crate::types::Timestamp;

/// Summary statistics for the data lake
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LakeSummary {
    /// Total bytes stored (uncompressed estimate)
    pub total_bytes: i64,
    /// Actual bytes on disk (compressed)
    pub compressed_bytes: i64,
    /// Compression ratio (0.0 - 1.0, lower is better)
    pub compression_ratio: f64,
    /// Whether data is encrypted at rest
    pub encrypted: bool,
    /// Number of unique streams
    pub stream_count: i64,
    /// Total number of archived objects
    pub object_count: i64,
    /// Total number of records across all objects
    pub record_count: i64,
}

/// A stream in the data lake with its statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LakeStream {
    /// Source connection ID
    pub source_id: String,
    /// Human-readable source name
    pub source_name: String,
    /// Source provider type (google, plaid, ios, etc.)
    pub source_type: String,
    /// Stream name (messages, events, heart_rate, etc.)
    pub stream_name: String,
    /// Total bytes for this stream
    pub size_bytes: i64,
    /// Total record count
    pub record_count: i64,
    /// Number of archived objects
    pub object_count: i64,
    /// Earliest record timestamp
    pub earliest_at: Option<Timestamp>,
    /// Latest record timestamp
    pub latest_at: Option<Timestamp>,
}

/// Internal struct for querying lake summary
#[derive(Debug, sqlx::FromRow)]
struct LakeSummaryRow {
    total_bytes: Option<i64>,
    object_count: Option<i64>,
    record_count: Option<i64>,
    stream_count: Option<i64>,
}

/// Internal struct for querying stream data
#[derive(Debug, sqlx::FromRow)]
struct LakeStreamRow {
    source_connection_id: String,
    source_name: String,
    source_type: String,
    stream_name: String,
    size_bytes: Option<i64>,
    record_count: Option<i64>,
    object_count: Option<i64>,
    earliest_at: Option<Timestamp>,
    latest_at: Option<Timestamp>,
}

/// Get summary statistics for the data lake
pub async fn get_lake_summary(pool: &SqlitePool) -> Result<LakeSummary> {
    // Get aggregate stats from elt_stream_objects
    let row = sqlx::query_as::<_, LakeSummaryRow>(
        r#"
        SELECT
            COALESCE(SUM(size_bytes), 0) as total_bytes,
            COUNT(*) as object_count,
            COALESCE(SUM(record_count), 0) as record_count,
            COUNT(DISTINCT source_connection_id || ':' || stream_name) as stream_count
        FROM elt_stream_objects
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get lake summary: {e}")))?;

    let total_bytes = row.total_bytes.unwrap_or(0);

    // For now, assume ~60% compression ratio as estimate
    // In practice, JSONL with gzip typically achieves 60-80% compression
    let compression_ratio = 0.60;
    let compressed_bytes = (total_bytes as f64 * compression_ratio) as i64;

    Ok(LakeSummary {
        total_bytes,
        compressed_bytes,
        compression_ratio,
        encrypted: true, // Lake data is always encrypted at rest
        stream_count: row.stream_count.unwrap_or(0),
        object_count: row.object_count.unwrap_or(0),
        record_count: row.record_count.unwrap_or(0),
    })
}

/// List all streams in the data lake with their statistics
pub async fn list_lake_streams(pool: &SqlitePool) -> Result<Vec<LakeStream>> {
    let rows = sqlx::query_as::<_, LakeStreamRow>(
        r#"
        SELECT
            so.source_connection_id,
            sc.name as source_name,
            sc.source as source_type,
            so.stream_name,
            COALESCE(SUM(so.size_bytes), 0) as size_bytes,
            COALESCE(SUM(so.record_count), 0) as record_count,
            COUNT(*) as object_count,
            MIN(so.min_timestamp) as earliest_at,
            MAX(so.max_timestamp) as latest_at
        FROM elt_stream_objects so
        JOIN elt_source_connections sc ON so.source_connection_id = sc.id
        GROUP BY so.source_connection_id, so.stream_name, sc.name, sc.source
        ORDER BY size_bytes DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list lake streams: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row| LakeStream {
            source_id: row.source_connection_id,
            source_name: row.source_name,
            source_type: row.source_type,
            stream_name: row.stream_name,
            size_bytes: row.size_bytes.unwrap_or(0),
            record_count: row.record_count.unwrap_or(0),
            object_count: row.object_count.unwrap_or(0),
            earliest_at: row.earliest_at,
            latest_at: row.latest_at,
        })
        .collect())
}
