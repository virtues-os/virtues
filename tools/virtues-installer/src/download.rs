//! Binary tarball download + SHA256 verify + extract → atomic release slot.
//!
//! Resolves "latest" via the GitHub Releases API when no version is pinned.
//! SHA-verifies the tarball against the sidecar `.sha256` (CI uploads both).
//! Extracts to a tempdir, stages the WHOLE release into
//! `$INSTALL_PREFIX/share/virtues/releases/<slot>/`, flips the `current`
//! symlink, and maintains the stable routing links every well-known path
//! resolves through (see virtues-core `cli/slots.rs` for the layout). The
//! installer is the ONLY creator of this layout; `virtues upgrade` requires
//! it and `virtues rollback` flips within it.

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

    // SHA verification. This tarball is extracted and installed as root, so it
    // is the MOST privileged artifact we fetch — it gets the same hard-fail the
    // model downloader below already uses, for the same reason: the release is
    // ours, so a missing sidecar is a packaging bug, not an optional nicety.
    //
    // This previously warned and continued when the sidecar fetch failed, which
    // meant anything that could suppress that one request (a captive portal, a
    // CDN edge 404, or an attacker serving a swapped tarball while dropping the
    // sidecar) silently downgraded verification to "HTTPS only" — the weakest
    // guarantee of the three download paths in this file, on the one artifact
    // that runs as root.
    let expected = client
        .get(&sha_url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .with_context(|| format!("GET {sha_url} — release must ship a .sha256 sidecar"))?
        .text()
        .await?;
    let expected = expected
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("malformed sha256 sidecar"))?;
    let actual = sha256_hex(&bytes);
    if !expected.eq_ignore_ascii_case(&actual) {
        return Err(anyhow!(
            "SHA256 mismatch on {tar_name} — refusing to install"
        ));
    }
    ui::ok("SHA256 verified");

    // Extract.
    let mut cmd = Command::new("tar");
    cmd.args(["-xzf", tar_path.to_str().unwrap(), "-C", tmpdir.path().to_str().unwrap()]);
    run_step(&format!("Extract {tar_name}"), cmd).await?;

    // ── Stage the whole release into its slot ──────────────────────────
    // Slot id comes from the tarball's BUILD.json (<version>-<sha7>); older
    // tarballs without one get a timestamp suffix.
    let slot_id = read_slot_id(tmpdir.path(), &version);
    let share = cfg.share_virtues_dir();
    let releases = share.join("releases");
    let slot = releases.join(&slot_id);
    fs::create_dir_all(&releases).with_context(|| format!("creating {}", releases.display()))?;
    let _ = fs::remove_dir_all(&slot); // re-install of the same build starts clean

    fs::create_dir_all(&slot)?;
    // Binaries — exec bits set; only `virtues` is mandatory.
    install_executable(&tmpdir.path().join("virtues"), &slot.join("virtues"))?;
    let has_llama = tmpdir.path().join("llama-server").is_file();
    if has_llama {
        install_executable(&tmpdir.path().join("llama-server"), &slot.join("llama-server"))?;
    } else {
        ui::warn("llama-server not in tarball — inference sidecars unavailable from this release");
    }
    let has_qnnd = tmpdir.path().join("virtues-qnnd").is_file();
    if has_qnnd {
        install_executable(&tmpdir.path().join("virtues-qnnd"), &slot.join("virtues-qnnd"))?;
    }
    if tmpdir.path().join("BUILD.json").is_file() {
        let _ = fs::copy(tmpdir.path().join("BUILD.json"), slot.join("BUILD.json"));
    }
    // Directories. Each is staged under a canonical SLOT name from whichever
    // tarball dir exists — the actions→applets rename changed the tarball dir
    // names (applets/ + applets-bin/), and an installer that only knew the old
    // names would stage NOTHING from a renamed tarball, leaving a box with no
    // cron actions at all. Prefer the new name, fall back to the legacy one.
    let staged_bins;
    {
        // (canonical slot name, tarball candidate names in preference order, exec?)
        let dirs: [(&str, &[&str], bool); 3] = [
            ("web", &["web"], false),
            ("applets", &["applets", "actions"], false),
            ("applets-bin", &["applets-bin", "actions-bin"], true),
        ];
        let mut applets_bin_staged = false;
        for (canonical, candidates, is_exec) in dirs {
            let src = candidates
                .iter()
                .map(|c| tmpdir.path().join(c))
                .find(|p| p.is_dir());
            match src {
                Some(src) if is_exec => {
                    // Compiled action executables need their exec bits.
                    let dst = slot.join(canonical);
                    fs::create_dir_all(&dst)?;
                    for entry in fs::read_dir(&src)? {
                        let entry = entry?;
                        if entry.file_type()?.is_file() {
                            install_executable(&entry.path(), &dst.join(entry.file_name()))?;
                        }
                    }
                    applets_bin_staged = true;
                }
                Some(src) => copy_dir_all(&src, &slot.join(canonical))?,
                None => ui::warn(&format!("{canonical}/ not in tarball — skipped")),
            }
        }
        staged_bins = applets_bin_staged;
    }
    ui::ok(&format!("Staged release slot {slot_id}"));

    // ── Activate: flip `current`, maintain the routing symlinks ────────────
    // Every well-known path is a symlink resolving THROUGH `current`, so env
    // files and systemd units never change across releases; a flip moves
    // binary + web + actions together. force_symlink replaces whatever is
    // there — including the pre-slot real files/dirs of an older install
    // (this is the one-time layout adoption; unlink works on a running
    // binary's path, the process keeps its inode until restart).
    atomic_flip(&share, &slot)?;
    let current = share.join("current");
    force_symlink(&current.join("web"), &share.join("web"))?;
    // Applet dir routing under BOTH the new and legacy well-known names, so the
    // runtime resolves it whether the box env points at VIRTUES_APPLETS_DIR
    // (share/virtues/applets) or the legacy VIRTUES_ACTIONS_DIR
    // (share/virtues/actions) — both land on the slot's `applets/`.
    force_symlink(&current.join("applets"), &share.join("applets"))?;
    force_symlink(&current.join("applets"), &share.join("actions"))?;
    force_symlink(&current.join("virtues"), &cfg.binary_path())?;
    if has_llama {
        force_symlink(&current.join("llama-server"), &cfg.llama_binary_path())?;
    }
    if has_qnnd {
        force_symlink(&current.join("virtues-qnnd"), &cfg.qnnd_binary_path())?;
    }
    if staged_bins {
        // The bin dir is a single filesystem path (libexec/virtues) regardless
        // of the env var name, so one symlink serves both schemes.
        force_symlink(&current.join("applets-bin"), &cfg.applets_bin_dir())?;
    }
    ui::ok(&format!("Activated {slot_id} (current → releases/{slot_id})"));

    // Keep current + one previous; prune the rest.
    prune_slots(&share, 1);

    cfg.pinned_version = Some(version);
    Ok(())
}

/// Download a GGUF from the models release into the models dir, with a
/// progress bar (these are 0.6–1.2 GB) and mandatory SHA256 sidecar
/// verification. Files already on disk are skipped: they were verified
/// when fetched and are immutable afterwards — model updates ship under
/// NEW file names, never in place, which is what makes the skip safe.
pub async fn fetch_model(cfg: &InstallConfig, name: &str) -> Result<()> {
    let dest = cfg.models_dir().join(name);
    fetch_asset(cfg, name, dest).await
}

/// Download release asset `name` (from the models bucket) to an arbitrary
/// `dest`, SHA256-verified, skipping if already present. `fetch_model` is the
/// common case (into the GGUF models dir); the QNN path uses this directly to
/// place `.bin`s + tokenizers under the QNN models dir with a nested layout.
pub async fn fetch_asset(cfg: &InstallConfig, name: &str, dest: std::path::PathBuf) -> Result<()> {
    if dest.is_file() && fs::metadata(&dest).map(|m| m.len() > 0).unwrap_or(false) {
        ui::skip(&format!("Model already present: {name}"));
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
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

/// Slot directory name for this tarball: `<version>-<sha7>` from BUILD.json,
/// else a timestamp suffix (older tarballs).
fn read_slot_id(tmpdir: &Path, version: &str) -> String {
    let sha7 = fs::read(tmpdir.join("BUILD.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v.get("sha").and_then(|s| s.as_str()).map(|s| s[..s.len().min(7)].to_string()));
    match sha7 {
        Some(sha) => format!("{version}-{sha}"),
        None => format!("{version}-{}", chrono_free_timestamp()),
    }
}

/// UTC timestamp without pulling chrono into the installer: seconds since
/// epoch is unique enough for a slot suffix.
fn chrono_free_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

/// Atomically point `share/current` at `slot` (tmp symlink + rename — rename
/// on one filesystem is atomic, so `current` always resolves to a complete
/// release).
fn atomic_flip(share: &Path, slot: &Path) -> Result<()> {
    let tmp = share.join(".current.tmp");
    let _ = fs::remove_file(&tmp);
    std::os::unix::fs::symlink(slot, &tmp).with_context(|| format!("symlink {}", tmp.display()))?;
    fs::rename(&tmp, share.join("current")).context("flipping current")?;
    Ok(())
}

/// Replace whatever exists at `link` (file, dir, or old symlink) with a
/// symlink to `target`. The dir case is the one-time adoption of a pre-slot
/// install, whose web/actions were real directories.
///
/// That dir case used to be `remove_dir_all` — a recursive delete of whatever
/// the pre-slot install had there. Correct for a tree the installer owns
/// outright, catastrophic once anything else lives inside it: chat-authored
/// applets land in `<applets>/user/`, so adopting the slot layout would have
/// silently deleted every applet the user had written, along with its face
/// HTML and schema. The result discarded (`let _ =`), so a partial failure
/// wasn't even reported.
///
/// Now it renames the directory aside and says so. The upgrade still proceeds,
/// nothing is lost, and the operator has a recoverable copy. That does not
/// make it correct to keep user state under a versioned prefix — it makes the
/// blast radius survivable while that gets fixed properly.
fn force_symlink(target: &Path, link: &Path) -> Result<()> {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)?;
    }
    if link.is_symlink() || link.is_file() {
        fs::remove_file(link)
            .with_context(|| format!("removing {}", link.display()))?;
    } else if link.is_dir() {
        let aside = preserved_path(link);
        fs::rename(link, &aside).with_context(|| {
            format!(
                "preserving existing directory {} as {}",
                link.display(),
                aside.display()
            )
        })?;
        ui::warn(&format!(
            "{} was a real directory — moved to {} (contents preserved)",
            link.display(),
            aside.display()
        ));
    }
    std::os::unix::fs::symlink(target, link)
        .with_context(|| format!("symlink {} -> {}", link.display(), target.display()))?;
    Ok(())
}

/// A non-colliding sibling path to park a displaced directory at. Suffixes
/// with a counter rather than a timestamp so repeated runs stay deterministic
/// and an operator can see the order things were displaced in.
fn preserved_path(link: &Path) -> std::path::PathBuf {
    let base = format!("{}.preserved", link.display());
    let first = std::path::PathBuf::from(&base);
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| std::path::PathBuf::from(format!("{base}.{n}")))
        .find(|p| !p.exists())
        .expect("an unused .preserved suffix exists")
}

/// Delete release slots beyond the newest `keep` (never the one `current`
/// points at). Best-effort.
fn prune_slots(share: &Path, keep: usize) {
    let current = fs::read_link(share.join("current"))
        .ok()
        .and_then(|t| if t.is_absolute() { t.canonicalize().ok() } else { share.join(t).canonicalize().ok() });
    let mut slots: Vec<(std::time::SystemTime, std::path::PathBuf)> =
        fs::read_dir(share.join("releases"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| {
                Some((e.metadata().ok()?.modified().ok()?, e.path().canonicalize().ok()?))
            })
            .collect();
    slots.sort_by(|a, b| b.0.cmp(&a.0));
    let mut kept = 0usize;
    for (_, slot) in slots {
        let is_current = current.as_ref().map(|c| c == &slot).unwrap_or(false);
        if is_current || kept < keep {
            if !is_current {
                kept += 1;
            }
            continue;
        }
        let _ = fs::remove_dir_all(&slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The regression that matters: adopting the slot layout must not destroy
    /// a pre-slot `applets/` directory, because chat-authored applets live in
    /// `applets/user/`. Before the fix this was `remove_dir_all` and the file
    /// below was simply gone.
    #[test]
    fn force_symlink_preserves_an_existing_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let link = tmp.path().join("applets");
        let authored = link.join("user").join("wife_week");
        fs::create_dir_all(&authored).unwrap();
        fs::write(authored.join("manifest.toml"), b"name = \"Wife Week\"").unwrap();

        let target = tmp.path().join("releases/slot-1/applets");
        fs::create_dir_all(&target).unwrap();

        force_symlink(&target, &link).unwrap();

        assert!(link.is_symlink(), "link should now be a symlink");
        let preserved = tmp.path().join("applets.preserved");
        assert_eq!(
            fs::read_to_string(preserved.join("user/wife_week/manifest.toml")).unwrap(),
            "name = \"Wife Week\"",
            "authored applet must survive the flip"
        );
    }

    /// A second adoption must not clobber the first rescue.
    #[test]
    fn preserved_path_does_not_collide() {
        let tmp = tempfile::tempdir().unwrap();
        let link = tmp.path().join("applets");

        fs::create_dir_all(&link).unwrap();
        assert_eq!(preserved_path(&link), tmp.path().join("applets.preserved"));

        fs::create_dir_all(tmp.path().join("applets.preserved")).unwrap();
        assert_eq!(preserved_path(&link), tmp.path().join("applets.preserved.2"));
    }

    /// Replacing an existing symlink stays a plain swap — no stray
    /// `.preserved` clutter on every routine upgrade.
    #[test]
    fn force_symlink_replaces_a_symlink_without_preserving() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("old");
        let new = tmp.path().join("new");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        let link = tmp.path().join("applets");

        force_symlink(&old, &link).unwrap();
        force_symlink(&new, &link).unwrap();

        assert_eq!(fs::read_link(&link).unwrap(), new);
        assert!(!tmp.path().join("applets.preserved").exists());
    }
}
