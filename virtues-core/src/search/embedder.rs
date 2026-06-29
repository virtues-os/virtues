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
//! `EmbeddingGemma-300M` (QAT Q8_0): a Gemma-3-lineage bidirectional encoder,
//! mean-pooled, **768-dim native** which we Matryoshka-truncate to `EMBED_DIM`
//! (256) and re-normalize — a 4× lighter HNSW index (`search_vectors.embedding`
//! is `vector(256)`). Asymmetric: queries and documents get different prompt
//! prefixes (see `QUERY_PROMPT`/`DOC_PROMPT`). The sidecar must emit 768-dim
//! vectors (`validate_native_dim`); run it on CPU — its activations require
//! bf16/fp32, so the Orin CUDA path forces fp32 and is slower than CPU.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::OnceCell;

/// EmbeddingGemma's native output is 768-dim; we keep only the first
/// `EMBED_DIM` (Matryoshka truncation) and re-normalize — a 3× storage/RAM
/// cut on the index for ~minimal quality loss. `search_vectors.embedding`
/// is `vector(256)` (migration 0017).
const NATIVE_DIM: usize = 768;
const EMBED_DIM: usize = 256;
const DEFAULT_URL: &str = "http://127.0.0.1:18181";

/// EmbeddingGemma is asymmetric — queries and documents get different prompt
/// prefixes (official Gemma formats). On personal data, where queries
/// ("why have I felt off?") look nothing like passages (event records), this
/// is free recall.
const QUERY_PROMPT: &str = "task: search result | query: ";
const DOC_PROMPT: &str = "title: none | text: ";

/// Truncate a native (768-dim) embedding to `EMBED_DIM` and L2-renormalize
/// (Matryoshka). Renormalization is required for cosine after truncation.
fn matryoshka_truncate(v: &mut Vec<f32>) {
    v.truncate(EMBED_DIM);
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

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
        // reqwest is `rustls-tls-no-provider`; building any client (even for
        // loopback HTTP) panics "No provider set" unless the process default
        // provider was installed first. main.rs does it for the server, but
        // tests/other entrypoints construct the embedder directly — be self-
        // sufficient. Idempotent.
        crate::http_client::ensure_crypto_provider();
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

    /// Embed a **query** (asymmetric — uses the query prompt). Use this for
    /// the search-time side; everything that embeds stored content uses the
    /// document-prompt variants below.
    pub async fn embed_query_async(self: &Arc<Self>, text: &str) -> Result<Vec<f32>> {
        let mut vecs = self.request(vec![format!("{QUERY_PROMPT}{text}")]).await?;
        vecs.pop().ok_or_else(|| anyhow!("embedding sidecar returned no embedding"))
    }

    /// Embed a single **document/content** string (document prompt).
    pub async fn embed_async(self: &Arc<Self>, text: &str) -> Result<Vec<f32>> {
        let mut vecs = self.request(vec![format!("{DOC_PROMPT}{text}")]).await?;
        vecs.pop().ok_or_else(|| anyhow!("embedding sidecar returned no embedding"))
    }

    /// Embed a batch of **documents/content** (document prompt).
    pub async fn embed_batch_async(self: &Arc<Self>, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let prompted: Vec<String> = texts.into_iter().map(|t| format!("{DOC_PROMPT}{t}")).collect();
        self.request(prompted).await
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
        let mut out = Vec::with_capacity(rows.len());
        for mut r in rows {
            validate_native_dim(&r.embedding)?;
            matryoshka_truncate(&mut r.embedding); // 768 -> EMBED_DIM, re-normalized
            out.push(r.embedding);
        }
        Ok(out)
    }
}

impl Embedder for LocalEmbedder {
    fn dimension(&self) -> usize {
        EMBED_DIM
    }
}

fn validate_native_dim(v: &[f32]) -> Result<()> {
    if v.len() != NATIVE_DIM {
        return Err(anyhow!(
            "embedding dim {} != expected native {NATIVE_DIM} — check the sidecar's GGUF is EmbeddingGemma-300M",
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
