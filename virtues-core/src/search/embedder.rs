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
//! ## Model — there isn't one. There are three paths.
//!
//! No model is hardcoded anywhere, and none should be: the box must embed with
//! whatever its owner points it at. Which model produced a vector is *recorded*
//! (`search_embeddings.model`, from `model_id()`), never assumed.
//!
//! **Dragon (NPU)** is the same HTTP path: `virtues-qnnd` serves this exact
//! contract (`/v1/embeddings` + `/v1/models`) on :18181, backed by the Hexagon
//! NPU running **gte-small (384-d, native, untruncated)**. The tokenization/
//! packing intelligence lives in that daemon (`crates/virtues-qnnd`), not here
//! — the box no longer has a QNN-specific code path, and Dragon gets the same
//! probe/fingerprint/dim guards as every other endpoint.
//!
//! **Sidecar (default DIY/dev)** is the HTTP path below, and the installer's
//! current GGUF is EmbeddingGemma-300M (QAT Q8_0) — a Gemma-3-lineage
//! bidirectional encoder, mean-pooled, 768-d native, Matryoshka-truncated to 256
//! and renormalized for a 4× lighter HNSW index. Run it on CPU: its activations
//! want bf16/fp32, so fp16 GPU paths force fp32 and end up slower. **This is a
//! default, not a commitment** — swap the GGUF and the only thing that must
//! follow is the stored width.
//!
//! **BYO (manual)** is any OpenAI-compatible endpoint. Stored width is the
//! model's native dims (no truncation — most models aren't Matryoshka-trained),
//! resolved from `VIRTUES_EMBED_DIMS`; the vector column is sized to match at
//! bringup (`database::ensure_embedding_dims`). Prompt prefixes come from
//! `VIRTUES_EMBED_QUERY_PROMPT` / `_DOC_PROMPT`. A fingerprint is pinned at setup
//! and re-checked at boot, so a silently-swapped model cannot corrupt the index.
//!
//! Mixing two models in one index is silent corruption — their vectors live in
//! different geometries and the cosine between them means nothing. Changing model
//! therefore requires a full re-embed (`virtues reindex`), never a shrug.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::OnceCell;

// `SIDECAR_NATIVE_DIM = 768` and `SIDECAR_STORED_DIM = 256` lived here (named
// `DRAGON_*`, which was doubly wrong — Dragon does not even use this path; it
// runs the NPU daemon serving gte-small at 384-d).
//
// A width is a property of a MODEL. Hardcoding one meant the box could only ever
// run the model those numbers described. Both are gone: the width is probed at
// startup and recorded in `search_index_meta`, and truncation is one opt-in env
// var (`VIRTUES_EMBED_DIMS`) rather than a constant baked into the binary.

/// pgvector's HNSW index tops out at 4000 dims for `halfvec`, which is what the
/// vector column is. That covers every embedding model in common use (the widest
/// mainstream models are 3072-d).
pub const MAX_INDEXED_DIM: usize = 4000;
const DEFAULT_URL: &str = "http://127.0.0.1:18181";

/// An explicit width the operator wants vectors stored at, if any.
///
/// Set → **truncate to this** (Matryoshka) and store at this width. Unset → store
/// whatever the model natively emits, which is the only safe default: most models
/// are not Matryoshka-trained, and lopping dimensions off one that isn't destroys
/// it.
///
/// This is the *whole* of the width configuration. There is no
/// `SIDECAR_STORED_DIM`, no per-board constant, no "Dragon does 256". A width is
/// a property of a model, and the model is asked, not assumed.
pub fn requested_embed_dim() -> Option<usize> {
    std::env::var("VIRTUES_EMBED_DIMS")
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .filter(|d| *d > 0)
}

/// The width the index is CURRENTLY built at — read from the database, never
/// from a constant and never from the network.
///
/// Bringup has to size the vector column before anything embeds, and it must do
/// so on a box whose sidecar is not running (`virtues migrate`, most of the CLI).
/// So the geometry lives in `search_index_meta` and the embedder's job at runtime
/// is to *verify* it, not to supply it.
///
/// `None` means the index has never been built and has no geometry yet — which is
/// the truth, and better than asserting a model we never ran.
pub async fn index_dim(pool: &sqlx::PgPool) -> Option<usize> {
    sqlx::query_scalar::<_, Option<i32>>("SELECT dim FROM search_index_meta WHERE singleton")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten()
        .map(|d| d as usize)
}

// EmbeddingGemma's prompt formats were consts HERE, and — worse — the DEFAULT for
// any endpoint without a pinned fingerprint. So a box running a different GGUF
// quietly embedded `"title: none | text: <your email>"` through a model that had
// never seen that string in training. A foreign prompt is not a harmless string;
// it is noise prepended to every vector in the index.
//
// A prompt format is a property of a MODEL, so it lives where models are
// configured (`VIRTUES_EMBED_QUERY_PROMPT` / `_DOC_PROMPT`, written by the
// installer for the model it ships). The default here is NO PREFIX — correct for
// symmetric models, and the only safe assumption about an unknown one.

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

/// Ask an OpenAI-compatible endpoint what it is actually serving.
///
/// Model-agnostic on purpose: the box must work with whatever embedder its owner
/// points it at. `/v1/models` is the one thing llama.cpp, Ollama, TEI, vLLM and
/// OpenAI all answer, and they disagree about the shape of the answer — so try
/// both spellings and give up quietly rather than guess.
///
/// Best-effort by design. A model we cannot name is recorded as unnamed; it is
/// never *assumed*. The hard guards elsewhere (native-dim validation, and the
/// fingerprint for pinned endpoints) are what actually protect the index — this
/// is the label on the jar.
async fn probe_served_model(client: &reqwest::Client, base_url: &str) -> Option<String> {
    let body: serde_json::Value = client
        .get(format!("{base_url}/v1/models"))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;

    let pick = |v: &serde_json::Value| -> Option<String> {
        v.get("id")
            .or_else(|| v.get("model"))
            .or_else(|| v.get("name"))
            .and_then(|s| s.as_str())
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    };

    // OpenAI / vLLM / TEI: {"data":[{"id":…}]}.  llama.cpp / Ollama: {"models":[{"name":…}]}.
    for key in ["data", "models"] {
        if let Some(first) = body.get(key).and_then(|a| a.as_array()).and_then(|a| a.first()) {
            if let Some(name) = pick(first) {
                return Some(name);
            }
        }
    }
    None
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
/// (loopback HTTP). One of the two backends behind [`LocalEmbedder`].
struct HttpEmbedder {
    client: reqwest::Client,
    base_url: String,
    /// The `model` field sent on every /v1/embeddings request. llama.cpp
    /// ignores it; Ollama routes by it and 404s on unknown names. Set from
    /// `VIRTUES_EMBED_MODEL` (written by the installer's manual flow); the
    /// literal `"default"` otherwise. MUST match what the installer's
    /// setup-time probes sent, or the boot fingerprint check would compare
    /// vectors from different requests.
    model: String,
    /// What the endpoint says it is actually serving (probed once, at init). NOT
    /// the same as `model` above, which is a routing key we send and llama.cpp
    /// ignores — this is the GGUF it loaded. Stamped on every indexed row so the
    /// index can say which model's geometry it lives in.
    served_model: String,
    /// Prefixes prepended to queries / documents before embedding — a property of
    /// the MODEL, so there is no default. Empty = none, which is right for
    /// symmetric models and the only safe assumption about an unknown one.
    query_prompt: String,
    doc_prompt: String,
    /// What the model natively emits. **Probed, never assumed.** This used to be
    /// the constant 768 with a hard rejection of anything else — the single line
    /// that made "bring your own model" untrue.
    native_dim: usize,
    /// The width vectors are STORED at. Equals `native_dim` unless the operator
    /// asked for truncation (`VIRTUES_EMBED_DIMS`).
    stored_dim: usize,
}

impl HttpEmbedder {
    async fn new() -> Result<Self> {
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

        // Liveness check so we fail at startup with a clear message instead of
        // every embed call returning a confusing transport error.
        //
        // ADVISORY, not a gate. `/health` is llama-server's readiness route; it
        // is NOT part of the OpenAI shape, and mainstream servers don't
        // implement it. Ollama answers 404 there while serving `/v1/embeddings`
        // flawlessly — so requiring 2xx here meant the installer's own
        // documented Ollama recipe produced a box that installed cleanly and
        // then could not embed a single row. Measured, not assumed: Ollama
        // 0.30.6, `/health` → 404, `/v1/embeddings` → 200 with 768-d vectors.
        //
        // A transport failure IS fatal — nothing is listening, and that is the
        // common case this check exists to name. Any HTTP answer, including a
        // 404, means something is there; the verdict then comes from the real
        // embed probe below, which runs unconditionally either way.
        let health = format!("{base_url}/health");
        let health_status = match client
            .get(&health)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(r) => r.status(),
            Err(e) => {
                return Err(anyhow!(e)).with_context(|| {
                    format!(
                        "embedding endpoint unreachable at {base_url} — \
                         nothing accepted a connection; \
                         check: systemctl status virtues-embed"
                    )
                })
            }
        };
        if !health_status.is_success() {
            tracing::info!(
                "embedding endpoint {health} answered {health_status} — no /health route, \
                 which is fine; verifying with a real embed instead"
            );
        }

        let pinned = std::env::var("VIRTUES_EMBED_FINGERPRINT")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let model = std::env::var("VIRTUES_EMBED_MODEL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "default".to_string());
        // Prefixes: a property of the model, so there is NO default. An unset var
        // means no prefix — correct for symmetric models, and the only safe
        // assumption about a model we have not been told about. Setting the wrong
        // prefix is not a missed optimisation; it is noise prepended to every
        // vector in the index.
        let query_prompt = std::env::var("VIRTUES_EMBED_QUERY_PROMPT").unwrap_or_default();
        let doc_prompt = std::env::var("VIRTUES_EMBED_DOC_PROMPT").unwrap_or_default();

        // What the endpoint is actually SERVING, as opposed to the routing key we
        // send it. Best-effort: if it will not say, record that it would not say.
        let served_model = probe_served_model(&client, &base_url)
            .await
            .unwrap_or_else(|| "unreported".to_string());

        // ASK THE MODEL. Embed one probe string and count what comes back.
        //
        // This replaces `validate_native_dim`, which rejected anything that was not
        // 768-d and told the user to "check the sidecar's GGUF is
        // EmbeddingGemma-300M". That one line is what made BYO a claim rather than
        // a fact: you could not change model without editing Rust.
        let mut probe = Self::embed_raw(&client, &base_url, &model, vec!["dim probe".into()])
            .await
            .with_context(|| {
                if health_status.is_success() {
                    format!(
                        "embedding endpoint {base_url} accepted /health but would not embed \
                         — cannot determine its vector width"
                    )
                } else {
                    format!(
                        "embedding endpoint {base_url} answered {health_status} on /health \
                         and would not embed either — cannot determine its vector width"
                    )
                }
            })?;
        let native_dim = probe.pop().map(|v| v.len()).filter(|d| *d > 0).ok_or_else(|| {
            anyhow!("embedding endpoint returned no vector for the width probe")
        })?;

        // Truncation is opt-in and must be honest: asking for a width WIDER than
        // the model emits is a configuration error, not something to paper over
        // with zero-padding.
        let stored_dim = match requested_embed_dim() {
            Some(d) if d > native_dim => {
                return Err(anyhow!(
                    "VIRTUES_EMBED_DIMS={d} but {served_model} emits only {native_dim} \
                     dimensions — a vector cannot be widened, only truncated"
                ))
            }
            Some(d) => d,
            None => native_dim,
        };

        if stored_dim > MAX_INDEXED_DIM {
            return Err(anyhow!(
                "{served_model} emits {native_dim}-d vectors, above the {MAX_INDEXED_DIM} \
                 ceiling pgvector's HNSW index supports — set VIRTUES_EMBED_DIMS to \
                 truncate (only safe if the model is Matryoshka-trained)"
            ));
        }

        tracing::info!(
            model = %served_model,
            native = native_dim,
            stored = stored_dim,
            truncating = stored_dim < native_dim,
            "embedding endpoint identified"
        );

        let embedder = Self {
            client,
            base_url,
            model,
            served_model,
            query_prompt,
            doc_prompt,
            native_dim,
            stored_dim,
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

    /// Embed a batch of **queries** (query prompt) in one sidecar call — the
    /// batched sibling of `embed_query_async`, for multi-query fan-out. Returns
    /// one vector per input, in input order.
    pub async fn embed_query_batch(self: &Arc<Self>, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let prompted: Vec<String> =
            texts.iter().map(|t| format!("{}{t}", self.query_prompt)).collect();
        self.request(prompted).await
    }

    /// The model this endpoint is serving — stamped on every indexed row.
    pub fn model_id(&self) -> String {
        self.served_model.clone()
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
            // The width is checked against what THIS endpoint proved it emits at
            // startup, not against a constant. An endpoint that changes width
            // mid-run has been swapped underneath us, and its vectors do not
            // belong in this index.
            if v.len() != self.native_dim {
                return Err(anyhow!(
                    "embedding endpoint returned {}-d vectors but emitted {}-d at startup \
                     — the model behind {} changed. Its vectors live in a different \
                     geometry than everything already indexed; run `virtues reindex`.",
                    v.len(),
                    self.native_dim,
                    self.base_url
                ));
            }
            // Truncate only when asked to. Matryoshka is a property of the model,
            // not of the box, and lopping dimensions off a model that was not
            // trained for it destroys the vector.
            if self.stored_dim < self.native_dim {
                matryoshka_truncate(&mut v, self.stored_dim);
            }
            out.push(v);
        }
        Ok(out)
    }

    /// Embed without a constructed `HttpEmbedder` — the width probe has to run
    /// before the struct exists, since the struct's width is what it discovers.
    async fn embed_raw(
        client: &reqwest::Client,
        base_url: &str,
        model: &str,
        input: Vec<String>,
    ) -> Result<Vec<Vec<f32>>> {
        let resp = client
            .post(format!("{base_url}/v1/embeddings"))
            .json(&serde_json::json!({ "input": input, "model": model }))
            .send()
            .await
            .map_err(|e| anyhow!("embed request failed: {e}"))?
            .error_for_status()
            .map_err(|e| anyhow!("embed request failed: {e}"))?;

        let body: EmbeddingsResponse =
            resp.json().await.context("parsing /v1/embeddings response")?;
        let mut rows = body.data;
        rows.sort_by_key(|r| r.index);
        Ok(rows.into_iter().map(|r| r.embedding).collect())
    }
}

/// The embedder callers use — a thin wrapper preserving the public surface
/// from when this dispatched between an HTTP backend and a native QNN client.
/// There is now exactly ONE inference path: the HTTP contract. On Dragon the
/// endpoint behind `VIRTUES_EMBED_URL` is `virtues-qnnd` (gte-small on the
/// NPU); everywhere else it's llama-server or a BYO endpoint. The box can't
/// tell the difference, which is the point.
pub struct LocalEmbedder {
    inner: Arc<HttpEmbedder>,
    /// The stored vector width (= the vector column's dims), resolved from the
    /// endpoint/config at startup.
    stored_dim: usize,
}

impl LocalEmbedder {
    pub async fn new() -> Result<Self> {
        let http = HttpEmbedder::new().await?;
        let stored_dim = http.stored_dim;
        Ok(Self { inner: Arc::new(http), stored_dim })
    }

    /// Embed a search **query** (asymmetric — applies the query prompt; empty
    /// for symmetric models like gte).
    pub async fn embed_query_async(&self, text: &str) -> Result<Vec<f32>> {
        self.inner.embed_query_async(text).await
    }

    /// Embed a batch of search **queries** in one call (multi-query fan-out).
    pub async fn embed_query_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.inner.embed_query_batch(texts).await
    }

    /// Embed a single stored **document/content** string.
    pub async fn embed_async(&self, text: &str) -> Result<Vec<f32>> {
        self.inner.embed_async(text).await
    }

    /// Embed a batch of **documents/content**.
    pub async fn embed_batch_async(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        self.inner.embed_batch_async(texts).await
    }

    pub fn dimension(&self) -> usize {
        self.stored_dim
    }

    /// Which model actually produced these vectors — stamped onto every row it
    /// writes (`search_embeddings.model`).
    ///
    /// That column used to hold the literal `'embeddinggemma'`, written by the
    /// indexer and read by nobody. The only real guard against a model swap is
    /// dimensional, so a BYO embedder of the SAME width could be swapped in
    /// silently: its vectors land in a different geometry from their neighbours,
    /// cosine between them means nothing, and the index degrades with no error
    /// anywhere. Recording the truth is what makes that detectable — and it is
    /// what lets a user bring their own model at all. (On Dragon, `virtues-qnnd`
    /// answers `/v1/models` with `gte-small` — the same stamp the old native
    /// path wrote, so an existing index stays valid across the consolidation.)
    pub fn model_id(&self) -> String {
        self.inner.model_id()
    }

    fn backend_label(&self) -> &'static str {
        "http-contract"
    }
}

impl Embedder for LocalEmbedder {
    fn dimension(&self) -> usize {
        self.stored_dim
    }
}

// `validate_native_dim` lived here. It rejected any vector that wasn't 768-d,
// with the message "check the sidecar's GGUF is EmbeddingGemma-300M" — a single
// function that made "bring your own model" false. The width is now PROBED at
// startup and checked against itself; see `HttpEmbedder::new`.

pub(crate) fn resolve_base_url() -> String {
    std::env::var("VIRTUES_EMBED_URL")
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|_| DEFAULT_URL.to_string())
}

/// Probe the currently-configured embedding endpoint WITHOUT the boot-time
/// fingerprint guard. `configure-inference` needs to inspect an endpoint whose
/// model may have changed — the exact case `LocalEmbedder::new` refuses — so it
/// can't go through the normal constructor. Returns the freshly-computed
/// fingerprint and the endpoint's native dims.
pub async fn probe_current_endpoint() -> Result<(String, usize)> {
    crate::http_client::ensure_crypto_provider();
    let base_url = resolve_base_url();
    let model = std::env::var("VIRTUES_EMBED_MODEL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "default".to_string());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let resp = client
        .post(format!("{base_url}/v1/embeddings"))
        .json(&serde_json::json!({ "input": FINGERPRINT_PROBES, "model": model }))
        .send()
        .await
        .with_context(|| format!("probing embedding endpoint {base_url}"))?
        .error_for_status()
        .with_context(|| format!("probing embedding endpoint {base_url}"))?;
    let body: EmbeddingsResponse =
        resp.json().await.context("parsing /v1/embeddings response")?;
    if body.data.len() != FINGERPRINT_PROBES.len() {
        return Err(anyhow!(
            "endpoint returned {} vectors for {} probe strings",
            body.data.len(),
            FINGERPRINT_PROBES.len()
        ));
    }
    let mut rows = body.data;
    rows.sort_by_key(|r| r.index);
    let vecs: Vec<Vec<f32>> = rows.into_iter().map(|r| r.embedding).collect();
    let dims = vecs.first().map(|v| v.len()).unwrap_or(0);
    if dims == 0 {
        return Err(anyhow!("endpoint returned empty vectors"));
    }
    Ok((fingerprint_vectors(&vecs), dims))
}

static EMBEDDER: OnceCell<Arc<LocalEmbedder>> = OnceCell::const_new();

pub async fn get_embedder() -> Result<Arc<LocalEmbedder>> {
    let embedder = EMBEDDER
        .get_or_try_init(|| async {
            tracing::info!("Initializing embedding sidecar client...");
            let start = std::time::Instant::now();
            let embedder = LocalEmbedder::new().await?;
            tracing::info!(
                "Embedder ready in {:.1}s (backend={}, dim={})",
                start.elapsed().as_secs_f64(),
                embedder.backend_label(),
                embedder.dimension()
            );
            Ok::<_, anyhow::Error>(Arc::new(embedder))
        })
        .await?;
    Ok(embedder.clone())
}

