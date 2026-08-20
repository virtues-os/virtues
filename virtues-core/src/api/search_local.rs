//! Local content search — the server half of ⌘K.
//!
//! Distinct from `/api/search/web` (Exa), which reaches outside the box. This
//! one only ever returns what the box has already indexed, and is the only
//! search the command palette calls.
//!
//! **No reranker.** The palette runs on every keystroke, so this stops at
//! `recall_and_fuse` — hybrid dense + BM25 with adaptive fusion — and skips
//! `rerank_and_finalize`. Reranking is a cross-encoder/ColBERT pass over the
//! shortlist and costs tens of milliseconds it would spend re-ordering results
//! the user is about to discard by typing another character. Committing to a
//! query (Enter into full search) is where that pass belongs.
//!
//! Objects (a page, a chat, a notebook — the things you *navigate to*) are not
//! here either: the client already holds those in its stores and matches them
//! locally with zero latency. This endpoint is only the content half, and the
//! palette groups the two rather than interleaving them — their scores are on
//! incomparable scales, and merging them by score is precisely the failure the
//! IR notes call the score-scale schism.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::search::query::{SearchFilters, SearchResult};
use crate::search::SemanticSearchEngine;

/// Hard ceiling on returned hits. The palette shows a handful; anything past
/// this is scroll nobody reads.
const MAX_LIMIT: i64 = 20;
const DEFAULT_LIMIT: i64 = 8;

/// Shortest query worth embedding. One or two characters match everything and
/// rank nothing, and the embed round-trip isn't free.
const MIN_QUERY_CHARS: usize = 2;

#[derive(Debug, Deserialize)]
pub struct LocalSearchRequest {
    pub q: String,
    pub limit: Option<i64>,
    /// Restrict to these ontologies. Absent or empty = everything.
    ///
    /// The palette is global by definition, but the wiki's own search box is
    /// not: asking a question from inside the wiki should search the wiki. That
    /// is a scope, not a ranking change — `SearchFilters` already carries the
    /// field; nothing had ever passed it here.
    #[serde(default)]
    pub ontologies: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct LocalSearchResponse {
    pub hits: Vec<SearchResult>,
    /// Echoed so a client dropping stale responses doesn't need to track them.
    pub query: String,
}

/// Content hits for a palette query. Returns empty (not an error) for a query
/// too short to be meaningful — the palette asks on every keystroke and an
/// error per character would be noise.
pub async fn search_local(
    pool: &PgPool,
    request: LocalSearchRequest,
) -> Result<LocalSearchResponse> {
    let query = request.q.trim().to_string();
    if query.chars().count() < MIN_QUERY_CHARS {
        return Ok(LocalSearchResponse { hits: Vec::new(), query });
    }

    let limit = request.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    let engine = SemanticSearchEngine::new(Arc::new(pool.clone()));

    let embedder = crate::search::embedder::get_embedder()
        .await
        .map_err(|e| Error::Other(format!("embedder unavailable: {e}")))?;
    let query_vec = embedder
        .embed_query_async(&query)
        .await
        .map_err(|e| Error::Other(format!("embed query: {e}")))?;
    let query_vector = pgvector::Vector::from(query_vec);
    let terms = crate::search::bm25::tokens(&query);

    // No NOTEBOOK scoping: that is the notebook's own surface, not this one.
    // Ontology scoping is different — it is the caller saying which room they
    // are standing in, and an empty list means the whole house.
    let filters = SearchFilters {
        ontologies: request
            .ontologies
            .unwrap_or_default()
            .into_iter()
            .filter(|o| !o.trim().is_empty())
            .collect(),
        ..SearchFilters::default()
    };

    let mut hits = engine
        .recall_and_fuse(&query_vector, &terms, &filters, limit)
        .await
        .map_err(|e| Error::Other(format!("recall: {e}")))?;

    // `SearchResult` documents its score as normalized to [0, 1]; that
    // normalization lives in `rerank_and_finalize`, which this path skips.
    // Without this the endpoint returns raw fused scores while claiming
    // otherwise — harmless for the palette, which ignores score, and a trap
    // for anything that later believes the type's own contract.
    crate::search::query::normalize_scores(&mut hits);

    Ok(LocalSearchResponse { hits, query })
}
