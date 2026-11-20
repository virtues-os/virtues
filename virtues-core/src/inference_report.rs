//! Inference resolution report.
//!
//! On the recommended setup, local ML is one llama-server embedding sidecar the
//! installer ships and pins, on :18181 (see `search/embedder.rs`,
//! `cli/model_set.rs`). `virtues doctor`, `upgrade`, `deploy` and box status
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

impl ResolutionReport {
    /// GGUF file names of models not present on disk (`ModelSource::Download`).
    /// One place to answer "which models are missing" so the several callers
    /// (`doctor`, `upgrade`, `deploy`, box status) don't each open-code the
    /// `ModelSource::Download` match and drift apart.
    pub fn missing(&self) -> Vec<&str> {
        self.models
            .iter()
            .filter(|m| matches!(m.source, ModelSource::Download))
            .map(|m| m.gguf_file)
            .collect()
    }
}

/// The GGUF the installer provisions: EmbeddingGemma 2 (text tower), Q8_0 —
/// mean pooling, 768-dim stored at native width (256 kept only half its recall
/// gain on the personal-data eval). Q8_0 matches the fp32 reference at cosine
/// 0.9998. Its activations require bf16/fp32, not fp16 — run it on CPU; see
/// embedder.rs.
pub const EMBED_GGUF: &str = "embeddinggemma-2-Q8_0.gguf";

/// The recommended embedder's checksum, pinned in the binary so `virtues
/// upgrade` (`cli::model_set`) verifies the download against what this release
/// was built with, not against a sidecar file on the same host.
pub const EMBED_GGUF_SHA256: &str =
    "2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135";

/// The recommended embedder's prompt prefixes. Facts about the model, written
/// into the box env alongside it when an upgrade moves a box to it; never a
/// fallback for another endpoint. Must match the installer's GEMMA_*_PROMPT.
pub const EMBED_QUERY_PROMPT: &str = "task: search result | query: ";
pub const EMBED_DOC_PROMPT: &str = "title: none | text: ";

/// The Dragon NPU path's QAIRT context binaries (Hexagon v68): gte-small embed
/// and answerai-colbert@256 rerank, served by `virtues-qnnd` behind the same
/// llama-compatible HTTP contract on :18181/:18182, on a Dragon that hasn't
/// moved its search to the CPU. Named here only to report what such a box
/// runs; nothing fetches them any more.
pub const QNN_EMBED_BIN: &str = "gte_v68_vtcm2.bin";
pub const QNN_RERANK_BIN: &str = "cb256_v68_vtcm2.bin";

fn models_dir() -> PathBuf {
    std::env::var("VIRTUES_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/var/lib/virtues/models"))
}

/// Is this box's search still on the Dragon NPU profile? virtues-qnnd serves
/// the same HTTP contract as llama-server, so this decides which model
/// artifacts reports list and which unit a remedy names, never how search
/// runs. False once a Dragon's search moves to its CPU (`cli::model_set`);
/// for the board itself, see [`is_dragon_board`]. Keyed off the installer's
/// `VIRTUES_INFERENCE=dragon`, with the qnnd unit file as a fallback for `sudo`
/// CLI contexts that don't load the box env. A Dragon on its owner's own
/// server (`manual`) keeps the qnnd unit file but doesn't use it.
pub(crate) fn is_dragon_profile() -> bool {
    match std::env::var("VIRTUES_INFERENCE").as_deref() {
        Ok("dragon") => true,
        Ok("manual") => false,
        _ => std::path::Path::new("/etc/systemd/system/virtues-qnnd.service").exists(),
    }
}

/// Is this machine a Radxa Dragon Q6A, by its device tree? A question about the
/// board, not the search setup: a Dragon keeps being one after its search moves
/// from the NPU to the CPU. Its embedding sidecar runs gently there
/// (`cli::model_set::embed_unit_on`), because it has no fan; local mode is
/// offered on it (`local_model::supported`); and its hardware watchdog is armed
/// (`cli::system_repairs`). Only the Q6A: other QCM6490 boards list
/// `qcom,qcm6490` too, and none of that was measured or wanted on them. Must
/// match the installer's `dragon_pacing`.
pub(crate) fn is_dragon_board() -> bool {
    std::fs::read("/proc/device-tree/compatible")
        .map(|c| c.split(|b| *b == 0).any(|s| s == b"radxa,dragon-q6a"))
        .unwrap_or(false)
}

/// The unit that serves search's embedding endpoint at `base_url`, for a
/// remedy to name: the next model's sidecar while a change is under way, the
/// NPU daemon on a Dragon still on it, and the CPU sidecar otherwise.
pub(crate) fn serving_unit(base_url: &str) -> &'static str {
    if base_url.trim_end_matches('/').ends_with(":18183") {
        "virtues-embed-next"
    } else if is_dragon_profile() {
        "virtues-qnnd"
    } else {
        "virtues-embed"
    }
}

fn qnn_models_dir() -> PathBuf {
    std::env::var("VIRTUES_QNND_MODELS_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| models_dir().join("qnn"))
}

/// Report llama-server-backed inference resolution.
///
/// We deliberately don't reach across the network to the sidecars here —
/// this function is called from sync CLI surfaces and synchronous status
/// endpoints. The "is the sidecar actually up?" checks live in the embedder's
/// startup path and in `virtues warm-models`.
pub fn resolution_report() -> ResolutionReport {
    // Dragon NPU profile: report the QNN daemon + its context binaries, not the
    // HTTP-sidecar GGUFs. Checked first so a Dragon box never mis-reports
    // EmbeddingGemma as "missing" (it isn't used here).
    if is_dragon_profile() {
        let dir = qnn_models_dir();
        let source_for = |f: &str| {
            let p = dir.join(f);
            if p.is_file() {
                ModelSource::Baked(p)
            } else {
                ModelSource::Download
            }
        };
        return ResolutionReport {
            accelerator: "qnn-npu (Qualcomm Hexagon v68)".to_string(),
            precision: "int8 w8a16 (QNN HTP)".to_string(),
            models_dir: Some(dir.clone()),
            models: vec![
                ModelEntry {
                    name: "embed",
                    repo: "gte-small · Hexagon NPU :18181",
                    gguf_file: QNN_EMBED_BIN,
                    source: source_for(QNN_EMBED_BIN),
                },
                // Search doesn't rerank, but the daemon still loads this
                // binary at start, so it must be on disk.
                ModelEntry {
                    name: "rerank",
                    repo: "answerai-colbert@256 · Hexagon NPU (unused by search)",
                    gguf_file: QNN_RERANK_BIN,
                    source: source_for(QNN_RERANK_BIN),
                },
            ],
        };
    }

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
    let models = vec![ModelEntry {
        name: "embed",
        repo: "embeddinggemma-2 @ :18181",
        gguf_file: EMBED_GGUF,
        source: source_for(EMBED_GGUF),
    }];
    ResolutionReport {
        // The qualifier belongs to this path only — CUDA vs CPU is decided by
        // which llama-server binary CI built. The QNN path's accelerator string
        // is self-describing, so doctor prints `accelerator` verbatim.
        accelerator: "llama-server (GPU or CPU per sidecar build)".to_string(),
        precision: "Q8_0".to_string(),
        models_dir: Some(dir),
        models,
    }
}
