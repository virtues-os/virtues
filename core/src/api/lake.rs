//! Lake API — summary + stream listing for the data archive.
//!
//! Returns empty summaries until a `data_archives` table is wired up.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::Result;
use crate::types::Timestamp;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LakeSummary {
    pub total_bytes: i64,
    pub compressed_bytes: i64,
    pub compression_ratio: f64,
    pub encrypted: bool,
    pub stream_count: i64,
    pub object_count: i64,
    pub record_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LakeStream {
    pub source_id: String,
    pub source_name: String,
    pub source_type: String,
    pub stream_name: String,
    pub size_bytes: i64,
    pub record_count: i64,
    pub object_count: i64,
    pub earliest_at: Option<Timestamp>,
    pub latest_at: Option<Timestamp>,
}

pub async fn get_lake_summary(_pool: &SqlitePool) -> Result<LakeSummary> {
    Ok(LakeSummary {
        total_bytes: 0,
        compressed_bytes: 0,
        compression_ratio: 0.0,
        encrypted: true,
        stream_count: 0,
        object_count: 0,
        record_count: 0,
    })
}

pub async fn list_lake_streams(_pool: &SqlitePool) -> Result<Vec<LakeStream>> {
    Ok(Vec::new())
}
