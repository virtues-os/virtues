//! Settings → Search: where search runs, whether it is answering, and how much
//! of the record it covers. Read-only — moving search to another server is
//! `virtues configure-inference --embed-url`, which the page names.

use serde::Serialize;
use sqlx::PgPool;

use crate::error::{Error, Result};

#[derive(Debug, Serialize)]
pub struct SearchStatus {
    /// `dragon` (the NPU daemon), `bundled` (the CPU engine the installer set
    /// up), `manual` (the owner's server), or `unknown` (dev, unset env).
    pub mode: String,
    /// Base URL of the embedding server.
    pub embed_url: String,
    /// The model the index was built with (`search_index_meta.model`), if any.
    pub index_model: Option<String>,
    /// Stored vector width.
    pub index_dims: Option<i32>,
    /// Records with at least one searchable chunk, and the chunks themselves.
    pub records_searchable: i64,
    pub chunks: i64,
    pub last_indexed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The embedding server answered the probe, and how long it took.
    pub reachable: bool,
    pub probe_ms: Option<u64>,
    pub probe_error: Option<String>,
    /// Reranking runs only when `VIRTUES_RERANK_GAP` opts in (search::query).
    pub rerank_on: bool,
    /// An accelerator the installer found on this machine and isn't in use
    /// (written to the env as VIRTUES_ACCELERATOR / _GUIDE), with its guide.
    pub accelerator: Option<String>,
    pub accelerator_guide: Option<String>,
}

pub async fn status(pool: &PgPool) -> Result<SearchStatus> {
    let mode = std::env::var("VIRTUES_INFERENCE").unwrap_or_else(|_| "unknown".to_string());
    let embed_url = crate::search::embedder::resolve_base_url();

    let meta: Option<(Option<String>, Option<i32>)> =
        sqlx::query_as("SELECT model, dim FROM search_index_meta WHERE singleton")
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("reading search_index_meta: {e}")))?;
    // absent-ok: no row means the index has never been built; the query error
    // itself is already propagated by `?` above.
    let (index_model, index_dims) = meta.unwrap_or((None, None));

    let (records_searchable, chunks, last_indexed_at): (i64, i64, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT count(DISTINCT record_id), count(*), max(created_at) \
             FROM search_embeddings WHERE model <> 'skip'",
        )
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("counting the search index: {e}")))?;

    // absent-ok: an unset model name is "default", as the embedder sends it.
    let model = std::env::var("VIRTUES_EMBED_MODEL").unwrap_or_else(|_| "default".to_string());
    let started = std::time::Instant::now();
    let probe = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        crate::search::embedder::probe_vectors(&embed_url, &model),
    )
    .await;
    let (reachable, probe_ms, probe_error) = match probe {
        Ok(Ok(_)) => (true, Some(started.elapsed().as_millis() as u64), None),
        Ok(Err(e)) => (false, None, Some(format!("{e:#}"))),
        Err(_) => (false, None, Some("no answer within 5 seconds".to_string())),
    };

    let rerank_on = std::env::var("VIRTUES_RERANK_GAP")
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .is_some_and(|g| g > 0.0);

    // Only worth showing while search isn't already on the owner's server.
    let env = |k: &str| std::env::var(k).ok().filter(|s| !s.trim().is_empty());
    let (accelerator, accelerator_guide) = if mode == "manual" {
        (None, None)
    } else {
        (env("VIRTUES_ACCELERATOR"), env("VIRTUES_ACCELERATOR_GUIDE"))
    };

    Ok(SearchStatus {
        mode,
        embed_url,
        index_model,
        index_dims,
        records_searchable,
        chunks,
        last_indexed_at,
        reachable,
        probe_ms,
        probe_error,
        rerank_on,
        accelerator,
        accelerator_guide,
    })
}
