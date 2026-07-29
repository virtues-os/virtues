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
    /// GGUF file names the inference sidecars load. EmbeddingGemma-300M
    /// (QAT Q8_0, 768-dim native → Matryoshka-256) for embedding;
    /// gte-reranker-modernbert-base (Q8_0, stateless) for the reranker.
    /// Must stay in sync with virtues-core's `inference_report::{EMBED_GGUF,
    /// RERANK_GGUF}` — the sidecar `-m` path and the runtime's dim/pooling
    /// expectations have to agree or embeds are rejected at runtime.
    pub embed_gguf: String,
    pub rerank_gguf: String,
    /// QNN context-binary names for the Dragon NPU path (Hexagon v68). These are
    /// the QAIRT-compiled artifacts the `virtues-qnnd` daemon loads: gte-small
    /// embed (idx 0) + answerai-colbert-small@256 rerank (idx 1). Fetched from
    /// the same `models-*` bucket as the GGUFs, SHA-verified. Must agree with the
    /// tokenizers shipped beside them and with `search::qnn_client` in core.
    pub qnn_embed_bin: String,
    pub qnn_rerank_bin: String,
    /// Tokenizer files for the QNN path (BERT WordPiece), fetched into
    /// `tok_gte/` and `tok_colbert/` under the QNN models dir. `search::qnn_client`
    /// loads them by that layout.
    pub qnn_tokenizers: Vec<(String, String)>,
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
            embed_gguf: "embeddinggemma-300m-qat-Q8_0.gguf".to_string(),
            rerank_gguf: "gte-reranker-modernbert-base-Q8_0.gguf".to_string(),
            qnn_embed_bin: "gte_v68_vtcm2.bin".to_string(),
            qnn_rerank_bin: "cb256_v68_vtcm2.bin".to_string(),
            // (relative-dest, asset-name) — dest is under the QNN models dir.
            qnn_tokenizers: vec![
                ("tok_gte/tokenizer.json".to_string(), "tok_gte-tokenizer.json".to_string()),
                ("tok_colbert/tokenizer.json".to_string(), "tok_colbert-tokenizer.json".to_string()),
            ],
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
        let mut assets = vec![
            self.embed_gguf.clone(),
            self.rerank_gguf.clone(),
            self.qnn_embed_bin.clone(),
            self.qnn_rerank_bin.clone(),
        ];
        assets.extend(self.qnn_tokenizers.iter().map(|(_dest, asset)| asset.clone()));
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

    /// Where the action tree (manifests + UI + sources.toml) lands on the box.
    /// virtues-core reads this via `VIRTUES_ACTIONS_DIR` (see
    /// `applet_templates::actions_root`); the default here must match
    /// `WELL_KNOWN_ACTIONS_DIR` in virtues-core. Shipped in the release
    /// tarball as `actions/`; not baked into the binary, so a box with no
    /// copy here has no actions at all.
    pub fn actions_dir(&self) -> PathBuf {
        self.install_prefix.join("share/virtues/actions")
    }

    /// The WRITABLE applet tree — chat-authored applets and imported packs.
    ///
    /// Separate from [`Self::actions_dir`] because the two have opposite
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
    pub fn actions_bin_dir(&self) -> PathBuf {
        self.install_prefix.join("libexec/virtues")
    }

    /// The llama-server binary that hosts both inference sidecars (Dragon
    /// mode only). Ships in the release tarball (built per-arch in our CI at
    /// a pinned llama.cpp tag).
    pub fn llama_binary_path(&self) -> PathBuf {
        self.install_prefix.join("bin/llama-server")
    }

    /// Where the GGUFs live on the box. virtues-core reads the same default
    /// via `VIRTUES_MODELS_DIR` for its resolution report.
    pub fn models_dir(&self) -> PathBuf {
        self.data_dir.join("models")
    }

    /// The `virtues-qnnd` NPU daemon binary (Dragon only). Ships in the release
    /// tarball, built for aarch64 by CI against the QAIRT SDK (a build leg that
    /// only produces a real daemon when `QNN_SDK_ROOT` is set — see
    /// `crates/virtues-qnnd`).
    pub fn qnnd_binary_path(&self) -> PathBuf {
        self.install_prefix.join("bin/virtues-qnnd")
    }

    /// Where the QNN context binaries + tokenizers live on the box. Matches the
    /// `VIRTUES_QNND_MODELS_DIR` default that `search::qnn_client` reads.
    pub fn qnn_models_dir(&self) -> PathBuf {
        self.models_dir().join("qnn")
    }

    /// Root of the QAIRT runtime libs this installer fetches and owns, with
    /// `host/` and `dsp/` beneath it — the two must stay separate directories
    /// because `LD_LIBRARY_PATH` and `ADSP_LIBRARY_PATH` point at different
    /// halves of the SDK. Under the models dir so backup/GC treat them like the
    /// other fetched artifacts. See `qairt.rs` for why we fetch rather than ship.
    pub fn qnn_managed_lib_dir(&self) -> PathBuf {
        self.qnn_models_dir().join("lib")
    }

    /// Directory holding the Qualcomm QAIRT runtime libs (`libQnnHtp.so`,
    /// `libQnnSystem.so`, the v68 skel/stub) that `virtues-qnnd` dlopen's at
    /// runtime. These are Qualcomm proprietary — NOT shipped by us — so the
    /// appliance image must provide them: either on the default loader path
    /// (ldconfig, `None` here) or at a directory pinned via `VIRTUES_QNN_LIB_DIR`,
    /// which the daemon unit adds to `LD_LIBRARY_PATH`.
    pub fn qnn_lib_dir(&self) -> Option<String> {
        std::env::var("VIRTUES_QNN_LIB_DIR").ok().filter(|s| !s.trim().is_empty())
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
