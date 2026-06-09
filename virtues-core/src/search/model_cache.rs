//! Compatibility shim for the old ORT model-cache module.
//!
//! v0.1.0 routes all ML through Ollama, so there's no on-box ONNX cache
//! to manage anymore. But three callers (`virtues doctor`, `setup`'s
//! status banner, and the web `/api/box/status` route) still consume
//! the old report shape. This module preserves that shape with values
//! that describe the Ollama-backed reality so the CLI/web UI keep working
//! without each caller having to rewrite its output.
//!
//! When the dust settles in v0.1.1 we can rename this to `inference_report`
//! and drop the `search/` location.

use std::path::PathBuf;

/// What backs a model on the box.
#[derive(Debug, Clone)]
pub enum ModelSource {
    /// Reserved for a future "GGUF file shipped in the install tarball"
    /// option. Unused in v0.1.0.
    Baked(PathBuf),
    /// Ollama will pull the model on first use (or the operator pulled it
    /// already via `ollama pull`).
    Download,
}

/// One model entry in the resolution report.
#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub name: &'static str,
    pub repo: &'static str,
    pub onnx_file: &'static str,
    pub source: ModelSource,
}

/// The shape the CLI + web UI consume. Field semantics are preserved from
/// the ORT era; values are reinterpreted for the Ollama backend.
#[derive(Debug, Clone)]
pub struct ResolutionReport {
    pub accelerator: String,
    pub precision: String,
    pub cuda_compiled: bool,
    pub models_dir: Option<PathBuf>,
    pub models: Vec<ModelEntry>,
}

/// Report Ollama-backed inference resolution.
///
/// We deliberately don't reach across the network to query Ollama here —
/// this function is called from sync CLI surfaces and synchronous status
/// endpoints. The "is Ollama actually up?" check lives in the embedder's
/// startup path and in `virtues doctor`'s extended runtime checks.
pub fn resolution_report() -> ResolutionReport {
    let embed_model = std::env::var("VIRTUES_EMBED_MODEL")
        .unwrap_or_else(|_| "bge-m3".to_string());

    // We can't introspect the Ollama daemon's GPU vs CPU choice from
    // outside the daemon — it picks per-call. Surface "ollama" as the
    // accelerator label; the daemon's logs reveal the real backend.
    ResolutionReport {
        accelerator: "ollama".to_string(),
        precision: "managed-by-ollama".to_string(),
        cuda_compiled: false,
        models_dir: None,
        models: vec![ModelEntry {
            name: "embed",
            repo: Box::leak(embed_model.into_boxed_str()),
            onnx_file: "gguf (via Ollama)",
            source: ModelSource::Download,
        }],
    }
}
