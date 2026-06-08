//! Model resolution and (optional) first-boot download.
//!
//! On the appliance the models are baked into the image under
//! `$VIRTUES_MODELS_DIR` (default `/opt/virtues/models`). When that directory
//! exists and contains the expected files, we skip HuggingFace entirely — no
//! network needed at first boot.
//!
//! For local dev / DIY self-host (`cargo run`, or a fresh CPU box) the dir
//! typically isn't present, so we fall back to downloading from HF via
//! `hf-hub` into the platform cache.
//!
//! ## Precision is accelerator-driven
//!
//! The ONNX graph is portable across execution providers, but the optimal
//! *quantization* is not: int8 dynamic-quant is the CPU/CoreML floor, fp16 is
//! preferred on a GPU. [`accelerator::detect`](super::accelerator) picks the
//! precision; we resolve a precision-appropriate ONNX variant and fall back to
//! the int8 floor when the fp16 export isn't present in the repo or baked dir.
//!
//! ## Models
//!
//! - Embedder: `google/embeddinggemma-300m` (community ONNX export at
//!   `onnx-community/embeddinggemma-300m-ONNX`), 768-dim. EmbeddingGemma uses
//!   Matryoshka representation; the exports keep the full 768-dim head we pin
//!   in the pgvector schema. The export ships external weights (`.onnx_data`).
//! - Reranker: `jinaai/jina-reranker-v2-base-multilingual`.

use anyhow::{Context, Result};
use hf_hub::api::tokio::ApiBuilder;
use hf_hub::{Repo, RepoType};
use std::path::{Path, PathBuf};

use super::accelerator::{self, Precision};

/// Embedder model identity. Pin updated when bench validates a new revision.
pub const EMBEDDER_REPO: &str = "onnx-community/embeddinggemma-300m-ONNX";
pub const EMBEDDER_REVISION: Option<&str> = None;
pub const EMBEDDER_TOKENIZER_FILE: &str = "tokenizer.json";
/// `model_quantized.onnx` is onnx-community's int8 dynamic-quant export (the
/// universal floor); `model_fp16.onnx` is preferred on GPU. Sibling exports in
/// the repo: model.onnx (fp32) | model_fp16.onnx | model_q4.onnx | model_q4f16.onnx.
pub const EMBEDDER_ONNX_INT8: &str = "onnx/model_quantized.onnx";
pub const EMBEDDER_ONNX_FP16: &str = "onnx/model_fp16.onnx";

/// Reranker model identity (jina v2 multilingual).
pub const RERANKER_REPO: &str = "jinaai/jina-reranker-v2-base-multilingual";
pub const RERANKER_REVISION: Option<&str> = None;
pub const RERANKER_TOKENIZER_FILE: &str = "tokenizer.json";
pub const RERANKER_ONNX_INT8: &str = "onnx/model_int8.onnx";
pub const RERANKER_ONNX_FP16: &str = "onnx/model_fp16.onnx";

/// ONNX variants to try, in priority order, for the embedder at a given precision.
fn embedder_onnx_candidates(p: Precision) -> &'static [&'static str] {
    match p {
        Precision::Fp16 => &[EMBEDDER_ONNX_FP16, EMBEDDER_ONNX_INT8],
        Precision::Int8 => &[EMBEDDER_ONNX_INT8],
    }
}

/// ONNX variants to try, in priority order, for the reranker at a given precision.
fn reranker_onnx_candidates(p: Precision) -> &'static [&'static str] {
    match p {
        Precision::Fp16 => &[RERANKER_ONNX_FP16, RERANKER_ONNX_INT8],
        Precision::Int8 => &[RERANKER_ONNX_INT8],
    }
}

/// Local paths for one model.
pub struct ModelPaths {
    pub onnx: PathBuf,
    pub tokenizer: PathBuf,
}

/// Bake-in directory used by appliance images. Override with `$VIRTUES_MODELS_DIR`.
const BAKED_MODELS_DIR_DEFAULT: &str = "/opt/virtues/models";

/// Resolve the bake-in directory. If it exists, we use it; otherwise return None
/// and the caller falls back to HF download.
fn baked_models_dir() -> Option<PathBuf> {
    let p = std::env::var("VIRTUES_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(BAKED_MODELS_DIR_DEFAULT));
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

/// Where the platform cache is, used for HF downloads in dev.
fn hf_cache_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "virtues", "virtues")
        .context("could not resolve platform cache directory")?;
    let dir = dirs.cache_dir().join("models");
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    Ok(dir)
}

/// Find a single file under the baked dir, accepting either a flat layout
/// (`<dir>/<repo_basename>/<file>`) or the full-repo layout (`<dir>/<repo>/<file>`).
fn baked_file(base: &Path, repo: &str, file: &str) -> Option<PathBuf> {
    let repo_basename = repo.rsplit('/').next().unwrap_or(repo);
    let candidates = [
        base.join(repo_basename).join(file),
        base.join(repo).join(file),
    ];
    candidates.into_iter().find(|c| c.exists())
}

/// External-weights sibling for an ONNX file: ORT discovers `<name>.onnx_data`
/// relative to the `.onnx` at load time, so it must sit beside it. Returns the
/// repo-relative sibling path (e.g. `onnx/model_quantized.onnx_data`).
fn external_data_name(onnx_file: &str) -> String {
    format!("{onnx_file}_data")
}

/// Fetch files from HF Hub. Used as the fallback when the baked dir isn't present.
async fn fetch_repo(repo_id: &str, revision: Option<&str>, files: &[&str]) -> Result<Vec<PathBuf>> {
    let api = ApiBuilder::new()
        .with_cache_dir(hf_cache_dir()?)
        .with_progress(true)
        .build()
        .context("build hf-hub api")?;
    let repo = match revision {
        Some(rev) => Repo::with_revision(repo_id.to_string(), RepoType::Model, rev.to_string()),
        None => Repo::model(repo_id.to_string()),
    };
    let handle = api.repo(repo);
    let mut out = Vec::with_capacity(files.len());
    for f in files {
        let p = handle
            .get(f)
            .await
            .with_context(|| format!("download {repo_id} :: {f}"))?;
        out.push(p);
    }
    Ok(out)
}

/// Best-effort fetch of an ONNX file's `.onnx_data` sibling. Larger exports
/// store weights externally; smaller ones don't have the sibling at all, so a
/// 404 here is expected and ignored. hf-hub caches both into the same snapshot
/// directory, which is exactly where ORT looks for the data file.
async fn fetch_external_data(repo_id: &str, revision: Option<&str>, onnx_file: &str) {
    let sibling = external_data_name(onnx_file);
    if let Err(e) = fetch_repo(repo_id, revision, &[&sibling]).await {
        tracing::debug!(repo = repo_id, file = %sibling, "no external-data sibling (ok): {e}");
    }
}

/// Resolve paths for a model: baked dir first (trying each ONNX candidate in
/// priority order), HF fallback second (with int8 fallback if the preferred
/// variant is missing upstream).
async fn resolve_model(
    repo: &str,
    revision: Option<&str>,
    onnx_candidates: &[&str],
    tokenizer_file: &str,
) -> Result<ModelPaths> {
    if let Some(base) = baked_models_dir() {
        if let Some(tokenizer) = baked_file(&base, repo, tokenizer_file) {
            for onnx_file in onnx_candidates {
                if let Some(onnx) = baked_file(&base, repo, onnx_file) {
                    tracing::info!(
                        repo,
                        onnx = onnx_file,
                        base = %base.display(),
                        "using baked model files (no HF download)"
                    );
                    return Ok(ModelPaths { onnx, tokenizer });
                }
            }
        }
        tracing::warn!(
            repo,
            base = %base.display(),
            "VIRTUES_MODELS_DIR is set but expected files missing; falling back to HF"
        );
    }

    // HF: tokenizer is precision-independent, fetch once; then try ONNX
    // candidates in order, fetching each one's external-data sibling alongside.
    let tokenizer = fetch_repo(repo, revision, &[tokenizer_file])
        .await?
        .pop()
        .expect("fetch_repo returns one path per requested file");

    let mut last_err = None;
    for onnx_file in onnx_candidates {
        match fetch_repo(repo, revision, &[onnx_file]).await {
            Ok(mut v) => {
                let onnx = v.pop().expect("one path per requested file");
                fetch_external_data(repo, revision, onnx_file).await;
                return Ok(ModelPaths { onnx, tokenizer });
            }
            Err(e) => {
                tracing::warn!(repo, onnx = onnx_file, "ONNX variant unavailable, trying next: {e}");
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no ONNX candidates configured for {repo}")))
}

pub async fn embedder_paths() -> Result<ModelPaths> {
    let precision = accelerator::detect().precision();
    resolve_model(
        EMBEDDER_REPO,
        EMBEDDER_REVISION,
        embedder_onnx_candidates(precision),
        EMBEDDER_TOKENIZER_FILE,
    )
    .await
}

pub async fn reranker_paths() -> Result<ModelPaths> {
    let precision = accelerator::detect().precision();
    resolve_model(
        RERANKER_REPO,
        RERANKER_REVISION,
        reranker_onnx_candidates(precision),
        RERANKER_TOKENIZER_FILE,
    )
    .await
}

// ─── Resolution report (doctor / web-status surface) ───────────────────────

/// Where a model's files would come from this run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSource {
    /// Present in the baked dir at this path — no network needed.
    Baked(PathBuf),
    /// Not baked; would be downloaded from HF on first use.
    Download,
}

/// One model's resolved plan, without performing any download.
#[derive(Debug, Clone)]
pub struct ModelStatus {
    pub name: &'static str,
    pub repo: &'static str,
    pub onnx_file: String,
    pub source: ModelSource,
}

/// A pure, side-effect-free snapshot of what the resolver would do this run.
/// Drives `virtues doctor` and the on-box web status page — no HF calls, no
/// session construction, safe to call anywhere.
#[derive(Debug, Clone)]
pub struct ResolutionReport {
    pub accelerator: &'static str,
    pub precision: &'static str,
    pub cuda_compiled: bool,
    pub models_dir: Option<PathBuf>,
    pub models: Vec<ModelStatus>,
}

fn plan_model(
    base: Option<&Path>,
    name: &'static str,
    repo: &'static str,
    candidates: &[&'static str],
    tokenizer_file: &str,
) -> ModelStatus {
    if let Some(base) = base {
        if baked_file(base, repo, tokenizer_file).is_some() {
            for onnx_file in candidates {
                if let Some(p) = baked_file(base, repo, onnx_file) {
                    return ModelStatus {
                        name,
                        repo,
                        onnx_file: (*onnx_file).to_string(),
                        source: ModelSource::Baked(p),
                    };
                }
            }
        }
    }
    // Not baked → would download the preferred (first) candidate.
    ModelStatus {
        name,
        repo,
        onnx_file: candidates.first().copied().unwrap_or_default().to_string(),
        source: ModelSource::Download,
    }
}

/// Compute the resolution plan for the active accelerator without downloading.
pub fn resolution_report() -> ResolutionReport {
    let accel = accelerator::detect();
    let precision = accel.precision();
    let base = baked_models_dir();
    let base_ref = base.as_deref();

    let models = vec![
        plan_model(
            base_ref,
            "embedder",
            EMBEDDER_REPO,
            embedder_onnx_candidates(precision),
            EMBEDDER_TOKENIZER_FILE,
        ),
        plan_model(
            base_ref,
            "reranker",
            RERANKER_REPO,
            reranker_onnx_candidates(precision),
            RERANKER_TOKENIZER_FILE,
        ),
    ];

    ResolutionReport {
        accelerator: accel.as_str(),
        precision: precision.as_str(),
        cuda_compiled: accelerator::cuda_compiled(),
        models_dir: base,
        models,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_data_sibling_naming() {
        assert_eq!(
            external_data_name("onnx/model_quantized.onnx"),
            "onnx/model_quantized.onnx_data"
        );
    }

    #[test]
    fn fp16_prefers_fp16_then_int8_floor() {
        assert_eq!(
            embedder_onnx_candidates(Precision::Fp16),
            &[EMBEDDER_ONNX_FP16, EMBEDDER_ONNX_INT8]
        );
        // int8 accelerators never reach for an fp16 export.
        assert_eq!(embedder_onnx_candidates(Precision::Int8), &[EMBEDDER_ONNX_INT8]);
    }

    #[test]
    fn report_is_pure_and_well_formed() {
        let r = resolution_report();
        assert_eq!(r.models.len(), 2);
        assert_eq!(r.models[0].name, "embedder");
        assert_eq!(r.models[1].name, "reranker");
        // precision must match the detected accelerator's policy.
        assert!(matches!(r.precision, "fp16" | "int8"));
    }
}
