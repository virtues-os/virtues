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
    /// GGUF file name the embedding sidecar loads: EmbeddingGemma 2 (Q8_0,
    /// stored at its native 768 dims). Must stay in sync with virtues-core's
    /// `inference_report::{EMBED_GGUF, EMBED_GGUF_SHA256}`, which `virtues
    /// upgrade` uses to move installed boxes to it.
    pub embed_gguf: String,
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
            embed_gguf: "embeddinggemma-2-Q8_0.gguf".to_string(),
            atlas_url: "https://atlas.virtues.com".to_string(),
            virtues_api_url: "https://api.virtues.com".to_string(),
        }
    }

    /// pdfium build version re-hosted on the models release (from
    /// bblanchon/pdfium-binaries, BSD-licensed). Version is baked into the
    /// asset name — updates ship under a NEW name + a config bump here,
    /// never replaced in place (same doctrine as the GGUFs).
    pub const PDFIUM_VERSION: &'static str = "7961";

    /// Per-arch pdfium asset name on the models release. The installer is
    /// compiled per-target, so the arch is a compile-time fact.
    pub fn pdfium_asset(&self) -> String {
        #[cfg(target_arch = "aarch64")]
        let arch = "arm64";
        #[cfg(target_arch = "x86_64")]
        let arch = "x64";
        Self::pdfium_asset_for(arch)
    }

    fn pdfium_asset_for(arch: &str) -> String {
        format!("libpdfium-{}-linux-{arch}.so", Self::PDFIUM_VERSION)
    }

    /// Every asset name this installer can fetch from the models release,
    /// across ALL target arches — one release tag serves every box, so the
    /// release must carry the union, not just the current compile target's
    /// slice. Anything fetched through `download::fetch_asset` belongs here;
    /// the release audit test below holds the models release to this list.
    #[cfg(test)]
    pub fn models_release_assets(&self) -> Vec<String> {
        let mut assets = vec![self.embed_gguf.clone()];
        assets.extend(["arm64", "x64"].map(Self::pdfium_asset_for));
        assets
    }

    /// Where libpdfium lands on the box. virtues-core's PDF extractor finds
    /// it via the VIRTUES_PDFIUM_PATH env line (written at install) and via
    /// its VIRTUES_MODELS_DIR/pdfium fallback.
    pub fn pdfium_dir(&self) -> PathBuf {
        self.models_dir().join("pdfium")
    }

    pub fn pdfium_lib_path(&self) -> PathBuf {
        self.pdfium_dir().join("libpdfium.so")
    }

    pub fn env_file_path(&self) -> PathBuf {
        self.data_dir.join("virtues.env")
    }

    pub fn binary_path(&self) -> PathBuf {
        self.install_prefix.join("bin/virtues")
    }

    pub fn web_dir(&self) -> PathBuf {
        self.install_prefix.join("share/virtues/web")
    }

    /// The slot-layout root (`share/virtues`) — holds `releases/`, the
    /// `current` flip link, the routing symlinks, and `install.json`.
    pub fn share_virtues_dir(&self) -> PathBuf {
        self.install_prefix.join("share/virtues")
    }

    /// Where the applet tree (manifests + UI + sources.toml) lands on the box.
    ///
    /// The installer writes `VIRTUES_APPLETS_DIR=<prefix>/share/virtues/applets`,
    /// matching `WELL_KNOWN_APPLETS_DIR` in virtues-core.
    ///
    /// Safe on an existing box because activation already symlinks BOTH
    /// `share/virtues/applets` and the legacy `share/virtues/actions` at the
    /// slot's `applets/` (see `download::atomic_flip`), and core resolves
    /// `VIRTUES_APPLETS_DIR` then `VIRTUES_ACTIONS_DIR` then the two well-known
    /// paths. So old env files, new env files, and either directory name all
    /// land in the same place — which is what made this a rename rather than a
    /// migration.
    ///
    /// Shipped in the release tarball; not baked into the binary, so a box with
    /// no copy here has no applets at all.
    pub fn applets_dir(&self) -> PathBuf {
        self.install_prefix.join("share/virtues/applets")
    }

    /// The WRITABLE applet tree — chat-authored applets and imported packs.
    ///
    /// Separate from [`Self::applets_dir`] because the two have opposite
    /// lifecycles: that one is package data the installer replaces wholesale
    /// on every release, this one is user data that must survive it. They
    /// used to be the same directory, which meant the slot flip deleted
    /// authored applets and a fresh box couldn't create them at all (nothing
    /// made a service-writable directory). virtues-core reads this via
    /// `VIRTUES_APPLET_STATE_DIR`; the default must match
    /// `WELL_KNOWN_APPLET_STATE_DIR` in virtues-core.
    pub fn applet_state_dir(&self) -> PathBuf {
        self.data_dir.join("applets")
    }

    /// Where the compiled function-action executables land (libexec = helper
    /// binaries not meant for direct user invocation). virtues-core resolves
    /// action `command[0]` here via `VIRTUES_ACTIONS_BIN_DIR` (see
    /// `applet_runner::resolve_program`); the default must match
    /// `WELL_KNOWN_ACTIONS_BIN_DIR` in virtues-core. Shipped as `actions-bin/`.
    pub fn applets_bin_dir(&self) -> PathBuf {
        self.install_prefix.join("libexec/virtues")
    }

    /// The llama-server binary that hosts the embedding sidecar. Ships in the
    /// release tarball (built per-arch in our CI at a pinned llama.cpp tag).
    pub fn llama_binary_path(&self) -> PathBuf {
        self.install_prefix.join("bin/llama-server")
    }

    /// Where the GGUFs live on the box. virtues-core reads the same default
    /// via `VIRTUES_MODELS_DIR` for its resolution report.
    pub fn models_dir(&self) -> PathBuf {
        self.data_dir.join("models")
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    /// Audit the live models release against `models_release_assets`: every
    /// asset must be downloadable WITH its `.sha256` sidecar (fetch_asset
    /// hard-fails on a missing sidecar). Exists because libpdfium was wired
    /// into the install flow without its assets ever being uploaded to
    /// models-1 — every real install then died 404 on its last step while CI
    /// stayed green. Network test: ignored by default, run explicitly by
    /// ci.yml's "Models-release asset audit" step.
    #[tokio::test]
    #[ignore = "network: audits the live models release"]
    async fn models_release_serves_every_expected_asset() {
        // Same provider install main() does — rustls panics on first TLS use
        // without it, and the test binary never runs main().
        let _ = rustls::crypto::ring::default_provider().install_default();

        let cfg = InstallConfig::recommended_defaults();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap();
        let mut missing = Vec::new();
        for name in cfg.models_release_assets() {
            for url in [
                format!("{}/{name}", cfg.models_base),
                format!("{}/{name}.sha256", cfg.models_base),
            ] {
                match client.head(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {}
                    Ok(resp) => missing.push(format!("{url} — HTTP {}", resp.status())),
                    Err(e) => missing.push(format!("{url} — {e}")),
                }
            }
        }
        assert!(
            missing.is_empty(),
            "models release is missing assets the installer will 404 on:\n{}",
            missing.join("\n")
        );
    }
}
