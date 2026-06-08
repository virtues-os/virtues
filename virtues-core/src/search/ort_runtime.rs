//! ORT session construction shared by embedder and reranker.
//!
//! Execution-provider (EP) selection is driven by [`accelerator::detect`](super::accelerator),
//! the single policy source. EPs are tried in priority order; the first one ORT
//! can initialize wins, and CPU is always appended as the final fallback.
//!
//! - **Cuda** (Jetson/NVIDIA appliance, built `--features cuda[,tensorrt]`):
//!   TensorRT → CUDA → CPU.
//! - **CoreMl** (macOS dev hosts): CoreML → CPU.
//! - **Cpu** (portable DIY image): CPU only.
//!
//! `detect()` returns the *effective* accelerator — already reconciled against
//! what this binary was compiled to support — so a `Cuda` result guarantees the
//! `cuda` feature is linked. The `#[cfg]` guards below keep the GPU EP types out
//! of CPU-only builds.

use anyhow::Result;
use ort::execution_providers::{CPUExecutionProvider, ExecutionProviderDispatch};
use ort::session::{builder::GraphOptimizationLevel, Session};
use std::path::Path;

use super::accelerator::{self, Accelerator};

/// Build an ORT session from an ONNX file with the active accelerator's EPs.
pub fn build_session(onnx_path: &Path) -> Result<Session> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    let accel = accelerator::detect();

    let mut eps: Vec<ExecutionProviderDispatch> = Vec::new();
    match accel {
        Accelerator::Cuda => {
            // TensorRT first (fuses + fp16 on Jetson), CUDA as the GPU fallback.
            #[cfg(feature = "tensorrt")]
            eps.push(ort::execution_providers::TensorRTExecutionProvider::default().build());
            #[cfg(feature = "cuda")]
            eps.push(ort::execution_providers::CUDAExecutionProvider::default().build());
        }
        Accelerator::CoreMl => {
            #[cfg(target_os = "macos")]
            eps.push(ort::execution_providers::CoreMLExecutionProvider::default().build());
        }
        Accelerator::Cpu => {}
    }
    // CPU is always the final fallback and is always available.
    eps.push(CPUExecutionProvider::default().build());
    let ep_count = eps.len();

    let builder = Session::builder().map_err(|e| anyhow::anyhow!("ort session builder: {e}"))?;
    let mut builder = builder
        .with_execution_providers(eps)
        .map_err(|e| anyhow::anyhow!("register execution providers: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| anyhow::anyhow!("set graph opt level: {e}"))?
        .with_intra_threads(threads)
        .map_err(|e| anyhow::anyhow!("set intra threads: {e}"))?;

    let session = builder
        .commit_from_file(onnx_path)
        .map_err(|e| anyhow::anyhow!("load onnx {}: {e}", onnx_path.display()))?;

    tracing::info!(
        path = %onnx_path.display(),
        accelerator = %accel.as_str(),
        execution_providers = ep_count,
        threads = threads,
        "ORT session initialized",
    );
    Ok(session)
}
