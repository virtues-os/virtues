//! Virtues client - Main interface for the Virtues data pipeline

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use crate::database::Database;
use crate::error::{Error, Result};
use crate::storage::Storage;

/// Main Virtues client for managing personal data
pub struct Virtues {
    pub database: Arc<Database>,
    pub storage: Arc<Storage>,
}

impl Virtues {
    /// Create a new Virtues client builder
    pub fn builder() -> VirtuesBuilder {
        VirtuesBuilder::default()
    }

    /// Initialize the client and verify connections
    pub async fn initialize(&self) -> Result<()> {
        self.database.initialize().await?;
        self.storage.initialize().await?;
        Ok(())
    }

    /// Get the status of all components
    pub async fn status(&self) -> Result<Status> {
        let db_status = self.database.health_check().await?;
        let storage_status = self.storage.health_check().await?;

        // Count active sources
        let active_sources = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM elt_source_connections WHERE is_active = true"
        )
        .fetch_one(self.database.pool())
        .await
        .unwrap_or(0) as usize;

        Ok(Status {
            is_healthy: db_status.is_healthy && storage_status.is_healthy,
            database_status: format!("{db_status:?}"),
            storage_status: format!("{storage_status:?}"),
            active_sources,
        })
    }

    /// Ingest data from a source
    ///
    /// Currently stores raw data to storage. Processing is handled by:
    /// - Device-specific processors (see `sources/*/mod.rs`)
    /// - Transform jobs (see `jobs/transform_job.rs`)
    /// - Sync jobs (see `jobs/sync_job.rs`)
    pub async fn ingest(&self, source: &str, data: Value) -> Result<IngestResult> {
        // Store raw data
        let key = format!("raw/{}/{}.json", source, chrono::Utc::now().timestamp());
        let bytes = serde_json::to_vec(&data)?;
        self.storage.upload(&key, bytes).await?;

        Ok(IngestResult {
            records_ingested: 1,
            source: source.to_string(),
        })
    }

    // Source management operations are now in api.rs
    // Use virtues::list_sources(), virtues::sync_stream(), etc.

    /// Run the HTTP ingestion server
    pub async fn run_server(&self, host: &str, port: u16) -> Result<()> {
        use crate::server;
        server::run(self.clone(), host, port).await
    }
}

impl Clone for Virtues {
    fn clone(&self) -> Self {
        Self {
            database: self.database.clone(),
            storage: self.storage.clone(),
        }
    }
}

/// Builder for creating Virtues clients
#[derive(Default)]
pub struct VirtuesBuilder {
    database_url: Option<String>,
    storage_path: Option<String>,
}

impl VirtuesBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set database connection string
    pub fn database(mut self, url: &str) -> Self {
        self.database_url = Some(url.to_string());
        self
    }

    /// Set storage path for stream archives (local file storage only)
    ///
    /// Note: This is ignored when S3 is configured via environment variables.
    ///
    /// Default paths for file storage:
    /// - Dev: ./core/data/lake
    /// - Prod: Uses S3 storage instead
    pub fn storage_path(mut self, path: &str) -> Self {
        self.storage_path = Some(path.to_string());
        self
    }

    /// Build the Virtues client
    ///
    /// Storage backend selection:
    /// - If S3_ENDPOINT is set, uses S3 storage (production)
    /// - Otherwise, uses file storage (local development)
    pub async fn build(self) -> Result<Virtues> {
        let database_url = self
            .database_url
            .or_else(|| std::env::var("DATABASE_URL").ok())
            .ok_or_else(|| Error::Configuration("Database URL required".to_string()))?;

        let database = Database::new(&database_url)?;

        // Storage backend selection:
        // 1. If S3 is configured, use S3 storage
        // 2. Otherwise, use file storage with explicit path or defaults
        let file_storage_path = self
            .storage_path
            .or_else(|| std::env::var("STORAGE_PATH").ok())
            .unwrap_or_else(|| "./data/lake".to_string());

        let storage = if crate::storage::S3Config::is_configured() {
            match Storage::s3_from_env().await {
                Ok(s3) => match s3.health_check().await {
                    Ok(_) => {
                        tracing::info!("Using S3 storage backend");
                        s3
                    }
                    Err(e) => {
                        tracing::warn!("S3 unreachable ({}), falling back to file storage", e);
                        Storage::file(file_storage_path)?
                    }
                },
                Err(e) => {
                    tracing::warn!("S3 init failed ({}), falling back to file storage", e);
                    Storage::file(file_storage_path)?
                }
            }
        } else {
            tracing::info!(path = %file_storage_path, "Using file storage backend");
            Storage::file(file_storage_path)?
        };

        Ok(Virtues {
            database: Arc::new(database),
            storage: Arc::new(storage),
        })
    }
}

/// Status of the Virtues system
#[derive(Debug, Serialize)]
pub struct Status {
    pub is_healthy: bool,
    pub database_status: String,
    pub storage_status: String,
    pub active_sources: usize,
}

/// Result of an ingestion operation
#[derive(Debug, Serialize)]
pub struct IngestResult {
    pub records_ingested: usize,
    pub source: String,
}

// Source and sync types are now in api.rs
// Use virtues::Source, virtues::SyncLog, etc.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder() {
        let builder = VirtuesBuilder::new()
            .database("sqlite:./data/test.db")
            .storage_path("./test/storage");

        assert!(builder.database_url.is_some());
        assert!(builder.storage_path.is_some());
    }
}
