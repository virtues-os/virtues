//! Embedding trait + Ollama-backed implementation.
//!
//! v0.1.0 routes all embedding through a local Ollama daemon
//! (`http://localhost:11434` by default, `OLLAMA_HOST` to override). The
//! previous build linked ONNX Runtime in-process, but ORT 1.24's prebuilt
//! manylinux blobs reference glibc 2.38+ symbols (`__isoc23_strtoll`),
//! which doesn't ship on JetPack 6.x (glibc 2.35) — and there's no JetPack
//! upgrade path that fixes it without waiting for JetPack 7. Ollama solves
//! all of this: it runs as a separate daemon, handles GPU/CPU detection,
//! manages model pulls, and is the de facto standard for self-hosted ML
//! (Open WebUI, Continue.dev, AnythingLLM, etc.). install.sh ensures it's
//! present + the embedding model is pulled.
//!
//! ## Model
//!
//! Default: `nomic-embed-text` (768-dim, fast, multilingual). Override via
//! `VIRTUES_EMBED_MODEL`. Whatever model you pick must produce 768-dim
//! vectors — the `wiki_embeddings.embedding` column is `vector(768)`.

use anyhow::{anyhow, Context, Result};
use ollama_rs::generation::embeddings::request::GenerateEmbeddingsRequest;
use ollama_rs::Ollama;
use std::sync::Arc;
use tokio::sync::OnceCell;

const EMBED_DIM: usize = 768;
const DEFAULT_MODEL: &str = "nomic-embed-text";
const DEFAULT_HOST: &str = "http://localhost";
const DEFAULT_PORT: u16 = 11434;

pub trait Embedder: Send + Sync {
    fn dimension(&self) -> usize;
}

/// Ollama HTTP-backed embedder. The daemon owns the model, GPU, threading.
/// Per-call latency is dominated by network roundtrip + inference; both
/// happen off the main async runtime via the ollama-rs client.
pub struct LocalEmbedder {
    client: Ollama,
    model: String,
}

impl LocalEmbedder {
    pub async fn new() -> Result<Self> {
        let (host, port) = resolve_endpoint();
        let model =
            std::env::var("VIRTUES_EMBED_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
        let client = Ollama::new(host, port);

        // Liveness check so we fail at startup with a clear message instead
        // of every embed call returning a confusing transport error.
        client.list_local_models().await.context(
            "Ollama daemon unreachable. Install: `curl https://ollama.com/install.sh | sh`",
        )?;

        Ok(Self { client, model })
    }

    pub async fn embed_async(self: &Arc<Self>, text: &str) -> Result<Vec<f32>> {
        let req = GenerateEmbeddingsRequest::new(self.model.clone(), text.into());
        let resp = self
            .client
            .generate_embeddings(req)
            .await
            .map_err(|e| anyhow!("Ollama embed failed: {e}"))?;
        let mut emb = resp.embeddings;
        let vec = emb.pop().ok_or_else(|| anyhow!("Ollama returned no embedding"))?;
        validate_dim(&vec)?;
        Ok(vec)
    }

    pub async fn embed_batch_async(self: &Arc<Self>, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let req = GenerateEmbeddingsRequest::new(self.model.clone(), texts.into());
        let resp = self
            .client
            .generate_embeddings(req)
            .await
            .map_err(|e| anyhow!("Ollama batch embed failed: {e}"))?;
        for v in &resp.embeddings {
            validate_dim(v)?;
        }
        Ok(resp.embeddings)
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
            "embedding dim {} != expected {EMBED_DIM} — check VIRTUES_EMBED_MODEL is a 768-dim model",
            v.len()
        ));
    }
    Ok(())
}

fn resolve_endpoint() -> (String, u16) {
    // OLLAMA_HOST can be `http://host:port`, `host:port`, or just `host`.
    if let Ok(raw) = std::env::var("OLLAMA_HOST") {
        if let Some(rest) = raw.strip_prefix("http://").or_else(|| raw.strip_prefix("https://")) {
            let (host, port) = split_host_port(rest);
            return (format!("http://{host}"), port);
        }
        let (host, port) = split_host_port(&raw);
        return (format!("http://{host}"), port);
    }
    (DEFAULT_HOST.to_string(), DEFAULT_PORT)
}

fn split_host_port(s: &str) -> (&str, u16) {
    match s.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().unwrap_or(DEFAULT_PORT)),
        None => (s, DEFAULT_PORT),
    }
}

static EMBEDDER: OnceCell<Arc<LocalEmbedder>> = OnceCell::const_new();

pub async fn get_embedder() -> Result<Arc<LocalEmbedder>> {
    let embedder = EMBEDDER
        .get_or_try_init(|| async {
            tracing::info!("Initializing Ollama embedder client...");
            let start = std::time::Instant::now();
            let embedder = LocalEmbedder::new().await?;
            tracing::info!(
                "Ollama embedder ready in {:.1}s (model={}, dim={})",
                start.elapsed().as_secs_f64(),
                embedder.model,
                embedder.dimension()
            );
            Ok::<_, anyhow::Error>(Arc::new(embedder))
        })
        .await?;
    Ok(embedder.clone())
}
