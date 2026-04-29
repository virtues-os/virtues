//! Storage API — list and view stored stream objects.
//!
//! Returns empty results until a replacement archive table lands.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::{Error, Result};
use crate::storage::Storage;
use crate::types::Timestamp;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectContent {
    pub id: String,
    pub storage_key: String,
    pub records: Vec<serde_json::Value>,
    pub record_count: usize,
}

pub async fn list_recent_objects(
    _pool: &SqlitePool,
    _limit: i64,
) -> Result<Vec<StreamObjectSummary>> {
    Ok(Vec::new())
}

pub async fn get_object_content(
    _pool: &SqlitePool,
    _storage: &Storage,
    object_id: String,
) -> Result<ObjectContent> {
    Err(Error::NotFound(format!("Stream object not found: {object_id}")))
}
