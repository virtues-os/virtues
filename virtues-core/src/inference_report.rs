//! Inference resolution report.
//!
//! v0.1.1 routes all local ML through two llama-server sidecars that the
//! installer ships and pins (embedding on :18181, rerank on :18182 — see
//! `search/embedder.rs` / `search/reranker.rs`). Three callers (`virtues
//! doctor`, `setup`'s status banner, and the web `/api/box/status` route)
//! consume this report shape.

use std::path::PathBuf;

/// What backs a model on the box.
#[derive(Debug, Clone)]
pub enum ModelSource {
    /// GGUF present on disk at the given path (the installer downloaded it
    /// from the pinned models release and verified its SHA).
    Baked(PathBuf),
    /// GGUF not found in the models dir — the sidecar can't be running this
    /// model. `virtues doctor` surfaces this; re-run the installer to fetch.
    Download,
}

/// One model entry in the resolution report.
#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub name: &'static str,
    pub repo: &'static str,
    pub gguf_file: &'static str,
    pub source: ModelSource,
}

/// The shape the CLI + web UI consume.
#[derive(Debug, Clone)]
pub struct ResolutionReport {
    pub accelerator: String,
    pub precision: String,
    pub models_dir: Option<PathBuf>,
    pub models: Vec<ModelEntry>,
}

/// The GGUFs the installer provisions. Embed stays F16 to match the
/// precision the v0.1.0 Ollama daemon served (existing `search_vectors`
/// rows were embedded with F16 weights); the reranker has no stored state,
/// so it takes the 2× smaller/faster Q8_0.
pub const EMBED_GGUF: &str = "bge-m3-FP16.gguf";
pub const RERANK_GGUF: &str = "bge-reranker-v2-m3-Q8_0.gguf";

fn models_dir() -> PathBuf {
    std::env::var("VIRTUES_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/var/lib/virtues/models"))
}

/// Report llama-server-backed inference resolution.
///
/// We deliberately don't reach across the network to the sidecars here —
/// this function is called from sync CLI surfaces and synchronous status
/// endpoints. The "are the sidecars actually up?" checks live in the
/// embedder/reranker startup paths and in `virtues warm-models`.
pub fn resolution_report() -> ResolutionReport {
    let dir = models_dir();
    let source_for = |gguf: &str| {
        let p = dir.join(gguf);
        if p.is_file() {
            ModelSource::Baked(p)
        } else {
            ModelSource::Download
        }
    };

    // Whether the sidecar runs CUDA or CPU is decided by which llama-server
    // binary CI built for this arch (the appliance-vs-DIY seam) — outside
    // this process. The sidecar's own logs are the source of truth.
    let models = vec![
        ModelEntry {
            name: "embed",
            repo: "bge-m3 @ :18181",
            gguf_file: EMBED_GGUF,
            source: source_for(EMBED_GGUF),
        },
        ModelEntry {
            name: "rerank",
            repo: "bge-reranker-v2-m3 @ :18182",
            gguf_file: RERANK_GGUF,
            source: source_for(RERANK_GGUF),
        },
    ];
    ResolutionReport {
        accelerator: "llama-server".to_string(),
        precision: "F16 embed / Q8_0 rerank".to_string(),
        models_dir: Some(dir),
        models,
    }
}
