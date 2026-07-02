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

use semver::Version;

use sha2::{Digest, Sha256};

const BINARY_PATH: &str = "/usr/local/bin/virtues";
const RELEASE_REPO: &str = "virtues-os/virtues";
const USER_AGENT: &str = concat!("virtues-upgrade/", env!("CARGO_PKG_VERSION"));

pub async fn run(
    check: bool,
    version: Option<String>,
    pre: bool,
    force: bool,
) -> Result<(), crate::Error> {
    let target_tag = match version {
        Some(v) => v,
        None if pre => fetch_latest_prerelease().await?,
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

    // Downgrade guard. A stale or tampered "latest", or an explicit older
    // `--version`, could otherwise roll the box back to a known-vulnerable
    // build. Skip when:
    //  · `--pre` — the prerelease channel is an explicit opt-in, and staging
    //    builds report the bare `CARGO_PKG_VERSION` (no `-staging.N` suffix),
    //    so semver would read every prerelease as "older" than stable.
    //  · `--force` — operator override.
    // Unparseable versions (dev builds) skip the check rather than block.
    if !pre && !force {
        if let (Ok(cur), Ok(tgt)) = (Version::parse(current), Version::parse(target)) {
            if tgt < cur {
                return Err(crate::Error::Other(format!(
                    "target {target} is older than current {current} — refusing to \
                     downgrade (pass --force to override)"
                )));
            }
        }
    }

    // Only root can write to /usr/local/bin or restart the service.
    if !running_as_root() {
        return Err(crate::Error::Other(
            "virtues upgrade must run as root (try with sudo)".to_string(),
        ));
    }

    // Single-flight: two concurrent upgrades interleaving binary swaps would
    // corrupt the install. Held until `run` returns (RAII). A crashed prior run
    // leaves a stale lock, which we reclaim once its PID is gone.
    let _lock = acquire_lock()?;

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
    // Unpacks the whole tarball under `work_path/extracted/` and returns the
    // `virtues` binary; the sibling artifacts (sidecars, web/, actions/,
    // actions-bin/) are pulled out of the same tree below.
    let new_binary = extract_binary(&asset_path, work_path)?;
    let extracted = work_path.join("extracted");

    // Which artifacts this tarball actually carries. Older releases ship a
    // subset — every one is optional and best-effort so an upgrade from any
    // historical tarball still swaps whatever it can. `virtues` itself is the
    // only mandatory member (already located by `extract_binary`).
    let new_llama = find_named(&extracted, "llama-server").ok();
    let web_src = find_dir_named(&extracted, "web").ok();
    let actions_src = find_dir_named(&extracted, "actions").ok();
    let actions_bin_src = find_dir_named(&extracted, "actions-bin").ok();

    let dirs = InstallDirs::resolve();

    // Relay migration: an upgrade from a WireGuard-era release has an orphaned
    // `virtues-wireguard.service` (the box now reaches via the relay). Stop +
    // disable it so it doesn't idle forever reconciling a wg0 nothing creates.
    // Best-effort; a no-op on boxes that never had it.
    disable_legacy_wireguard();

    // ── Stop affected services ──────────────────────────────────────────────
    // The main app always. The inference sidecars only when we're actually
    // replacing their binaries — restarting the sidecars reloads multi-GB GGUFs
    // (slow, esp. on Jetson), so don't pay that cost for a binary-only app
    // upgrade. A running process holds the old inode until restarted, so the
    // stop→swap→start ordering is what makes the new bytes take effect.
    println!("→ stopping virtues.service…");
    service_stop("virtues");
    if new_llama.is_some() {
        service_stop("virtues-embed");
        service_stop("virtues-rerank");
    }

    // Bring the sidecars we stopped back up. Called on every abort path below
    // so a failed upgrade doesn't leave the box with dead search — without this,
    // stopping `virtues-embed`/`-rerank` early then returning on a migrate or
    // start failure left them down until a manual `systemctl start`. Idempotent
    // and best-effort.
    let revive_llama = new_llama.is_some();
    let revive_sidecars = move || {
        if revive_llama {
            let _ = service_start("virtues-embed");
            let _ = service_start("virtues-rerank");
        }
    };

    // ── Swap the main binary (mandatory; keep .bak for rollback) ────────────
    let bak = match swap_with_bak(&new_binary, Path::new(BINARY_PATH)) {
        Ok(b) => b.unwrap_or_else(|| format!("{BINARY_PATH}.bak")),
        Err(e) => {
            // Nothing irreversible happened yet (swap_with_bak restores the
            // original on failure), but be explicit about the manual path.
            revive_sidecars();
            print_rollback_hint(&format!("{BINARY_PATH}.bak"));
            return Err(e);
        }
    };
    println!("→ swapped {BINARY_PATH} (rollback copy at {bak})");

    // ── Swap the sidecar binaries (best-effort; they sit next to `virtues`) ─
    // A failed sidecar swap must not abort an otherwise-good app upgrade —
    // swap_with_bak restores the prior binary on failure, so the box keeps a
    // working sidecar. We warn and continue.
    if let Some(src) = &new_llama {
        let dst = dirs.bin_dir.join("llama-server");
        match swap_with_bak(src, &dst) {
            Ok(_) => println!("→ swapped {}", dst.display()),
            Err(e) => eprintln!("  ⚠ llama-server not swapped ({e}); prior binary kept"),
        }
        // Every tarball ships the CPU llama-server. On Jetson the installer
        // then swaps in the CUDA (sm_87) build from a separate asset — replays
        // that here, else the upgrade would silently downgrade inference from
        // GPU to CPU. Best-effort: the CPU build already in place is a valid
        // fallback if the CUDA asset is missing or the fetch fails.
        if is_jetson() {
            match fetch_jetson_cuda_llama(&base, &target_tag, &dst).await {
                Ok(true) => println!("→ swapped in CUDA llama-server (Jetson, sm_87)"),
                Ok(false) => println!("  · no CUDA llama-server on this release — sidecars run on CPU"),
                Err(e) => eprintln!("  ⚠ CUDA llama-server fetch failed ({e}); sidecars run on CPU"),
            }
        }
    }

    // ── Refresh the shipped directories (best-effort, atomic per-dir) ───────
    // web/: a binary-only swap leaves the browser served a stale SvelteKit
    // build. actions/: manifests + UI the server globs at runtime.
    // actions-bin/: the compiled per-source action executables the box forks
    // by name — the one whose staleness caused the rustls "No provider set"
    // panic that motivated making this path complete.
    if let Some(src) = &web_src {
        refresh_named("web UI", src, &dirs.web);
    }
    if let Some(src) = &actions_src {
        refresh_named("actions", src, &dirs.actions);
    }
    if let Some(src) = &actions_bin_src {
        refresh_named("action binaries", src, &dirs.actions_bin);
    }

    println!("→ running migrations under new binary…");
    let migrate = Command::new(BINARY_PATH).arg("migrate").status();
    match migrate {
        Ok(s) if s.success() => {}
        Ok(s) => {
            revive_sidecars();
            print_rollback_hint(&bak);
            return Err(crate::Error::Other(format!(
                "new binary's `migrate` exited {s}"
            )));
        }
        Err(e) => {
            revive_sidecars();
            print_rollback_hint(&bak);
            return Err(crate::Error::Other(format!("invoke migrate: {e}")));
        }
    }

    // The box keeps its default `virtues.local` name (no in-app rename step),
    // so the old hostname-rename sudoers grant is dead. Remove it from boxes
    // that were installed when the rename step still existed. Best-effort.
    remove_stale_setup_sudoers();

    // ── Start services back up ──────────────────────────────────────────────
    // The main app is the one whose failure aborts (and prints the rollback
    // hint); the sidecars are best-effort restarts that won't undo a good app
    // upgrade — but a sidecar that won't start means degraded search, so warn
    // loudly.
    println!("→ starting virtues.service…");
    match service_start("virtues") {
        Ok(true) => {}
        Ok(false) => {
            revive_sidecars();
            print_rollback_hint(&bak);
            return Err(crate::Error::Other(
                "systemctl start virtues failed".to_string(),
            ));
        }
        Err(e) => {
            revive_sidecars();
            print_rollback_hint(&bak);
            return Err(crate::Error::Other(format!("invoke systemctl: {e}")));
        }
    }
    if new_llama.is_some() {
        for unit in ["virtues-embed", "virtues-rerank"] {
            if let Ok(false) | Err(_) = service_start(unit) {
                eprintln!("  ⚠ {unit} did not start — search/embeddings degraded; check `systemctl status {unit}`");
            }
        }
    }

    // Model-set drift check. `virtues upgrade` swaps binaries but does NOT
    // fetch model GGUFs or rewrite the sidecar `-m`/pooling in the unit files —
    // those are provisioned by the installer. So a release that changes the
    // model set (e.g. bge → EmbeddingGemma) leaves the box serving the OLD
    // models against a runtime that expects the new ones (embeds get rejected
    // at the native-dim check). Detect it and tell the user exactly how to
    // reconcile, instead of degrading search silently.
    //
    // Only meaningful on a box that actually runs the local AI sidecars — a
    // DIY/AI-less box (no embed unit) legitimately has no GGUFs, so skip it
    // there rather than nag on every upgrade. And resolve the models dir from
    // the box env file first: `sudo virtues upgrade` doesn't inherit the
    // systemd EnvironmentFile, so without this a custom DATA_DIR box would
    // probe the wrong (default) path and always report "missing".
    if Path::new("/etc/systemd/system/virtues-embed.service").exists() {
        if let Some(dir) = read_box_env_var("VIRTUES_MODELS_DIR") {
            std::env::set_var("VIRTUES_MODELS_DIR", dir);
        }
        let report = crate::inference_report::resolution_report();
        let missing = report.missing();
        if !missing.is_empty() {
            eprintln!();
            eprintln!("  ⚠ this release expects models not present on the box:");
            for f in &missing {
                eprintln!("      · {f}");
            }
            eprintln!("    `virtues upgrade` doesn't migrate the model set — the sidecars are");
            eprintln!("    still on the old GGUFs, so search/embeddings will fail until you");
            eprintln!("    re-run the installer (fetches the new models + rewrites the units):");
            eprintln!();
            eprintln!("      curl -sSL https://virtues.com/sh | sudo VIRTUES_VERSION={target_tag} sh");
            eprintln!();
        }
    }

    println!();
    println!("✓ upgraded to {target_tag}. Rollback copy kept at {bak}.");
    Ok(())
}

/// Read a single `KEY=value` from the box env file (`/var/lib/virtues/virtues.env`).
/// `sudo virtues upgrade` doesn't inherit the systemd EnvironmentFile, so this is
/// how the upgrade path recovers box-specific settings (e.g. a custom models dir).
fn read_box_env_var(key: &str) -> Option<String> {
    let contents = fs::read_to_string("/var/lib/virtues/virtues.env").ok()?;
    for line in contents.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key).and_then(|r| r.strip_prefix('=')) {
            let val = rest.trim().trim_matches('"');
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

/// Resolved on-box install destinations. The binaries live next to
/// `/usr/local/bin/virtues`; the shipped dirs follow the env vars the
/// installer writes into the box env file, falling back to the same
/// `/usr/local` well-known defaults `virtues-installer` uses so a manual
/// `sudo virtues upgrade` (which doesn't load that env file) still targets the
/// right paths. Mirrors `InstallConfig` in the installer crate.
struct InstallDirs {
    /// Directory holding `virtues`, `virtues-wireguard`, `llama-server`.
    bin_dir: PathBuf,
    web: PathBuf,
    actions: PathBuf,
    actions_bin: PathBuf,
}

impl InstallDirs {
    fn resolve() -> Self {
        let bin_dir = Path::new(BINARY_PATH)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/usr/local/bin"));
        let env_dir = |var: &str, default: &str| {
            std::env::var(var)
                .ok()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(default))
        };
        Self {
            bin_dir,
            web: env_dir("STATIC_DIR", "/usr/local/share/virtues/web"),
            actions: env_dir("VIRTUES_ACTIONS_DIR", "/usr/local/share/virtues/actions"),
            actions_bin: env_dir("VIRTUES_ACTIONS_BIN_DIR", "/usr/local/libexec/virtues"),
        }
    }
}

/// Replace `dest` with `new`, keeping the prior file as `dest.bak` for
/// rollback. Restores the original if the swap fails partway, so a caller that
/// treats a failure as non-fatal still has a working binary in place. Returns
/// the `.bak` path when one was created (`None` for a fresh install).
fn swap_with_bak(new: &Path, dest: &Path) -> Result<Option<String>, crate::Error> {
    let bak = format!("{}.bak", dest.display());
    let had_prior = dest.exists();
    if had_prior {
        let _ = fs::remove_file(&bak);
        fs::rename(dest, &bak)
            .map_err(|e| crate::Error::Other(format!("rename {} to {bak}: {e}", dest.display())))?;
    }
    if let Err(e) = swap_binary(new, dest) {
        if had_prior {
            let _ = fs::rename(&bak, dest); // put the working binary back
        }
        return Err(e);
    }
    Ok(had_prior.then_some(bak))
}

/// Atomically replace a shipped directory, logging a uniform success/skip line.
/// Best-effort: a copy failure leaves the prior dir untouched (install_web
/// stages into a sibling and only swaps on success) and never aborts the run.
fn refresh_named(label: &str, src: &Path, dst: &Path) {
    match install_web(src, dst) {
        Ok(()) => println!("→ refreshed {label} → {}", dst.display()),
        Err(e) => eprintln!("  ⚠ {label} not refreshed ({e}); prior copy still in effect"),
    }
}

/// `systemctl stop <unit>` — best-effort (a not-yet-running unit is fine).
fn service_stop(unit: &str) {
    let _ = Command::new("systemctl").arg("stop").arg(unit).status();
}

/// Relay migration: retire a leftover `virtues-wireguard.service` from a
/// WireGuard-era box. Stop + disable + remove the unit and binary so it doesn't
/// idle reconciling a wg0 that pairing no longer populates. All best-effort and
/// a no-op on boxes that never had WireGuard.
fn disable_legacy_wireguard() {
    const UNIT: &str = "virtues-wireguard.service";
    if !std::path::Path::new("/etc/systemd/system/virtues-wireguard.service").exists() {
        return;
    }
    println!("→ retiring legacy virtues-wireguard.service (relay model)…");
    let _ = Command::new("systemctl").arg("stop").arg(UNIT).status();
    let _ = Command::new("systemctl").arg("disable").arg(UNIT).status();
    let _ = std::fs::remove_file("/etc/systemd/system/virtues-wireguard.service");
    let _ = std::fs::remove_file("/usr/local/bin/virtues-wireguard");
    let _ = Command::new("systemctl").arg("daemon-reload").status();
}

/// `systemctl start <unit>` — `Ok(true)` on success, `Ok(false)` on non-zero
/// exit, `Err` if systemctl couldn't be invoked.
fn service_start(unit: &str) -> std::io::Result<bool> {
    Command::new("systemctl")
        .arg("start")
        .arg(unit)
        .status()
        .map(|s| s.success())
}

/// Exclusive process lock for the duration of an upgrade. Created with
/// `create_new` (atomic O_EXCL), removed on drop. Lives in `/run` (tmpfs →
/// auto-cleared on reboot) so a power-loss mid-upgrade never leaves a
/// permanently-stuck lock.
const LOCK_PATH: &str = "/run/virtues-upgrade.lock";

struct UpgradeLock;
impl Drop for UpgradeLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(LOCK_PATH);
    }
}

/// Acquire the single-flight lock, reclaiming a stale one left by a crashed
/// prior run (its recorded PID no longer exists). Returns an RAII guard whose
/// drop releases the lock.
fn acquire_lock() -> Result<UpgradeLock, crate::Error> {
    let path = Path::new(LOCK_PATH);
    loop {
        match fs::OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(mut f) => {
                let _ = write!(f, "{}", std::process::id());
                return Ok(UpgradeLock);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // A live holder keeps the lock; a dead one's PID is gone from
                // /proc, so we reclaim it and retry. An unreadable/corrupt lock
                // is treated as held (safer to refuse than to clobber).
                let holder_alive = fs::read_to_string(path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok())
                    .map(|pid| Path::new(&format!("/proc/{pid}")).exists())
                    .unwrap_or(true);
                if holder_alive {
                    return Err(crate::Error::Other(
                        "another `virtues upgrade` is already running".to_string(),
                    ));
                }
                let _ = fs::remove_file(path); // stale — reclaim and retry
            }
            Err(e) => {
                return Err(crate::Error::Other(format!("acquire upgrade lock: {e}")));
            }
        }
    }
}

/// L4T's marker file — present on every Jetson, absent everywhere else.
/// Mirrors `is_jetson()` in `virtues-installer`.
fn is_jetson() -> bool {
    Path::new("/etc/nv_tegra_release").exists()
}

/// Replace the CPU `llama-server` at `dest` with the Jetson CUDA (sm_87) build
/// attached to the same release. Returns `Ok(false)` when the asset isn't
/// published for this release (its CI job is allowed to fail); `Err` only on a
/// real fetch/verify failure. Mirrors the installer's `fetch_jetson_cuda_llama`,
/// including the mandatory SHA256 sidecar check (this binary runs as a daemon).
async fn fetch_jetson_cuda_llama(
    base: &str,
    tag: &str,
    dest: &Path,
) -> Result<bool, crate::Error> {
    let name = format!("llama-server-{tag}-aarch64-cuda-linux");
    let url = format!("{base}/{name}");

    let resp = build_client(false)?
        .get(&url)
        .send()
        .await
        .map_err(|e| crate::Error::Other(format!("GET {url}: {e}")))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(false);
    }
    let bytes = resp
        .error_for_status()
        .map_err(|e| crate::Error::Other(format!("GET {url}: {e}")))?
        .bytes()
        .await
        .map_err(|e| crate::Error::Other(format!("read {name}: {e}")))?;

    let expected = fetch_text(&format!("{url}.sha256")).await?;
    let expected = expected.split_whitespace().next().unwrap_or("");
    let got = {
        let mut h = Sha256::new();
        h.update(&bytes);
        format!("{:x}", h.finalize())
    };
    if !expected.eq_ignore_ascii_case(&got) {
        return Err(crate::Error::Other(format!(
            "sha256 mismatch on {name} (expected {expected}, got {got})"
        )));
    }

    // Stage next to the destination (same fs → atomic swap) then install.
    let staged = dest.with_extension("cuda-tmp");
    fs::write(&staged, &bytes)
        .map_err(|e| crate::Error::Other(format!("stage CUDA llama-server: {e}")))?;
    swap_binary(&staged, dest)?;
    Ok(true)
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

/// Remove the dead `/etc/sudoers.d/virtues-setup` grant from boxes installed
/// when the in-app "name your box" rename step still existed. The box now keeps
/// its default `virtues.local` name, so nothing shells out to `sudo hostnamectl`
/// anymore. Best-effort — a missing file is the common (clean) case.
fn remove_stale_setup_sudoers() {
    const PATH: &str = "/etc/sudoers.d/virtues-setup";
    if Path::new(PATH).exists() {
        match fs::remove_file(PATH) {
            Ok(()) => println!("→ removed dead box-rename sudoers rule"),
            Err(e) => eprintln!("→ note: couldn't remove stale {PATH} ({e})"),
        }
    }
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

/// Build the HTTP client used for all release fetches. When `force_ipv4`, bind
/// the local socket to an IPv4 address so the connection can't attempt IPv6.
///
/// Why: some networks (Tailscale-only, corporate Wi-Fi like WeWork) advertise
/// an IPv6 default route that black-holes — the default connect stalls on the
/// AAAA record and the update fails with "error sending request", even though
/// IPv4 egress works fine (curl -4 succeeds). A short connect timeout makes that
/// stall fail fast so we can retry over IPv4.
fn build_client(force_ipv4: bool) -> Result<reqwest::Client, crate::Error> {
    // Start from the shared rooted builder — `reqwest` is `rustls-tls-no-provider`,
    // so a bare client has no CA roots and every HTTPS request fails to send
    // (the bug this fixes). `base_builder` installs the provider + OS native
    // roots and is already IPv4-only (so the IPv6-black-hole case is moot).
    let mut b = crate::http_client::base_builder()
        .user_agent(USER_AGENT)
        .connect_timeout(std::time::Duration::from_secs(10));
    if force_ipv4 {
        b = b.local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
    }
    b.build()
        .map_err(|e| crate::Error::Other(format!("build client: {e}")))
}

/// GET a URL, falling back to IPv4 if the default attempt can't connect/times
/// out (the IPv6-black-hole symptom). Returns the response after `error_for_status`.
async fn send_get(url: &str) -> Result<reqwest::Response, crate::Error> {
    let resp = match build_client(false)?.get(url).send().await {
        Ok(r) => r,
        Err(e) if e.is_connect() || e.is_timeout() => {
            tracing::warn!("GET {url} failed ({e}); retrying over IPv4");
            build_client(true)?
                .get(url)
                .send()
                .await
                .map_err(|e| crate::Error::Other(format!("GET {url} (ipv4 retry): {e}")))?
        }
        Err(e) => return Err(crate::Error::Other(format!("GET {url}: {e}"))),
    };
    resp.error_for_status()
        .map_err(|e| crate::Error::Other(format!("GET {url}: {e}")))
}

/// This repo ships BOTH channels' releases in one list: the Linux box
/// (`edge`, `vX.Y.Z`) and the macOS app (`mac-edge`, `mac-vX.Y.Z`,
/// `mac-latest`). The Linux upgrader must ignore the macOS tags — otherwise
/// `--pre` picks the newest prerelease overall (often `mac-edge`) and then 404s
/// fetching a Linux asset named after a macOS tag.
fn is_linux_tag(tag: &str) -> bool {
    !tag.starts_with("mac-")
}

async fn list_releases() -> Result<Vec<serde_json::Value>, crate::Error> {
    let url = format!("https://api.github.com/repos/{RELEASE_REPO}/releases?per_page=30");
    let body: serde_json::Value = send_get(&url)
        .await
        .map_err(|e| crate::Error::Other(format!("github api: {e}")))?
        .json()
        .await
        .map_err(|e| crate::Error::Other(format!("parse github json: {e}")))?;
    body.as_array()
        .cloned()
        .ok_or_else(|| crate::Error::Other("github releases: expected an array".to_string()))
}

fn release_tag(r: &serde_json::Value) -> Option<&str> {
    r.get("tag_name").and_then(|v| v.as_str())
}
fn is_prerelease(r: &serde_json::Value) -> bool {
    r.get("prerelease").and_then(|v| v.as_bool()).unwrap_or(false)
}
fn is_draft(r: &serde_json::Value) -> bool {
    r.get("draft").and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Newest stable (non-prerelease) Linux release tag. The releases list is
/// newest-first, so the first match wins.
async fn fetch_latest_tag() -> Result<String, crate::Error> {
    let rels = list_releases().await?;
    rels.iter()
        .filter(|r| !is_draft(r) && !is_prerelease(r))
        .filter_map(release_tag)
        .find(|t| is_linux_tag(t))
        .map(|s| s.to_string())
        .ok_or_else(|| crate::Error::Other("no stable Linux release found".to_string()))
}

/// Newest *prerelease* Linux tag (the staging/edge channel). Skips macOS tags.
async fn fetch_latest_prerelease() -> Result<String, crate::Error> {
    let rels = list_releases().await?;
    rels.iter()
        .filter(|r| !is_draft(r) && is_prerelease(r))
        .filter_map(release_tag)
        .find(|t| is_linux_tag(t))
        .map(|s| s.to_string())
        .ok_or_else(|| {
            crate::Error::Other("no Linux prerelease found in the latest 30 releases".to_string())
        })
}

async fn fetch_text(url: &str) -> Result<String, crate::Error> {
    let body = send_get(url)
        .await?
        .text()
        .await
        .map_err(|e| crate::Error::Other(format!("read body: {e}")))?;
    Ok(body)
}

async fn download(url: &str, dest: &Path) -> Result<(), crate::Error> {
    let mut resp = send_get(url).await?;
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

/// Like [`find_named`] but for a directory (e.g. the `web` build dir).
fn find_dir_named(dir: &Path, name: &str) -> Result<PathBuf, crate::Error> {
    for entry in fs::read_dir(dir)
        .map_err(|e| crate::Error::Other(format!("read {}: {e}", dir.display())))?
    {
        let entry = entry.map_err(|e| crate::Error::Other(format!("dir entry: {e}")))?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|s| s.to_str()) == Some(name) {
                return Ok(path);
            }
            if let Ok(found) = find_dir_named(&path, name) {
                return Ok(found);
            }
        }
    }
    Err(crate::Error::Other(format!(
        "{name}/ not found inside the release tarball"
    )))
}

/// Replace the contents of `dst` with the freshly-extracted `src` directory.
/// Copies into a sibling temp dir, then swaps via move-aside + rename so a
/// failure mid-swap leaves the prior dir recoverable rather than an empty hole.
/// Used for `web/`, `actions/`, and `actions-bin/` alike — `fs::copy` preserves
/// the exec bit, so the swapped-in action binaries stay runnable. (Named for
/// its original web-only use; now generic via `refresh_named`.)
fn install_web(src: &Path, dst: &Path) -> Result<(), crate::Error> {
    let parent = dst
        .parent()
        .ok_or_else(|| crate::Error::Other("target dir has no parent".to_string()))?;
    fs::create_dir_all(parent)
        .map_err(|e| crate::Error::Other(format!("mkdir {}: {e}", parent.display())))?;
    // Per-destination temp names (derived from the leaf) so refreshing two dirs
    // that share a parent — `web/` and `actions/` both live under
    // /usr/local/share/virtues — can't stomp each other's staging area.
    let leaf = dst
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("dir");
    let staged = parent.join(format!(".virtues-{leaf}.upgrade-tmp"));
    let old = parent.join(format!(".virtues-{leaf}.upgrade-old"));
    let _ = fs::remove_dir_all(&staged);
    let _ = fs::remove_dir_all(&old);
    copy_dir_all(src, &staged)?;

    // Non-destructive swap: move the live dir ASIDE first, then move the new
    // one into place. A failure (or a kill) between the two renames leaves the
    // old dir recoverable at `old` — never an empty hole. Critical for
    // `actions-bin/`: a destructive remove-then-rename that died mid-swap would
    // wipe every action executable until a reinstall.
    let had_prior = dst.exists();
    if had_prior {
        fs::rename(dst, &old)
            .map_err(|e| crate::Error::Other(format!("move aside {}: {e}", dst.display())))?;
    }
    if let Err(e) = fs::rename(&staged, dst) {
        if had_prior {
            let _ = fs::rename(&old, dst); // restore the working dir
        }
        return Err(crate::Error::Other(format!(
            "swap dir into {}: {e}",
            dst.display()
        )));
    }
    let _ = fs::remove_dir_all(&old);
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), crate::Error> {
    fs::create_dir_all(dst)
        .map_err(|e| crate::Error::Other(format!("mkdir {}: {e}", dst.display())))?;
    for entry in
        fs::read_dir(src).map_err(|e| crate::Error::Other(format!("read {}: {e}", src.display())))?
    {
        let entry = entry.map_err(|e| crate::Error::Other(format!("dir entry: {e}")))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)
                .map_err(|e| crate::Error::Other(format!("copy {}: {e}", from.display())))?;
        }
    }
    Ok(())
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
    if fs::rename(new_binary, dest).is_err() {
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
