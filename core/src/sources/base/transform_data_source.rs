//! Data source abstraction for transforms
//!
//! Provides a unified interface for transforms to read data from
//! in-memory records (hot path). This enables direct transforms with async S3 archival.
//!
//! ## Chunked Processing
//!
//! To avoid memory pressure with large datasets, records are split into
//! configurable chunks. Set the `TRANSFORM_CHUNK_SIZE` environment variable
//! to control the number of records per batch (default: 10,000).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::error::Result;

/// Default number of records per chunk for transform processing
const DEFAULT_CHUNK_SIZE: usize = 10_000;

/// Get chunk size from environment variable or use default
pub fn get_chunk_size() -> usize {
    std::env::var("TRANSFORM_CHUNK_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CHUNK_SIZE)
}

/// Batch of records from a stream
#[derive(Debug, Clone)]
pub struct StreamBatch {
    pub source_id: Uuid,
    pub stream_name: String,
    pub records: Vec<Value>,
    pub object_id: Uuid,
    pub max_timestamp: Option<DateTime<Utc>>,
}

/// Trait for providing data to transforms
///
/// Currently only supports:
/// - In-memory records (MemoryDataSource) - hot path for real-time transforms
#[async_trait]
pub trait TransformDataSource: Send + Sync {
    /// Read records with checkpoint tracking
    ///
    /// # Arguments
    ///
    /// * `source_id` - UUID of the data source
    /// * `stream_name` - Name of the stream (e.g., "app_export")
    /// * `checkpoint_key` - Unique key for this transform's checkpoint
    ///
    /// # Returns
    ///
    /// Vector of StreamBatch objects containing records and metadata
    async fn read_with_checkpoint(
        &self,
        source_id: Uuid,
        stream_name: &str,
        checkpoint_key: &str,
    ) -> Result<Vec<StreamBatch>>;

    /// Update checkpoint after successful processing
    ///
    /// # Arguments
    ///
    /// * `source_id` - UUID of the data source
    /// * `stream_name` - Name of the stream
    /// * `checkpoint_key` - Unique key for this transform's checkpoint
    /// * `timestamp` - Latest timestamp successfully processed
    async fn update_checkpoint(
        &self,
        source_id: Uuid,
        stream_name: &str,
        checkpoint_key: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<()>;

    /// Get the data source type for logging/metrics
    fn source_type(&self) -> DataSourceType;
}

/// Type of data source
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSourceType {
    /// In-memory records from sync job (hot path - only path)
    Memory,
}

impl std::fmt::Display for DataSourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataSourceType::Memory => write!(f, "memory"),
        }
    }
}

/// In-memory data source for direct transforms (hot path)
///
/// Provides records directly from sync job memory without S3 round-trip.
/// This is used for real-time transforms where records are available immediately.
pub struct MemoryDataSource {
    /// In-memory records from sync job
    records: Vec<Value>,
    /// Source ID for these records
    source_id: Uuid,
    /// Stream name
    stream_name: String,
    /// Minimum timestamp in records (currently unused, reserved for future use)
    #[allow(dead_code)]
    min_timestamp: Option<DateTime<Utc>>,
    /// Maximum timestamp in records
    max_timestamp: Option<DateTime<Utc>>,
    /// Database connection for checkpoint updates
    db: sqlx::PgPool,
}

impl MemoryDataSource {
    /// Create a new memory data source
    pub fn new(
        records: Vec<Value>,
        source_id: Uuid,
        stream_name: String,
        min_timestamp: Option<DateTime<Utc>>,
        max_timestamp: Option<DateTime<Utc>>,
        db: sqlx::PgPool,
    ) -> Self {
        Self {
            records,
            source_id,
            stream_name,
            min_timestamp,
            max_timestamp,
            db,
        }
    }
}

#[async_trait]
impl TransformDataSource for MemoryDataSource {
    async fn read_with_checkpoint(
        &self,
        source_id: Uuid,
        stream_name: &str,
        _checkpoint_key: &str,
    ) -> Result<Vec<StreamBatch>> {
        // Validate source and stream match
        if source_id != self.source_id || stream_name != self.stream_name {
            return Err(crate::error::Error::InvalidInput(format!(
                "Mismatched source/stream: expected {}:{}, got {}:{}",
                self.source_id, self.stream_name, source_id, stream_name
            )));
        }

        // For memory source, we don't have object_id since data isn't in S3 yet
        // Use a nil UUID as placeholder
        let object_id = Uuid::nil();

        // Get chunk size from environment or use default
        let chunk_size = get_chunk_size();

        // If small enough, return as single batch
        if self.records.len() <= chunk_size {
            return Ok(vec![StreamBatch {
                source_id: self.source_id,
                stream_name: self.stream_name.clone(),
                records: self.records.clone(),
                object_id,
                max_timestamp: self.max_timestamp,
            }]);
        }

        // Split records into chunks to avoid memory pressure
        let total_records = self.records.len();
        let num_chunks = (total_records + chunk_size - 1) / chunk_size;

        tracing::info!(
            total_records,
            chunk_size,
            num_chunks,
            "Splitting records into chunks to manage memory"
        );

        let batches: Vec<StreamBatch> = self
            .records
            .chunks(chunk_size)
            .enumerate()
            .map(|(i, chunk)| {
                // Only set max_timestamp on the last chunk
                let is_last = i == num_chunks - 1;
                StreamBatch {
                    source_id: self.source_id,
                    stream_name: self.stream_name.clone(),
                    records: chunk.to_vec(),
                    object_id,
                    max_timestamp: if is_last { self.max_timestamp } else { None },
                }
            })
            .collect();

        Ok(batches)
    }

    async fn update_checkpoint(
        &self,
        source_id: Uuid,
        stream_name: &str,
        checkpoint_key: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        // Update checkpoint in database
        sqlx::query(
            "INSERT INTO data.stream_checkpoints (source_id, stream_name, checkpoint_key, last_processed_at)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (source_id, stream_name, checkpoint_key)
             DO UPDATE SET last_processed_at = EXCLUDED.last_processed_at,
                           updated_at = NOW()",
        )
        .bind(source_id)
        .bind(stream_name)
        .bind(checkpoint_key)
        .bind(timestamp)
        .execute(&self.db)
        .await?;

        Ok(())
    }

    fn source_type(&self) -> DataSourceType {
        DataSourceType::Memory
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_chunk_size_default() {
        // Without env var set, should return default
        std::env::remove_var("TRANSFORM_CHUNK_SIZE");
        assert_eq!(get_chunk_size(), DEFAULT_CHUNK_SIZE);
    }

    #[test]
    fn test_stream_batch_chunking() {
        // Create a large number of records
        let chunk_size = 100;
        let total_records = 350;

        let records: Vec<Value> = (0..total_records)
            .map(|i| serde_json::json!({"id": i}))
            .collect();

        // Manually chunk like MemoryDataSource does
        let num_chunks = (total_records + chunk_size - 1) / chunk_size;
        let chunks: Vec<Vec<Value>> = records.chunks(chunk_size).map(|c| c.to_vec()).collect();

        // Should have 4 chunks: 100, 100, 100, 50
        assert_eq!(num_chunks, 4);
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].len(), 100);
        assert_eq!(chunks[1].len(), 100);
        assert_eq!(chunks[2].len(), 100);
        assert_eq!(chunks[3].len(), 50);
    }
}
