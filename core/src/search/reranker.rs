//! Cross-encoder reranker via fastembed
//!
//! Uses BGE-reranker-v2-m3 to rerank semantic search results by scoring
//! query-document pairs jointly. This dramatically improves precision over
//! bi-encoder cosine similarity alone.

use anyhow::Result;
use std::sync::{Arc, Mutex};
use tokio::sync::OnceCell;

/// Local cross-encoder reranker using fastembed (BGE-reranker-v2-m3, ONNX Runtime).
///
/// Loaded lazily on first use (~2-5s model load). Stays in memory for the
/// server's lifetime. Subsequent calls are fast (~15-30ms for 30 documents).
///
/// Uses interior mutability (Mutex) because fastembed's `rerank()` requires `&mut self`.
pub struct LocalReranker {
    model: Mutex<fastembed::TextRerank>,
}

/// Score from the cross-encoder reranker.
#[derive(Debug, Clone)]
pub struct RerankScore {
    /// Original index in the input documents slice
    pub index: usize,
    /// Relevance score (higher = more relevant, raw logit)
    pub score: f32,
}

impl LocalReranker {
    /// Create a new LocalReranker with BGE-reranker-v2-m3.
    ///
    /// Downloads the model on first use (~50MB) and loads it into memory.
    pub fn new() -> Result<Self> {
        let model = fastembed::TextRerank::try_new(
            fastembed::RerankInitOptions::new(
                fastembed::RerankerModel::BGERerankerV2M3,
            )
            .with_show_download_progress(true),
        )?;
        Ok(Self {
            model: Mutex::new(model),
        })
    }

    /// Rerank documents against a query using the cross-encoder.
    ///
    /// Returns scores sorted by relevance (descending).
    pub fn rerank(&self, query: &str, documents: &[&str]) -> Result<Vec<RerankScore>> {
        let mut model = self
            .model
            .lock()
            .map_err(|e| anyhow::anyhow!("Reranker lock poisoned: {}", e))?;
        let results = model.rerank(query, documents, false, None)?;
        Ok(results
            .into_iter()
            .map(|r| RerankScore {
                index: r.index,
                score: r.score,
            })
            .collect())
    }

    /// Rerank on the blocking thread pool (async-safe).
    pub async fn rerank_async(
        self: &Arc<Self>,
        query: &str,
        documents: &[String],
    ) -> Result<Vec<RerankScore>> {
        let this = self.clone();
        let query = query.to_string();
        let docs = documents.to_vec();
        tokio::task::spawn_blocking(move || {
            let doc_refs: Vec<&str> = docs.iter().map(|s| s.as_str()).collect();
            this.rerank(&query, &doc_refs)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Rerank task panicked: {}", e))?
    }
}

/// Global lazy-initialized reranker instance.
static RERANKER: OnceCell<Arc<LocalReranker>> = OnceCell::const_new();

/// Get the shared reranker instance (lazy init on first call).
///
/// Model loading (~2-5s) runs on the blocking thread pool to avoid
/// stalling tokio worker threads.
pub async fn get_reranker() -> Result<Arc<LocalReranker>> {
    let reranker = RERANKER
        .get_or_try_init(|| async {
            tracing::info!("Loading cross-encoder reranker (bge-reranker-v2-m3)...");
            let start = std::time::Instant::now();
            let reranker = tokio::task::spawn_blocking(LocalReranker::new)
                .await
                .map_err(|e| anyhow::anyhow!("Reranker init panicked: {}", e))??;
            tracing::info!(
                "Reranker model loaded in {:.1}s",
                start.elapsed().as_secs_f64(),
            );
            Ok::<_, anyhow::Error>(Arc::new(reranker))
        })
        .await?;
    Ok(reranker.clone())
}
