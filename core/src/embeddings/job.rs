//! Embedding batch job
//!
//! NOTE: Vector embeddings are currently disabled for SQLite migration.
//! This module will be re-enabled when sqlite-vec is integrated.
//!
//! Processes unembedded records from ontology tables in batches.
//! Uses the ontology registry to dynamically discover searchable tables.

use super::client::EmbeddingClient;
use crate::error::Result;
use crate::ontologies::registry::ontology_registry;
use sqlx::SqlitePool;

/// Result of an embedding job execution
#[derive(Debug, Clone, serde::Serialize)]
pub struct EmbeddingJobResult {
    pub table: String,
    pub records_processed: usize,
    pub records_failed: usize,
    pub duration_ms: u64,
}

/// Embedding job for batch processing ontology tables
///
/// NOTE: Vector embeddings are currently disabled for SQLite migration.
/// All methods return stub/empty results until sqlite-vec is integrated.
pub struct EmbeddingJob {
    pool: SqlitePool,
    #[allow(dead_code)]
    client: EmbeddingClient,
}

impl EmbeddingJob {
    /// Create a new embedding job
    pub fn new(pool: SqlitePool, client: EmbeddingClient) -> Self {
        Self { pool, client }
    }

    /// Create embedding job from environment configuration
    pub fn from_env(pool: SqlitePool) -> Result<Self> {
        let client = EmbeddingClient::from_env()?;
        Ok(Self::new(pool, client))
    }

    /// Create embedding job with custom configuration
    pub fn with_config(pool: SqlitePool, endpoint: &str, model: &str) -> Result<Self> {
        let client = EmbeddingClient::with_config(endpoint, model)?;
        Ok(Self::new(pool, client))
    }

    /// Process all searchable ontology tables (discovered from registry)
    ///
    /// NOTE: Currently disabled - returns empty results.
    /// Vector embeddings will be re-enabled when sqlite-vec is integrated.
    pub async fn process_all(&self) -> Result<Vec<EmbeddingJobResult>> {
        tracing::info!("Embedding job skipped: vector search disabled for SQLite migration");

        // Return stub results for each searchable table
        let results = ontology_registry()
            .list_searchable()
            .iter()
            .filter(|o| o.embedding.is_some())
            .map(|o| EmbeddingJobResult {
                table: o.table_name.to_string(),
                records_processed: 0,
                records_failed: 0,
                duration_ms: 0,
            })
            .collect();

        Ok(results)
    }

    /// Get embedding statistics for all searchable tables (discovered from registry)
    ///
    /// NOTE: Returns stub stats showing all records as pending since embeddings are disabled.
    pub async fn get_stats(&self) -> Result<EmbeddingStats> {
        let mut tables = Vec::new();

        for ontology in ontology_registry().list_searchable() {
            let stats = self.get_table_stats(ontology.table_name).await?;
            tables.push(stats);
        }

        Ok(EmbeddingStats { tables })
    }

    async fn get_table_stats(&self, table: &str) -> Result<TableEmbeddingStats> {
        // Just count total records - embeddings are disabled
        let row =
            sqlx::query_as::<_, (i64,)>(&format!("SELECT COUNT(*) as total FROM data_{}", table))
                .fetch_one(&self.pool)
                .await?;

        let total = row.0 as usize;

        Ok(TableEmbeddingStats {
            table: table.to_string(),
            total,
            embedded: 0, // No embeddings in SQLite yet
            skipped: 0,
            pending: total, // All records are pending
        })
    }
}

/// Statistics for embedding coverage across all searchable ontologies
#[derive(Debug, Clone, serde::Serialize)]
pub struct EmbeddingStats {
    /// Stats for each searchable table (dynamically discovered from registry)
    pub tables: Vec<TableEmbeddingStats>,
}

/// Statistics for a single table
#[derive(Debug, Clone, serde::Serialize)]
pub struct TableEmbeddingStats {
    pub table: String,
    pub total: usize,
    /// Records with embeddings
    pub embedded: usize,
    /// Records skipped (empty content, marked as processed)
    pub skipped: usize,
    /// Records not yet processed
    pub pending: usize,
}
