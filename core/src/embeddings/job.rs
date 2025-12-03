//! Embedding batch job
//!
//! Processes unembedded records from ontology tables in batches.
//! Uses the ontology registry to dynamically discover searchable tables.

use super::client::{format_embedding_for_pg, EmbeddingClient};
use crate::error::Result;
use crate::ontologies::descriptor::EmbeddingConfig;
use crate::ontologies::registry::ontology_registry;
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Batch size for processing embeddings
const BATCH_SIZE: i64 = 50;

/// Result of an embedding job execution
#[derive(Debug, Clone, serde::Serialize)]
pub struct EmbeddingJobResult {
    pub table: String,
    pub records_processed: usize,
    pub records_failed: usize,
    pub duration_ms: u64,
}

/// Embedding job for batch processing ontology tables
pub struct EmbeddingJob {
    pool: PgPool,
    client: EmbeddingClient,
}

impl EmbeddingJob {
    /// Create a new embedding job
    pub fn new(pool: PgPool, client: EmbeddingClient) -> Self {
        Self { pool, client }
    }

    /// Create embedding job from environment configuration
    pub fn from_env(pool: PgPool) -> Result<Self> {
        let client = EmbeddingClient::from_env()?;
        Ok(Self::new(pool, client))
    }

    /// Create embedding job with custom configuration
    pub fn with_config(pool: PgPool, endpoint: &str, model: &str) -> Result<Self> {
        let client = EmbeddingClient::with_config(endpoint, model)?;
        Ok(Self::new(pool, client))
    }

    /// Process all searchable ontology tables (discovered from registry)
    pub async fn process_all(&self) -> Result<Vec<EmbeddingJobResult>> {
        let mut results = Vec::new();

        for ontology in ontology_registry().list_searchable() {
            if let Some(emb) = &ontology.embedding {
                let result = self.process_table(ontology.table_name, emb).await?;
                results.push(result);
            }
        }

        Ok(results)
    }

    /// Process a single ontology table using its embedding configuration
    async fn process_table(
        &self,
        table_name: &str,
        config: &EmbeddingConfig,
    ) -> Result<EmbeddingJobResult> {
        let start = std::time::Instant::now();
        let mut processed = 0;
        let mut failed = 0;

        // Build the SELECT query dynamically from EmbeddingConfig
        let select_query = format!(
            r#"
            SELECT id, ({}) as embed_text
            FROM data.{}
            WHERE embedding IS NULL
            ORDER BY {} DESC NULLS LAST
            LIMIT $1
            "#,
            config.embed_text_sql, table_name, config.timestamp_sql
        );

        // Build the UPDATE query
        let update_query = format!(
            r#"
            UPDATE data.{}
            SET embedding = $1::vector, embedded_at = NOW()
            WHERE id = $2
            "#,
            table_name
        );

        loop {
            let records = sqlx::query(&select_query)
                .bind(BATCH_SIZE)
                .fetch_all(&self.pool)
                .await?;

            if records.is_empty() {
                break;
            }

            for record in records {
                let id: Uuid = record.get("id");
                let text: Option<String> = record.get("embed_text");
                let text = text.unwrap_or_default();

                // Skip if empty
                if text.trim().is_empty() {
                    continue;
                }

                match self.client.embed(&text).await {
                    Ok(embedding) => {
                        let embedding_str = format_embedding_for_pg(&embedding);

                        if let Err(e) = sqlx::query(&update_query)
                            .bind(&embedding_str)
                            .bind(id)
                            .execute(&self.pool)
                            .await
                        {
                            tracing::error!(
                                "Failed to store {} embedding for {}: {}",
                                table_name,
                                id,
                                e
                            );
                            failed += 1;
                        } else {
                            processed += 1;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to embed {} record {}: {}", table_name, id, e);
                        failed += 1;
                    }
                }
            }

            tracing::info!("Processed {} {} records so far", processed, table_name);
        }

        Ok(EmbeddingJobResult {
            table: table_name.to_string(),
            records_processed: processed,
            records_failed: failed,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Get embedding statistics for all searchable tables (discovered from registry)
    pub async fn get_stats(&self) -> Result<EmbeddingStats> {
        let mut tables = Vec::new();

        for ontology in ontology_registry().list_searchable() {
            let stats = self.get_table_stats(ontology.table_name).await?;
            tables.push(stats);
        }

        Ok(EmbeddingStats { tables })
    }

    async fn get_table_stats(&self, table: &str) -> Result<TableEmbeddingStats> {
        let row = sqlx::query_as::<_, (i64, i64)>(&format!(
            r#"
            SELECT
                COUNT(*) as total,
                COUNT(embedding) as embedded
            FROM data.{}
            "#,
            table
        ))
        .fetch_one(&self.pool)
        .await?;

        Ok(TableEmbeddingStats {
            table: table.to_string(),
            total: row.0 as usize,
            embedded: row.1 as usize,
            pending: (row.0 - row.1) as usize,
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
    pub embedded: usize,
    pub pending: usize,
}
