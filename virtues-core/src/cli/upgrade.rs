//! `virtues upgrade` — self-update from a GitHub Release, via atomic slots.
//!
//! A release is staged whole into `releases/<slot>/`, preflighted (the STAGED
//! binary runs `migrate --check` + a `--version` smoke before anything is
//! touched), then activated by flipping the `current` symlink — binary + web
//! + actions move together, atomically. Any failure before the flip leaves
//! the box byte-identical; any failure after flips straight back. `virtues
//! rollback` is the same flip in reverse. See `cli/slots.rs` for the layout.
//!
//! Strictly opt-in. No background auto-upgrade — that lands when the
//! upgrade path has been battle-tested.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use semver::Version;

use sha2::{Digest, Sha256};

use super::{slots, ui};

const BINARY_PATH: &str = "/usr/local/bin/virtues";
const RELEASE_REPO: &str = "virtues-os/virtues";
const USER_AGENT: &str = concat!("virtues-upgrade/", env!("CARGO_PKG_VERSION"));

pub async fn run(
    check: bool,
    version: Option<String>,
    pre: bool,
    force: bool,
    only: Option<String>,
) -> Result<(), crate::Error> {
    let target_tag = match version {
        Some(v) => v,
        None if pre => fetch_latest_prerelease().await?,
        None => fetch_latest_tag().await?,
    };
    let current = env!("CARGO_PKG_VERSION");
    let target = target_tag.trim_start_matches('v');

    ui::section("Upgrade");
    ui::kv("current", current);
    ui::kv("target", target);
    println!();

    // Cheap no-op detection works only for STABLE tags, where the semver is
    // the whole identity. Prerelease/edge builds all report the bare crate
    // version, so equality there means nothing — those fall through to the
    // SHA comparison after download (the fix for "edge→edge is impossible").
    // `--force` bypasses every equality short-circuit.
    let is_stable_tag = !target.contains('-') && target.chars().next().is_some_and(|c| c.is_ascii_digit());
    if !force && is_stable_tag && target == current {
        ui::ok(&format!("already on {current} — nothing to do (--force to reinstall)"));
        return Ok(());
    }
    if check {
        ui::ok(&format!("{target} is available — run `virtues upgrade` to apply"));
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

    ui::step(&format!("downloading {asset_name}…"));
    download(&asset_url, &asset_path).await?;

    ui::step("verifying sha256…");
    let expected = fetch_text(&sha_url).await?;
    let expected_hex = expected.split_whitespace().next().unwrap_or("").to_string();
    verify_sha(&asset_path, &expected_hex)?;
    ui::ok("sha256 verified");

    ui::step("extracting…");
    // Unpacks the whole tarball under `work_path/extracted/` and returns the
    // `virtues` binary; the sibling artifacts (sidecars, web/, actions/,
    // actions-bin/) are pulled out of the same tree below.
    let new_binary = extract_binary(&asset_path, work_path)?;
    let extracted = work_path.join("extracted");

    // Which artifacts this tarball actually carries. Older releases ship a
    // subset — every member but `virtues` itself is optional.
    let new_llama = find_named(&extracted, "llama-server").ok();
    let new_qnnd = find_named(&extracted, "virtues-qnnd").ok();
    let web_src = find_dir_named(&extracted, "web").ok();
    let actions_src = find_dir_named(&extracted, "applets")
        .or_else(|_| find_dir_named(&extracted, "actions"))
        .ok();
    let actions_bin_src = find_dir_named(&extracted, "applets-bin")
        .or_else(|_| find_dir_named(&extracted, "actions-bin"))
        .ok();

    // Build identity from the tarball's BUILD.json (releases since the slot
    // era carry one). The SHA is the only honest identity for prerelease
    // builds — every edge build reports the same crate version.
    let build = read_build_manifest(&extracted);
    if !force {
        if let Some(sha) = build.as_ref().and_then(|b| b.sha.as_deref()) {
            let running = env!("GIT_COMMIT");
            if !running.is_empty() && running != "unknown" && sha.starts_with(&running[..running.len().min(7)]) {
                ui::ok(&format!(
                    "already on this exact build ({}) — nothing to do (--force to reinstall)",
                    &sha[..sha.len().min(7)]
                ));
                return Ok(());
            }
        }
    }

    // ── `--only web[,actions]` — the fast path ──────────────────────────────
    // Refresh just the named components IN the current slot, no binary swap,
    // no migration, no restart (web is static files; actions are re-globbed).
    // Deliberately mutates the live slot: this is the dev/UI iteration loop,
    // not a release activation.
    if let Some(list) = only {
        let dirs = InstallDirs::resolve();
        for part in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            match part {
                "web" => match &web_src {
                    Some(src) => refresh_named("web UI", src, &canonical(&dirs.web)),
                    None => ui::warn("tarball carries no web/ — skipped"),
                },
                "applets" | "actions" => {
                    match &actions_src {
                        Some(src) => refresh_named("applets", src, &canonical(&dirs.actions)),
                        None => ui::warn("tarball carries no applets/ — skipped"),
                    }
                    match &actions_bin_src {
                        Some(src) => {
                            refresh_named("applet binaries", src, &canonical(&dirs.actions_bin))
                        }
                        None => ui::warn("tarball carries no applets-bin/ — skipped"),
                    }
                }
                other => {
                    return Err(crate::Error::Other(format!(
                        "--only {other}: unknown component (web, applets)"
                    )))
                }
            }
        }
        println!();
        ui::ok(&format!("refreshed --only components from {target_tag}"));
        return Ok(());
    }

    // ── Slot layout required from here on ───────────────────────────────────
    let layout = slots::SlotLayout::system();
    if !layout.exists() {
        return Err(crate::Error::Other(
            "this box predates release slots (no `current` link under \
             /usr/local/share/virtues). Re-run the installer once to adopt the \
             slot layout:\n\n  curl -sSL https://virtues.com/sh | sudo sh\n"
                .to_string(),
        ));
    }
    let prior_slot = layout.current_slot();

    // ── Stage the whole release into its slot ───────────────────────────────
    let slot_id = build
        .as_ref()
        .and_then(|b| b.slot_id(&target_tag))
        .unwrap_or_else(|| format!("{target_tag}-{}", chrono::Utc::now().format("%Y%m%dT%H%M%S")));
    let slot = layout.slot_dir(&slot_id);
    ui::step(&format!("staging release into {}…", slot.display()));
    stage_slot(&slot, &new_binary, &new_llama, &new_qnnd, &web_src, &actions_src, &actions_bin_src)?;

    // ── Preflight — the STAGED binary must prove itself before any swap ────
    // 1. `--version` smoke: the binary runs on this box at all.
    // 2. `migrate --check`: lineage compatibility, applying nothing. This is
    //    what turns "brick mid-swap on a migration mismatch" into a clean
    //    refusal with the box untouched. `--force` skips only the lineage
    //    gate (older targets don't know `--check`), never the smoke test.
    let staged_bin = slot.join("virtues");
    ui::step("preflight: staged binary smoke test…");
    match Command::new(&staged_bin).arg("--version").output() {
        Ok(o) if o.status.success() => {
            ui::ok(&format!("staged: {}", String::from_utf8_lossy(&o.stdout).trim()));
        }
        Ok(o) => {
            let _ = fs::remove_dir_all(&slot);
            return Err(crate::Error::Other(format!(
                "staged binary failed --version (exit {}); box untouched",
                o.status
            )));
        }
        Err(e) => {
            let _ = fs::remove_dir_all(&slot);
            return Err(crate::Error::Other(format!(
                "staged binary would not run ({e}); box untouched"
            )));
        }
    }
    if !force {
        ui::step("preflight: migration lineage check…");
        match Command::new(&staged_bin).args(["migrate", "--check"]).output() {
            Ok(o) if o.status.success() => ui::ok("lineage OK"),
            Ok(o) => {
                let _ = fs::remove_dir_all(&slot);
                eprintln!("{}", String::from_utf8_lossy(&o.stderr).trim_end());
                return Err(crate::Error::Other(
                    "migration preflight refused this release — box untouched \
                     (see reason above; `--force` overrides at your own risk)"
                        .to_string(),
                ));
            }
            Err(e) => {
                let _ = fs::remove_dir_all(&slot);
                return Err(crate::Error::Other(format!(
                    "could not run migration preflight ({e}); box untouched"
                )));
            }
        }
    }

    // Relay migration: an upgrade from a WireGuard-era release has an orphaned
    // `virtues-wireguard.service`. Best-effort; no-op on boxes that never had it.
    disable_legacy_wireguard();

    // ── Stop → flip → migrate → start ───────────────────────────────────────
    // Sidecars restart only when the release replaces their binaries
    // (reloading multi-GB GGUFs is slow; don't pay it for an app-only bump).
    ui::step("stopping virtues.service…");
    service_stop("virtues");
    let sidecars = if new_llama.is_some() || new_qnnd.is_some() {
        installed_inference_units()
    } else {
        Vec::new()
    };
    for unit in &sidecars {
        service_stop(unit);
    }

    // On any failure after this point: flip back to the prior slot and
    // restart, so the box always runs a COMPLETE release (binary + web +
    // actions together — the old per-component path could not promise that).
    let flip_back = |layout: &slots::SlotLayout, why: String| -> crate::Error {
        if let Some(prior) = &prior_slot {
            let _ = layout.flip(prior);
            ui::warn(&format!("rolled back to {}", prior.display()));
        }
        let _ = service_start("virtues");
        for unit in &sidecars {
            let _ = service_start(unit);
        }
        crate::Error::Other(why)
    };

    ui::step("activating release (symlink flip)…");
    if let Err(e) = layout.flip(&slot) {
        return Err(flip_back(&layout, format!("could not flip current → {slot_id}: {e}")));
    }

    ui::step("running migrations under the new binary…");
    match Command::new(BINARY_PATH).arg("migrate").status() {
        Ok(s) if s.success() => {}
        Ok(s) => {
            return Err(flip_back(
                &layout,
                format!("new binary's `migrate` exited {s} — rolled back"),
            ))
        }
        Err(e) => return Err(flip_back(&layout, format!("invoke migrate: {e} — rolled back"))),
    }

    // The box keeps its default `virtues.local` name; remove the dead
    // hostname-rename sudoers grant from older installs. Best-effort.
    remove_stale_setup_sudoers();

    ui::step("starting virtues.service…");
    match service_start("virtues") {
        Ok(true) => {}
        _ => {
            return Err(flip_back(
                &layout,
                "systemctl start virtues failed on the new release — rolled back".to_string(),
            ))
        }
    }

    // `systemctl start` on a Type=simple unit returns as soon as the process is
    // SPAWNED — it proves nothing about whether the new release can actually
    // serve. A binary that panics on a migration it doesn't understand, or that
    // can't reach Postgres, "starts" cleanly and then fails every request; with
    // `Restart=on-failure` it crash-loops while the upgrade reports success.
    //
    // /health is the honest signal: it returns 200 only after a `SELECT 1`
    // against the pool succeeds, so it covers both "bound its port" and "has a
    // working database". Not becoming healthy is a failed upgrade, and gets the
    // same slot flip-back as a failed start.
    ui::step("waiting for the new release to serve…");
    if !wait_healthy(service_port()).await {
        // The unit is RUNNING (badly) on this path, unlike every other
        // flip_back caller. Stop it first: flipping `current` only re-points
        // the symlink, and the live process keeps its old inode open, so
        // without this the box would go on serving the broken binary from a
        // rolled-back slot.
        service_stop("virtues");
        return Err(flip_back(
            &layout,
            format!("{target_tag} started but never became healthy — rolled back"),
        ));
    }

    for unit in &sidecars {
        if let Ok(false) | Err(_) = service_start(unit) {
            ui::warn(&format!(
                "{unit} did not start — search/embeddings degraded; check `systemctl status {unit}`"
            ));
        }
    }

    // Keep current + one previous; delete older slots.
    layout.prune(slots::KEEP_SLOTS - 1);

    // Model-set drift check. `virtues upgrade` swaps binaries but does NOT
    // fetch model GGUFs or rewrite the sidecar `-m`/pooling in the unit files —
    // those are provisioned by the installer. So a release that changes the
    // model set (e.g. swapping the embedder or reranker) leaves the box serving the OLD
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
            println!();
            ui::warn("this release expects models not present on the box:");
            for f in &missing {
                ui::skip(f);
            }
            println!("     `virtues upgrade` doesn't migrate the model set — the sidecars are");
            println!("     still on the old GGUFs, so search/embeddings will fail until you");
            println!("     re-run the installer (fetches the new models + rewrites the units):");
            println!();
            println!(
                "       {}",
                console::style(format!(
                    "curl -sSL https://virtues.com/sh | sudo VIRTUES_VERSION={target_tag} sh"
                ))
                .cyan()
            );
            println!();
        }
    }

    println!();
    ui::ok(&format!(
        "upgraded to {target_tag} (slot {slot_id}) — `virtues rollback` restores the previous release"
    ));
    println!();
    Ok(())
}

/// `virtues rollback` — flip `current` back to the previous release slot and
/// restart. The inverse of an upgrade's activation: one atomic symlink flip
/// moves binary + web + actions together. No migrations run — schema rolls
/// FORWARD only; a rolled-back binary boots with `ignore_missing`, so a
/// newer-schema DB is tolerated (features that need the newer binary are
/// simply gone until you upgrade again).
pub async fn rollback() -> Result<(), crate::Error> {
    ui::section("Rollback");
    let layout = slots::SlotLayout::system();
    if !layout.exists() {
        return Err(crate::Error::Other(
            "no release slots on this box (re-run the installer once to adopt the slot layout)"
                .to_string(),
        ));
    }
    let current = layout
        .current_slot()
        .ok_or_else(|| crate::Error::Other("current release link dangles".to_string()))?;
    let Some(previous) = layout.previous_slot() else {
        return Err(crate::Error::Other(
            "no previous release kept — nothing to roll back to".to_string(),
        ));
    };
    ui::kv("current", &current.file_name().unwrap_or_default().to_string_lossy());
    ui::kv("target", &previous.file_name().unwrap_or_default().to_string_lossy());
    println!();

    if !running_as_root() {
        return Err(crate::Error::Other(
            "virtues rollback must run as root (try with sudo)".to_string(),
        ));
    }
    let _lock = acquire_lock()?;

    ui::step("stopping services…");
    service_stop("virtues");
    let sidecars = installed_inference_units();
    for unit in &sidecars {
        service_stop(unit);
    }

    ui::step("flipping current → previous release…");
    layout
        .flip(&previous)
        .map_err(|e| crate::Error::Other(format!("flip failed: {e}")))?;

    ui::step("starting services…");
    match service_start("virtues") {
        Ok(true) => {}
        _ => {
            // Roll forward again rather than leave the box down on a release
            // that won't boot.
            let _ = layout.flip(&current);
            let _ = service_start("virtues");
            for unit in &sidecars {
                let _ = service_start(unit);
            }
            return Err(crate::Error::Other(
                "previous release would not start — flipped forward again".to_string(),
            ));
        }
    }
    for unit in &sidecars {
        let _ = service_start(unit);
    }

    println!();
    ui::ok("rolled back — the previous release is active");
    Ok(())
}

/// The tarball's build identity, written by CI next to the binaries.
#[derive(serde::Deserialize)]
struct BuildManifest {
    version: Option<String>,
    sha: Option<String>,
}

impl BuildManifest {
    /// Directory name for this release's slot: `<tag>-<sha7>`. Unique per
    /// build (two edge cuts differ by sha), stable per artifact (re-running
    /// an upgrade re-stages the same slot).
    fn slot_id(&self, tag: &str) -> Option<String> {
        let sha = self.sha.as_deref()?;
        Some(format!("{tag}-{}", &sha[..sha.len().min(7)]))
    }
}

/// Read `BUILD.json` from the extracted tarball. Absent on pre-slot-era
/// releases — every consumer treats it as optional.
fn read_build_manifest(extracted: &Path) -> Option<BuildManifest> {
    let p = find_named(extracted, "BUILD.json").ok()?;
    serde_json::from_slice(&fs::read(p).ok()?).ok()
}

/// Copy one whole release into its slot dir. The slot is the unit of
/// activation and rollback, so it carries EVERYTHING the tarball shipped;
/// only `virtues` itself is mandatory.
fn stage_slot(
    slot: &Path,
    binary: &Path,
    llama: &Option<PathBuf>,
    qnnd: &Option<PathBuf>,
    web: &Option<PathBuf>,
    actions: &Option<PathBuf>,
    actions_bin: &Option<PathBuf>,
) -> Result<(), crate::Error> {
    // Re-staging the same slot id (a retried upgrade) starts clean.
    let _ = fs::remove_dir_all(slot);
    fs::create_dir_all(slot)
        .map_err(|e| crate::Error::Other(format!("mkdir {}: {e}", slot.display())))?;

    let copy_bin = |src: &Path, name: &str| -> Result<(), crate::Error> {
        let dst = slot.join(name);
        fs::copy(src, &dst)
            .map_err(|e| crate::Error::Other(format!("stage {name}: {e}")))?;
        fs::set_permissions(&dst, fs::Permissions::from_mode(0o755))
            .map_err(|e| crate::Error::Other(format!("chmod {name}: {e}")))?;
        Ok(())
    };
    copy_bin(binary, "virtues")?;
    if let Some(p) = llama {
        copy_bin(p, "llama-server")?;
    }
    if let Some(p) = qnnd {
        copy_bin(p, "virtues-qnnd")?;
    }
    for (src, name) in [(web, "web"), (actions, "applets"), (actions_bin, "applets-bin")] {
        if let Some(s) = src {
            copy_dir_all(s, &slot.join(name))?;
        }
    }
    Ok(())
}

/// Resolve a well-known path through its symlinks to the real directory —
/// `--only` must write INTO the current slot, not replace the routing link.
fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
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
        // Applets dir: honor whichever env var the box was provisioned with,
        // in the SAME order the runtime resolves it (action_templates.rs /
        // action_runner.rs try APPLETS_ first, then legacy ACTIONS_). A box
        // installed before the actions→applets rename only sets the ACTIONS_
        // vars; defaulting straight to /applets here would refresh into a dir
        // the runtime never reads (the bug that stranded document_extraction
        // on the dragon). Falls through to the new default only when neither
        // is set.
        let env_dir_multi = |vars: &[&str], default: &str| {
            vars.iter()
                .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(default))
        };
        Self {
            bin_dir,
            web: env_dir("STATIC_DIR", "/usr/local/share/virtues/web"),
            actions: env_dir_multi(
                &["VIRTUES_APPLETS_DIR", "VIRTUES_ACTIONS_DIR"],
                "/usr/local/share/virtues/applets",
            ),
            actions_bin: env_dir_multi(
                &["VIRTUES_APPLETS_BIN_DIR", "VIRTUES_ACTIONS_BIN_DIR"],
                "/usr/local/libexec/virtues",
            ),
        }
    }
}

/// Atomically replace a shipped directory, logging a uniform success/skip line.
/// Best-effort: a copy failure leaves the prior dir untouched (install_web
/// stages into a sibling and only swaps on success) and never aborts the run.
fn refresh_named(label: &str, src: &Path, dst: &Path) {
    match install_web(src, dst) {
        Ok(()) => ui::ok(&format!("refreshed {label} → {}", dst.display())),
        Err(e) => ui::warn(&format!("{label} not refreshed ({e}); prior copy still in effect")),
    }
}

/// `systemctl stop <unit>` — best-effort (a not-yet-running unit is fine).
/// The inference sidecars actually installed on THIS box.
///
/// We ship two backends: the llama.cpp sidecars (`virtues-embed` + `virtues-rerank`,
/// on Jetson/DIY) and the QNN NPU daemon (`virtues-qnnd`, on Q6A). Hardcoding either
/// set is wrong for the other — assuming llama.cpp made a healthy Q6A box print
/// "Unit virtues-embed.service not loaded" and a false "search/embeddings degraded"
/// on every upgrade. So ask the filesystem instead of guessing.
fn installed_inference_units() -> Vec<String> {
    // Prefer the installer's topology manifest — DECLARED shape, not a guess.
    if let Ok(bytes) = fs::read("/usr/local/share/virtues/install.json") {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if let Some(units) = v.get("sidecars").and_then(|s| s.as_array()) {
                return units
                    .iter()
                    .filter_map(|u| u.as_str().map(str::to_string))
                    .collect();
            }
        }
    }
    // Fallback for boxes installed before the manifest existed: ask the
    // filesystem which unit files are present.
    ["virtues-embed", "virtues-rerank", "virtues-qnnd"]
        .into_iter()
        .filter(|u| Path::new(&format!("/etc/systemd/system/{u}.service")).exists())
        .map(str::to_string)
        .collect()
}

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
    ui::step("retiring legacy virtues-wireguard.service (relay model)…");
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

/// The port `virtues.service` actually serves on, read from the installed
/// unit's `ExecStart` (`… server --host [::] --port 8000`).
///
/// Parsed rather than hardcoded because the probe's failure mode is asymmetric:
/// probing the wrong port would roll back a perfectly good upgrade. If the unit
/// is unreadable or its ExecStart has no `--port`, fall back to the same 8000
/// the installer writes.
fn service_port() -> u16 {
    fs::read_to_string("/etc/systemd/system/virtues.service")
        .ok()
        .and_then(|u| parse_port_from_unit(&u))
        .unwrap_or(DEFAULT_SERVICE_PORT)
}

/// The port the installer writes into `virtues.service`.
const DEFAULT_SERVICE_PORT: u16 = 8000;

/// Pull `--port N` (or `--port=N`) out of a unit file's `ExecStart` line.
/// Split out from [`service_port`] so it can be tested without a real
/// `/etc/systemd/system`.
fn parse_port_from_unit(unit: &str) -> Option<u16> {
    let line = unit
        .lines()
        .find(|l| l.trim_start().starts_with("ExecStart="))?;
    let mut it = line.split_whitespace();
    while let Some(tok) = it.next() {
        if tok == "--port" {
            return it.next().and_then(|p| p.parse().ok());
        }
        if let Some(p) = tok.strip_prefix("--port=") {
            return p.parse().ok();
        }
    }
    None
}

/// Poll the box's own `/health` until it answers 200, or the budget runs out.
///
/// Retries rather than probing once: a fresh process has to bind its listener
/// and open a Postgres pool, and the unit's `Restart=on-failure`/`RestartSec=5`
/// means a crash-looping binary needs a few cycles before it's distinguishable
/// from a slow-but-healthy start. 60s covers a cold start comfortably (the unit
/// itself allows `TimeoutStartSec=120`) without hanging a broken upgrade for
/// minutes before rolling it back.
async fn wait_healthy(port: u16) -> bool {
    const ATTEMPTS: u32 = 30;
    const EVERY: Duration = Duration::from_secs(2);

    let url = format!("http://127.0.0.1:{port}/health");
    // Loopback plain HTTP — `base_builder` is the codebase's known-good builder
    // and is already IPv4-only, which is exactly right for 127.0.0.1.
    let Ok(client) = crate::http_client::base_builder()
        .timeout(Duration::from_secs(3))
        .build()
    else {
        // Can't probe → don't claim unhealthy and trigger a spurious rollback.
        tracing::warn!("could not build health-probe client; skipping the probe");
        return true;
    };

    for attempt in 1..=ATTEMPTS {
        tokio::time::sleep(EVERY).await;
        if let Ok(r) = client.get(&url).send().await {
            if r.status().is_success() {
                ui::ok(&format!(
                    "health check passed after {}s",
                    attempt * EVERY.as_secs() as u32
                ));
                return true;
            }
        }
    }
    false
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
            Ok(()) => ui::ok("removed dead box-rename sudoers rule"),
            Err(e) => ui::warn(&format!("couldn't remove stale {PATH} ({e})")),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The unit the installer actually writes today.
    const REAL_UNIT: &str = "\
[Service]
Type=simple
ExecStartPre=/bin/sh -c 'until pg_isready -t 1; do sleep 1; done'
ExecStart=/usr/local/bin/virtues server --host [::] --port 8000
TimeoutStartSec=120
";

    #[test]
    fn reads_the_port_the_installer_writes() {
        assert_eq!(parse_port_from_unit(REAL_UNIT), Some(8000));
    }

    #[test]
    fn reads_a_hand_edited_port() {
        // The whole reason this is parsed and not hardcoded: probing the wrong
        // port would roll back a healthy upgrade.
        let unit = REAL_UNIT.replace("--port 8000", "--port 9123");
        assert_eq!(parse_port_from_unit(&unit), Some(9123));
    }

    #[test]
    fn accepts_the_equals_form() {
        let unit = REAL_UNIT.replace("--port 8000", "--port=9123");
        assert_eq!(parse_port_from_unit(&unit), Some(9123));
    }

    #[test]
    fn ignores_a_port_outside_execstart() {
        // ExecStartPre mentions no port, but a sloppier scan of the whole file
        // could pick up an unrelated one. Only ExecStart counts.
        let unit = "[Service]\nEnvironment=SOME_PORT=--port 1234\nExecStart=/usr/local/bin/virtues server --port 8000\n";
        assert_eq!(parse_port_from_unit(unit), Some(8000));
    }

    #[test]
    fn no_port_falls_back_to_the_caller_default() {
        let unit = "[Service]\nExecStart=/usr/local/bin/virtues server\n";
        assert_eq!(parse_port_from_unit(unit), None);
    }

    #[test]
    fn no_execstart_at_all() {
        assert_eq!(parse_port_from_unit("[Unit]\nDescription=x\n"), None);
    }
}
