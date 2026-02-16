//! Embedding trait and local implementation via fastembed
//!
//! The `Embedder` trait abstracts over embedding backends so we can
//! swap from local (fastembed/ONNX) to API-based later if needed.

use anyhow::Result;
use std::sync::{Arc, Mutex};
use tokio::sync::OnceCell;

/// Trait for text embedding (synchronous — CPU-bound work)
pub trait Embedder: Send + Sync {
    /// Embed a single text string, returning a 768-dim f32 vector.
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed multiple texts in a batch.
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// Embedding dimension (768 for nomic-embed-text-v1.5).
    fn dimension(&self) -> usize;
}

/// Local embedder using fastembed (nomic-embed-text-v1.5, ONNX Runtime).
///
/// Loaded lazily on first use (~2-5s model load). Stays in memory (~100-150MB)
/// for the server's lifetime. Subsequent calls are fast (~15ms/embed).
///
/// Uses interior mutability (Mutex) because fastembed's `embed()` requires `&mut self`.
/// All embedding calls should go through `embed_async()` to avoid blocking the
/// tokio async runtime — CPU-bound work runs on the blocking thread pool.
pub struct LocalEmbedder {
    model: Mutex<fastembed::TextEmbedding>,
}

impl LocalEmbedder {
    /// Create a new LocalEmbedder with nomic-embed-text-v1.5.
    ///
    /// This downloads the model on first use (~50MB) and loads it into memory.
    pub fn new() -> Result<Self> {
        let model = fastembed::TextEmbedding::try_new(
            fastembed::InitOptions::new(fastembed::EmbeddingModel::NomicEmbedTextV15)
                .with_show_download_progress(true),
        )?;
        Ok(Self { model: Mutex::new(model) })
    }

    /// Embed text on the blocking thread pool (async-safe).
    ///
    /// Moves the CPU-bound ONNX inference to `spawn_blocking` so it
    /// doesn't block tokio worker threads.
    pub async fn embed_async(self: &Arc<Self>, text: &str) -> Result<Vec<f32>> {
        let this = self.clone();
        let text = text.to_string();
        tokio::task::spawn_blocking(move || this.embed(&text))
            .await
            .map_err(|e| anyhow::anyhow!("Embedding task panicked: {}", e))?
    }

    /// Embed multiple texts in a single batch on the blocking thread pool (async-safe).
    ///
    /// More efficient than calling `embed_async` in a loop — single ONNX call
    /// for all texts (e.g. 8-16 events at once).
    pub async fn embed_batch_async(self: &Arc<Self>, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
            this.embed_batch(&refs)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Batch embedding task panicked: {}", e))?
    }
}

impl Embedder for LocalEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut model = self.model.lock().map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        let results = model.embed(vec![text], None)?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No embedding returned"))
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let texts: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
        let mut model = self.model.lock().map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        let results = model.embed(texts, None)?;
        Ok(results)
    }

    fn dimension(&self) -> usize {
        768
    }
}

/// Global lazy-initialized embedder instance.
///
/// First access triggers model load (~2-5s). All subsequent accesses are instant.
static EMBEDDER: OnceCell<Arc<LocalEmbedder>> = OnceCell::const_new();

/// Get the shared embedder instance (lazy init on first call).
///
/// Model loading (~2-5s) runs on the blocking thread pool to avoid
/// stalling tokio worker threads.
pub async fn get_embedder() -> Result<Arc<LocalEmbedder>> {
    let embedder = EMBEDDER
        .get_or_try_init(|| async {
            tracing::info!("Loading local embedding model (nomic-embed-text-v1.5)...");
            let start = std::time::Instant::now();
            let embedder = tokio::task::spawn_blocking(LocalEmbedder::new)
                .await
                .map_err(|e| anyhow::anyhow!("Embedder init panicked: {}", e))??;
            tracing::info!(
                "Embedding model loaded in {:.1}s (dim={})",
                start.elapsed().as_secs_f64(),
                embedder.dimension()
            );
            Ok::<_, anyhow::Error>(Arc::new(embedder))
        })
        .await?;
    Ok(embedder.clone())
}
