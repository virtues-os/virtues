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
//!
//! In **manual inference mode** the endpoint is user-run rather than a
//! sidecar we provisioned; the installer pins a fingerprint of the model at
//! setup time (`VIRTUES_EMBED_FINGERPRINT`) and we re-check it at boot so a
//! silently-swapped model can't corrupt the vector index (see
//! `verify_fingerprint`).
//!
//! ## Model
//!
//! **Dragon** runs `EmbeddingGemma-300M` (QAT Q8_0): a Gemma-3-lineage
//! bidirectional encoder, mean-pooled, **768-dim native** which we
//! Matryoshka-truncate to 256 and re-normalize — a 4× lighter HNSW index. The
//! sidecar must emit 768-dim vectors (`validate_native_dim`); run it on CPU —
//! its activations require bf16/fp32, so fp16 GPU paths force fp32 and end up
//! slower than CPU.
//!
//! **Manual** runs whatever model the user pointed us at: the stored width is
//! its native dims (no truncation — most models aren't Matryoshka-trained),
//! resolved from `VIRTUES_EMBED_DIMS`, and the vector column is sized to match
//! at bringup (`database::ensure_embedding_dims`). Asymmetric query/document
//! prompt prefixes are configurable (`VIRTUES_EMBED_QUERY_PROMPT` / `_DOC_PROMPT`,
//! resolved by the installer); Dragon defaults to EmbeddingGemma's formats.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::OnceCell;

/// EmbeddingGemma's native output is 768-dim; on Dragon we keep only the first
/// `DRAGON_STORED_DIM` (Matryoshka truncation) and re-normalize — a 3× storage/
/// RAM cut on the index for ~minimal quality loss. A manual endpoint stores its
/// **native** dims (no truncation — most models aren't Matryoshka-trained), so
/// the stored width is model-dependent and resolved from `VIRTUES_EMBED_DIMS`
/// at boot (see `configured_embed_dim`); the vector column is sized to match at
/// bringup (`database::embedding_dims`).
const NATIVE_DIM: usize = 768;
const DRAGON_STORED_DIM: usize = 256;
/// pgvector's HNSW index tops out at 2000 dims for the `vector` type. Larger
/// models would need `halfvec` (≤4000) or truncation — not in this build.
pub const MAX_INDEXED_DIM: usize = 2000;
const DEFAULT_URL: &str = "http://127.0.0.1:18181";

/// Is a model fingerprint pinned? True in manual inference mode — the installer
/// probed the user's endpoint and recorded its fingerprint + native dims.
fn fingerprint_pinned_env() -> bool {
    std::env::var("VIRTUES_EMBED_FINGERPRINT")
        .ok()
        .is_some_and(|s| !s.trim().is_empty())
}

/// The stored vector width the index must use — the single source of truth for
/// both the runtime embedder and the bringup column-sizing step. Manual: the
/// probed native dims (`VIRTUES_EMBED_DIMS`, no truncation). Dragon/dev: 256
/// (Matryoshka truncation of EmbeddingGemma's 768).
pub fn configured_embed_dim() -> usize {
    if fingerprint_pinned_env() {
        std::env::var("VIRTUES_EMBED_DIMS")
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
            .filter(|d| *d > 0)
            // Installer writes DIMS alongside the fingerprint; a missing value
            // means a hand-edited env — fall back to native (no truncation)
            // rather than silently truncating the user's model.
            .unwrap_or(NATIVE_DIM)
    } else {
        DRAGON_STORED_DIM
    }
}

/// EmbeddingGemma is asymmetric — queries and documents get different prompt
/// prefixes (official Gemma formats). On personal data, where queries
/// ("why have I felt off?") look nothing like passages (event records), this
/// is free recall. These are the **Dragon defaults**; in manual mode the
/// installer resolves the right prefixes for the user's model (from its
/// HuggingFace `config_sentence_transformers.json`, a known-family table, or
/// none) and pins them via `VIRTUES_EMBED_QUERY_PROMPT` / `_DOC_PROMPT`. Prompt
/// prefixes never touch the fingerprint (probes are embedded raw), so changing
/// them can't trip the boot guard.
const QUERY_PROMPT: &str = "task: search result | query: ";
const DOC_PROMPT: &str = "title: none | text: ";

/// Fixed probe strings for the endpoint fingerprint. Embedded RAW (no
/// query/doc prompt prefix) so setup time and boot time hash the exact same
/// inputs. MUST match the installer's copy in
/// `tools/virtues-installer/src/mode.rs` — the installer pins the
/// fingerprint at setup, we recompute it here at boot.
const FINGERPRINT_PROBES: [&str; 2] =
    ["virtues fingerprint probe 0", "virtues fingerprint probe 1"];

/// SHA256 over the probe vectors with each component quantized to
/// `(x * 10000).round() as i32` (LE bytes). Quantization keeps the hash
/// stable across float formatting / minor backend jitter while still
/// changing on any real model swap. MUST match
/// `tools/virtues-installer/src/mode.rs::fingerprint_vectors`.
fn fingerprint_vectors(vectors: &[Vec<f32>]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for v in vectors {
        for &x in v {
            let q = (x as f64 * 10000.0).round() as i32;
            h.update(q.to_le_bytes());
        }
    }
    hex::encode(h.finalize())
}

/// Truncate a native embedding to `dim` and L2-renormalize (Matryoshka).
/// Renormalization is required for cosine after truncation.
fn matryoshka_truncate(v: &mut Vec<f32>, dim: usize) {
    v.truncate(dim);
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
    /// Set when the installer pinned an endpoint fingerprint (manual
    /// inference mode). When present, the fingerprint check owns model
    /// identity and the fixed `NATIVE_DIM` check is skipped — the pinned
    /// model may legitimately have different native dims.
    fingerprint_pinned: bool,
    /// The `model` field sent on every /v1/embeddings request. llama.cpp
    /// ignores it; Ollama routes by it and 404s on unknown names. Set from
    /// `VIRTUES_EMBED_MODEL` (written by the installer's manual flow); the
    /// literal `"default"` otherwise. MUST match what the installer's
    /// setup-time probes sent, or the boot fingerprint check would compare
    /// vectors from different requests.
    model: String,
    /// Asymmetric prompt prefixes prepended to queries / documents before
    /// embedding. Dragon → EmbeddingGemma's official formats; manual → whatever
    /// the installer resolved for the user's model (possibly empty). Empty
    /// string = no prefix.
    query_prompt: String,
    doc_prompt: String,
    /// The width vectors are stored at (= the vector column's dims). Dragon:
    /// 256 (truncated). Manual: the model's native dims.
    stored_dim: usize,
    /// Whether to Matryoshka-truncate native vectors down to `stored_dim`.
    /// Only Dragon's EmbeddingGemma is Matryoshka-trained; manual endpoints
    /// store native vectors untouched.
    truncate: bool,
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

        let pinned = std::env::var("VIRTUES_EMBED_FINGERPRINT")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let model = std::env::var("VIRTUES_EMBED_MODEL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "default".to_string());
        let is_pinned = pinned.is_some();
        // Prompt prefixes: an explicitly-set env var wins (installer-resolved or
        // power-user), honoring even an empty value ("no prefix"). Unset falls
        // back to the EmbeddingGemma defaults for Dragon, and to no prefix for a
        // pinned (manual) endpoint whose model we couldn't identify.
        let query_prompt = std::env::var("VIRTUES_EMBED_QUERY_PROMPT")
            .ok()
            .unwrap_or_else(|| if is_pinned { String::new() } else { QUERY_PROMPT.to_string() });
        let doc_prompt = std::env::var("VIRTUES_EMBED_DOC_PROMPT")
            .ok()
            .unwrap_or_else(|| if is_pinned { String::new() } else { DOC_PROMPT.to_string() });
        let embedder = Self {
            client,
            base_url,
            fingerprint_pinned: is_pinned,
            model,
            query_prompt,
            doc_prompt,
            // Manual endpoints store native dims (no truncation); Dragon
            // Matryoshka-truncates EmbeddingGemma's 768 to 256.
            stored_dim: configured_embed_dim(),
            truncate: !is_pinned,
        };
        if let Some(expected) = pinned {
            embedder.verify_fingerprint(expected.trim()).await?;
        }
        Ok(embedder)
    }

    /// Boot-time model-identity check for manual inference mode: re-embed
    /// the fixed probe strings and compare the quantized hash of the NATIVE
    /// (pre-truncation) vectors against the fingerprint the installer
    /// recorded at setup. A user swapping the model behind their endpoint
    /// would otherwise silently produce vectors incompatible with every
    /// vector already in the index.
    async fn verify_fingerprint(&self, expected: &str) -> Result<()> {
        let vecs = self
            .request_native(FINGERPRINT_PROBES.iter().map(|s| s.to_string()).collect())
            .await
            .context("embedding the fingerprint probes")?;
        let actual = fingerprint_vectors(&vecs);
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(anyhow!(
                "embedding endpoint is serving a different model than this index was \
                 built with (fingerprint mismatch) — run `virtues configure-inference` \
                 to re-validate, or restore the original endpoint. Continuing would \
                 corrupt search results."
            ));
        }
        Ok(())
    }

    /// Embed a **query** (asymmetric — uses the query prompt). Use this for
    /// the search-time side; everything that embeds stored content uses the
    /// document-prompt variants below.
    pub async fn embed_query_async(self: &Arc<Self>, text: &str) -> Result<Vec<f32>> {
        let mut vecs = self.request(vec![format!("{}{text}", self.query_prompt)]).await?;
        vecs.pop().ok_or_else(|| anyhow!("embedding sidecar returned no embedding"))
    }

    /// Embed a single **document/content** string (document prompt).
    pub async fn embed_async(self: &Arc<Self>, text: &str) -> Result<Vec<f32>> {
        let mut vecs = self.request(vec![format!("{}{text}", self.doc_prompt)]).await?;
        vecs.pop().ok_or_else(|| anyhow!("embedding sidecar returned no embedding"))
    }

    /// Embed a batch of **documents/content** (document prompt).
    pub async fn embed_batch_async(self: &Arc<Self>, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let prompted: Vec<String> =
            texts.into_iter().map(|t| format!("{}{t}", self.doc_prompt)).collect();
        self.request(prompted).await
    }

    /// POST to `/v1/embeddings` and return NATIVE (untruncated, unvalidated)
    /// vectors in input order. The fingerprint check hashes these raw; the
    /// search path goes through `request` below.
    async fn request_native(&self, input: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let n = input.len();
        let resp = self
            .client
            .post(format!("{}/v1/embeddings", self.base_url))
            .json(&serde_json::json!({ "input": input, "model": self.model }))
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
        Ok(rows.into_iter().map(|r| r.embedding).collect())
    }

    async fn request(&self, input: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let rows = self.request_native(input).await?;
        let mut out = Vec::with_capacity(rows.len());
        for mut v in rows {
            if self.truncate {
                // Dragon: EmbeddingGemma is the contract — assert its native
                // dim, then Matryoshka-truncate to the stored width.
                validate_native_dim(&v)?;
                matryoshka_truncate(&mut v, self.stored_dim);
            } else if v.len() != self.stored_dim {
                // Manual: vectors are stored native. A width other than what
                // the index was sized for can't be inserted; fail with a clear
                // message rather than a raw pgvector dimension error. (A model
                // swap that changes dims is already caught by the fingerprint.)
                return Err(anyhow!(
                    "embedding endpoint returned {}-dim vectors but the index is sized \
                     for {} — run `virtues configure-inference` to re-validate the model",
                    v.len(),
                    self.stored_dim
                ));
            }
            out.push(v);
        }
        Ok(out)
    }
}

impl Embedder for LocalEmbedder {
    fn dimension(&self) -> usize {
        self.stored_dim
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
