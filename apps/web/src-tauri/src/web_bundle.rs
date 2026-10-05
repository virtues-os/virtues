//! OTA web-bundle store — the client half of `agents/record/spa-delivery.md`.
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
//! **Why the box and nothing else?** Because the box is the only source that
//! is guaranteed to have the API the UI calls. That was once the stronger claim
//! "a client that can only run a bundle the box handed it cannot get ahead of
//! the box" — see the next section for why it is no longer true.
//!
//! # Forward only, and what it costs
//!
//! That same property, unguarded, is a downgrade machine. The phone updates on
//! Apple's cadence; the box updates when its owner runs `sudo virtues upgrade`.
//! A shell NEWER than its box is therefore the ordinary state of affairs, not a
//! rare window — and "take whatever the box serves" then means a fresh
//! TestFlight build OTAs itself backwards onto the box's older SPA on its first
//! launch, silently. The airlock is where that bites: `ui/connect.html` is
//! baked into the binary while `app.html` and the views ride the bundle, so a
//! downgrade desynchronizes two halves of one launch that shipped together and
//! were only ever tested together. Anything spanning that seam — a handoff, a
//! shared constant, chrome one side draws and the other clears — is written
//! against a partner the box can silently replace with an older one.
//!
//! So the bundle only ever moves forward: `version_gate` refuses an offer that
//! is not provably newer than what this device can already serve, and
//! `drop_stale_overlay` puts a device back on its baked build when an App Store
//! update has overtaken the overlay it was running.
//!
//! **This retires the invariant the module was designed around.** "The client
//! can never outrun the box" is now "the client runs whichever of {baked,
//! box-served} is newer", and the class that invariant killed by construction —
//! UI calling an endpoint the box does not have, the wrong-midnight `tz` bug of
//! 2026-08-05 — is back in play whenever a phone is ahead of its box. The trade
//! was made knowingly: the alternative is silently downgrading a freshly
//! installed app onto UI its own airlock was not built against, which breaks a
//! device that is otherwise fine. `minShellVersion` has no mirror image today
//! (a bundle cannot declare a minimum BOX), and `shellSupports()` gates
//! commands against the shell, not endpoints against the box. If the class
//! comes back, that mirror is the structural answer.
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
/// The pending bundle a launch has actually ATTEMPTED to boot. `apply` runs
/// mid-session, so a pending pointer alone does not mean "tried and failed" —
/// it usually means "no launch has tried this yet". Only pending + booting
/// agreeing at startup is evidence of a bundle that does not boot.
const PTR_BOOTING: &str = "booting";
/// The content hash of the last bundle abandoned by rollback. Without it the
/// very next check re-downloads the identical failing bundle, every launch and
/// every foreground, forever.
const POISON_FILE: &str = "rolled-back";
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
    /// Parse, requiring the three fields decisions are made on. A manifest
    /// missing `version`, `contentHash` or `minShellVersion` is unusable rather
    /// than defaulted: defaulting `minShellVersion` to 0 would let an
    /// unrunnable bundle install itself, and defaulting `version` would let an
    /// older bundle pass the forward-only gate.
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
    /// Box offers the exact bundle a previous launch rolled back — it failed
    /// to boot here once and is not given a second attempt. Clears itself when
    /// the box serves anything else.
    RolledBack { content_hash: String },
    /// Box offers an OLDER release than this device already runs. The normal
    /// case, not an error: the phone updates on Apple's cadence and the box
    /// when its owner asks it to. Clears itself when the box is upgraded.
    BoxBehind { box_version: String, have: String },
    /// Neither forward nor backward could be established — one side's version
    /// is not semver and the two are not the same string. Refuse: an update
    /// that cannot be shown to move forward is not taken. `have` is `None` when
    /// this device could not read its own version at all.
    VersionUnreadable {
        box_version: String,
        have: Option<String>,
    },
    /// The downloaded bundle is not what it says it is: its files do not hash
    /// to the `contentHash` in its own manifest, or it carries no readable
    /// manifest. Nothing is applied. Not poisoned either: a box mid-upgrade can
    /// serve a torn tarball once, and the next check simply tries again.
    Corrupt {
        expected: Option<String>,
        got: String,
    },
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
///
/// BOTH documents are required, because `resolve_request_path` hands out both:
/// `index.html` for `/` and `200.html` for every client route, mirroring what
/// `ServeDir` does on the box. A bundle missing one of them would serve half
/// its routes from the overlay and fall the rest through to the BAKED build —
/// whose shell names different content-hashed chunks, so a deep link would
/// 404 its way into a white screen. Rejecting the whole bundle drops the
/// device cleanly onto the build it shipped with instead.
fn is_usable(dir: &Path) -> bool {
    dir.join(MANIFEST_NAME).is_file()
        && dir.join("index.html").is_file()
        && dir.join("200.html").is_file()
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

// ─── The bundle a page load serves from ─────────────────────────────────────
//
// **A page load is pinned to one bundle for its whole life.** A loaded
// `index.html` goes on requesting its own content-hashed chunks, which exist
// only in the bundle it came from. So which bundle serves is decided once,
// when the page document is requested, and every later request of that page
// is answered from the same place.
//
// Until 2026-09-29 the pin was per PROCESS for boot-ok (`capture_booted`) but
// per REQUEST for serving (`active_bundle`, re-read every time), and an apply
// moves `active` mid-session. So after a second update, a running page asked
// for its own chunks and got the new bundle, which does not have them: a
// lazily loaded view 404'd into a blank pane until the app was relaunched.
// Pinning per page load fixes that, and is what lets a staged bundle apply
// with a plain page reload (on both platforms: a Mac is almost never
// relaunched, since closing its window only hides it) instead of waiting for
// the next launch. See agents/plan/local-ui-plan.md.

/// Held while pointers move, by an apply and by a page load settling
/// rollback, so neither sees the other half-done. Held for file writes only,
/// never across a download, so a page load never waits on the network.
fn pointer_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The current page load's bundle, per bundle store (one per app; keyed so the
/// tests, which each use their own store, cannot see each other's pins). No
/// entry: no page has loaded yet. `None`: pinned to the baked build.
fn serving() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, Option<String>>> {
    static SERVING: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, Option<String>>>,
    > = std::sync::OnceLock::new();
    SERVING.get_or_init(Default::default)
}

/// The pin for `app_data`: outer `None` when no page has loaded.
fn pinned(app_data: &Path) -> Option<Option<String>> {
    serving().lock().ok()?.get(&bundles_root(app_data)).cloned()
}

/// The box's own paths. They are data, never part of the app, so the app's
/// scheme answers them with a 404 rather than the SPA fallback. The page is
/// meant to reach them on the box (`backendUrl()` / the fetch rewrite in
/// `apps/web/src/lib/config/backend.ts`); a request that lands here instead
/// is a call site that forgot to, and serving it `200.html` both hid that
/// (an image that renders nothing, an upload that "succeeds") and started a
/// new page load in the middle of the page, moving its bundle pin.
pub fn is_backend_path(uri_path: &str) -> bool {
    const PREFIXES: [&str; 7] = ["/api", "/auth", "/webhook", "/health", "/face", "/ws", "/oauth"];
    PREFIXES
        .iter()
        .any(|p| uri_path == *p || uri_path.starts_with(&format!("{p}/")))
}

/// Is `resolved` (from [`resolve_request_path`]) the document a page load
/// starts with, rather than one of its assets?
pub fn is_page_document(resolved: &str) -> bool {
    resolved == "index.html" || resolved == "200.html"
}

/// The `Content-Type` for a file served from an overlay bundle, by extension.
///
/// Not borrowed from the baked build's file at the same path: a newer build's
/// chunks have new hashed names, so most of its files have no baked twin, and
/// WebKit refuses to run a module script served as anything but JavaScript.
/// Borrowing with a `text/html` default made every update that changed code
/// boot to a white page.
pub fn content_type(resolved: &str) -> &'static str {
    let ext = resolved.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("html") => "text/html",
        Some("js" | "mjs") => "text/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("webmanifest") => "application/manifest+json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("wasm") => "application/wasm",
        Some("mp3") => "audio/mpeg",
        Some("mp4") => "video/mp4",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    }
}

/// Does this request start a page load? Only a navigation does: it asks for
/// the page document and accepts HTML (`accept` is the request's `Accept`
/// header). Every extension-less path resolves to `200.html`, so a `fetch` or
/// an `<img>` aimed at one is answered with the document too, and counting it
/// as a load re-pins the running page to a just-applied bundle it never ran.
/// That page never confirms the bundle, so the next real load rolled it back,
/// and no over-the-air update survived.
pub fn is_page_load(resolved: &str, accept: Option<&str>) -> bool {
    is_page_document(resolved) && accept.is_some_and(|a| a.contains("text/html"))
}

/// A page is loading. Settle rollback ([`resolve_pending`]), then pin this load
/// to whatever is active now. Call for the page document, before reading it.
/// Returns true when a rollback happened (worth a log line).
pub fn begin_page_load(app_data: &Path) -> bool {
    let _pointers = pointer_lock();
    let rolled_back = resolve_pending(app_data);
    let id = active_bundle_id(app_data);
    if let Ok(mut g) = serving().lock() {
        g.insert(bundles_root(app_data), id);
    }
    rolled_back
}

/// The bundle the current page load serves from; `None` for the baked build
/// or before any page has loaded.
pub fn serving_bundle_id(app_data: &Path) -> Option<String> {
    pinned(app_data).flatten()
}

/// Where the current page's assets come from: its pinned bundle, or `None`
/// for the baked build. A pinned bundle that has become unusable degrades to
/// the baked build rather than serving half a page. Before any page has loaded
/// this reads `active`, which is what the first load would pin anyway.
fn serving_dir(app_data: &Path) -> Option<PathBuf> {
    match pinned(app_data) {
        Some(Some(hash)) => {
            let dir = bundles_root(app_data).join(hash);
            is_usable(&dir).then_some(dir)
        }
        Some(None) => None,
        None => active_bundle(app_data),
    }
}

/// Has a newer bundle been applied since this page loaded? Then the next page
/// load picks it up, and the UI can reload at a quiet moment (while hidden).
/// False before any page has loaded.
pub fn update_ready(app_data: &Path) -> bool {
    let Some(pin) = pinned(app_data) else {
        return false;
    };
    active_bundle_id(app_data) != pin
}

/// The page rendered: confirm the bundle it loaded from (see [`mark_boot_ok`]).
pub fn confirm_page_load(app_data: &Path) {
    mark_boot_ok(app_data, serving_bundle_id(app_data).as_deref());
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

/// Resolve rollback state as a page loads, before it is served.
///
/// A pending pointer means a bundle was applied and no page load from it has
/// confirmed yet. That is TWO cases, and they used to be conflated:
///
///   • No load has been attempted (apply runs mid-session; the old bundle kept
///     serving). This load IS the attempt — mark it and serve the bundle.
///   • The previous load attempted it (booting == pending) and never
///     confirmed — so it does not boot. Abandon it, remember it as poisoned,
///     and fall back to whatever it replaced.
///
/// Per page load, not per launch, since 2026-09-29: a staged bundle now takes
/// effect at the next reload (see "The bundle a page load serves from").
///
/// Rolling back on sight of a bare pending pointer — the old behavior — meant
/// a boot-ok landing before the mid-session apply left a good bundle to be
/// "resolved" away at the next launch and re-downloaded, in a loop.
///
/// Returns true when a rollback happened (worth logging; the user sees only
/// that the app works).
pub fn resolve_pending(app_data: &Path) -> bool {
    let root = bundles_root(app_data);
    let Some(pending) = read_pointer(&root, PTR_PENDING) else {
        // No pending bundle — a leftover attempt marker refers to nothing.
        clear_pointer(&root, PTR_BOOTING);
        return false;
    };

    // A pending bundle that is not the ACTIVE one was never served: `apply`
    // died between writing the two pointers. It has not failed to boot — it
    // has not been tried — so clear the marker rather than poisoning a hash
    // this device has no evidence against.
    if read_pointer(&root, PTR_ACTIVE).as_deref() != Some(pending.as_str()) {
        clear_pointer(&root, PTR_PENDING);
        clear_pointer(&root, PTR_BOOTING);
        return false;
    }

    if read_pointer(&root, PTR_BOOTING).as_deref() != Some(pending.as_str()) {
        let _ = write_pointer(&root, PTR_BOOTING, &pending);
        return false;
    }

    // Attempted last launch, never confirmed: it booted badly enough not to
    // land boot-ok. Do not try it again — and do not re-download it either.
    clear_pointer(&root, PTR_PENDING);
    clear_pointer(&root, PTR_BOOTING);
    let _ = fs::write(root.join(POISON_FILE), &pending);
    match read_pointer(&root, PTR_PREVIOUS) {
        Some(prev) if is_usable(&root.join(&prev)) => {
            let _ = write_pointer(&root, PTR_ACTIVE, &prev);
        }
        _ => clear_pointer(&root, PTR_ACTIVE), // back to the baked bundle
    }
    let _ = fs::remove_dir_all(root.join(&pending));
    true
}

/// Abandon an active overlay that is OLDER than the build baked into this
/// binary, and return the version dropped.
///
/// The other half of the forward-only rule, and the half the download gate
/// cannot reach. An App Store update replaces the binary but keeps the app
/// container, so the overlay applied weeks ago survives into a shell whose
/// baked build is newer — and since `active_bundle` is consulted on every asset
/// request, that stale overlay goes on shadowing the newer build it shipped
/// with. `version_gate` stops the device taking anything worse; only this puts
/// it back on what it already has.
///
/// # Why startup, and nowhere else
///
/// This clears the pointer the running session serves from, so it must happen
/// before a page exists. Doing it from the mid-session check thread would swap
/// the bundle under a live page: the `index.html` already loaded from the old
/// overlay goes on requesting its own content-hashed chunks, which by
/// construction are not in the baked build, and the app 404s its way into a
/// white screen. Same reason `apply` only ever takes effect at the next launch.
///
/// # Fail-safe
///
/// Every ambiguity keeps the overlay: no baked version, no active pointer, an
/// unreadable bundle manifest, either side unparseable, or versions merely
/// equal. Only a *strictly* newer baked build displaces one — equal versions
/// mean the overlay is a rebuild of the same release, which is the ordinary
/// state after any OTA and must be left alone.
///
/// Pointers only; the directories stay for `prune` to sweep on the next apply,
/// so reclaiming disk never delays a launch.
pub fn drop_stale_overlay(app_data: &Path, baked_version: Option<&str>) -> Option<String> {
    let baked = baked_version?;
    let root = bundles_root(app_data);
    let active = read_pointer(&root, PTR_ACTIVE)?;
    let have = bundle_version(&root, &active)?;

    let (Ok(baked), Ok(overlay)) = (
        semver::Version::parse(baked),
        semver::Version::parse(&have),
    ) else {
        return None;
    };
    if baked <= overlay {
        return None;
    }

    // Back to the baked build outright, not to `previous`: if the active
    // overlay is behind the binary, whatever it replaced is further behind
    // still. Pending and booting go too — they describe a bundle this device
    // has just stopped serving, and left behind they would have the next launch
    // "roll back" something that is no longer active.
    for ptr in [PTR_ACTIVE, PTR_PENDING, PTR_BOOTING, PTR_PREVIOUS] {
        clear_pointer(&root, ptr);
    }
    Some(have)
}

/// Called by the SPA once it has actually rendered. `booted` is the bundle id
/// the SHELL captured at webview creation — not anything the page reports.
///
/// Confirm only when the pending bundle IS the one this session rendered from.
/// The launch-time check thread can apply a new bundle while an older session
/// is still the one calling home, and that session's boot-ok must not vouch
/// for a bundle it never booted — that race silently disarmed rollback (or,
/// in the other ordering, rolled back a good bundle forever).
pub fn mark_boot_ok(app_data: &Path, booted: Option<&str>) {
    let root = bundles_root(app_data);
    if read_pointer(&root, PTR_PENDING).as_deref() == booted && booted.is_some() {
        clear_pointer(&root, PTR_PENDING);
        clear_pointer(&root, PTR_BOOTING);
    }
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
    let mut keep: Vec<String> = [PTR_ACTIVE, PTR_PREVIOUS, PTR_PENDING]
        .iter()
        .filter_map(|p| read_pointer(&root, p))
        .collect();
    // And the bundle the open page is still serving from: two applies without
    // a reload in between move it out of every pointer, and deleting it would
    // pull the files out from under a live page.
    keep.extend(serving_bundle_id(app_data));

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
        Outcome::RolledBack { content_hash } => serde_json::json!({
            "state": "rolled_back", "contentHash": content_hash,
        }),
        Outcome::BoxBehind { box_version, have } => serde_json::json!({
            "state": "box_behind", "boxVersion": box_version, "have": have,
        }),
        Outcome::VersionUnreadable { box_version, have } => serde_json::json!({
            "state": "version_unreadable", "boxVersion": box_version, "have": have,
        }),
        Outcome::Corrupt { expected, got } => serde_json::json!({
            "state": "corrupt", "expected": expected, "got": got,
        }),
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

// ─── Forward-only ───────────────────────────────────────────────────────────

/// How a version the box offers relates to one this device can already serve.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Step {
    /// Provably not a downgrade — apply.
    Forward,
    /// Provably a downgrade — refuse.
    Backward,
    /// Not orderable. Refuse, because "cannot tell" is not "forward".
    Unreadable,
}

/// Order one version string against another.
///
/// `version` is whatever the build was stamped with
/// (`apps/web/scripts/write-bundle-manifest.mjs`: `GIT_DESCRIBE`, else
/// `VIRTUES_BUILD_VERSION`, else the literal `dev`, with a leading `v`
/// stripped). So ordering is semver, the same rule the box's own downgrade
/// guard uses for release tags (`virtues-core/src/cli/upgrade.rs`). Two
/// deliberate differences from that guard:
///
///   • **An unparseable version refuses here, where `virtues upgrade` waves it
///     through.** That guard runs because an operator typed the command, and
///     blocking them over an odd version string would be worse than the risk it
///     guards. This one runs on a background thread with nobody watching, so
///     the safe default is to do nothing and stay on what we have.
///   • **Byte-equal strings count as forward.** A same-tag rebuild must still
///     update — that is precisely why the manifest carries a content hash
///     *as well as* a version, and it is what keeps the dev loop working, where
///     both sides are stamped the literal `dev` and nothing is orderable.
///     Equality is decided by the content hash before this is ever consulted;
///     reaching here with equal versions means the builds genuinely differ.
fn step(remote: &str, have: &str) -> Step {
    if remote == have {
        return Step::Forward;
    }
    match (semver::Version::parse(remote), semver::Version::parse(have)) {
        (Ok(r), Ok(h)) if r >= h => Step::Forward,
        (Ok(_), Ok(_)) => Step::Backward,
        _ => Step::Unreadable,
    }
}

/// Refuse a bundle that cannot be shown to move forward against **every**
/// version this device can already serve, or `None` to let it through.
///
/// `have` carries up to two entries and both matter:
///
///   • the build baked into this binary — the one an App Store update just
///     replaced, and the floor the device can always fall back to;
///   • the overlay bundle currently active, if any — which can be *older* than
///     the baked build right after the app updates, and must not be treated as
///     the ceiling.
///
/// Empty `have` is not a free pass. No version information at all is the
/// ambiguous case, and ambiguity stays put; the refusal is recorded, so it
/// reads as a stated reason rather than as OTA quietly not working.
fn version_gate(remote: &str, have: &[String]) -> Option<Outcome> {
    if have.is_empty() {
        return Some(Outcome::VersionUnreadable {
            box_version: remote.to_string(),
            have: None,
        });
    }
    // A proven downgrade is the more useful thing to report, so it wins over an
    // unorderable sibling rather than being masked by it.
    let mut unreadable = None;
    for h in have {
        match step(remote, h) {
            Step::Forward => {}
            Step::Backward => {
                return Some(Outcome::BoxBehind {
                    box_version: remote.to_string(),
                    have: h.clone(),
                })
            }
            Step::Unreadable => unreadable = unreadable.or(Some(h)),
        }
    }
    unreadable.map(|h| Outcome::VersionUnreadable {
        box_version: remote.to_string(),
        have: Some(h.clone()),
    })
}

/// The version recorded inside a bundle directory, if it is readable.
fn bundle_version(root: &Path, hash: &str) -> Option<String> {
    let raw = fs::read_to_string(root.join(hash).join(MANIFEST_NAME)).ok()?;
    Manifest::parse(&raw).map(|m| m.version)
}

/// Every version this device can already serve, for `version_gate`.
fn have_versions(root: &Path, baked: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = baked.map(str::to_string).into_iter().collect();
    if let Some(active) = read_pointer(root, PTR_ACTIVE) {
        if let Some(v) = bundle_version(root, &active) {
            if !out.contains(&v) {
                out.push(v);
            }
        }
    }
    out
}

/// Every reason to stop before the tarball is downloaded, in order.
///
/// `Some(outcome)` means stay on what we have and record why; `None` means the
/// offer is good and worth the bytes. Split out from [`check_and_apply`] so the
/// whole decision is testable without a box on the other end of a socket —
/// which is the half of this module that decides what a device runs.
fn decide(
    root: &Path,
    remote: &Manifest,
    shell_surface: u32,
    baked_version: Option<&str>,
) -> Option<Outcome> {
    if remote.min_shell_version > shell_surface {
        return Some(Outcome::ShellTooOld {
            needs: remote.min_shell_version,
            have: shell_surface,
        });
    }

    if read_pointer(root, PTR_ACTIVE).as_deref() == Some(remote.content_hash.as_str()) {
        return Some(Outcome::UpToDate);
    }

    // Forward only. After the content-hash equality above, so the bundle we
    // already run is "up to date" whatever its version string says, and before
    // the download, so a downgrade costs nothing on the wire.
    if let Some(refused) = version_gate(&remote.version, &have_versions(root, baked_version)) {
        return Some(refused);
    }

    // A bundle this device already rolled back gets no second download — the
    // old behavior re-fetched the identical failing bundle on every launch and
    // every foreground, forever.
    //
    // The marker is cleared once the box serves anything else. The docstring
    // above has always claimed it "clears itself"; nothing ever removed the
    // file, so a device carried a permanent record of one bad bundle and the
    // next reader of that directory would have had to guess whether it still
    // meant anything.
    match fs::read_to_string(root.join(POISON_FILE)).ok() {
        Some(poisoned) if poisoned == remote.content_hash => {
            return Some(Outcome::RolledBack {
                content_hash: remote.content_hash.clone(),
            })
        }
        Some(_) => {
            let _ = fs::remove_file(root.join(POISON_FILE));
        }
        None => {}
    }

    None
}

/// Check the box and apply a newer bundle if there is one this shell can run.
///
/// `shell_surface` is `COMMAND_SURFACE_VERSION` — the contract the bundle is
/// checked against before it is allowed anywhere near the active pointer.
///
/// `baked_version` is the `version` of the build compiled into this binary,
/// read off its own `.virtues-bundle.json` rather than asserted — the shell
/// reports what it observed, per the delivery plan's invariant 4. `None` means
/// it could not be read, which the gate treats as ambiguous, not as consent.
pub fn check_and_apply(
    app_data: &Path,
    shell_surface: u32,
    baked_version: Option<&str>,
) -> std::io::Result<Outcome> {
    let Some(body) = http_get(&box_addr(), "/api/web-bundle/version")? else {
        return Ok(Outcome::NoBundleOnBox);
    };
    let Some(remote) = Manifest::parse(&String::from_utf8_lossy(&body)) else {
        return Ok(Outcome::NoBundleOnBox);
    };

    let root = bundles_root(app_data);
    if let Some(stop) = decide(&root, &remote, shell_surface, baked_version) {
        return Ok(stop);
    }

    let Some(tar_gz) = http_get(&box_addr(), "/api/web-bundle/tarball")? else {
        return Ok(Outcome::NoBundleOnBox);
    };

    apply_tarball(app_data, &tar_gz, &remote.content_hash, shell_surface, baked_version)
}

/// Unpack a downloaded bundle, prove it is what it says, and make it active.
///
/// **The bundle is judged by the manifest INSIDE it, never by the offer.** The
/// offer (`/api/web-bundle/version`) and the tarball are two requests, so a
/// box upgraded between them hands over a different build than the one decided
/// on, and filing it under the offer's hash would put one build in another's
/// directory. So: read the manifest the tarball carries, recompute the hash of
/// what was unpacked ([`content_hash`]), refuse on any mismatch, and when the
/// tarball turns out to be a different build than offered, run the same
/// pre-download checks on it before it goes anywhere near a pointer.
///
/// Until 2026-09-29 nothing re-hashed an unpacked bundle; the directory was
/// simply named after the hash the offer claimed.
///
/// Split from [`check_and_apply`] so everything after the download is testable
/// with a real archive and no box.
fn apply_tarball(
    app_data: &Path,
    tar_gz: &[u8],
    offered_hash: &str,
    shell_surface: u32,
    baked_version: Option<&str>,
) -> std::io::Result<Outcome> {
    let root = bundles_root(app_data);

    // Unpack beside the target, then rename into place: a half-written
    // directory must never be reachable through the pointer.
    let staging = root.join(format!(".staging-{offered_hash}"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    let discard = |outcome: Outcome| -> std::io::Result<Outcome> {
        let _ = fs::remove_dir_all(&staging);
        Ok(outcome)
    };
    if let Err(e) = unpack(tar_gz, &staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    if !is_usable(&staging) {
        return discard(Outcome::NoBundleOnBox);
    }
    let packed = fs::read_to_string(staging.join(MANIFEST_NAME))
        .ok()
        .and_then(|m| Manifest::parse(&m));
    let actual = content_hash(&staging)?;
    let remote = match packed {
        Some(m) if m.content_hash == actual => m,
        Some(m) => {
            return discard(Outcome::Corrupt {
                expected: Some(m.content_hash),
                got: actual,
            })
        }
        None => return discard(Outcome::Corrupt { expected: None, got: actual }),
    };
    if remote.content_hash != offered_hash {
        // A different build than the one decided on: decide again, on it.
        if let Some(stop) = decide(&root, &remote, shell_surface, baked_version) {
            return discard(stop);
        }
    }

    let target = root.join(&remote.content_hash);
    if is_usable(&target) {
        // Already here under this hash, so the same files: keep the directory
        // (a page may be serving from it) and drop the download.
        let _ = fs::remove_dir_all(&staging);
    } else {
        let _ = fs::remove_dir_all(&target);
        fs::rename(&staging, &target)?;
    }

    // The pointer writes below and a page load's rollback settling
    // (`begin_page_load`) must not interleave: a load landing between PENDING
    // and ACTIVE would read "pending names a bundle that is not active", clear
    // it as a torn apply, and leave the new bundle active with no rollback.
    let _pointers = pointer_lock();

    // Remember what we are replacing, as the rollback target, only if it is
    // known to boot. An active bundle still pending never confirmed a page
    // load (two applies without a reload in between), so it stays out and the
    // last confirmed `previous` stands: rolling back must land on something
    // that has rendered.
    let pending_now = read_pointer(&root, PTR_PENDING);
    match read_pointer(&root, PTR_ACTIVE) {
        Some(prev) if pending_now.as_deref() != Some(prev.as_str()) => {
            let _ = write_pointer(&root, PTR_PREVIOUS, &prev);
        }
        Some(_) => {}
        None => clear_pointer(&root, PTR_PREVIOUS), // replacing the baked bundle
    }
    // PENDING first, then ACTIVE. These are two files, so a crash lands
    // between them, and the order decides which way that cuts. ACTIVE-first
    // leaves a bundle being served with no pending marker — which is precisely
    // the state rollback keys on, so a bundle that does not boot would have no
    // way back and the app would be stuck on it. PENDING-first leaves a marker
    // naming a bundle that was never activated, which `resolve_pending`
    // now recognizes and clears.
    write_pointer(&root, PTR_PENDING, &remote.content_hash)?;
    write_pointer(&root, PTR_ACTIVE, &remote.content_hash)?;
    // Released before the sweep, so a page load never waits on deleting old
    // bundles. The sweep reads the pointers fresh, and anything a load
    // settles in the meantime lands on a bundle a pointer still names.
    drop(_pointers);

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

    // `Accept-Encoding: identity` is not decoration. This client understands
    // no content coding, and the box's static serving already hands out `.gz`
    // siblings by negotiation — a default-encoding request is one middleware
    // away from returning gzip that `unpack` would try to read as a tarball.
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nAccept-Encoding: identity\r\n\
         Connection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes())?;

    let mut raw = Vec::new();
    stream.take(MAX_TARBALL_BYTES as u64).read_to_end(&mut raw)?;
    Ok(split_http_response(&raw))
}

/// Split a raw HTTP/1.1 response into its body, if the status is 2xx.
///
/// Separated from the socket so it can be tested — response framing is where
/// hand-rolled HTTP goes wrong. It went wrong here: this used to take
/// everything after the header terminator as the body, which is correct for
/// exactly one of the three framings HTTP/1.1 allows. It happens to be the one
/// the box uses today (a buffered `Body::from(bytes)` behind
/// `Connection: close`), so the bug was invisible — and would have surfaced as
/// "OTA stopped working" the day the tarball handler started streaming or
/// anything added a compression layer, with chunk-size lines being fed to a
/// gzip decoder.
///
/// All three framings, in the precedence RFC 9112 gives them:
///   • `Transfer-Encoding: chunked` wins over any content-length;
///   • `Content-Length` otherwise;
///   • close-delimited (read to EOF) when neither is present.
fn split_http_response(raw: &[u8]) -> Option<Vec<u8>> {
    let head_end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..head_end]).ok()?;

    let mut lines = head.split("\r\n");
    let status = lines.next()?.split_whitespace().nth(1)?;
    if !status.starts_with('2') {
        return None;
    }

    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    for line in lines {
        // A line we cannot read is skipped, not fatal: refusing the whole
        // response over one odd header would be a worse failure than ignoring
        // it, and the framing headers we care about are the ones below.
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value.parse().ok();
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked = value
                .split(',')
                .any(|c| c.trim().eq_ignore_ascii_case("chunked"));
        }
    }

    let body = &raw[head_end + 4..];
    if chunked {
        return dechunk(body);
    }
    match content_length {
        // Shorter than advertised means the read was cut off — `MAX_TARBALL_BYTES`
        // does exactly that at the cap. A truncated archive must not be
        // reported as a body; the caller would unpack a torn bundle.
        Some(n) if n <= body.len() => Some(body[..n].to_vec()),
        Some(_) => None,
        None => Some(body.to_vec()),
    }
}

/// Reassemble a `Transfer-Encoding: chunked` body.
///
/// `None` for anything malformed or truncated, which the caller reads as "the
/// box said nothing usable" — the same fail-safe every other path here takes.
fn dechunk(mut body: &[u8]) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    loop {
        let eol = body.windows(2).position(|w| w == b"\r\n")?;
        let line = std::str::from_utf8(&body[..eol]).ok()?;
        // Chunk extensions (`1a;name=value`) are legal and ignorable.
        let size = usize::from_str_radix(line.split(';').next()?.trim(), 16).ok()?;
        body = &body[eol + 2..];
        if size == 0 {
            return Some(out);
        }
        if out.len().saturating_add(size) > MAX_TARBALL_BYTES {
            return None;
        }
        // size + 2: the chunk, then its trailing CRLF.
        if body.len() < size + 2 {
            return None;
        }
        out.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
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
    // being handed HTML — a JS request answered with HTML fails in a way that
    // is very hard to read from the console.
    //
    // Routes get `200.html`, NOT `index.html`. One rule, spelled the same way
    // on both serving paths: the box's `ServeDir` answers `/` from
    // `index.html` and every other route from its `200.html` fallback
    // (`virtues-core/src/server/mod.rs`). The two files are byte-identical in
    // today's build, which is exactly why the divergence was invisible — and
    // why it would have surfaced as a routing bug on the day someone gave the
    // root route something to prerender.
    let last = clean.rsplit('/').next().unwrap_or("");
    if last.contains('.') {
        Some(clean.to_string())
    } else {
        Some("200.html".into())
    }
}

/// Read `path` out of the current page's overlay bundle, if it has one and the
/// file is there.
///
/// `None` means "fall through to the baked bundle" for every reason: no
/// overlay, missing file, unreadable file. The caller must always have that
/// fallback — this function never being able to fail is the property that keeps
/// a bad bundle from costing the app its UI.
pub fn read_from_overlay(app_data: &Path, path: &str) -> Option<Vec<u8>> {
    let dir = serving_dir(app_data)?;
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

/// A bundle's content hash, exactly as `apps/web/scripts/write-bundle-manifest.mjs`
/// computes it: SHA-256 over each file's path relative to the bundle root
/// (`/`-separated) followed by its bytes, in sorted path order, skipping the
/// `.gz` siblings; the first 16 hex characters. The manifest itself is skipped
/// too: the build stamps it AFTER hashing, so it was never part of the hash.
///
/// Checked against a real box's shipped build (2026-09-29, 597 files): this
/// recomputation equals the `contentHash` its manifest carries.
fn content_hash(dir: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                walk(root, &path, out)?;
            } else if path.extension().map_or(true, |e| e != "gz") {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                if rel != MANIFEST_NAME {
                    out.push(rel);
                }
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files)?;
    files.sort();
    let mut hash = Sha256::new();
    for rel in &files {
        hash.update(rel.as_bytes());
        hash.update(fs::read(dir.join(rel))?);
    }
    let digest = hash.finalize();
    let mut hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    hex.truncate(16);
    Ok(hex)
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
        plant_at(root, hash, "0.1.0");
    }

    fn plant_at(root: &Path, hash: &str, version: &str) {
        let d = root.join(hash);
        fs::create_dir_all(&d).unwrap();
        // Both documents: `/` is served from one and every client route from
        // the other, exactly as the box serves them.
        fs::write(d.join("index.html"), "<html></html>").unwrap();
        fs::write(d.join("200.html"), "<html></html>").unwrap();
        fs::write(
            d.join(MANIFEST_NAME),
            format!(r#"{{"version":"{version}","contentHash":"{hash}","minShellVersion":1}}"#),
        )
        .unwrap();
    }

    fn offer(version: &str, hash: &str) -> Manifest {
        Manifest {
            version: version.into(),
            content_hash: hash.into(),
            min_shell_version: 1,
        }
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

    // ── forward only ────────────────────────────────────────────────────────

    #[test]
    fn a_newer_box_is_taken() {
        let d = tmp();
        let root = bundles_root(&d);
        // Nothing applied yet: this device runs the build baked into the app.
        assert_eq!(decide(&root, &offer("0.1.5", "new1"), 4, Some("0.1.4")), None);
        // Prereleases order below their release, exactly as semver intends —
        // the whole reason the version line was reset to 0.1.0 (Cargo.toml).
        assert_eq!(
            decide(&root, &offer("0.1.5", "new1"), 4, Some("0.1.5-staging.7")),
            None
        );
    }

    #[test]
    fn an_older_box_is_refused_and_recorded() {
        let d = tmp();
        let root = bundles_root(&d);
        // The ordinary state: the phone updated through TestFlight, the box has
        // not been upgraded yet. Without this it OTAs itself backwards.
        let stop = decide(&root, &offer("0.1.4", "old9"), 4, Some("0.1.6"));
        assert_eq!(
            stop,
            Some(Outcome::BoxBehind {
                box_version: "0.1.4".into(),
                have: "0.1.6".into(),
            })
        );

        record_outcome(&d, &stop.unwrap());
        let v = last_outcome(&d).expect("recorded");
        assert_eq!(v["state"], "box_behind");
        assert_eq!(v["boxVersion"], "0.1.4");
        assert_eq!(v["have"], "0.1.6");
    }

    #[test]
    fn an_overlay_does_not_shadow_a_newer_baked_build() {
        // An App Store update can land a baked build NEWER than the overlay
        // applied weeks ago. The offer is ordered against both, so the stale
        // overlay cannot vouch for a bundle the baked build has already passed.
        let d = tmp();
        let root = bundles_root(&d);
        plant_at(&root, "stale1", "0.1.3");
        write_pointer(&root, PTR_ACTIVE, "stale1").unwrap();

        assert_eq!(
            decide(&root, &offer("0.1.4", "next2"), 4, Some("0.1.6")),
            Some(Outcome::BoxBehind {
                box_version: "0.1.4".into(),
                have: "0.1.6".into(),
            }),
            "forward against the overlay, backward against the baked build"
        );
    }

    #[test]
    fn the_bundle_we_run_is_up_to_date_whatever_its_version_says() {
        let d = tmp();
        let root = bundles_root(&d);
        plant_at(&root, "same3", "0.1.4");
        write_pointer(&root, PTR_ACTIVE, "same3").unwrap();
        // Content-hash equality answers first — it is the only thing that
        // actually says "this is the build you are running".
        assert_eq!(
            decide(&root, &offer("0.1.4", "same3"), 4, Some("0.1.6")),
            Some(Outcome::UpToDate)
        );
    }

    #[test]
    fn an_unorderable_version_refuses_rather_than_risking_a_downgrade() {
        let d = tmp();
        let root = bundles_root(&d);
        // A box build that was never stamped. `virtues upgrade` waves this case
        // through because an operator asked; nobody asked for this one.
        assert_eq!(
            decide(&root, &offer("dev", "odd4"), 4, Some("0.1.6")),
            Some(Outcome::VersionUnreadable {
                box_version: "dev".into(),
                have: Some("0.1.6".into()),
            })
        );
        // Our own side unstamped is the same answer from the other direction.
        assert_eq!(
            decide(&root, &offer("0.1.6", "odd4"), 4, Some("dev")),
            Some(Outcome::VersionUnreadable {
                box_version: "0.1.6".into(),
                have: Some("dev".into()),
            })
        );
        // And no version information at all is ambiguity, not consent.
        assert_eq!(
            decide(&root, &offer("0.1.6", "odd4"), 4, None),
            Some(Outcome::VersionUnreadable {
                box_version: "0.1.6".into(),
                have: None,
            })
        );

        record_outcome(&d, &decide(&root, &offer("dev", "odd4"), 4, None).unwrap());
        let v = last_outcome(&d).expect("recorded");
        assert_eq!(v["state"], "version_unreadable");
        assert!(v["have"].is_null());
    }

    #[test]
    fn an_app_update_drops_an_overlay_it_has_overtaken() {
        // The state a TestFlight update leaves behind: the container survives,
        // so the overlay fetched by the OLD binary is still active inside a new
        // one whose own build is newer. Nothing else notices — `active_bundle`
        // just keeps serving it.
        let d = tmp();
        let root = bundles_root(&d);
        plant_at(&root, "older1", "0.1.4");
        plant_at(&root, "oldest0", "0.1.2");
        write_pointer(&root, PTR_ACTIVE, "older1").unwrap();
        write_pointer(&root, PTR_PREVIOUS, "oldest0").unwrap();
        write_pointer(&root, PTR_PENDING, "older1").unwrap();

        assert_eq!(drop_stale_overlay(&d, Some("0.1.7")).as_deref(), Some("0.1.4"));
        assert_eq!(active_bundle(&d), None, "serves the build it shipped with");
        // `previous` goes too: if the active overlay is behind the binary,
        // what it replaced is further behind still.
        assert_eq!(read_pointer(&root, PTR_PREVIOUS), None);
        // And so do pending/booting, which named a bundle we no longer serve —
        // left behind, the next launch would "roll back" what is not active.
        assert_eq!(read_pointer(&root, PTR_PENDING), None);
        assert!(!resolve_pending(&d), "nothing left to resolve");
    }

    #[test]
    fn a_current_overlay_is_left_alone() {
        let d = tmp();
        let root = bundles_root(&d);
        plant_at(&root, "ahead1", "0.1.7");
        write_pointer(&root, PTR_ACTIVE, "ahead1").unwrap();

        // Newer than the baked build — the ordinary, working state.
        assert_eq!(drop_stale_overlay(&d, Some("0.1.4")), None);
        // Equal is a rebuild of the same release, not a stale overlay. Only a
        // STRICTLY newer baked build displaces one.
        assert_eq!(drop_stale_overlay(&d, Some("0.1.7")), None);
        assert!(active_bundle(&d).is_some(), "still served");
    }

    #[test]
    fn an_overlay_survives_every_ambiguity() {
        let d = tmp();
        let root = bundles_root(&d);
        plant_at(&root, "kept1", "0.1.4");
        write_pointer(&root, PTR_ACTIVE, "kept1").unwrap();

        assert_eq!(drop_stale_overlay(&d, None), None, "no baked version");
        assert_eq!(drop_stale_overlay(&d, Some("dev")), None, "unparseable baked");
        assert!(active_bundle(&d).is_some());

        // An unreadable overlay manifest is the rollback path's business, not
        // this one's — dropping the pointer on a read error would turn a
        // transient failure into a downgrade of its own.
        fs::write(root.join("kept1").join(MANIFEST_NAME), "not json").unwrap();
        assert_eq!(drop_stale_overlay(&d, Some("9.9.9")), None);
        assert_eq!(read_pointer(&root, PTR_ACTIVE).as_deref(), Some("kept1"));
    }

    #[test]
    fn a_same_tag_rebuild_still_updates() {
        // Byte-equal versions are forward: a rebuild at one tag changes the
        // content hash and nothing else, and the dev loop stamps the literal
        // `dev` on both sides forever. Refusing here would freeze both.
        let d = tmp();
        let root = bundles_root(&d);
        assert_eq!(decide(&root, &offer("0.1.5", "rebuilt"), 4, Some("0.1.5")), None);
        assert_eq!(decide(&root, &offer("dev", "rebuilt"), 4, Some("dev")), None);
    }

    #[test]
    fn a_too_new_bundle_is_still_refused_first() {
        // The shell-surface guard answers before the version gate: a bundle
        // that is both newer AND unrunnable should say why it cannot run.
        let d = tmp();
        let root = bundles_root(&d);
        let mut m = offer("0.9.0", "future5");
        m.min_shell_version = 9;
        assert_eq!(
            decide(&root, &m, 4, Some("0.1.6")),
            Some(Outcome::ShellTooOld { needs: 9, have: 4 })
        );
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
        assert_eq!(active_bundle(&d), None, "dir without the documents");

        // Half a bundle is not a bundle: serving `/` from the overlay and every
        // route from the baked build mixes two shells and 404s the chunks.
        fs::write(root.join("deadbeef/index.html"), "<html></html>").unwrap();
        fs::write(
            root.join("deadbeef").join(MANIFEST_NAME),
            r#"{"version":"0.1.0","contentHash":"deadbeef","minShellVersion":1}"#,
        )
        .unwrap();
        assert_eq!(active_bundle(&d), None, "no 200.html");

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

        // First launch after the apply: this IS the attempt, so it is marked
        // and served, not rolled back.
        assert!(!resolve_pending(&d), "first launch attempts it");
        assert_eq!(read_pointer(&root, PTR_ACTIVE).as_deref(), Some("new2"));
        // Second launch with the attempt still unconfirmed: it does not boot.
        assert!(resolve_pending(&d));
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

        assert!(!resolve_pending(&d), "first launch attempts it");
        assert!(resolve_pending(&d));
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

        mark_boot_ok(&d, Some("good3")); // the SPA rendered, from that bundle
        assert!(!resolve_pending(&d), "nothing pending to resolve");
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
    fn http_body_split_honors_all_three_framings() {
        // Content-Length wins over whatever else arrived on the socket — a
        // keep-alive peer can leave the next response in the buffer.
        assert_eq!(
            split_http_response(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nbodyLEFTOVER"),
            Some(b"body".to_vec())
        );
        // Short of what was advertised is a truncated read, not a body. This is
        // what MAX_TARBALL_BYTES produces at the cap, and unpacking it would
        // write a torn bundle.
        assert_eq!(
            split_http_response(b"HTTP/1.1 200 OK\r\nContent-Length: 99\r\n\r\nbody"),
            None
        );
        // Chunked, case-insensitive, with a chunk extension on the first size
        // line and a trailing zero chunk.
        assert_eq!(
            split_http_response(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: Chunked\r\n\r\n\
                  4;x=1\r\nbody\r\n3\r\nend\r\n0\r\n\r\n"
            ),
            Some(b"bodyend".to_vec())
        );
        // Chunked beats a content-length if a peer sends both.
        assert_eq!(
            split_http_response(
                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nTransfer-Encoding: chunked\r\n\r\n\
                  4\r\nbody\r\n0\r\n\r\n"
            ),
            Some(b"body".to_vec())
        );
        // A chunked body cut off before its terminator is refused outright.
        assert_eq!(
            split_http_response(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nbo"),
            None
        );
        // Neither header: close-delimited, the framing this has always handled.
        assert_eq!(
            split_http_response(b"HTTP/1.1 200 OK\r\nX: 1\r\n\r\nbody"),
            Some(b"body".to_vec())
        );
    }

    #[test]
    fn request_paths_resolve_spa_routes_to_the_fallback_document() {
        // Assets keep their path.
        assert_eq!(resolve_request_path("/_app/x.js").as_deref(), Some("_app/x.js"));
        assert_eq!(resolve_request_path("/favicon.png").as_deref(), Some("favicon.png"));
        // `/` is a directory request: the box answers it from index.html.
        assert_eq!(resolve_request_path("/").as_deref(), Some("index.html"));
        // Every other client route gets the SPA fallback — the SAME file the
        // box's ServeDir falls back to, which is the whole point.
        assert_eq!(resolve_request_path("/wiki/foo").as_deref(), Some("200.html"));
        assert_eq!(resolve_request_path("/virtues/billing").as_deref(), Some("200.html"));
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
    fn box_paths_are_never_the_app() {
        assert!(is_backend_path("/api/drive/files/f1/download"));
        assert!(is_backend_path("/api"));
        assert!(is_backend_path("/ws/yjs/p1"));
        assert!(is_backend_path("/health"));
        assert!(!is_backend_path("/apiary"), "a route that merely starts with the letters");
        assert!(!is_backend_path("/"));
        assert!(!is_backend_path("/setup"));
        assert!(!is_backend_path("/_app/immutable/a.js"));
    }

    #[test]
    fn overlay_files_are_typed_by_extension() {
        // A chunk the baked build has never seen must still run as a module.
        assert_eq!(content_type("_app/immutable/chunks/pPhroByK.js"), "text/javascript");
        assert_eq!(content_type("_app/immutable/assets/0.Ab.css"), "text/css");
        assert_eq!(content_type("index.html"), "text/html");
        assert_eq!(content_type("fonts/EBGaramond-Regular-latin.woff2"), "font/woff2");
        assert_eq!(content_type("favicon.PNG"), "image/png");
        assert_eq!(content_type("LICENSE"), "application/octet-stream");
    }

    #[test]
    fn only_a_navigation_starts_a_page_load() {
        let nav = Some("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8");
        assert!(is_page_load("index.html", nav));
        assert!(is_page_load("200.html", nav));
        // A fetch or an image that lands on an extension-less path gets the
        // document back, but it is not a page load.
        assert!(!is_page_load("200.html", Some("*/*")));
        assert!(!is_page_load("200.html", Some("image/webp,image/*,*/*;q=0.8")));
        assert!(!is_page_load("200.html", None));
        assert!(!is_page_load("_app/immutable/a.js", nav));
    }

    #[test]
    fn a_page_keeps_its_own_bundle_until_the_next_load() {
        // The bug this pin exists for: an apply mid-session moved `active`, and
        // the open page's next chunk request was answered from the new bundle,
        // which does not have it.
        let app = tmp();
        let root = bundles_root(&app);
        plant(&root, "pinA");
        fs::write(root.join("pinA").join("chunk-a.js"), "A").unwrap();
        write_pointer(&root, PTR_ACTIVE, "pinA").unwrap();
        assert!(!begin_page_load(&app));
        assert!(!update_ready(&app));

        // A background check applies B while the page is open.
        plant(&root, "pinB");
        write_pointer(&root, PTR_PREVIOUS, "pinA").unwrap();
        write_pointer(&root, PTR_PENDING, "pinB").unwrap();
        write_pointer(&root, PTR_ACTIVE, "pinB").unwrap();
        assert_eq!(read_from_overlay(&app, "chunk-a.js").as_deref(), Some(&b"A"[..]), "still served from A");
        assert!(update_ready(&app), "B waits for the next load");
        prune(&app);
        assert!(root.join("pinA").exists(), "the open page's bundle is never pruned");

        // The next load takes B, and marks it as the attempt.
        assert!(!begin_page_load(&app));
        assert_eq!(serving_bundle_id(&app).as_deref(), Some("pinB"));
        assert_eq!(read_from_overlay(&app, "chunk-a.js"), None, "A's chunk is not in B");
        assert!(!update_ready(&app));
        confirm_page_load(&app);
        assert_eq!(read_pointer(&root, PTR_PENDING), None, "confirmed");
    }

    #[test]
    fn two_applies_without_a_reload_keep_the_confirmed_rollback_target() {
        // A confirmed; B applied but never loaded; then C applied. If C fails
        // to boot, rollback must land on A, which rendered, not on B.
        let app = tmp();
        let root = bundles_root(&app);
        plant(&root, "prevA");
        write_pointer(&root, PTR_ACTIVE, "prevA").unwrap();

        let tar_b = tarball(&bundle("0.2.0", BUNDLE_HASH));
        assert!(matches!(apply_tarball(&app, &tar_b, BUNDLE_HASH, 1, Some("0.1.0")).unwrap(), Outcome::Applied { .. }));
        assert_eq!(read_pointer(&root, PTR_PREVIOUS).as_deref(), Some("prevA"));

        // C: a different bundle, hashed for real, applied while B is pending.
        let c_dir = tmp();
        fs::write(c_dir.join("index.html"), "<html>c</html>").unwrap();
        fs::write(c_dir.join("200.html"), "<html>c</html>").unwrap();
        let c_hash = content_hash(&c_dir).unwrap();
        let tar_c = tarball(&[
            ("index.html".into(), "<html>c</html>".into()),
            ("200.html".into(), "<html>c</html>".into()),
            (
                MANIFEST_NAME.into(),
                format!(r#"{{"version":"0.3.0","contentHash":"{c_hash}","minShellVersion":1}}"#),
            ),
        ]);
        assert!(matches!(apply_tarball(&app, &tar_c, &c_hash, 1, Some("0.1.0")).unwrap(), Outcome::Applied { .. }));
        assert_eq!(read_pointer(&root, PTR_PREVIOUS).as_deref(), Some("prevA"), "the unconfirmed B never becomes previous");
        assert_eq!(read_pointer(&root, PTR_ACTIVE).as_deref(), Some(c_hash.as_str()));
    }

    #[test]
    fn a_load_that_never_confirms_is_rolled_back_by_the_next() {
        let app = tmp();
        let root = bundles_root(&app);
        plant(&root, "goodA");
        plant(&root, "badB");
        write_pointer(&root, PTR_PREVIOUS, "goodA").unwrap();
        write_pointer(&root, PTR_PENDING, "badB").unwrap();
        write_pointer(&root, PTR_ACTIVE, "badB").unwrap();
        assert!(!begin_page_load(&app), "the first load is the attempt");
        assert_eq!(serving_bundle_id(&app).as_deref(), Some("badB"));
        // No confirm: the page never rendered. The next load rolls back.
        assert!(begin_page_load(&app));
        assert_eq!(serving_bundle_id(&app).as_deref(), Some("goodA"));
        assert_eq!(fs::read_to_string(root.join(POISON_FILE)).unwrap(), "badB");
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

    /// A gzipped tar of `files`, as the box's `/api/web-bundle/tarball` builds it.
    fn tarball(files: &[(String, String)]) -> Vec<u8> {
        let src = tmp();
        for (path, body) in files {
            let p = src.join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, body).unwrap();
        }
        let mut buf = Vec::new();
        {
            let enc = flate2::write::GzEncoder::new(&mut buf, flate2::Compression::fast());
            let mut b = tar::Builder::new(enc);
            b.append_dir_all(".", &src).unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }
        buf
    }

    /// A small bundle whose manifest claims `hash`. The `.gz` file is an
    /// encoding of a real one, not content, so it never enters the hash.
    fn bundle(version: &str, hash: &str) -> Vec<(String, String)> {
        vec![
            ("index.html".into(), "<html>i</html>".into()),
            ("200.html".into(), "<html>f</html>".into()),
            ("_app/immutable/a.js".into(), "console.log(1)".into()),
            ("_app/immutable/a.js.gz".into(), "gzipped".into()),
            (
                MANIFEST_NAME.into(),
                format!(r#"{{"version":"{version}","contentHash":"{hash}","minShellVersion":1}}"#),
            ),
        ]
    }

    /// What `bundle`'s files really hash to, computed with write-bundle-manifest.mjs's
    /// own algorithm in Node. Pinned here so this test proves the two
    /// implementations AGREE rather than that Rust agrees with itself: if it
    /// breaks, every OTA would be refused as corrupt. Fix the drift, never the
    /// constant.
    const BUNDLE_HASH: &str = "6519392619004f08";

    #[test]
    fn the_hash_matches_the_build_script() {
        let d = tmp();
        for (path, body) in bundle("0", "anything") {
            let p = d.join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, body).unwrap();
        }
        assert_eq!(content_hash(&d).unwrap(), BUNDLE_HASH);
    }

    #[test]
    fn a_bundle_that_hashes_to_its_manifest_is_applied() {
        let app = tmp();
        let tar = tarball(&bundle("0.2.0", BUNDLE_HASH));
        let out = apply_tarball(&app, &tar, BUNDLE_HASH, 1, Some("0.1.0")).unwrap();
        assert_eq!(out, Outcome::Applied { content_hash: BUNDLE_HASH.into() });
        assert_eq!(read_pointer(&bundles_root(&app), PTR_ACTIVE).as_deref(), Some(BUNDLE_HASH));
    }

    #[test]
    fn files_that_do_not_match_their_manifest_are_refused() {
        let app = tmp();
        let fake = "0000000000000000";
        let tar = tarball(&bundle("0.2.0", fake));
        let out = apply_tarball(&app, &tar, fake, 1, Some("0.1.0")).unwrap();
        assert_eq!(
            out,
            Outcome::Corrupt { expected: Some(fake.into()), got: BUNDLE_HASH.into() }
        );
        let root = bundles_root(&app);
        assert_eq!(read_pointer(&root, PTR_ACTIVE), None, "nothing may become active");
        assert!(!root.join(fake).exists());
        assert!(!root.join(format!(".staging-{fake}")).exists(), "staging is cleaned up");
    }

    #[test]
    fn a_different_build_than_offered_goes_under_its_own_hash() {
        // The box was upgraded between the offer and the download. The
        // tarball is a sound bundle, just not the one decided on: it is filed
        // under the hash it actually has, after passing the same checks.
        let app = tmp();
        let tar = tarball(&bundle("0.3.0", BUNDLE_HASH));
        let out = apply_tarball(&app, &tar, "offered0000000000", 1, Some("0.1.0")).unwrap();
        assert_eq!(out, Outcome::Applied { content_hash: BUNDLE_HASH.into() });
        let root = bundles_root(&app);
        assert!(root.join(BUNDLE_HASH).exists());
        assert!(!root.join("offered0000000000").exists());
    }

    #[test]
    fn a_different_build_that_moved_backward_is_refused() {
        // Same race, but the box went BACK between the two requests: the
        // forward-only gate judges what arrived, not what was offered.
        let app = tmp();
        let tar = tarball(&bundle("0.0.9", BUNDLE_HASH));
        let out = apply_tarball(&app, &tar, "offered0000000000", 1, Some("0.1.0")).unwrap();
        assert!(matches!(out, Outcome::BoxBehind { .. }), "{out:?}");
        assert_eq!(read_pointer(&bundles_root(&app), PTR_ACTIVE), None);
    }

    #[test]
    fn a_poison_marker_is_dropped_once_the_box_moves_on() {
        let d = tmp();
        let root = bundles_root(&d);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(POISON_FILE), "bad1").unwrap();

        // The bundle that failed is still refused.
        assert_eq!(
            decide(&root, &offer("0.1.5", "bad1"), 4, Some("0.1.4")),
            Some(Outcome::RolledBack { content_hash: "bad1".into() })
        );
        assert!(root.join(POISON_FILE).exists(), "still the offered bundle");

        // A different bundle is taken, and the marker it has outlived goes.
        assert_eq!(decide(&root, &offer("0.1.5", "good2"), 4, Some("0.1.4")), None);
        assert!(!root.join(POISON_FILE).exists(), "marker cleared");
    }

    #[test]
    fn a_torn_apply_is_cleared_rather_than_poisoned() {
        let d = tmp();
        let root = bundles_root(&d);
        plant(&root, "new1");
        // `apply` died between writing PENDING and writing ACTIVE: the bundle
        // was never served, so there is no evidence it fails to boot.
        write_pointer(&root, PTR_PENDING, "new1").unwrap();

        assert!(!resolve_pending(&d), "not a rollback");
        assert_eq!(read_pointer(&root, PTR_PENDING), None, "marker cleared");
        assert!(
            !root.join(POISON_FILE).exists(),
            "a bundle that was never tried is not poisoned"
        );
        assert!(root.join("new1").is_dir(), "and its files are left alone");
    }
}
