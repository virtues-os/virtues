//! Binary tarball download + SHA256 verify + extract.
//!
//! Resolves "latest" via the GitHub Releases API when no version is pinned.
//! SHA-verifies the tarball against the sidecar `.sha256` (CI uploads both).
//! Extracts to a tempdir, then installs the `virtues` binary to
//! `$INSTALL_PREFIX/bin/` and `web/` to `$INSTALL_PREFIX/share/virtues/web/`.

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

use crate::config::InstallConfig;
use crate::steps::run_step;
use crate::ui;

pub async fn download_binary(cfg: &mut InstallConfig, arch: &str) -> Result<()> {
    let version = resolve_version(cfg).await?;
    let base = match &cfg.download_base {
        Some(b) => b.clone(),
        None => format!(
            "https://github.com/{owner}/{repo}/releases/download/{version}",
            owner = cfg.github_owner,
            repo = cfg.github_repo,
        ),
    };
    let tar_name = format!("virtues-{version}-{arch}-linux.tar.gz");
    let tar_url = format!("{base}/{tar_name}");
    let sha_url = format!("{tar_url}.sha256");
    let tmpdir = tempfile::tempdir().context("creating tempdir")?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;

    ui::skip(&format!("Downloading {tar_name}"));
    let bytes = client
        .get(&tar_url)
        .send()
        .await
        .with_context(|| format!("GET {tar_url}"))?
        .error_for_status()?
        .bytes()
        .await?;
    let tar_path = tmpdir.path().join(&tar_name);
    fs::write(&tar_path, &bytes).context("writing tarball to tempdir")?;
    ui::ok(&format!("Downloaded ({} MB)", bytes.len() / 1024 / 1024));

    // SHA verification (defense-in-depth over HTTPS).
    if let Ok(resp) = client.get(&sha_url).send().await.and_then(|r| r.error_for_status()) {
        let expected = resp.text().await?;
        let expected = expected
            .split_whitespace()
            .next()
            .ok_or_else(|| anyhow!("malformed sha256 sidecar"))?;
        let actual = sha256_hex(&bytes);
        if expected.eq_ignore_ascii_case(&actual) {
            ui::ok("SHA256 verified");
        } else {
            return Err(anyhow!(
                "SHA256 mismatch on {tar_name} — refusing to install"
            ));
        }
    } else {
        ui::warn("SHA256 sidecar missing — proceeding without verification");
    }

    // Extract.
    let mut cmd = Command::new("tar");
    cmd.args(["-xzf", tar_path.to_str().unwrap(), "-C", tmpdir.path().to_str().unwrap()]);
    run_step(&format!("Extract {tar_name}"), cmd).await?;

    // Install binary (atomic replace — the dst is very likely the running box
    // binary, so a plain truncating copy would hit ETXTBSY "text file busy").
    let bin_src = tmpdir.path().join("virtues");
    let bin_dst = cfg.binary_path();
    install_executable(&bin_src, &bin_dst)?;

    // Install virtues-wireguard alongside it. Older tarballs may not ship the
    // WG binary yet (pre-v0.2.1 releases) — log a warning rather than failing
    // the install so the box can still come up; the installer's systemd step
    // will then skip the WG unit and surface a clear message.
    let wg_src = tmpdir.path().join("virtues-wireguard");
    if wg_src.is_file() {
        let wg_dst = cfg.wg_binary_path();
        install_executable(&wg_src, &wg_dst)?;
        ui::ok(&format!("Installed virtues-wireguard → {}", wg_dst.display()));
    } else {
        ui::warn(
            "virtues-wireguard not in tarball (pre-v0.2.1 release) — WG tunnel \
             reconciler will not be installed. Upgrade once a newer release is available.",
        );
    }

    // llama-server — the inference sidecar engine (embed + rerank; see
    // install::install_inference). Every tarball ships the CPU build; on
    // Jetson we then try to swap in the CUDA (sm_87) build, published as a
    // separate release asset because it only exists for aarch64 and its CI
    // job is non-fatal (release-linux.yml). CPU fallback always works —
    // bge-m3-class models rerank/embed fine on Orin's cores, just slower.
    let llama_src = tmpdir.path().join("llama-server");
    if llama_src.is_file() {
        let llama_dst = cfg.llama_binary_path();
        install_executable(&llama_src, &llama_dst)?;
        ui::ok(&format!("Installed llama-server → {}", llama_dst.display()));
        if is_jetson() {
            match fetch_jetson_cuda_llama(cfg, &base, &version).await {
                Ok(true) => ui::ok("Swapped in CUDA llama-server (Jetson, sm_87)"),
                Ok(false) => ui::warn(
                    "No CUDA llama-server asset on this release — inference sidecars run on CPU",
                ),
                Err(e) => {
                    ui::warn(&format!("CUDA llama-server fetch failed ({e}) — sidecars run on CPU"))
                }
            }
        }
    } else {
        ui::warn(
            "llama-server not in tarball (pre-v0.1.1 release) — inference sidecars \
             will not be installed. Upgrade once a newer release is available.",
        );
    }

    // Install web dir (newer tarballs ship apps/web/build/ as web/).
    let web_src = tmpdir.path().join("web");
    if web_src.is_dir() {
        let web_dst = cfg.web_dir();
        // Remove any prior copy so we replace cleanly.
        let _ = fs::remove_dir_all(&web_dst);
        copy_dir_all(&web_src, &web_dst)?;
        ui::ok(&format!("Installed web UI → {}", web_dst.display()));
    }

    cfg.pinned_version = Some(version);
    Ok(())
}

/// L4T's marker file — present on every Jetson, absent everywhere else.
fn is_jetson() -> bool {
    Path::new("/etc/nv_tegra_release").exists()
}

/// Try to replace the CPU llama-server with the Jetson CUDA build attached
/// to the same release. Returns Ok(false) when the asset simply isn't there
/// (its CI job is allowed to fail without blocking the release).
async fn fetch_jetson_cuda_llama(
    cfg: &InstallConfig,
    base: &str,
    version: &str,
) -> Result<bool> {
    let name = format!("llama-server-{version}-aarch64-cuda-linux");
    let url = format!("{base}/{name}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()?;

    let resp = client.get(&url).send().await.with_context(|| format!("GET {url}"))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(false);
    }
    let bytes = resp.error_for_status()?.bytes().await?;

    // SHA sidecar is mandatory here — this binary runs as a daemon.
    let expected = client
        .get(format!("{url}.sha256"))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .with_context(|| format!("GET {url}.sha256"))?
        .text()
        .await?;
    let expected = expected
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("malformed sha256 sidecar for {name}"))?;
    if !expected.eq_ignore_ascii_case(&sha256_hex(&bytes)) {
        return Err(anyhow!("SHA256 mismatch on {name}"));
    }

    let tmpdir = tempfile::tempdir().context("creating tempdir")?;
    let staged = tmpdir.path().join("llama-server");
    fs::write(&staged, &bytes).context("staging CUDA llama-server")?;
    install_executable(&staged, &cfg.llama_binary_path())?;
    Ok(true)
}

/// Download a GGUF from the models release into the models dir, with a
/// progress bar (these are 0.6–1.2 GB) and mandatory SHA256 sidecar
/// verification. Files already on disk are skipped: they were verified
/// when fetched and are immutable afterwards — model updates ship under
/// NEW file names, never in place, which is what makes the skip safe.
pub async fn fetch_model(cfg: &InstallConfig, name: &str) -> Result<()> {
    let dest = cfg.models_dir().join(name);
    if dest.is_file() && fs::metadata(&dest).map(|m| m.len() > 0).unwrap_or(false) {
        ui::skip(&format!("Model already present: {name}"));
        return Ok(());
    }

    let url = format!("{}/{name}", cfg.models_base);
    let client = reqwest::Client::builder()
        // Whole-request timeout; generous because this covers the full
        // body transfer of a ~1 GB file on a slow home link.
        .timeout(Duration::from_secs(3600))
        .build()?;

    let mut resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?
        .error_for_status()?;

    let pb = match resp.content_length() {
        Some(total) => {
            let pb = indicatif::ProgressBar::new(total);
            pb.set_style(
                indicatif::ProgressStyle::with_template(
                    "  {spinner:.dim} {msg} {bytes}/{total_bytes} ({bytes_per_sec})",
                )
                .unwrap(),
            );
            pb
        }
        None => indicatif::ProgressBar::new_spinner(),
    };
    pb.set_message(name.to_string());

    // Stream to a temp file in the destination dir (same-filesystem rename
    // at the end), hashing as we go — never the whole GGUF in memory.
    let tmp = dest.with_extension("part");
    let mut file = fs::File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
    let mut hasher = Sha256::new();
    {
        use std::io::Write;
        while let Some(chunk) = resp.chunk().await? {
            hasher.update(&chunk);
            file.write_all(&chunk).context("writing model chunk")?;
            pb.inc(chunk.len() as u64);
        }
        file.sync_all().ok();
    }
    pb.finish_and_clear();
    let actual = hex::encode(hasher.finalize());

    // The models release is ours, so a missing sidecar is a packaging bug,
    // not an optional nicety — hard-fail rather than install unverified.
    let sha_url = format!("{url}.sha256");
    let expected = client
        .get(&sha_url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .with_context(|| format!("GET {sha_url} — models release must ship .sha256 sidecars"))?
        .text()
        .await?;
    let expected = expected
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("malformed sha256 sidecar for {name}"))?;
    if !expected.eq_ignore_ascii_case(&actual) {
        let _ = fs::remove_file(&tmp);
        return Err(anyhow!("SHA256 mismatch on {name} — refusing to install"));
    }

    fs::rename(&tmp, &dest).with_context(|| format!("install {}", dest.display()))?;
    ui::ok(&format!("Model downloaded + verified: {name}"));
    Ok(())
}

/// Install an executable to `dst` atomically, tolerating the common case where
/// `dst` is a CURRENTLY-RUNNING binary (the systemd service). A plain
/// `fs::copy` opens the destination with `O_TRUNC`, which the kernel refuses
/// with ETXTBSY ("text file busy") while the file is being executed — that's
/// the upgrade failure on a live box. Instead, stage the new bytes in a temp
/// file in the SAME directory (so the rename stays on one filesystem) and
/// `rename(2)` it over the target: rename only swaps the directory entry, so a
/// running process keeps its old inode and the next exec picks up the new file.
/// No need to stop the service to upgrade.
fn install_executable(src: &Path, dst: &Path) -> Result<()> {
    let dir = dst.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).ok();
    let name = dst.file_name().and_then(|s| s.to_str()).unwrap_or("virtues");
    let tmp = dir.join(format!(".{name}.new"));
    let _ = fs::remove_file(&tmp); // clear any stale temp from a prior aborted run
    fs::copy(src, &tmp)
        .with_context(|| format!("staging {} → {}", src.display(), tmp.display()))?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("chmod 0755 {}", tmp.display()))?;
    fs::rename(&tmp, dst).with_context(|| {
        format!("install {} → {} (atomic rename)", src.display(), dst.display())
    })?;
    Ok(())
}

async fn resolve_version(cfg: &InstallConfig) -> Result<String> {
    if let Some(v) = &cfg.pinned_version {
        return Ok(v.clone());
    }
    if let Ok(v) = std::env::var("VIRTUES_VERSION") {
        if v != "latest" && !v.is_empty() {
            return Ok(v);
        }
    }
    let api = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        cfg.github_owner, cfg.github_repo
    );
    let resp = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("virtues-installer")
        .build()?
        .get(&api)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?;
    let json: serde_json::Value = resp.json().await?;
    let tag = json
        .get("tag_name")
        .and_then(|t| t.as_str())
        .ok_or_else(|| anyhow!("releases/latest had no tag_name"))?;
    Ok(tag.to_string())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else if ty.is_file() {
            let mut from = fs::File::open(entry.path())?;
            let mut bytes = Vec::new();
            from.read_to_end(&mut bytes)?;
            fs::write(dst.join(entry.file_name()), bytes)?;
        }
    }
    Ok(())
}
