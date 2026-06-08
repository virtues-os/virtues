//! Cross-encoder reranker — disabled in v0.1.0.
//!
//! The previous build used a Jina v2 cross-encoder loaded via ORT.
//! That dependency was removed when we migrated embeddings to Ollama
//! (see `embedder.rs` for the why). Ollama doesn't expose a native
//! cross-encoder rerank endpoint, so for v0.1.0 we return an error from
//! `get_reranker()` and let the search pipeline fall back to bi-encoder
//! cosine ranking (the fallback already exists in `query.rs`).
//!
//! Planned for v0.1.1: route reranking through a small generative call
//! against the chat model (LLM-as-judge pattern) OR ship a bge-reranker
//! GGUF that Ollama can host. Either choice keeps the binary clean.

use anyhow::{anyhow, Result};
use std::sync::Arc;

/// Score from the cross-encoder reranker (unused in v0.1.0 but retained
/// for the public type so callers don't need conditional types).
#[derive(Debug, Clone)]
pub struct RerankScore {
    pub index: usize,
    pub score: f32,
}

/// Placeholder reranker type. Held as `Arc<LocalReranker>` by callers; the
/// `rerank_async` method is unreachable because `get_reranker()` always
/// returns an error in v0.1.0.
pub struct LocalReranker;

impl LocalReranker {
    pub async fn rerank_async(
        self: &Arc<Self>,
        _query: &str,
        _documents: &[String],
    ) -> Result<Vec<RerankScore>> {
        Err(anyhow!("reranker disabled in v0.1.0"))
    }
}

pub async fn get_reranker() -> Result<Arc<LocalReranker>> {
    Err(anyhow!(
        "reranker disabled in v0.1.0 — falling back to bi-encoder cosine ranking"
    ))
}
