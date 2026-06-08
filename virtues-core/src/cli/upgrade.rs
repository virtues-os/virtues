//! `virtues upgrade` — self-update from a GitHub Release.
//!
//! Stops the service, swaps `/usr/local/bin/virtues` with the downloaded
//! binary (keeping one `.bak` for rollback), applies any pending DB
//! migrations under the new binary, restarts the service. On any failure
//! mid-swap the rollback command is printed verbatim so a tired operator at
//! 2 AM can paste it without thinking.
//!
//! Strictly opt-in. No background auto-upgrade — that lands when the
//! upgrade path has been battle-tested.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

const BINARY_PATH: &str = "/usr/local/bin/virtues";
const RELEASE_REPO: &str = "virtues-os/virtues";
const USER_AGENT: &str = concat!("virtues-upgrade/", env!("CARGO_PKG_VERSION"));

pub async fn run(check: bool, version: Option<String>) -> Result<(), crate::Error> {
    let target_tag = match version {
        Some(v) => v,
        None => fetch_latest_tag().await?,
    };
    let current = env!("CARGO_PKG_VERSION");
    let target = target_tag.trim_start_matches('v');

    println!("→ current version: {current}");
    println!("→ target  version: {target}");

    if target == current {
        println!("✓ already on {current}; nothing to do.");
        return Ok(());
    }
    if check {
        println!("→ an upgrade is available (run `virtues upgrade` to apply).");
        return Ok(());
    }

    // Only root can write to /usr/local/bin or restart the service.
    if !running_as_root() {
        return Err(crate::Error::Other(
            "virtues upgrade must run as root (try with sudo)".to_string(),
        ));
    }

    let arch = host_arch();
    let asset_name = format!("virtues-{target_tag}-{arch}-linux.tar.gz");
    let sha_name = format!("{asset_name}.sha256");
    let base = format!("https://github.com/{RELEASE_REPO}/releases/download/{target_tag}");
    let asset_url = format!("{base}/{asset_name}");
    let sha_url = format!("{base}/{sha_name}");

    let work = mkstage()?;
    let work_path: &Path = work.as_ref();
    let asset_path = work_path.join(&asset_name);

    println!("→ downloading {asset_url}…");
    download(&asset_url, &asset_path).await?;

    println!("→ verifying sha256 from {sha_url}…");
    let expected = fetch_text(&sha_url).await?;
    let expected_hex = expected.split_whitespace().next().unwrap_or("").to_string();
    verify_sha(&asset_path, &expected_hex)?;

    println!("→ extracting…");
    let new_binary = extract_binary(&asset_path, work_path)?;

    println!("→ stopping virtues.service…");
    let _ = Command::new("systemctl").arg("stop").arg("virtues").status();

    let bak = format!("{BINARY_PATH}.bak");
    if Path::new(BINARY_PATH).exists() {
        println!("→ saving rollback copy at {bak}…");
        let _ = fs::remove_file(&bak);
        fs::rename(BINARY_PATH, &bak)
            .map_err(|e| crate::Error::Other(format!("rename to {bak}: {e}")))?;
    }

    if let Err(e) = swap_binary(&new_binary, Path::new(BINARY_PATH)) {
        print_rollback_hint(&bak);
        return Err(e);
    }

    println!("→ running migrations under new binary…");
    let migrate = Command::new(BINARY_PATH).arg("migrate").status();
    match migrate {
        Ok(s) if s.success() => {}
        Ok(s) => {
            print_rollback_hint(&bak);
            return Err(crate::Error::Other(format!(
                "new binary's `migrate` exited {s}"
            )));
        }
        Err(e) => {
            print_rollback_hint(&bak);
            return Err(crate::Error::Other(format!("invoke migrate: {e}")));
        }
    }

    println!("→ starting virtues.service…");
    let status = Command::new("systemctl").arg("start").arg("virtues").status();
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            print_rollback_hint(&bak);
            return Err(crate::Error::Other(format!(
                "systemctl start exited {s}"
            )));
        }
        Err(e) => {
            print_rollback_hint(&bak);
            return Err(crate::Error::Other(format!("invoke systemctl: {e}")));
        }
    }

    println!();
    println!("✓ upgraded to {target_tag}. Rollback copy kept at {bak}.");
    Ok(())
}

fn running_as_root() -> bool {
    // Avoids pulling in `nix` or `libc` crates — we read EUID from the
    // `/proc/self/status` file. Falls back to false (refuse to run) if
    // we can't tell.
    let Ok(s) = fs::read_to_string("/proc/self/status") else {
        return false;
    };
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            // Format: `Uid:\t<real>\t<effective>\t<saved>\t<fs>`
            let mut parts = rest.split_whitespace();
            let _real = parts.next();
            if let Some(euid) = parts.next() {
                return euid == "0";
            }
        }
    }
    false
}

fn host_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    }
}

async fn fetch_latest_tag() -> Result<String, crate::Error> {
    let url = format!("https://api.github.com/repos/{RELEASE_REPO}/releases/latest");
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| crate::Error::Other(format!("build client: {e}")))?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| crate::Error::Other(format!("github api: {e}")))?
        .error_for_status()
        .map_err(|e| crate::Error::Other(format!("github api: {e}")))?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| crate::Error::Other(format!("parse github json: {e}")))?;
    body.get("tag_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| crate::Error::Other("no tag_name in github response".to_string()))
}

async fn fetch_text(url: &str) -> Result<String, crate::Error> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| crate::Error::Other(format!("build client: {e}")))?;
    let body = client
        .get(url)
        .send()
        .await
        .map_err(|e| crate::Error::Other(format!("GET {url}: {e}")))?
        .error_for_status()
        .map_err(|e| crate::Error::Other(format!("GET {url}: {e}")))?
        .text()
        .await
        .map_err(|e| crate::Error::Other(format!("read body: {e}")))?;
    Ok(body)
}

async fn download(url: &str, dest: &Path) -> Result<(), crate::Error> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| crate::Error::Other(format!("build client: {e}")))?;
    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| crate::Error::Other(format!("GET {url}: {e}")))?
        .error_for_status()
        .map_err(|e| crate::Error::Other(format!("GET {url}: {e}")))?;
    let mut file = File::create(dest)
        .map_err(|e| crate::Error::Other(format!("create {}: {e}", dest.display())))?;
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| crate::Error::Other(format!("download chunk: {e}")))?
    {
        file.write_all(&chunk)
            .map_err(|e| crate::Error::Other(format!("write chunk: {e}")))?;
    }
    file.flush()
        .map_err(|e| crate::Error::Other(format!("flush {}: {e}", dest.display())))?;
    Ok(())
}

fn verify_sha(path: &Path, expected_hex: &str) -> Result<(), crate::Error> {
    let mut f = File::open(path)
        .map_err(|e| crate::Error::Other(format!("open {}: {e}", path.display())))?;
    let mut buf = vec![];
    f.read_to_end(&mut buf)
        .map_err(|e| crate::Error::Other(format!("read for sha: {e}")))?;
    let mut h = Sha256::new();
    h.update(&buf);
    let got = format!("{:x}", h.finalize());
    if got != expected_hex.to_lowercase() {
        return Err(crate::Error::Other(format!(
            "sha256 mismatch (expected {expected_hex}, got {got}) — refusing to install"
        )));
    }
    Ok(())
}

fn extract_binary(tarball: &Path, work: &Path) -> Result<PathBuf, crate::Error> {
    let file = File::open(tarball)
        .map_err(|e| crate::Error::Other(format!("open tarball: {e}")))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut t = tar::Archive::new(gz);
    let extract_dir = work.join("extracted");
    fs::create_dir_all(&extract_dir)
        .map_err(|e| crate::Error::Other(format!("mkdir extract: {e}")))?;
    t.unpack(&extract_dir)
        .map_err(|e| crate::Error::Other(format!("tar unpack: {e}")))?;
    // Find `virtues` somewhere in the extracted tree.
    let bin = find_named(&extract_dir, "virtues")?;
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755))
        .map_err(|e| crate::Error::Other(format!("chmod: {e}")))?;
    Ok(bin)
}

fn find_named(dir: &Path, name: &str) -> Result<PathBuf, crate::Error> {
    for entry in fs::read_dir(dir)
        .map_err(|e| crate::Error::Other(format!("read {}: {e}", dir.display())))?
    {
        let entry = entry.map_err(|e| crate::Error::Other(format!("dir entry: {e}")))?;
        let path = entry.path();
        if path.is_dir() {
            if let Ok(found) = find_named(&path, name) {
                return Ok(found);
            }
        } else if path.file_name().and_then(|s| s.to_str()) == Some(name) {
            return Ok(path);
        }
    }
    Err(crate::Error::Other(format!(
        "{name} not found inside the release tarball"
    )))
}

fn swap_binary(new_binary: &Path, dest: &Path) -> Result<(), crate::Error> {
    // `fs::rename` is atomic when src + dst share a filesystem. The
    // extracted binary may not — fall back to copy + remove.
    if let Err(_) = fs::rename(new_binary, dest) {
        fs::copy(new_binary, dest)
            .map_err(|e| crate::Error::Other(format!("copy new binary to {}: {e}", dest.display())))?;
    }
    fs::set_permissions(dest, fs::Permissions::from_mode(0o755))
        .map_err(|e| crate::Error::Other(format!("chmod {}: {e}", dest.display())))?;
    Ok(())
}

fn print_rollback_hint(bak: &str) {
    eprintln!();
    eprintln!("⚠  upgrade failed mid-swap. Roll back with:");
    eprintln!();
    eprintln!("      sudo systemctl stop virtues");
    eprintln!("      sudo mv {bak} {BINARY_PATH}");
    eprintln!("      sudo systemctl start virtues");
    eprintln!();
}

struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
impl AsRef<Path> for Stage {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

fn mkstage() -> Result<Stage, crate::Error> {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%f");
    let path = std::env::temp_dir().join(format!(".virtues-upgrade-{stamp}"));
    fs::create_dir_all(&path)
        .map_err(|e| crate::Error::Other(format!("staging dir: {e}")))?;
    Ok(Stage(path))
}
