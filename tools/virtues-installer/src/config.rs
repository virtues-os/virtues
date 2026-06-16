//! Resolved paths + defaults for an install run.
//!
//! Centralised so we don't sprinkle string literals through every step.
//! Honors the same env-var overrides the bash install.sh respected, so an
//! advanced operator can still pin install prefix / data dir / download
//! base without code changes.

use std::path::PathBuf;

pub struct InstallConfig {
    pub install_prefix: PathBuf,
    pub data_dir: PathBuf,
    pub github_owner: String,
    pub github_repo: String,
    /// Override the release-asset base URL (defaults to the GitHub
    /// release URL for the resolved tag). Set during dev to point at a
    /// local file server.
    pub download_base: Option<String>,
    /// Either a pinned tag ("v0.1.0") or `None` meaning "latest".
    pub pinned_version: Option<String>,
    /// Base URL for the GGUF model assets. Models live on a dedicated,
    /// stable release tag (they change far less often than code releases,
    /// and re-uploading ~2 GB per code release would be wasteful). The
    /// `.github/workflows/models-release.yml` workflow populates that tag
    /// from vetted upstream GGUFs, with `.sha256` sidecars.
    pub models_base: String,
    /// GGUF file names the inference sidecars load. F16 for embedding (to
    /// match the precision existing search_vectors rows were embedded
    /// with), Q8_0 for the reranker (stateless, so smaller/faster wins).
    /// Must stay in sync with virtues-core's `inference_report::{EMBED_GGUF,
    /// RERANK_GGUF}`.
    pub embed_gguf: String,
    pub rerank_gguf: String,
    /// Production environment URLs. Written into the box's env file so
    /// the runtime daemon talks to the real atlas/virtues-api instead of
    /// the localhost dev defaults baked into the box code.
    pub atlas_url: String,
    pub virtues_api_url: String,
}

impl InstallConfig {
    pub fn recommended_defaults() -> Self {
        Self {
            install_prefix: std::env::var_os("INSTALL_PREFIX")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/usr/local")),
            data_dir: std::env::var_os("DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/var/lib/virtues")),
            github_owner: std::env::var("VIRTUES_GITHUB_OWNER")
                .unwrap_or_else(|_| "virtues-os".to_string()),
            github_repo: std::env::var("VIRTUES_GITHUB_REPO")
                .unwrap_or_else(|_| "virtues".to_string()),
            download_base: std::env::var("VIRTUES_DOWNLOAD_BASE").ok(),
            pinned_version: None,
            models_base: std::env::var("VIRTUES_MODELS_BASE").unwrap_or_else(|_| {
                format!(
                    "https://github.com/{owner}/{repo}/releases/download/models-1",
                    owner = std::env::var("VIRTUES_GITHUB_OWNER")
                        .unwrap_or_else(|_| "virtues-os".to_string()),
                    repo = std::env::var("VIRTUES_GITHUB_REPO")
                        .unwrap_or_else(|_| "virtues".to_string()),
                )
            }),
            embed_gguf: "bge-m3-FP16.gguf".to_string(),
            rerank_gguf: "bge-reranker-v2-m3-Q8_0.gguf".to_string(),
            atlas_url: "https://atlas.virtues.com".to_string(),
            virtues_api_url: "https://api.virtues.com".to_string(),
        }
    }

    pub fn env_file_path(&self) -> PathBuf {
        self.data_dir.join("virtues.env")
    }

    pub fn binary_path(&self) -> PathBuf {
        self.install_prefix.join("bin/virtues")
    }

    /// Path to the privileged WireGuard reconciler. Ships in the same tarball
    /// as `virtues`; runs as its own systemd unit so the main app stays
    /// rootless. See `install::install_wireguard_unit`.
    pub fn wg_binary_path(&self) -> PathBuf {
        self.install_prefix.join("bin/virtues-wireguard")
    }

    pub fn web_dir(&self) -> PathBuf {
        self.install_prefix.join("share/virtues/web")
    }

    /// Where the action tree (manifests + UI + sources.toml) lands on the box.
    /// virtues-core reads this via `VIRTUES_ACTIONS_DIR` (see
    /// `action_templates::actions_root`); the default here must match
    /// `WELL_KNOWN_ACTIONS_DIR` in virtues-core. Shipped in the release
    /// tarball as `actions/`; not baked into the binary, so a box with no
    /// copy here has no actions at all.
    pub fn actions_dir(&self) -> PathBuf {
        self.install_prefix.join("share/virtues/actions")
    }

    /// Where the compiled function-action executables land (libexec = helper
    /// binaries not meant for direct user invocation). virtues-core resolves
    /// action `command[0]` here via `VIRTUES_ACTIONS_BIN_DIR` (see
    /// `action_runner::resolve_program`); the default must match
    /// `WELL_KNOWN_ACTIONS_BIN_DIR` in virtues-core. Shipped as `actions-bin/`.
    pub fn actions_bin_dir(&self) -> PathBuf {
        self.install_prefix.join("libexec/virtues")
    }

    /// The llama-server binary that hosts both inference sidecars. Ships in
    /// the release tarball (built per-arch in our CI at a pinned llama.cpp
    /// tag); a CUDA build for Jetson is swapped in when available.
    pub fn llama_binary_path(&self) -> PathBuf {
        self.install_prefix.join("bin/llama-server")
    }

    /// Where the GGUFs live on the box. virtues-core reads the same default
    /// via `VIRTUES_MODELS_DIR` for its resolution report.
    pub fn models_dir(&self) -> PathBuf {
        self.data_dir.join("models")
    }
}
