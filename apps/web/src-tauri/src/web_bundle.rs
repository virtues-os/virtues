//! OTA web-bundle store — the client half of `docs/spa-delivery-plan.md`.
//!
//! The box serves the UI build it carries (`/api/web-bundle/version` and
//! `/api/web-bundle/tarball`, see `virtues-core/src/api/web_bundle.rs`). This
//! module pulls that build, keeps it under the app's data directory, and lets
//! the shell serve it in place of the one compiled into the binary.
//!
//! **Why bother, when the app already bundles a build?** Because on mobile the
//! bundled build can only change through an App Store release. A UI fix that is
//! ready today waits days. The box already has the newer build; this is the
//! path from one to the other.
//!
//! **Why the box and nothing else?** A client that can only run a bundle the
//! box handed it cannot get ahead of the box, which kills the whole class of
//! "UI calls an endpoint the box does not have" by construction.
//!
//! # Fail-safe by construction
//!
//! Every lookup here answers "use the baked bundle" unless an overlay is
//! *provably* good: pointer present, directory there, `index.html` inside,
//! manifest parses. Corrupt state is not an error path, it is the default path.
//! Nothing this module can do should ever leave the app unable to show its UI —
//! the worst case is that it shows the version it shipped with.
//!
//! # Rollback
//!
//! A freshly flipped bundle is *pending* until the SPA that booted from it says
//! so (`bundle_boot_ok`). If the app starts and finds a pending bundle, that
//! bundle failed to confirm on its last try, so it is abandoned and the pointer
//! reverts. Same spirit as the box's `virtues.bak` swap: the thing that proves
//! a release works is that it came up, not that it downloaded.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Loopback the in-process reach layer serves the box on. A fn, not a const:
/// a dev profile (VIRTUES_PROFILE) serves its own port, and this fetcher must
/// talk to the same loopback that instance binds.
fn box_addr() -> String {
    format!("127.0.0.1:{}", tauri_plugin_reach::loopback_port())
}
const CONNECT_TIMEOUT: Duration = Duration::from_millis(1500);
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Refuse absurd payloads before allocating for them. The web build is a few
/// MB; anything past this is a bug or a hostile box, and either way not
/// something to unpack.
const MAX_TARBALL_BYTES: usize = 64 * 1024 * 1024;

const DIR_BUNDLES: &str = "web-bundles";
const PTR_ACTIVE: &str = "active";
const PTR_PENDING: &str = "pending";
const PTR_PREVIOUS: &str = "previous";
const MANIFEST_NAME: &str = ".virtues-bundle.json";

/// What a bundle says about itself. Mirrors what
/// `apps/web/scripts/write-bundle-manifest.mjs` stamps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub version: String,
    pub content_hash: String,
    pub min_shell_version: u32,
}

impl Manifest {
    /// Parse, requiring the two fields decisions are made on. A manifest
    /// missing `contentHash` or `minShellVersion` is unusable rather than
    /// defaulted: defaulting `minShellVersion` to 0 would let an
    /// unrunnable bundle install itself.
    pub fn parse(s: &str) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_str(s).ok()?;
        Some(Manifest {
            version: v.get("version")?.as_str()?.to_string(),
            content_hash: v.get("contentHash")?.as_str()?.to_string(),
            min_shell_version: u32::try_from(v.get("minShellVersion")?.as_u64()?).ok()?,
        })
    }
}

/// Why an update did not happen. All of these are ordinary, not failures —
/// they are logged at debug, never surfaced as errors.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Box offered the bundle we already run.
    UpToDate,
    /// Applied; the pointer now needs a boot to confirm it.
    Applied { content_hash: String },
    /// Box offers a bundle needing a newer native shell. This is the guard
    /// that keeps OTA from turning a store round-trip into a white screen.
    ShellTooOld { needs: u32, have: u32 },
    /// Box serves no static build (headless install, or a dev box whose UI
    /// comes from vite). Nothing to update from.
    NoBundleOnBox,
}

/// `<app-data>/web-bundles`.
pub fn bundles_root(app_data: &Path) -> PathBuf {
    app_data.join(DIR_BUNDLES)
}

fn pointer_path(root: &Path, name: &str) -> PathBuf {
    root.join(name)
}

fn read_pointer(root: &Path, name: &str) -> Option<String> {
    let s = fs::read_to_string(pointer_path(root, name)).ok()?;
    let s = s.trim().to_string();
    // A pointer is a bare content hash. Reject anything that could escape the
    // bundles directory when joined — this value came off disk and, before
    // that, off the network.
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(s)
}

fn write_pointer(root: &Path, name: &str, value: &str) -> std::io::Result<()> {
    fs::create_dir_all(root)?;
    fs::write(pointer_path(root, name), value)
}

fn clear_pointer(root: &Path, name: &str) {
    let _ = fs::remove_file(pointer_path(root, name));
}

/// A bundle directory is usable only if it can actually serve a page.
fn is_usable(dir: &Path) -> bool {
    dir.join("index.html").is_file() && dir.join(MANIFEST_NAME).is_file()
}

/// The overlay directory to serve from, or `None` to use the baked bundle.
///
/// Call this on every asset request — it is a couple of `stat`s and it means a
/// bundle that becomes unusable mid-session degrades to the baked build rather
/// than serving half a page.
pub fn active_bundle(app_data: &Path) -> Option<PathBuf> {
    let root = bundles_root(app_data);
    let hash = read_pointer(&root, PTR_ACTIVE)?;
    let dir = root.join(hash);
    is_usable(&dir).then_some(dir)
}

/// Identity of the active overlay bundle — its content hash — or `None` when
/// running the build baked into the binary.
///
/// This is the answer to "which UI is this device actually running", which is
/// unanswerable from the SPA alone: the bundle's own `$lib/build` constants say
/// what it was built from, but not whether it arrived over the air or shipped
/// in the app. Both halves are needed to interpret a bad update.
pub fn active_bundle_id(app_data: &Path) -> Option<String> {
    let root = bundles_root(app_data);
    let hash = read_pointer(&root, PTR_ACTIVE)?;
    is_usable(&root.join(&hash)).then_some(hash)
}

/// Resolve rollback state at startup, before any window loads.
///
/// A pending pointer here means the previous launch flipped to a bundle and
/// never came back to confirm it — so that bundle does not boot. Abandon it and
/// fall back to whatever it replaced.
///
/// Returns true when a rollback happened (worth logging; the user sees only
/// that the app works).
pub fn resolve_pending_at_startup(app_data: &Path) -> bool {
    let root = bundles_root(app_data);
    let Some(pending) = read_pointer(&root, PTR_PENDING) else {
        return false;
    };

    // It booted badly enough not to confirm. Do not try it again.
    clear_pointer(&root, PTR_PENDING);
    match read_pointer(&root, PTR_PREVIOUS) {
        Some(prev) if is_usable(&root.join(&prev)) => {
            let _ = write_pointer(&root, PTR_ACTIVE, &prev);
        }
        _ => clear_pointer(&root, PTR_ACTIVE), // back to the baked bundle
    }
    let _ = fs::remove_dir_all(root.join(&pending));
    true
}

/// Called by the SPA once it has actually rendered from the active bundle.
/// Promotes pending → confirmed, so the next startup keeps it.
pub fn mark_boot_ok(app_data: &Path) {
    let root = bundles_root(app_data);
    clear_pointer(&root, PTR_PENDING);
}

/// Delete bundle directories nothing points at.
///
/// Without this a phone accumulates one directory per update, forever — a few
/// MB each, on a device where storage is the user's, not ours. Keeps whatever
/// the three pointers name (active, previous, pending) and removes the rest.
///
/// Deliberately conservative: an unreadable directory is skipped rather than
/// forced, and failure is silent. Reclaiming disk is never worth risking the
/// bundle the app is about to boot from.
pub fn prune(app_data: &Path) -> usize {
    let root = bundles_root(app_data);
    let keep: Vec<String> = [PTR_ACTIVE, PTR_PREVIOUS, PTR_PENDING]
        .iter()
        .filter_map(|p| read_pointer(&root, p))
        .collect();

    let Ok(entries) = fs::read_dir(&root) else {
        return 0;
    };

    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue; // pointers and the outcome record are files
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // `.staging-*` dirs are abandoned mid-download attempts; they are never
        // pointed at, so they fall out here too.
        if keep.iter().any(|k| k == name) {
            continue;
        }
        if fs::remove_dir_all(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Filename holding the last check's outcome, so the UI can say why an update
/// did or did not happen.
const OUTCOME_FILE: &str = "last-outcome.json";

/// Record what the last check concluded.
///
/// The check runs on a background thread after launch, so by the time anyone
/// looks at a screen the result is long gone from anywhere it could be read.
/// Without this, a shell refusing every bundle because it is too old is
/// indistinguishable from OTA simply not being configured — the user sees
/// stale UI and no reason for it. Silence is the failure mode this whole
/// session kept running into; this is the fix for it here.
pub fn record_outcome(app_data: &Path, outcome: &Outcome) {
    let value = match outcome {
        Outcome::UpToDate => serde_json::json!({ "state": "up_to_date" }),
        Outcome::Applied { content_hash } => serde_json::json!({
            "state": "applied", "contentHash": content_hash,
        }),
        Outcome::ShellTooOld { needs, have } => serde_json::json!({
            "state": "shell_too_old", "needs": needs, "have": have,
        }),
        Outcome::NoBundleOnBox => serde_json::json!({ "state": "no_bundle_on_box" }),
    };
    let root = bundles_root(app_data);
    if fs::create_dir_all(&root).is_ok() {
        let _ = fs::write(root.join(OUTCOME_FILE), value.to_string());
    }
}

/// The last recorded outcome, for display. `None` when no check has run.
pub fn last_outcome(app_data: &Path) -> Option<serde_json::Value> {
    let raw = fs::read_to_string(bundles_root(app_data).join(OUTCOME_FILE)).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Check the box and apply a newer bundle if there is one this shell can run.
///
/// `shell_surface` is `COMMAND_SURFACE_VERSION` — the contract the bundle is
/// checked against before it is allowed anywhere near the active pointer.
pub fn check_and_apply(app_data: &Path, shell_surface: u32) -> std::io::Result<Outcome> {
    let Some(body) = http_get(&box_addr(), "/api/web-bundle/version")? else {
        return Ok(Outcome::NoBundleOnBox);
    };
    let Some(remote) = Manifest::parse(&String::from_utf8_lossy(&body)) else {
        return Ok(Outcome::NoBundleOnBox);
    };

    if remote.min_shell_version > shell_surface {
        return Ok(Outcome::ShellTooOld {
            needs: remote.min_shell_version,
            have: shell_surface,
        });
    }

    let root = bundles_root(app_data);
    if read_pointer(&root, PTR_ACTIVE).as_deref() == Some(remote.content_hash.as_str()) {
        return Ok(Outcome::UpToDate);
    }

    let Some(tar_gz) = http_get(&box_addr(), "/api/web-bundle/tarball")? else {
        return Ok(Outcome::NoBundleOnBox);
    };

    // Unpack beside the target, then rename into place: a half-written
    // directory must never be reachable through the pointer.
    let staging = root.join(format!(".staging-{}", remote.content_hash));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    unpack(&tar_gz, &staging)?;

    if !is_usable(&staging) {
        let _ = fs::remove_dir_all(&staging);
        return Ok(Outcome::NoBundleOnBox);
    }

    let target = root.join(&remote.content_hash);
    let _ = fs::remove_dir_all(&target);
    fs::rename(&staging, &target)?;

    // Remember what we are replacing before we replace it.
    if let Some(prev) = read_pointer(&root, PTR_ACTIVE) {
        let _ = write_pointer(&root, PTR_PREVIOUS, &prev);
    } else {
        clear_pointer(&root, PTR_PREVIOUS); // replacing the baked bundle
    }
    write_pointer(&root, PTR_ACTIVE, &remote.content_hash)?;
    write_pointer(&root, PTR_PENDING, &remote.content_hash)?;

    // Sweep anything the three pointers no longer name. Done here rather than
    // at startup so it never delays a launch, and after the pointers move so a
    // crash mid-prune cannot orphan the bundle we just staged.
    prune(app_data);

    Ok(Outcome::Applied {
        content_hash: remote.content_hash,
    })
}

/// Minimal HTTP/1.1 GET over the loopback, matching `main.rs`'s existing probe
/// rather than pulling an HTTP client into a shell that has none. The box is
/// reached in-process over the iroh loopback — no TLS, no proxies, no redirects
/// to follow.
///
/// `Ok(None)` = the box answered non-2xx (e.g. 404 for a headless box). `Err` =
/// could not talk to it at all.
fn http_get(addr: &str, path: &str) -> std::io::Result<Option<Vec<u8>>> {
    let sock = addr
        .parse()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "bad addr"))?;
    let mut stream = TcpStream::connect_timeout(&sock, CONNECT_TIMEOUT)?;
    stream.set_read_timeout(Some(READ_TIMEOUT))?;

    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes())?;

    let mut raw = Vec::new();
    stream.take(MAX_TARBALL_BYTES as u64).read_to_end(&mut raw)?;
    Ok(split_http_response(&raw))
}

/// Split a raw HTTP/1.1 response into its body, if the status is 2xx.
///
/// Separated from the socket so it can be tested — response framing is where
/// hand-rolled HTTP goes wrong.
fn split_http_response(raw: &[u8]) -> Option<Vec<u8>> {
    let head_end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..head_end]).ok()?;
    let status = head.split_whitespace().nth(1)?;
    if !status.starts_with('2') {
        return None;
    }
    Some(raw[head_end + 4..].to_vec())
}

// ─── Serving ────────────────────────────────────────────────────────────────

/// Normalize a request URI into a path inside a bundle, or `None` if it escapes.
///
/// SvelteKit is built with `adapter-static` and a `200.html` SPA fallback, so a
/// route like `/wiki/foo` is not a file — the shell must return `index.html`
/// and let the client router take it. That is the `is_none()` branch below, and
/// getting it wrong means every deep link 404s.
pub fn resolve_request_path(uri_path: &str) -> Option<String> {
    let trimmed = uri_path.trim_start_matches('/');
    let clean = trimmed.split(['?', '#']).next().unwrap_or("");

    if clean.is_empty() {
        return Some("index.html".into());
    }
    if escapes_dest(Path::new(clean)) {
        return None;
    }
    // A path with no extension is a client route, not a file. Anything with a
    // dot is an asset request and must 404 honestly if missing, rather than
    // being handed HTML — a JS request answered with `index.html` fails in a
    // way that is very hard to read from the console.
    let last = clean.rsplit('/').next().unwrap_or("");
    if last.contains('.') {
        Some(clean.to_string())
    } else {
        Some("index.html".into())
    }
}

/// Read `path` out of the active overlay bundle, if one is active and has it.
///
/// `None` means "fall through to the baked bundle" for every reason: no
/// overlay, missing file, unreadable file. The caller must always have that
/// fallback — this function never being able to fail is the property that keeps
/// a bad bundle from costing the app its UI.
pub fn read_from_overlay(app_data: &Path, path: &str) -> Option<Vec<u8>> {
    let dir = active_bundle(app_data)?;
    let file = dir.join(path);
    // Re-check after joining: `path` is already normalized, but the cost of
    // being wrong here is serving arbitrary files off the device.
    if !file.starts_with(&dir) {
        return None;
    }
    fs::read(file).ok()
}

/// Whether an archive entry's path would write outside the destination.
///
/// `tar`'s own `unpack_in` also refuses these (it errors rather than escaping),
/// so this is a second line, not the only one. It exists because the failure
/// modes differ and the safer one is ours: `unpack_in` returns `Err`, which
/// would abort the whole unpack and leave a good bundle unapplied because of
/// one bad entry. Skipping the entry here means a malformed archive fails the
/// `is_usable` check afterwards instead of exploding mid-write.
fn escapes_dest(path: &Path) -> bool {
    path.components()
        .any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::RootDir))
}

/// Unpack a gzipped tar into `dest`, skipping any entry that would escape it.
fn unpack(tar_gz: &[u8], dest: &Path) -> std::io::Result<()> {
    let decoder = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if escapes_dest(&path) {
            continue;
        }
        entry.unpack_in(dest)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory unique to this call.
    ///
    /// The counter is not belt-and-braces: tests run in parallel, and naming by
    /// timestamp alone let two of them land in the same nanosecond and share a
    /// directory — which showed up once as `prune_keeps_what_the_pointers_name`
    /// failing in isolation and passing on every rerun. A monotonic counter
    /// removes the possibility rather than making it rarer.
    fn tmp() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "virtues-bundle-test-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn plant(root: &Path, hash: &str) {
        let d = root.join(hash);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("index.html"), "<html></html>").unwrap();
        fs::write(
            d.join(MANIFEST_NAME),
            format!(r#"{{"version":"1","contentHash":"{hash}","minShellVersion":1}}"#),
        )
        .unwrap();
    }

    #[test]
    fn manifest_requires_the_fields_decisions_are_made_on() {
        assert!(Manifest::parse(r#"{"version":"1","contentHash":"a","minShellVersion":2}"#).is_some());
        // Missing minShellVersion must not default — that would let an
        // unrunnable bundle install itself.
        assert!(Manifest::parse(r#"{"version":"1","contentHash":"a"}"#).is_none());
        assert!(Manifest::parse(r#"{"version":"1","minShellVersion":2}"#).is_none());
        assert!(Manifest::parse("not json").is_none());
    }

    #[test]
    fn no_pointer_means_baked_bundle() {
        let d = tmp();
        assert_eq!(active_bundle(&d), None);
    }

    #[test]
    fn pointer_to_missing_or_incomplete_dir_falls_back() {
        let d = tmp();
        let root = bundles_root(&d);
        write_pointer(&root, PTR_ACTIVE, "deadbeef").unwrap();
        assert_eq!(active_bundle(&d), None, "pointer to nothing");

        fs::create_dir_all(root.join("deadbeef")).unwrap();
        assert_eq!(active_bundle(&d), None, "dir without index.html");

        plant(&root, "deadbeef");
        assert!(active_bundle(&d).is_some(), "complete bundle is usable");
    }

    #[test]
    fn traversal_in_a_pointer_is_rejected() {
        let d = tmp();
        let root = bundles_root(&d);
        fs::create_dir_all(&root).unwrap();
        fs::write(pointer_path(&root, PTR_ACTIVE), "../../etc").unwrap();
        assert_eq!(read_pointer(&root, PTR_ACTIVE), None);
        assert_eq!(active_bundle(&d), None);
    }

    #[test]
    fn unconfirmed_bundle_rolls_back_to_previous() {
        let d = tmp();
        let root = bundles_root(&d);
        plant(&root, "old1");
        plant(&root, "new2");
        write_pointer(&root, PTR_ACTIVE, "new2").unwrap();
        write_pointer(&root, PTR_PENDING, "new2").unwrap();
        write_pointer(&root, PTR_PREVIOUS, "old1").unwrap();

        assert!(resolve_pending_at_startup(&d));
        assert_eq!(read_pointer(&root, PTR_ACTIVE).as_deref(), Some("old1"));
        assert!(!root.join("new2").exists(), "bad bundle is removed");
    }

    #[test]
    fn unconfirmed_first_bundle_rolls_back_to_baked() {
        let d = tmp();
        let root = bundles_root(&d);
        plant(&root, "new2");
        write_pointer(&root, PTR_ACTIVE, "new2").unwrap();
        write_pointer(&root, PTR_PENDING, "new2").unwrap();
        // No previous: this overlaid the baked bundle.

        assert!(resolve_pending_at_startup(&d));
        assert_eq!(read_pointer(&root, PTR_ACTIVE), None);
        assert_eq!(active_bundle(&d), None, "serves the baked bundle again");
    }

    #[test]
    fn confirmed_bundle_survives_startup() {
        let d = tmp();
        let root = bundles_root(&d);
        plant(&root, "good3");
        write_pointer(&root, PTR_ACTIVE, "good3").unwrap();
        write_pointer(&root, PTR_PENDING, "good3").unwrap();

        mark_boot_ok(&d); // the SPA rendered
        assert!(!resolve_pending_at_startup(&d), "nothing pending to resolve");
        assert_eq!(read_pointer(&root, PTR_ACTIVE).as_deref(), Some("good3"));
    }

    #[test]
    fn http_body_split_honors_status() {
        assert_eq!(
            split_http_response(b"HTTP/1.1 200 OK\r\nX: 1\r\n\r\nbody"),
            Some(b"body".to_vec())
        );
        assert_eq!(split_http_response(b"HTTP/1.1 404 Not Found\r\n\r\nnope"), None);
        assert_eq!(split_http_response(b"garbage"), None);
    }

    #[test]
    fn request_paths_resolve_spa_routes_to_index() {
        // Assets keep their path.
        assert_eq!(resolve_request_path("/_app/x.js").as_deref(), Some("_app/x.js"));
        assert_eq!(resolve_request_path("/favicon.png").as_deref(), Some("favicon.png"));
        // Client routes get index.html — adapter-static + SPA fallback.
        assert_eq!(resolve_request_path("/").as_deref(), Some("index.html"));
        assert_eq!(resolve_request_path("/wiki/foo").as_deref(), Some("index.html"));
        assert_eq!(resolve_request_path("/virtues/billing").as_deref(), Some("index.html"));
        // Query and fragment are not part of the file path.
        assert_eq!(resolve_request_path("/_app/x.js?v=2").as_deref(), Some("_app/x.js"));
        // Escapes are refused outright.
        assert_eq!(resolve_request_path("/../../etc/passwd"), None);
    }

    #[test]
    fn overlay_read_falls_through_when_absent() {
        let d = tmp();
        // No overlay at all.
        assert_eq!(read_from_overlay(&d, "index.html"), None);

        let root = bundles_root(&d);
        plant(&root, "abc123");
        write_pointer(&root, PTR_ACTIVE, "abc123").unwrap();
        assert!(read_from_overlay(&d, "index.html").is_some(), "serves from overlay");
        // Present overlay, absent file → fall through, not an error.
        assert_eq!(read_from_overlay(&d, "_app/missing.js"), None);
    }

    #[test]
    fn prune_keeps_what_the_pointers_name() {
        let d = tmp();
        let root = bundles_root(&d);
        plant(&root, "active1");
        plant(&root, "prev2");
        plant(&root, "orphan3");
        plant(&root, "orphan4");
        fs::create_dir_all(root.join(".staging-abandoned")).unwrap();
        write_pointer(&root, PTR_ACTIVE, "active1").unwrap();
        write_pointer(&root, PTR_PREVIOUS, "prev2").unwrap();

        let removed = prune(&d);
        assert_eq!(removed, 3, "two orphans and one abandoned staging dir");
        assert!(root.join("active1").exists());
        assert!(root.join("prev2").exists());
        assert!(!root.join("orphan3").exists());
        assert!(!root.join(".staging-abandoned").exists());
        // Pointers are files, never swept.
        assert_eq!(read_pointer(&root, PTR_ACTIVE).as_deref(), Some("active1"));
    }

    #[test]
    fn prune_keeps_a_pending_bundle() {
        // A pending bundle has not booted yet — sweeping it would delete the
        // thing the next launch is about to try.
        let d = tmp();
        let root = bundles_root(&d);
        plant(&root, "staged9");
        write_pointer(&root, PTR_PENDING, "staged9").unwrap();
        assert_eq!(prune(&d), 0);
        assert!(root.join("staged9").exists());
    }

    #[test]
    fn outcome_round_trips_for_display() {
        let d = tmp();
        assert_eq!(last_outcome(&d), None, "no check has run");

        record_outcome(&d, &Outcome::ShellTooOld { needs: 3, have: 1 });
        let v = last_outcome(&d).expect("recorded");
        assert_eq!(v["state"], "shell_too_old");
        assert_eq!(v["needs"], 3);
        assert_eq!(v["have"], 1);

        record_outcome(&d, &Outcome::UpToDate);
        assert_eq!(last_outcome(&d).unwrap()["state"], "up_to_date");
    }

    #[test]
    fn traversal_entries_are_identified() {
        // Tested as a predicate rather than by unpacking a malicious archive:
        // the `tar` crate refuses to *build* one ("paths in archives must not
        // have `..`"), so such an archive can only arrive hand-crafted.
        assert!(escapes_dest(Path::new("../escaped.txt")));
        assert!(escapes_dest(Path::new("a/../../escaped.txt")));
        assert!(escapes_dest(Path::new("/etc/passwd")));
        assert!(!escapes_dest(Path::new("index.html")));
        assert!(!escapes_dest(Path::new("_app/immutable/chunk.js")));
    }

    #[test]
    fn round_trips_a_real_archive() {
        let src = tmp();
        fs::write(src.join("index.html"), "<html>hi</html>").unwrap();
        fs::create_dir_all(src.join("_app")).unwrap();
        fs::write(src.join("_app/chunk.js"), "console.log(1)").unwrap();

        let mut buf = Vec::new();
        {
            let enc = flate2::write::GzEncoder::new(&mut buf, flate2::Compression::fast());
            let mut b = tar::Builder::new(enc);
            b.append_dir_all(".", &src).unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }

        let dest = tmp();
        unpack(&buf, &dest).unwrap();
        assert_eq!(fs::read_to_string(dest.join("index.html")).unwrap(), "<html>hi</html>");
        assert_eq!(fs::read_to_string(dest.join("_app/chunk.js")).unwrap(), "console.log(1)");
    }
}
