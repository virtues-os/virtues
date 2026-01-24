//! Shared types used across the API

use crate::types::Timestamp;

/// A user's connected source instance
/// This represents an actual connected account with auth tokens.
/// For source type info, see registry::RegisteredSource.
///
/// Note: `id` is stored as TEXT in SQLite (UUID string format).
#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct SourceConnection {
    pub id: String,
    pub source: String,
    pub name: String,
    pub auth_type: String,
    pub is_active: bool,
    pub is_internal: bool,
    pub error_message: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub last_sync_at: Option<Timestamp>,
    pub enabled_streams_count: i64,
    pub total_streams_count: i64,
}

/// Connection status with sync statistics
#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct SourceConnectionStatus {
    pub id: String,
    pub name: String,
    pub source: String,
    pub is_active: bool,
    pub is_internal: bool,
    pub last_sync_at: Option<Timestamp>,
    pub error_message: Option<String>,
    pub total_syncs: i64,
    pub successful_syncs: i64,
    pub failed_syncs: i64,
    pub last_sync_status: Option<String>,
    pub last_sync_duration_ms: Option<i32>,
}

