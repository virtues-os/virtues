//! Semantic search API
//!
//! NOTE: Semantic search is currently disabled for SQLite migration.
//! This module will be re-enabled when sqlite-vec is integrated.
//!
//! Provides semantic search across embedded ontology tables.

use crate::embeddings::EmbeddingJob;
use crate::error::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Semantic search request
#[derive(Debug, Clone, Deserialize)]
pub struct SemanticSearchRequest {
    /// Natural language search query
    pub query: String,

    /// Maximum results to return (default: 20, max: 100)
    #[serde(default = "default_limit")]
    pub limit: u32,

    /// Content types to search (default: all)
    /// Dynamically discovered from ontology registry (ontologies with embedding config)
    #[serde(default)]
    pub content_types: Option<Vec<String>>,

    /// Minimum similarity score (0.0-1.0, default: 0.3)
    #[serde(default = "default_min_similarity")]
    pub min_similarity: f32,
}

fn default_limit() -> u32 {
    20
}

fn default_min_similarity() -> f32 {
    0.3
}

/// Individual search result
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    /// Content type (email, message, calendar, ai_conversation)
    pub content_type: String,

    /// Record ID
    pub id: Uuid,

    /// Title (if applicable)
    pub title: Option<String>,

    /// Content preview (first ~200 chars)
    pub preview: String,

    /// Author/sender name
    pub author: Option<String>,

    /// Timestamp
    pub timestamp: DateTime<Utc>,

    /// Similarity score (0.0-1.0)
    pub similarity: f32,
}

/// Semantic search response
#[derive(Debug, Clone, Serialize)]
pub struct SemanticSearchResponse {
    /// Search results ordered by similarity
    pub results: Vec<SearchResult>,

    /// Original query
    pub query: String,

    /// Total results returned
    pub total_results: usize,

    /// Search execution time in milliseconds
    pub search_time_ms: u64,
}

/// Execute semantic search across embedded ontology tables
///
/// NOTE: Semantic search is currently disabled for SQLite migration.
/// Returns empty results until sqlite-vec is integrated.
pub async fn semantic_search(
    _pool: &SqlitePool,
    request: SemanticSearchRequest,
) -> Result<SemanticSearchResponse> {
    let start = std::time::Instant::now();

    tracing::info!("Semantic search skipped: vector search disabled for SQLite migration");

    // Return empty results - vector search is disabled
    Ok(SemanticSearchResponse {
        results: vec![],
        query: request.query,
        total_results: 0,
        search_time_ms: start.elapsed().as_millis() as u64,
    })
}

/// Get embedding statistics
pub async fn get_embedding_stats(pool: &SqlitePool) -> Result<crate::embeddings::EmbeddingStats> {
    let job = EmbeddingJob::from_env(pool.clone())?;
    job.get_stats().await
}

/// Trigger embedding job for all tables
pub async fn trigger_embedding_job(
    pool: &SqlitePool,
) -> Result<Vec<crate::embeddings::EmbeddingJobResult>> {
    let job = EmbeddingJob::from_env(pool.clone())?;
    job.process_all().await
}
