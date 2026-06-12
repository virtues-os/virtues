//! Embedding trait + llama-server-backed implementation.
//!
//! v0.1.1 routes embedding through a dedicated `llama-server --embedding`
//! sidecar (`http://127.0.0.1:18181` by default, `VIRTUES_EMBED_URL` to
//! override) speaking the OpenAI-compatible `/v1/embeddings` JSON. The
//! installer ships the per-arch llama-server binary in the release tarball
//! and runs it as `virtues-embed.service`, so there's no third-party
//! daemon, no model registry, and no client crate — just reqwest.
//!
//! History: v0.1.0 used Ollama for this. That worked for embeddings but
//! dead-ended on reranking (Ollama has no rerank endpoint, and pushing a
//! cross-encoder GGUF through `/api/embed` silently returns hidden-state
//! garbage). llama-server hosts both model classes behind the same JSON
//! conventions, so the reranker (`reranker.rs`) is now this file's twin.
//! The even-earlier in-process ORT build died on glibc: pyke's prebuilt
//! blobs need glibc ≥2.38, JetPack 6.x ships 2.35. Compiling llama.cpp
//! per-arch in our own CI sidesteps that class of problem permanently.
//!
//! ## Model
//!
//! Default: `bge-m3` (1024-dim, multilingual, 8K-token context, dense+sparse
//! hybrid — top of MTEB for personal-data heterogeneous corpora), served as
//! an F16 GGUF to stay numerically close to the F16 weights Ollama served in
//! v0.1.0 (existing `search_vectors` rows were embedded with those). Whatever
//! model the sidecar loads must produce 1024-dim vectors — the
//! `search_vectors.embedding` column is `vector(1024)`.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::OnceCell;

const EMBED_DIM: usize = 1024;
const DEFAULT_URL: &str = "http://127.0.0.1:18181";

pub trait Embedder: Send + Sync {
    fn dimension(&self) -> usize;
}

#[derive(Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingRow>,
}

#[derive(Deserialize)]
struct EmbeddingRow {
    index: usize,
    embedding: Vec<f32>,
}

/// llama-server HTTP-backed embedder. The sidecar owns the model, GPU,
/// threading; per-call latency is dominated by inference, not transport
/// (loopback HTTP).
pub struct LocalEmbedder {
    client: reqwest::Client,
    base_url: String,
}

impl LocalEmbedder {
    pub async fn new() -> Result<Self> {
        let base_url = resolve_base_url();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?;

        // Liveness check so we fail at startup with a clear message instead
        // of every embed call returning a confusing transport error.
        // llama-server's /health returns 200 once the model is loaded.
        let health = format!("{base_url}/health");
        client
            .get(&health)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .with_context(|| {
                format!(
                    "embedding sidecar unreachable at {base_url} — \
                     check: systemctl status virtues-embed"
                )
            })?;

        Ok(Self { client, base_url })
    }

    pub async fn embed_async(self: &Arc<Self>, text: &str) -> Result<Vec<f32>> {
        let mut vecs = self.request(vec![text.to_string()]).await?;
        let vec = vecs.pop().ok_or_else(|| anyhow!("embedding sidecar returned no embedding"))?;
        Ok(vec)
    }

    pub async fn embed_batch_async(self: &Arc<Self>, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        self.request(texts).await
    }

    async fn request(&self, input: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let n = input.len();
        let resp = self
            .client
            .post(format!("{}/v1/embeddings", self.base_url))
            .json(&serde_json::json!({ "input": input }))
            .send()
            .await
            .map_err(|e| anyhow!("embed request failed: {e}"))?
            .error_for_status()
            .map_err(|e| anyhow!("embed request failed: {e}"))?;

        let body: EmbeddingsResponse =
            resp.json().await.context("parsing /v1/embeddings response")?;
        if body.data.len() != n {
            return Err(anyhow!(
                "embedding sidecar returned {} vectors for {n} inputs",
                body.data.len()
            ));
        }

        // OpenAI-compat responses are index-tagged; order by index rather
        // than trusting array order.
        let mut rows = body.data;
        rows.sort_by_key(|r| r.index);
        for r in &rows {
            validate_dim(&r.embedding)?;
        }
        Ok(rows.into_iter().map(|r| r.embedding).collect())
    }
}

impl Embedder for LocalEmbedder {
    fn dimension(&self) -> usize {
        EMBED_DIM
    }
}

fn validate_dim(v: &[f32]) -> Result<()> {
    if v.len() != EMBED_DIM {
        return Err(anyhow!(
            "embedding dim {} != expected {EMBED_DIM} — check the sidecar's GGUF is a 1024-dim model",
            v.len()
        ));
    }
    Ok(())
}

fn resolve_base_url() -> String {
    std::env::var("VIRTUES_EMBED_URL")
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|_| DEFAULT_URL.to_string())
}

static EMBEDDER: OnceCell<Arc<LocalEmbedder>> = OnceCell::const_new();

pub async fn get_embedder() -> Result<Arc<LocalEmbedder>> {
    let embedder = EMBEDDER
        .get_or_try_init(|| async {
            tracing::info!("Initializing embedding sidecar client...");
            let start = std::time::Instant::now();
            let embedder = LocalEmbedder::new().await?;
            tracing::info!(
                "Embedding sidecar ready in {:.1}s (url={}, dim={})",
                start.elapsed().as_secs_f64(),
                embedder.base_url,
                embedder.dimension()
            );
            Ok::<_, anyhow::Error>(Arc::new(embedder))
        })
        .await?;
    Ok(embedder.clone())
}
