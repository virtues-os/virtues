//! Shared types used across the API

use chrono::{DateTime, Utc};
use ts_rs::TS;

/// A user's connected source instance
/// This represents an actual connected account with auth tokens.
/// For source type info, see registry::RegisteredSource.
///
/// Note: `id` is stored as TEXT in SQLite (UUID string format).
#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow, TS)]
#[ts(export, export_to = "../../apps/web/src/lib/types/")]
pub struct SourceConnection {
    pub id: String,
    pub source: String,
    pub name: String,
    pub auth_type: String,
    pub is_active: bool,
    pub is_internal: bool,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_sync_at: Option<String>,
    pub enabled_streams_count: i64,
    pub total_streams_count: i64,
}

/// Connection status with sync statistics
#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow, TS)]
#[ts(export, export_to = "../../apps/web/src/lib/types/")]
pub struct SourceConnectionStatus {
    pub id: String,
    pub name: String,
    pub source: String,
    pub is_active: bool,
    pub is_internal: bool,
    pub last_sync_at: Option<String>,
    pub error_message: Option<String>,
    pub total_syncs: i64,
    pub successful_syncs: i64,
    pub failed_syncs: i64,
    pub last_sync_status: Option<String>,
    pub last_sync_duration_ms: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Export TypeScript types for frontend use
    #[test]
    fn export_typescript_types() {
        SourceConnection::export().expect("Failed to export SourceConnection");
        SourceConnectionStatus::export().expect("Failed to export SourceConnectionStatus");
    }
}
