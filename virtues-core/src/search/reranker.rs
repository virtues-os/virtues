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
//! Scores are the classifier's raw logits (unbounded). `query.rs` uses them
//! ONLY as an ordering — it sorts, then min-max normalizes to [0,1] (both
//! monotonic, so any strictly increasing score scale works; that is what lets
//! Dragon's ColBERT MaxSim serve the same contract). If the sidecar is down,
//! `get_reranker()` errors and the search pipeline falls back to the fused
//! hybrid ranking (fallback lives in `query.rs`).

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

/// One scored row from `/v1/rerank`.
///
/// The score field has two spellings in the wild: `relevance_score`
/// (llama.cpp, Jina) and `score` (Cohere-style). The installer's setup probe
/// has always accepted either, so a Cohere-shaped server passed validation and
/// then failed at every search against a runtime that demanded the first
/// spelling. The alias closes that gap at the end that was wrong — being
/// stricter at runtime than at setup buys nothing, since the number means the
/// same thing under either name.
#[derive(Deserialize)]
struct RerankRow {
    index: usize,
    #[serde(alias = "score")]
    relevance_score: f32,
}

/// HTTP-backed reranker speaking the `/v1/rerank` contract. The sidecar owns
/// the model, GPU/NPU, threading; one POST scores every (query, document) pair
/// in the batch. On Dragon the endpoint is `virtues-qnnd` (ColBERT MaxSim);
/// everywhere else, llama-server's cross-encoder. One inference path — this
/// was previously wrapped in a dispatch layer left over from a native QNN
/// client that no longer exists.
pub struct LocalReranker {
    client: reqwest::Client,
    base_url: String,
}

impl LocalReranker {
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
        //
        // ADVISORY on the status code, for the reason spelled out in
        // embedder.rs: `/health` is llama-server's route, not part of the
        // OpenAI/Jina shape, and a server that simply doesn't implement it
        // (404) can still rerank perfectly. Taking such an endpoint out of
        // service costs precision on every search for a route that was never
        // in the contract. A transport failure stays fatal — nothing is
        // listening, and `get_reranker()` retries on the next search.
        let health = format!("{base_url}/health");
        match client
            .get(&health)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => {}
            Ok(r) => tracing::info!(
                "rerank endpoint {health} answered {} — no /health route, which is fine; \
                 the first rerank call is the real verdict",
                r.status()
            ),
            Err(e) => {
                return Err(anyhow!(e)).with_context(|| {
                    format!(
                        "rerank endpoint unreachable at {base_url} — \
                         nothing accepted a connection; \
                         check: systemctl status virtues-rerank"
                    )
                })
            }
        }

        Ok(Self { client, base_url })
    }

    /// Score `documents` against `query`; one score per document, indexed into
    /// the input slice.
    pub async fn rerank_async(
        &self,
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

pub(crate) fn resolve_base_url() -> String {
    std::env::var("VIRTUES_RERANK_URL")
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|_| DEFAULT_URL.to_string())
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
                "Reranker ready in {:.1}s at {}",
                start.elapsed().as_secs_f64(),
                reranker.base_url
            );
            Ok::<_, anyhow::Error>(Arc::new(reranker))
        })
        .await?;
    Ok(reranker.clone())
}

