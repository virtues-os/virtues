//! Semantic search query engine
//!
//! Embeds the query text, then searches the sqlite-vec virtual table
//! joined with search_embeddings metadata for ranked results.
//! Optionally reranks top candidates using a cross-encoder for higher precision.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;

use super::embedder::get_embedder;
use super::reranker::get_reranker;

/// A single semantic search result.
///
/// The `score` field is always normalized to [0, 1]:
/// - Before reranking: cosine similarity (1 - distance)
/// - After reranking: sigmoid of the cross-encoder logit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub ontology: String,
    pub record_id: String,
    pub score: f64,
    pub title: Option<String>,
    pub preview: Option<String>,
    pub author: Option<String>,
    pub timestamp: Option<String>,
}

/// Semantic search engine
pub struct SemanticSearchEngine {
    pool: Arc<SqlitePool>,
}

impl SemanticSearchEngine {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self { pool }
    }

    /// Ensure the vec_search virtual table exists.
    ///
    /// Called at startup after sqlite-vec extension is registered.
    /// Safe to call multiple times (IF NOT EXISTS).
    pub async fn ensure_vec_table(&self) -> Result<()> {
        sqlx::query(
            "CREATE VIRTUAL TABLE IF NOT EXISTS vec_search USING vec0(\
             embedding_id TEXT PRIMARY KEY, \
             embedding float[768])",
        )
        .execute(self.pool.as_ref())
        .await?;
        tracing::info!("vec_search virtual table ready");
        Ok(())
    }

    /// Search for similar documents by natural language query.
    ///
    /// Uses a two-stage pipeline:
    /// 1. Bi-encoder (cosine similarity) retrieves top-30 candidates
    /// 2. Cross-encoder reranker scores query-document pairs for precision
    ///
    /// Falls back to bi-encoder-only if the reranker is unavailable.
    pub async fn search(
        &self,
        query: &str,
        ontologies: Option<&[String]>,
        date_after: Option<&str>,
        date_before: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<SearchResult>> {
        let embedder = get_embedder().await?;
        let query_vec = embedder.embed_async(query).await?;
        let limit = limit.unwrap_or(10).clamp(1, 50);

        // Over-fetch for reranking: 3x requested limit, minimum 30
        let recall_limit = (limit * 3).max(30);

        // Serialize embedding as bytes for sqlite-vec (f32 little-endian)
        let embedding_bytes: Vec<u8> = query_vec
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        // Build dynamic SQL with optional filters
        let mut sql = String::from(
            "SELECT se.ontology, se.record_id, se.title, se.preview, se.author, se.timestamp, \
             vec_distance_cosine(vs.embedding, ?) as distance \
             FROM vec_search vs \
             JOIN search_embeddings se ON vs.embedding_id = se.id \
             WHERE 1=1",
        );

        if let Some(onts) = ontologies {
            if !onts.is_empty() {
                let placeholders: Vec<String> = (0..onts.len()).map(|_| "?".to_string()).collect();
                sql.push_str(&format!(" AND se.ontology IN ({})", placeholders.join(",")));
            }
        }

        if date_after.is_some() {
            sql.push_str(" AND se.timestamp >= ?");
        }
        if date_before.is_some() {
            sql.push_str(" AND se.timestamp <= ?");
        }

        sql.push_str(" ORDER BY distance ASC LIMIT ?");

        // Use raw query with manual binding since we have dynamic params
        let mut db_query = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>, Option<String>, f64)>(&sql)
            .bind(&embedding_bytes);

        if let Some(onts) = ontologies {
            for ont in onts {
                db_query = db_query.bind(ont);
            }
        }

        if let Some(da) = date_after {
            db_query = db_query.bind(da);
        }
        if let Some(db) = date_before {
            db_query = db_query.bind(db);
        }

        db_query = db_query.bind(recall_limit);

        let rows = db_query.fetch_all(self.pool.as_ref()).await?;

        let mut candidates: Vec<SearchResult> = rows
            .into_iter()
            .map(|row| SearchResult {
                ontology: row.0,
                record_id: row.1,
                title: row.2,
                preview: row.3,
                author: row.4,
                timestamp: row.5,
                // Convert cosine distance to similarity score (1 - distance)
                score: 1.0 - row.6,
            })
            .collect();

        // Stage 2: Rerank candidates with cross-encoder
        if candidates.len() > 1 {
            match self.rerank_candidates(query, &mut candidates).await {
                Ok(()) => {
                    let truncated_query: String = query.chars().take(60).collect();
                    tracing::debug!(
                        "Reranked {} candidates for query: {}",
                        candidates.len(),
                        truncated_query
                    );
                }
                Err(e) => {
                    tracing::warn!("Reranker unavailable, using bi-encoder ranking: {}", e);
                    // Fall through — candidates keep their cosine similarity order
                }
            }
        }

        // Trim to requested limit
        candidates.truncate(limit as usize);
        Ok(candidates)
    }

    /// Rerank candidates using the cross-encoder.
    ///
    /// Fetches full text for each candidate from source tables,
    /// scores query-document pairs, and re-sorts candidates in place.
    /// Scores are normalized to [0, 1] via sigmoid.
    async fn rerank_candidates(
        &self,
        query: &str,
        candidates: &mut Vec<SearchResult>,
    ) -> Result<()> {
        let reranker = get_reranker().await?;

        // Fetch full text for reranking
        let full_texts = self.fetch_full_texts(candidates).await?;

        if full_texts.is_empty() {
            return Ok(());
        }

        // Build documents list aligned with candidates.
        // Track which indices have usable text for the reranker.
        let mut rerank_indices: Vec<usize> = Vec::new();
        let mut rerank_docs: Vec<String> = Vec::new();

        for (i, c) in candidates.iter().enumerate() {
            let text = full_texts
                .get(&i)
                .cloned()
                .or_else(|| c.preview.clone())
                .unwrap_or_default();

            if !text.is_empty() {
                rerank_indices.push(i);
                rerank_docs.push(text);
            }
            // Empty-text candidates keep their original cosine score
        }

        if rerank_docs.is_empty() {
            return Ok(());
        }

        let scores = reranker.rerank_async(query, &rerank_docs).await?;

        // Apply normalized reranker scores (sigmoid of logit → [0, 1])
        for score in &scores {
            let original_idx = rerank_indices[score.index];
            let normalized = 1.0 / (1.0 + (-(score.score as f64)).exp());
            candidates[original_idx].score = normalized;
        }

        // Sort by score descending (higher = more relevant)
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        Ok(())
    }

    /// Fetch full text for candidates from their source tables.
    ///
    /// Groups candidates by ontology and batch-queries each source table
    /// using the `embed_text_sql` expression from the ontology config.
    /// Returns a map of candidate_index -> full_text.
    async fn fetch_full_texts(
        &self,
        candidates: &[SearchResult],
    ) -> Result<HashMap<usize, String>> {
        let ontologies = virtues_registry::ontologies::registered_ontologies();
        let ont_map: HashMap<&str, &virtues_registry::ontologies::OntologyDescriptor> = ontologies
            .iter()
            .map(|o| (o.name, o))
            .collect();

        // Group candidate indices by ontology
        let mut by_ontology: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
        for (i, c) in candidates.iter().enumerate() {
            by_ontology
                .entry(c.ontology.as_str())
                .or_default()
                .push((i, c.record_id.as_str()));
        }

        let mut result: HashMap<usize, String> = HashMap::new();

        for (ont_name, items) in &by_ontology {
            let descriptor = match ont_map.get(ont_name) {
                Some(d) => d,
                None => continue,
            };
            let config = match &descriptor.embedding {
                Some(c) => c,
                None => continue,
            };

            let placeholders: Vec<&str> = (0..items.len()).map(|_| "?").collect();
            let sql = format!(
                "SELECT t.id, {} as full_text FROM {} t WHERE t.id IN ({})",
                config.embed_text_sql,
                descriptor.table_name,
                placeholders.join(","),
            );

            let mut query = sqlx::query_as::<_, (String, Option<String>)>(&sql);
            for (_, record_id) in items {
                query = query.bind(*record_id);
            }

            let rows = match query.fetch_all(self.pool.as_ref()).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("Failed to fetch full text for {}: {}", ont_name, e);
                    continue;
                }
            };

            // Map record_id -> full_text
            let text_map: HashMap<&str, String> = rows
                .iter()
                .filter_map(|(id, text)| text.as_ref().map(|t| (id.as_str(), t.clone())))
                .collect();

            // Map back to candidate indices
            for (idx, record_id) in items {
                if let Some(text) = text_map.get(record_id) {
                    result.insert(*idx, text.clone());
                }
            }
        }

        Ok(result)
    }
}
