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
    /// Embedding model Ollama should pull at install time. Picked here so
    /// bumping the default is a one-line code change.
    pub embed_model: String,
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
            embed_model: std::env::var("VIRTUES_EMBED_MODEL")
                .unwrap_or_else(|_| "bge-m3".to_string()),
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
}
