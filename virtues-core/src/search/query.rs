//! Semantic search query engine.
//!
//! Hybrid retrieval: embeds the query (EmbeddingGemma, 256-dim) and searches
//! `search_vectors` (pgvector `vector(256)`, HNSW cosine) AND the lexical
//! `content_tsv` (Postgres FTS), fuses the two with Reciprocal Rank Fusion,
//! dedupes to one chunk per record, then reranks the survivors with a
//! cross-encoder for precision.

use anyhow::Result;
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

use super::embedder::get_embedder;
use super::reranker::get_reranker;

/// A single semantic search result.
///
/// `score` is normalized to [0, 1] in all cases:
/// - After reranking (the normal path): sigmoid of the cross-encoder logit.
/// - When the reranker is unavailable / not run: the RRF fusion score,
///   min-max-rescaled to [0, 1] within the result set (raw RRF values are
///   tiny, e.g. ~0.03, and only meaningful as a within-query ordering).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub ontology: String,
    pub record_id: String,
    pub score: f64,
    pub title: Option<String>,
    pub preview: Option<String>,
    pub author: Option<String>,
    pub timestamp: Option<String>,
    /// The matched chunk's text — what the reranker scores (not the whole
    /// record). `None` only for legacy rows without `content`.
    #[serde(skip)]
    pub content: Option<String>,
}

/// Semantic search engine
pub struct SemanticSearchEngine {
    pool: Arc<PgPool>,
}

/// Cap each reranker candidate so a (query, doc) pair fits the rerank
/// sidecar's context. The sidecar runs `-c 2048` and llama-server *rejects*
/// (doesn't truncate) a rerank sequence that overflows the ubatch, which
/// would fail the whole batch and silently drop us to bi-encoder ranking.
/// A cross-encoder only needs a document's lead to judge relevance, so
/// truncating to ~512 tokens is standard practice and lossless in effect.
/// ~1000 chars ≈ ~256 tokens — the efficiency knee for a cross-encoder
/// (compute is O(L²), and positional bias anchors relevance to a document's
/// lead, so 256→512 buys ~2-5% accuracy for ~3.5× the latency). Char-based
/// (not byte) slicing keeps it UTF-8 safe.
const MAX_RERANK_CHARS: usize = 1000;

fn truncate_for_rerank(text: &str) -> String {
    if text.chars().count() <= MAX_RERANK_CHARS {
        text.to_string()
    } else {
        text.chars().take(MAX_RERANK_CHARS).collect()
    }
}

/// Min-max rescale candidate scores into [0, 1] in place. Used for the
/// no-rerank fallback, where raw RRF values are tiny (~0.03) and only mean
/// anything as a within-query ordering — this preserves the
/// `SearchResult.score ∈ [0,1]` contract without changing the order.
fn normalize_scores(candidates: &mut [SearchResult]) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for c in candidates.iter() {
        lo = lo.min(c.score);
        hi = hi.max(c.score);
    }
    let span = hi - lo;
    for c in candidates.iter_mut() {
        c.score = if span > 0.0 { (c.score - lo) / span } else { 1.0 };
    }
}

impl SemanticSearchEngine {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Confirms the search_vectors HNSW index is reachable.
    /// Schema creation happens via the migration file; this exists only as
    /// a startup sanity check.
    pub async fn ensure_vec_table(&self) -> Result<()> {
        sqlx::query("SELECT 1 FROM search_vectors LIMIT 0")
            .execute(self.pool.as_ref())
            .await?;
        tracing::info!("search_vectors table ready");
        Ok(())
    }

    /// Search for similar documents by natural language query.
    ///
    /// Pipeline:
    /// 1. Hybrid recall — dense (pgvector cosine) ⊕ lexical (FTS), fused by RRF,
    ///    deduped to one chunk per record, capped at `recall_limit` (≤20).
    /// 2. Cross-encoder reranker scores (query, chunk) pairs for precision.
    ///
    /// Falls back to RRF ordering if the reranker is unavailable.
    pub async fn search(
        &self,
        query: &str,
        ontologies: Option<&[String]>,
        date_after: Option<&str>,
        date_before: Option<&str>,
        // Resolved entity IDs (person/place/org/thing). When set, only rows
        // whose source record references one of these entities are returned —
        // entity-aware retrieval via wiki_entity_refs.
        entities: Option<&[String]>,
        limit: Option<i64>,
    ) -> Result<Vec<SearchResult>> {
        let embedder = get_embedder().await?;
        // Query side of the asymmetric embedding (query prompt).
        let query_vec = embedder.embed_query_async(query).await?;
        let query_vector = Vector::from(query_vec);
        let limit = limit.unwrap_or(10).clamp(1, 50);

        // Over-fetch for reranking: 2x requested limit, capped at 20. The
        // reranker is overhead/layer-bound (latency ~linear in candidate
        // count), and the first stage already surfaces the right doc in the
        // top-20 with good recall — so 20 is the tuned ceiling (~768ms on the
        // appliance), not 30. Bounded by first-stage recall; don't go lower
        // without confirming recall on an eval set.
        let recall_limit = (limit * 2).clamp(10, 20);

        // Hybrid retrieval: dense (pgvector cosine) ⊕ lexical (Postgres FTS),
        // fused with Reciprocal Rank Fusion (RRF, k=60) before reranking. The
        // lexical arm catches exact tokens dense embeddings smear (proper
        // nouns, project names, IDs). $1 = query vector, $2 = query text;
        // filters start at $3 and are shared by both arms; the final
        // placeholder is the recall limit (used as LIMIT in all three stages).
        let mut filter_sql = String::new();
        let mut next = 3usize;
        if let Some(onts) = ontologies {
            if !onts.is_empty() {
                let ph: Vec<String> = (0..onts.len()).map(|i| format!("${}", next + i)).collect();
                filter_sql.push_str(&format!(" AND se.ontology IN ({})", ph.join(",")));
                next += onts.len();
            }
        }
        if date_after.is_some() {
            filter_sql.push_str(&format!(" AND se.timestamp >= ${}", next));
            next += 1;
        }
        if date_before.is_some() {
            filter_sql.push_str(&format!(" AND se.timestamp <= ${}", next));
            next += 1;
        }
        let entity_filter = entities.map(|e| !e.is_empty()).unwrap_or(false);
        if entity_filter {
            filter_sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM wiki_entity_refs er \
                  WHERE er.source_table = se.source_table AND er.source_id = se.record_id \
                  AND er.entity_id = ANY(${}))",
                next
            ));
            next += 1;
        }
        let lim = next; // recall_limit placeholder

        // `, se.id` tiebreakers make ROW_NUMBER (and thus RRF) deterministic
        // across runs when many rows tie on distance/ts_rank. The final stage
        // dedupes to the best-RRF chunk per record (DISTINCT ON record_id) so a
        // multi-chunk record surfaces once, then re-sorts by RRF and caps.
        let sql = format!(
            "WITH dense AS ( \
               SELECT se.id, ROW_NUMBER() OVER (ORDER BY vs.embedding <=> $1, se.id) AS rnk \
               FROM search_vectors vs JOIN search_embeddings se ON vs.embedding_id = se.id \
               WHERE 1=1{f} \
               ORDER BY vs.embedding <=> $1, se.id LIMIT ${lim} \
             ), lexical AS ( \
               SELECT se.id, ROW_NUMBER() OVER (ORDER BY ts_rank(se.content_tsv, websearch_to_tsquery('english', $2)) DESC, se.id) AS rnk \
               FROM search_embeddings se \
               WHERE se.content_tsv @@ websearch_to_tsquery('english', $2){f} \
               ORDER BY ts_rank(se.content_tsv, websearch_to_tsquery('english', $2)) DESC, se.id LIMIT ${lim} \
             ), fused AS ( \
               SELECT COALESCE(d.id, l.id) AS id, \
                      (COALESCE(1.0/(60 + d.rnk), 0) + COALESCE(1.0/(60 + l.rnk), 0))::float8 AS rrf \
               FROM dense d FULL OUTER JOIN lexical l ON d.id = l.id \
             ), best AS ( \
               SELECT DISTINCT ON (se.record_id) \
                      se.ontology, se.record_id, se.title, se.preview, se.author, \
                      to_char(se.timestamp AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as ts, \
                      se.content, f.rrf \
               FROM fused f JOIN search_embeddings se ON se.id = f.id \
               ORDER BY se.record_id, f.rrf DESC, se.id \
             ) \
             SELECT ontology, record_id, title, preview, author, ts, content, rrf \
             FROM best ORDER BY rrf DESC, record_id LIMIT ${lim}",
            f = filter_sql,
            lim = lim,
        );

        let mut db_query = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>, // content (chunk text)
                f64,             // rrf
            ),
        >(&sql)
        .bind(&query_vector)
        .bind(query);

        if let Some(onts) = ontologies {
            for ont in onts {
                db_query = db_query.bind(ont);
            }
        }
        if let Some(da) = date_after {
            db_query = db_query.bind(
                chrono::DateTime::parse_from_rfc3339(da)
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .ok(),
            );
        }
        if let Some(db) = date_before {
            db_query = db_query.bind(
                chrono::DateTime::parse_from_rfc3339(db)
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .ok(),
            );
        }
        if entity_filter {
            db_query = db_query.bind(entities.unwrap().to_vec());
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
                content: row.6,
                // RRF fusion score (tiny; ordering only). Overwritten by the
                // reranker, or min-max-normalized below if rerank doesn't run.
                score: row.7,
            })
            .collect();

        let reranked = if candidates.len() > 1 {
            match self.rerank_candidates(query, &mut candidates).await {
                Ok(did) => {
                    if did {
                        let q: String = query.chars().take(60).collect();
                        tracing::debug!("Reranked {} candidates for query: {}", candidates.len(), q);
                    }
                    did
                }
                Err(e) => {
                    tracing::warn!("Reranker unavailable, using RRF ranking: {}", e);
                    false
                }
            }
        } else {
            false
        };

        // No rerank ran → scores are raw RRF (≈0.03). Min-max rescale to [0,1]
        // so the contract holds and callers/the LLM see interpretable values;
        // candidates are already RRF-ordered so ordering is unchanged.
        if !reranked {
            normalize_scores(&mut candidates);
        }

        candidates.truncate(limit as usize);
        Ok(candidates)
    }

    /// Rerank candidates with the cross-encoder over the matched **chunk**
    /// text (not the whole record — that's the point of chunking). Returns
    /// `true` if it actually reranked, `false` if there were no usable docs.
    /// Errors only when the reranker sidecar is unreachable.
    async fn rerank_candidates(
        &self,
        query: &str,
        candidates: &mut Vec<SearchResult>,
    ) -> Result<bool> {
        let reranker = get_reranker().await?;

        let mut rerank_indices: Vec<usize> = Vec::new();
        let mut rerank_docs: Vec<String> = Vec::new();
        for (i, c) in candidates.iter().enumerate() {
            // The matched chunk is the right rerank unit; `preview` is a
            // fallback for any legacy row indexed before `content` existed.
            let text = c
                .content
                .clone()
                .or_else(|| c.preview.clone())
                .unwrap_or_default();
            if !text.is_empty() {
                rerank_indices.push(i);
                rerank_docs.push(truncate_for_rerank(&text));
            }
        }

        if rerank_docs.is_empty() {
            return Ok(false);
        }

        let scores = reranker.rerank_async(query, &rerank_docs).await?;

        for score in &scores {
            let original_idx = rerank_indices[score.index];
            let normalized = 1.0 / (1.0 + (-(score.score as f64)).exp());
            candidates[original_idx].score = normalized;
        }

        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(true)
    }

}
