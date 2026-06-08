//! Hardware accelerator detection and the model policy derived from it.
//!
//! This is the single source of truth the two halves of the model layer read:
//!   - [`ort_runtime::build_session`](super::ort_runtime) asks which execution
//!     providers to register.
//!   - [`model_cache`](super::model_cache) asks which ONNX quantization variant
//!     to resolve.
//!
//! ## Why this split exists
//!
//! The ONNX *graph* is portable across execution providers — the same
//! `model.onnx` runs on CPU, CUDA, or TensorRT. So the model file is not
//! hardware-specific per se. What *is* hardware-specific:
//!
//!   1. **The execution provider** that runs the graph. CUDA / TensorRT must be
//!      linked into the ORT binary at build time, which is why they sit behind
//!      the `cuda` / `tensorrt` cargo features. The portable DIY image links
//!      neither and runs CPU; the Jetson appliance image is built with them.
//!   2. **The optimal quantization.** int8 dynamic-quant is best on CPU; fp16 is
//!      best on a GPU. So the resolver prefers a precision-appropriate ONNX
//!      variant per accelerator, falling back to the int8 floor when the fp16
//!      export isn't present.
//!
//! This keeps the base OCI image accelerator-agnostic: the appliance pre-bakes
//! the CUDA EP + fp16 models on its SSD; DIY resolves the CPU/int8 floor at
//! init. Same code, different build features + baked artifacts.
//!
//! Detection is cached process-wide. Override with `VIRTUES_ACCELERATOR`
//! (`cuda` | `coreml` | `cpu`) — useful for forcing the CPU floor on a GPU box,
//! or for tests.

use std::sync::OnceLock;

/// The accelerator the inference stack will actually use this run. This is the
/// *effective* accelerator: the detected hardware intersected with what the
/// binary was compiled to support (a CUDA GPU on a CPU-only build downgrades to
/// [`Accelerator::Cpu`] with a loud warning).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accelerator {
    /// NVIDIA GPU, including Jetson. Requires the `cuda` cargo feature linked.
    Cuda,
    /// Apple CoreML (Neural Engine / GPU) — macOS dev hosts only.
    CoreMl,
    /// Portable CPU floor. Always available, always the final fallback.
    Cpu,
}

/// ONNX quantization variant to prefer for a given accelerator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    /// fp16 — best throughput on GPU (CUDA / TensorRT).
    Fp16,
    /// int8 dynamic quant — best on CPU, fine on CoreML; the universal floor.
    Int8,
}

impl Accelerator {
    /// Stable lowercase token used in logs, the doctor report, and as the
    /// accepted value of `VIRTUES_ACCELERATOR`.
    pub fn as_str(self) -> &'static str {
        match self {
            Accelerator::Cuda => "cuda",
            Accelerator::CoreMl => "coreml",
            Accelerator::Cpu => "cpu",
        }
    }

    /// Quantization variant the resolver should prefer for this accelerator.
    pub fn precision(self) -> Precision {
        match self {
            Accelerator::Cuda => Precision::Fp16,
            Accelerator::CoreMl | Accelerator::Cpu => Precision::Int8,
        }
    }
}

impl Precision {
    pub fn as_str(self) -> &'static str {
        match self {
            Precision::Fp16 => "fp16",
            Precision::Int8 => "int8",
        }
    }
}

/// Whether this *binary* was compiled with the CUDA execution provider linked.
/// Detected GPU hardware is meaningless if the EP isn't in the build — this is
/// the honesty check behind the appliance-vs-DIY split.
pub const fn cuda_compiled() -> bool {
    cfg!(feature = "cuda")
}

/// The effective accelerator for this process, cached after first detection.
pub fn detect() -> Accelerator {
    static CACHED: OnceLock<Accelerator> = OnceLock::new();
    *CACHED.get_or_init(detect_uncached)
}

fn detect_uncached() -> Accelerator {
    // 1. Explicit override always wins — but still reconcile against compiled
    //    capability so `VIRTUES_ACCELERATOR=cuda` on a CPU-only build is honest.
    if let Ok(raw) = std::env::var("VIRTUES_ACCELERATOR") {
        let raw = raw.trim().to_ascii_lowercase();
        if !raw.is_empty() {
            let requested = match raw.as_str() {
                "cuda" | "gpu" | "nvidia" | "tensorrt" => Some(Accelerator::Cuda),
                "coreml" | "metal" | "ane" => Some(Accelerator::CoreMl),
                "cpu" | "none" => Some(Accelerator::Cpu),
                _ => {
                    tracing::warn!(
                        value = %raw,
                        "VIRTUES_ACCELERATOR not recognized (cuda|coreml|cpu); auto-detecting"
                    );
                    None
                }
            };
            if let Some(req) = requested {
                let eff = reconcile(req);
                tracing::info!(
                    requested = %req.as_str(),
                    effective = %eff.as_str(),
                    "accelerator selected from VIRTUES_ACCELERATOR"
                );
                return eff;
            }
        }
    }

    let eff = reconcile(probe_hardware());
    tracing::info!(effective = %eff.as_str(), "accelerator auto-detected");
    eff
}

/// Intersect a desired/detected accelerator with what the binary can actually
/// drive. A GPU on a CPU-only build downgrades to CPU and says so loudly — that
/// message is the DIY user's cue to use the Jetson appliance image (or rebuild
/// with `--features cuda`).
fn reconcile(want: Accelerator) -> Accelerator {
    match want {
        Accelerator::Cuda if !cuda_compiled() => {
            tracing::warn!(
                "NVIDIA GPU requested/detected but this build has no CUDA execution \
                 provider (CPU-only image). Falling back to CPU. Use the Jetson \
                 appliance image or rebuild core with `--features cuda` to use the GPU."
            );
            Accelerator::Cpu
        }
        other => other,
    }
}

/// Probe the host for an accelerator, ignoring compiled capability (that's
/// [`reconcile`]'s job). Pure inspection of platform + device files.
fn probe_hardware() -> Accelerator {
    #[cfg(target_os = "macos")]
    {
        return Accelerator::CoreMl;
    }

    #[cfg(target_os = "linux")]
    {
        if nvidia_present() {
            return Accelerator::Cuda;
        }
        return Accelerator::Cpu;
    }

    #[allow(unreachable_code)]
    Accelerator::Cpu
}

/// True if an NVIDIA GPU appears present on a Linux host. Covers discrete cards
/// (`/dev/nvidia*`, `/proc/driver/nvidia/version`) and Jetson's integrated GPU
/// (`/etc/nv_tegra_release`). Cheap path-existence checks — no driver calls.
#[cfg(target_os = "linux")]
fn nvidia_present() -> bool {
    use std::path::Path;
    const MARKERS: &[&str] = &[
        "/proc/driver/nvidia/version", // discrete GPU with driver loaded
        "/dev/nvidiactl",              // discrete GPU control device
        "/dev/nvidia0",                // first discrete GPU device
        "/etc/nv_tegra_release",       // Jetson (Tegra) release marker
        "/sys/module/nvgpu",           // Jetson integrated GPU kernel module
    ];
    MARKERS.iter().any(|p| Path::new(p).exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precision_policy_matches_accelerator() {
        assert_eq!(Accelerator::Cuda.precision(), Precision::Fp16);
        assert_eq!(Accelerator::CoreMl.precision(), Precision::Int8);
        assert_eq!(Accelerator::Cpu.precision(), Precision::Int8);
    }

    #[test]
    fn cuda_downgrades_to_cpu_without_feature() {
        // The default test build has no `cuda` feature, so a CUDA request must
        // reconcile down to CPU. (When the suite is run with --features cuda
        // this asserts the pass-through instead.)
        let got = reconcile(Accelerator::Cuda);
        if cuda_compiled() {
            assert_eq!(got, Accelerator::Cuda);
        } else {
            assert_eq!(got, Accelerator::Cpu);
        }
    }

    #[test]
    fn coreml_and_cpu_pass_through_reconcile() {
        assert_eq!(reconcile(Accelerator::CoreMl), Accelerator::CoreMl);
        assert_eq!(reconcile(Accelerator::Cpu), Accelerator::Cpu);
    }

    #[test]
    fn stable_string_tokens() {
        assert_eq!(Accelerator::Cuda.as_str(), "cuda");
        assert_eq!(Accelerator::CoreMl.as_str(), "coreml");
        assert_eq!(Accelerator::Cpu.as_str(), "cpu");
        assert_eq!(Precision::Fp16.as_str(), "fp16");
        assert_eq!(Precision::Int8.as_str(), "int8");
    }
}
