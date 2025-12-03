//! Semantic search API
//!
//! Provides semantic search across embedded ontology tables using pgvector.

use crate::embeddings::{EmbeddingClient, EmbeddingJob};
use crate::error::{Error, Result};
use crate::ontologies::registry::ontology_registry;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashSet;
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
pub async fn semantic_search(
    pool: &PgPool,
    request: SemanticSearchRequest,
) -> Result<SemanticSearchResponse> {
    let start = std::time::Instant::now();

    // Get embedding settings from assistant profile
    let (endpoint, model): (String, String) = sqlx::query_as(
        r#"
        SELECT
            COALESCE(ollama_endpoint, 'http://localhost:11434'),
            COALESCE(embedding_model_id, 'nomic-embed-text')
        FROM app.assistant_profile
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?
    .unwrap_or_else(|| {
        (
            "http://localhost:11434".to_string(),
            "nomic-embed-text".to_string(),
        )
    });

    // Create embedding client
    let client = EmbeddingClient::with_config(&endpoint, &model)?;

    // Generate query embedding
    let query_embedding = client
        .embed(&request.query)
        .await
        .map_err(|e| Error::Other(format!("Failed to embed query: {}", e)))?;

    // Format embedding for pgvector
    let embedding_str = format!(
        "[{}]",
        query_embedding
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );

    // Get all searchable ontologies from registry
    let searchable = ontology_registry().list_searchable();

    // Build set of content types to search
    let all_content_types: HashSet<&str> = searchable
        .iter()
        .filter_map(|o| o.embedding.as_ref().map(|e| e.content_type))
        .collect();

    let search_types: HashSet<String> = match &request.content_types {
        Some(types) => types
            .iter()
            .filter(|t| all_content_types.contains(t.as_str()))
            .cloned()
            .collect(),
        None => all_content_types.iter().map(|s| s.to_string()).collect(),
    };

    // Build UNION query dynamically from ontology registry
    let mut union_parts = Vec::new();

    for ontology in searchable {
        let emb = match &ontology.embedding {
            Some(e) => e,
            None => continue,
        };

        if !search_types.contains(emb.content_type) {
            continue;
        }

        let title_expr = emb.title_sql.unwrap_or("NULL");
        let author_expr = emb.author_sql.unwrap_or("NULL");

        union_parts.push(format!(
            r#"
            SELECT
                '{}' as content_type,
                id,
                {} as title,
                COALESCE(LEFT({}, 200), '') as preview,
                {} as author,
                {} as timestamp,
                (1 - (embedding <=> '{}'::vector))::real as similarity
            FROM data.{}
            WHERE embedding IS NOT NULL
              AND (1 - (embedding <=> '{}'::vector)) > {}
            "#,
            emb.content_type,
            title_expr,
            emb.preview_sql,
            author_expr,
            emb.timestamp_sql,
            embedding_str,
            ontology.table_name,
            embedding_str,
            request.min_similarity
        ));
    }

    if union_parts.is_empty() {
        return Ok(SemanticSearchResponse {
            results: vec![],
            query: request.query,
            total_results: 0,
            search_time_ms: start.elapsed().as_millis() as u64,
        });
    }

    let limit = request.limit.min(100);
    let query = format!(
        r#"
        SELECT * FROM (
            {}
        ) combined
        ORDER BY similarity DESC
        LIMIT {}
        "#,
        union_parts.join(" UNION ALL "),
        limit
    );

    // Execute search query
    let rows = sqlx::query_as::<_, SearchResultRow>(&query)
        .fetch_all(pool)
        .await?;

    let results: Vec<SearchResult> = rows
        .into_iter()
        .map(|row| SearchResult {
            content_type: row.content_type,
            id: row.id,
            title: row.title,
            preview: row.preview,
            author: row.author,
            timestamp: row.timestamp,
            similarity: row.similarity,
        })
        .collect();

    Ok(SemanticSearchResponse {
        total_results: results.len(),
        results,
        query: request.query,
        search_time_ms: start.elapsed().as_millis() as u64,
    })
}

/// Internal row type for query results
#[derive(sqlx::FromRow)]
struct SearchResultRow {
    content_type: String,
    id: Uuid,
    title: Option<String>,
    preview: String,
    author: Option<String>,
    timestamp: DateTime<Utc>,
    similarity: f32,
}

/// Get embedding statistics
pub async fn get_embedding_stats(pool: &PgPool) -> Result<crate::embeddings::EmbeddingStats> {
    let job = EmbeddingJob::from_env(pool.clone())?;
    job.get_stats().await
}

/// Trigger embedding job for all tables
pub async fn trigger_embedding_job(pool: &PgPool) -> Result<Vec<crate::embeddings::EmbeddingJobResult>> {
    let job = EmbeddingJob::from_env(pool.clone())?;
    job.process_all().await
}
