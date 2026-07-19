//! Cross-encoder reranker — llama-server-backed (enabled in v0.1.1).
//!
//! Mirrors `embedder.rs`: a dedicated `llama-server --rerank` sidecar
//! (`http://127.0.0.1:18182` by default, `VIRTUES_RERANK_URL` to override)
//! hosts a gte-reranker-modernbert-base GGUF and speaks the Jina/Cohere-style
//! `/v1/rerank` JSON that llama.cpp has shipped since late 2024. The
//! installer runs it as `virtues-rerank.service`.
//!
//! Why a second sidecar instead of Ollama (v0.1.0's embedding host):
//! Ollama has no rerank endpoint — its API surface stops at generate/chat/
//! embed. A cross-encoder produces its relevance score through a
//! classification head; pushing the GGUF through `/api/embed` runs the
//! encoder but never the head, returning plausible-looking vectors that
//! rank as noise. llama-server runs the head and returns real scores.
//!
//! Scores are the classifier's raw logits (unbounded); `query.rs` applies
//! the sigmoid when it folds them into result ordering. If the sidecar is
//! down, `get_reranker()` errors and the search pipeline falls back to
//! bi-encoder cosine ranking (fallback lives in `query.rs`).

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::OnceCell;

const DEFAULT_URL: &str = "http://127.0.0.1:18182";

/// Score from the cross-encoder reranker. `index` refers to the position
/// in the `documents` slice passed to `rerank_async`; `score` is the raw
/// classifier logit.
#[derive(Debug, Clone)]
pub struct RerankScore {
    pub index: usize,
    pub score: f32,
}

#[derive(Deserialize)]
struct RerankResponse {
    results: Vec<RerankRow>,
}

#[derive(Deserialize)]
struct RerankRow {
    index: usize,
    relevance_score: f32,
}

/// llama-server HTTP-backed reranker. The sidecar owns the model, GPU,
/// threading; one POST scores every (query, document) pair in the batch. One of
/// the two backends behind [`LocalReranker`].
struct HttpReranker {
    client: reqwest::Client,
    base_url: String,
}

impl HttpReranker {
    async fn new() -> Result<Self> {
        let base_url = resolve_base_url();
        // See embedder.rs: reqwest's rustls build panics "No provider set"
        // without a process default provider. Be self-sufficient. Idempotent.
        crate::http_client::ensure_crypto_provider();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        // Liveness check at init only — per-search rerank calls surface
        // their own errors and query.rs falls back to cosine ranking.
        let health = format!("{base_url}/health");
        client
            .get(&health)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .with_context(|| {
                format!(
                    "rerank sidecar unreachable at {base_url} — \
                     check: systemctl status virtues-rerank"
                )
            })?;

        Ok(Self { client, base_url })
    }

    pub async fn rerank_async(
        self: &Arc<Self>,
        query: &str,
        documents: &[String],
    ) -> Result<Vec<RerankScore>> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }
        let resp = self
            .client
            .post(format!("{}/v1/rerank", self.base_url))
            .json(&serde_json::json!({
                "query": query,
                "documents": documents,
                "top_n": documents.len(),
            }))
            .send()
            .await
            .map_err(|e| anyhow!("rerank request failed: {e}"))?
            .error_for_status()
            .map_err(|e| anyhow!("rerank request failed: {e}"))?;

        let body: RerankResponse = resp.json().await.context("parsing /v1/rerank response")?;
        let mut scores: Vec<RerankScore> = body
            .results
            .into_iter()
            .map(|r| {
                if r.index >= documents.len() {
                    return Err(anyhow!(
                        "rerank sidecar returned out-of-range index {}",
                        r.index
                    ));
                }
                Ok(RerankScore { index: r.index, score: r.relevance_score })
            })
            .collect::<Result<_>>()?;
        scores.sort_by_key(|s| s.index);
        Ok(scores)
    }
}

fn resolve_base_url() -> String {
    std::env::var("VIRTUES_RERANK_URL")
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|_| DEFAULT_URL.to_string())
}

/// The reranker callers use — a thin wrapper preserving the public surface
/// from when this dispatched between an HTTP backend and a native QNN client.
/// One inference path now: the `/v1/rerank` contract. On Dragon the endpoint
/// behind `VIRTUES_RERANK_URL` is `virtues-qnnd` (ColBERT MaxSim on the NPU —
/// unbounded-positive monotonic scores; `query.rs`'s sigmoid preserves their
/// order); everywhere else it's llama-server's cross-encoder logits.
pub struct LocalReranker {
    inner: Arc<HttpReranker>,
}

impl LocalReranker {
    async fn new() -> Result<Self> {
        Ok(Self { inner: Arc::new(HttpReranker::new().await?) })
    }

    /// Score `documents` against `query`; one score per document, indexed into
    /// the input slice.
    pub async fn rerank_async(
        &self,
        query: &str,
        documents: &[String],
    ) -> Result<Vec<RerankScore>> {
        self.inner.rerank_async(query, documents).await
    }

    fn backend_label(&self) -> &'static str {
        "http-contract"
    }
}

static RERANKER: OnceCell<Arc<LocalReranker>> = OnceCell::const_new();

/// Errors when the sidecar is unreachable; a failed init is retried on the
/// next call (OnceCell only caches success), so a rerank daemon that comes
/// up after the box does starts being used without a restart.
pub async fn get_reranker() -> Result<Arc<LocalReranker>> {
    let reranker = RERANKER
        .get_or_try_init(|| async {
            tracing::info!("Initializing reranker...");
            let start = std::time::Instant::now();
            let reranker = LocalReranker::new().await?;
            tracing::info!(
                "Reranker ready in {:.1}s (backend={})",
                start.elapsed().as_secs_f64(),
                reranker.backend_label()
            );
            Ok::<_, anyhow::Error>(Arc::new(reranker))
        })
        .await?;
    Ok(reranker.clone())
}
