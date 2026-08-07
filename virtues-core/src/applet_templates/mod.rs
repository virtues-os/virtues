//! Applet template loader, source catalog, and reconciler.
//!
//! Two on-disk inputs:
//!
//! 1. `actions/sources.toml` — the source catalog. Holds only `[[source]]`
//!    entries (one tile per provider on the Sources UI). Read once on first
//!    access; baked into the binary at compile time as a fallback for the
//!    case where the file is missing at runtime.
//!
//! 2. `actions/<name>/manifest.toml` — one per action. Each is a flat TOML
//!    document with the action's declarative metadata (name, runtime,
//!    command, triggers, schedule, etc.). Globbed at parse time;
//!    folder name becomes the action's `id_prefix` if not explicitly set.
//!
//! On startup, `reconcile_templates`:
//!   - Loads sources from sources.toml into a static catalog (lookup by `id`).
//!   - Globs `actions/*/manifest.toml`, parses each, and upserts into
//!     `app_applets`. Manifest-managed fields (name, owner, agent, runtime,
//!     command, triggers, condition, source) are overwritten
//!     on every system reconcile. User-managed runtime state (enabled,
//!     schedule, config, memory) is preserved.
//!   - Per-credential manifests fan out one row per matching `credentials`
//!     row, exactly as before.

use std::sync::{OnceLock, RwLock};

use crate::error::{Error, Result};
use serde::Deserialize;
use sqlx::PgPool;


// ─────────────────────────────────────────────────────────────────────────────
// TOML schema
// ─────────────────────────────────────────────────────────────────────────────

/// One `[[source]]` entry in `actions/sources.toml`. Catalog tile.
#[derive(Debug, Deserialize, Clone)]
pub struct Source {
    /// Stable id used as `credentials.source_id` and as `[[action]].source.id`.
    pub id: String,
    pub display_name: String,
    pub icon: String,
    pub description: String,
    pub auth: SourceAuth,
    /// Where this source's code can be read. Provenance, **not** an install or
    /// update path — the iOS and Mac collectors ship through the App Store, a
    /// notarized DMG, and the Tauri updater, and no git ref can deliver them.
    /// On an appliance whose pitch is verifiability, "here is the exact source
    /// for the collector you just paired" is worth saying out loud.
    #[serde(default)]
    pub repo: Option<String>,
    /// Branch, tag, or path within the repo, when the whole repo is too coarse.
    #[serde(default)]
    pub repo_ref: Option<String>,
    /// Which package contributed this row, relative to whichever root it was
    /// found in. Empty for the box's own top-level `sources.toml`. Populated by
    /// the loader from the on-disk path, never read from TOML — a file cannot
    /// know its own location. Without it a source cannot be attributed to the
    /// thing that would have to be uninstalled to remove it.
    #[serde(skip)]
    pub dir: String,
}

/// How a source authenticates. Matches the three auth kinds in the charter.
#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceAuth {
    /// Server mints a bearer (iOS-style). Webhook router validates HMAC lookup.
    SelfIssuedBearer,
    /// Browser redirect through the OAuth proxy (the `oauth` routes in
    /// `services/virtues-api`). Covers OAuth and Plaid Link.
    ViaProxy { start_path: String },
    /// User pastes one or more strings (MCP tokens, BYO API keys).
    ApiKey { fields: Vec<String> },
}

impl SourceAuth {
    /// Stable wire string for API responses + frontend dispatch.
    /// `self_issued_bearer` | `via_proxy` | `api_key`.
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::SelfIssuedBearer => "self_issued_bearer",
            Self::ViaProxy { .. } => "via_proxy",
            Self::ApiKey { .. } => "api_key",
        }
    }
}

/// One per-action `manifest.toml` (or a `[[action]]` row if we ever resurrect
/// a central registry). `id_prefix` is optional in the manifest — derived
/// from the folder name as `action_<folder>` when absent.
#[derive(Debug, Deserialize, Clone)]
struct Template {
    /// Stable id prefix. For non-per-credential entries this becomes the
    /// final action id. For per-credential entries the materialized id is
    /// `{id_prefix}_{credential_id}`. When omitted in `manifest.toml` the
    /// loader derives it from the folder name.
    #[serde(default)]
    id_prefix: Option<String>,
    name: String,
    /// The one-sentence intent: what this applet is for, in the user's terms.
    ///
    /// Every manifest in the tree has carried one since the beginning and this
    /// struct never had the field, so serde discarded it silently on every
    /// load — there is no `deny_unknown_fields` here, and an unknown key costs
    /// nothing but the thing it was carrying. The list's plain-English line,
    /// the detail headline, and the authoring gate's headline all read from
    /// this, which is why all three were empty.
    #[serde(default)]
    description: Option<String>,
    owner: String,
    #[serde(default)]
    triggers: Vec<String>,
    /// Cron seed for the live `schedule` value (SQL-owned after seeding).
    /// Canonical manifest key is `schedule`; `default_cron` is still accepted
    /// so a folder written against the old spelling — an import, an older
    /// backup — keeps working.
    #[serde(default, alias = "default_cron")]
    schedule: Option<String>,
    #[serde(default = "default_true")]
    default_enabled: bool,
    #[serde(default)]
    condition: Option<String>,
    /// Lifecycle: absent = forever · `"once"` = archive after first success ·
    /// SQL boolean = archive when true (evaluated post-success).
    #[serde(default)]
    until: Option<String>,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    per_credential: bool,
    /// Reference to a `[[source]]` entry in this same file. Required when
    /// `per_credential = true` — fan-out matches credentials by `source_id`.
    /// Absent for credential-less templates (housekeeping).
    #[serde(default)]
    source: Option<SourceRef>,
    /// Runtime contract — how the action executes.
    ///
    /// - `function` (default) — fork-per-trigger CLI; reads `AppletInput` JSON
    ///   from stdin, writes `AppletOutput` JSON to stdout, exits.
    /// - `view` — pure Svelte component; never invoked server-side. The
    ///   runner skips `view` actions; the scheduler refuses to enqueue them.
    /// Argv to spawn (JSON array in SQL). A bare `command[0]` resolves to a
    /// Cargo-built action binary; anything else (`python3 main.py`, `./x`) runs
    /// via PATH. Used by both `function` and `service` runtimes; unset for `view`.
    #[serde(default)]
    command: Option<Vec<String>>,
    /// Free-form config that flows from manifest into `app_applets.config`.
    /// Notable use: `[config.limits]`, the ceilings the runner enforces (see
    /// `applet_runner::limits`). The old `[config.view]` key is gone with the
    /// Svelte view registry it addressed — faces are sandboxed iframes now.
    ///
    /// For system-owned actions, reconcile **overwrites** this field on every
    /// startup (the manifest is canonical). For user-owned actions, it's only
    /// seeded once via `ON CONFLICT DO NOTHING`; subsequent edits via the UI win.
    #[serde(default)]
    config: Option<toml::Value>,
    /// Ontologies this applet writes, by registry name (`health_sleep`, not
    /// `data_health_sleep`). Declared rather than inferred: the box cannot know
    /// what a subprocess will INSERT into, and an installed package's streams
    /// have to be discoverable the same way a shipped one's are.
    ///
    /// This is what lets a dark stream say *which source would fill it* instead
    /// of the flat "not connected" that conflated "nothing provides this",
    /// "provided but switched off", and "the box derives this itself".
    #[serde(default)]
    writes: Vec<String>,
    /// This applet legitimately needs `VIRTUES_ENCRYPTION_KEY` — it re-encrypts
    /// secrets rather than merely reading the decrypted ones handed to it on
    /// stdin. Only `credential_refresh` does today.
    ///
    /// Forking one is refused: a fork moves the folder into the state root,
    /// which flips it from shipped to unshipped, which drops the key. The
    /// breakage would surface hours later as credentials silently going
    /// `reauth_required`.
    #[serde(default)]
    needs_vault_key: bool,
    /// Where this folder came from, when it is a copy of something else:
    /// `<origin>@<version>` — e.g. `virtues@v0.3.0` for a forked built-in, or
    /// `https://host/owner/repo@<sha>` for an edited import.
    ///
    /// Written by the fork operation, then carried in the manifest rather than
    /// only in the database, so it survives a DB rebuild and travels with the
    /// folder if it is ever committed or copied. Without it recorded at fork
    /// time the question "what did this diverge from" is unanswerable later.
    #[serde(default)]
    forked_from: Option<String>,
    /// Manifest folder relative to the actions root. Populated by the loader
    /// from the on-disk path, never read from TOML — a file can't know its own
    /// location. Examples: `morning_brief`, `team-pack/actions/foo`.
    #[serde(skip)]
    dir: String,
}

#[derive(Debug, Deserialize, Clone)]
struct SourceRef {
    id: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct ParsedTemplates {
    #[serde(default)]
    source: Vec<Source>,
    #[serde(default)]
    action: Vec<Template>,
    /// How many templates came from the SHIPPED root specifically.
    ///
    /// Reconcile's system-GC deletes `owner='system'` rows absent from the
    /// catalog, guarded on the catalog being non-empty so a load failure
    /// can't wipe the table. Once the catalog merges two roots that guard
    /// silently weakens: a shipped root that failed to load, plus one
    /// authored applet in the state root, is a "non-empty" catalog — and
    /// every system row would be deleted. The guard must key on THIS.
    #[serde(skip)]
    shipped_count: usize,
}

fn default_true() -> bool {
    true
}

// ─────────────────────────────────────────────────────────────────────────────
// Catalog loader
//
// Two inputs:
//   - sources.toml — single file, holds only [[source]] rows. Baked at compile
//     time as a fallback so the binary is self-contained when shipped without
//     the actions tree. The dev-loop source of truth is the on-disk file;
//     `cargo build` re-bakes it on every recompile.
//   - actions/<name>/manifest.toml — globbed at first access. Each is a
//     standalone Template document. Folder name supplies `id_prefix` when the
//     manifest doesn't set it explicitly.
//
// The loader runs once via OnceLock; subsequent calls hit the cache.
// ─────────────────────────────────────────────────────────────────────────────

/// Compile-time fallback for the source catalog — used when `actions/sources.toml`
/// can't be read at runtime (binary shipped without the tree, tests, etc.).
const SOURCES_TOML: &str = include_str!("../../../applets/sources.toml");

/// Absolute path to the on-disk `actions/` root. Resolved in priority order:
///
/// 1. `$VIRTUES_ACTIONS_DIR` — set by the installer in the box env file (and
///    by the Docker image), so a deployed box points at the real install
///    location. This is the production path; without it a binary-only deploy
///    would have no actions at all (the tree is *not* baked into the binary).
/// 2. The well-known install location (`/usr/local/share/virtues/actions`,
///    mirroring where the installer drops `web/`) — used only if it exists, so
///    dev builds fall through to (3).
/// 3. The in-tree `actions/` relative to this crate, via `CARGO_MANIFEST_DIR`,
///    so `cargo run`/tests work regardless of the user's CWD.
///
/// Importers (`/api/admin/actions/import-git`) clone into whichever directory
/// this resolves to, so the standard scanner picks the new folder up — there
/// is no separate "imported actions" location.
pub fn shipped_root() -> std::path::PathBuf {
    for var in ["VIRTUES_APPLETS_DIR", "VIRTUES_ACTIONS_DIR"] {
        if let Ok(dir) = std::env::var(var) {
            if !dir.is_empty() {
                return std::path::PathBuf::from(dir);
            }
        }
    }
    let installed = std::path::PathBuf::from(WELL_KNOWN_APPLETS_DIR);
    let installed = if installed.is_dir() {
        installed
    } else {
        let legacy = std::path::PathBuf::from(WELL_KNOWN_APPLETS_DIR_LEGACY);
        if legacy.is_dir() { legacy } else { installed }
    };
    if installed.is_dir() {
        return installed;
    }
    let core_manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    core_manifest.join(ACTIONS_DIR_FROM_CORE)
}

/// The WRITABLE applet tree: per-box state that must survive upgrades.
///
/// Everything this process creates goes here — chat-authored applets under
/// `user/`, imported Git packs under their slug. Nothing the installer ships
/// lives here, and the installer never rewrites it.
///
/// This exists because the two trees have opposite lifecycles. [`shipped_root`]
/// is package data: root-owned, read-only, replaced wholesale on every
/// release. Authored applets are irreplaceable user data. Keeping both under
/// one path forced a single answer to "does upgrade delete this?", and the
/// answer was wrong for one of them — the slot flip `remove_dir_all`'d the
/// shipped tree with `user/` inside it, and applet authoring failed outright
/// on a fresh box because nothing ever created a service-writable directory.
///
/// `/var/lib/virtues` is where the service's other durable state already lives
/// (the lake, models), so this follows the existing convention rather than
/// inventing one.
/// Resolution mirrors [`shipped_root`]: explicit env var, then the deployed
/// location, then an in-tree path so `cargo run` works. Without that last tier
/// a dev box resolved to `/var/lib/virtues/applets`, which is root-owned and
/// not creatable — authoring failed with a permission error on a machine that
/// has no box install at all.
///
/// The box-vs-dev discriminator is `/var/lib/virtues` existing, the same
/// marker `main.rs` uses to detect a box install. Keying on the applets dir
/// itself would be wrong: it legitimately doesn't exist yet on a box that has
/// never authored anything, and falling back to a source path there would
/// write applets somewhere production never reads.
pub fn state_root() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("VIRTUES_APPLET_STATE_DIR") {
        if !dir.is_empty() {
            return std::path::PathBuf::from(dir);
        }
    }
    let installed = std::path::PathBuf::from(WELL_KNOWN_APPLET_STATE_DIR);
    if installed.parent().is_some_and(|p| p.is_dir()) {
        return installed;
    }
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEV_APPLET_STATE_DIR_FROM_CORE)
}

/// Resolve a `dir` (as recorded on an `app_applets` row) to its folder on
/// disk. State wins over shipped.
///
/// Precedence is a feature, not just conflict resolution: authoring an applet
/// whose dir matches a shipped one shadows it, and deleting your copy reverts
/// cleanly to the shipped version. Forking a system applet to tweak it becomes
/// a first-class operation instead of an edit the next upgrade eats.
///
/// Falls back to the shipped path when neither exists, so error messages name
/// the location a reader expects rather than a state dir they've never heard
/// of.
pub fn resolve_applet_dir(dir: &str) -> std::path::PathBuf {
    let stateful = state_root().join(dir);
    if stateful.is_dir() {
        return stateful;
    }
    shipped_root().join(dir)
}

/// Default deployed location, matching the installer's `share/virtues/web`
/// convention (`InstallConfig::web_dir`). Kept in sync with the path the
/// installer copies `actions/` to and sets `VIRTUES_ACTIONS_DIR` to.
const WELL_KNOWN_APPLETS_DIR: &str = "/usr/local/share/virtues/applets";
/// Writable applet state, alongside the lake and models under the service's
/// data dir. The installer creates it `virtues:virtues`; systemd's
/// `StateDirectory=` would be the more idiomatic owner of that guarantee.
const WELL_KNOWN_APPLET_STATE_DIR: &str = "/var/lib/virtues/applets";
/// Dev-only applet state, relative to virtues-core. Deliberately NOT
/// `applets/user/` — that lives inside the shipped tree, which is the mixing
/// this split exists to undo. Gitignored.
const DEV_APPLET_STATE_DIR_FROM_CORE: &str = "../.applet-state";
/// Pre-rename deployments (transition fallback; removed once the fleet moves).
const WELL_KNOWN_APPLETS_DIR_LEGACY: &str = "/usr/local/share/virtues/actions";

/// The actions directory, relative to the repo root. Resolved against
/// `CARGO_MANIFEST_DIR`'s parent at runtime so `cargo run` works regardless
/// of the user's CWD.
const ACTIONS_DIR_FROM_CORE: &str = "../applets";

/// Cached merged catalog. Initialized lazily on first access; the inner
/// `RwLock` allows `reload_catalog()` to replace the contents in-place when
/// `/api/admin/reconcile` fires after a manifest edit.
static CATALOG: OnceLock<RwLock<ParsedTemplates>> = OnceLock::new();

fn catalog_lock() -> &'static RwLock<ParsedTemplates> {
    CATALOG.get_or_init(|| RwLock::new(load_catalog()))
}

/// Force a re-read of `actions/sources.toml` and every
/// `actions/*/manifest.toml`. Subsequent `lookup_source` /
/// `list_sources_sorted` / `reconcile_templates` calls see the new data.
///
/// Called by the `/api/admin/reconcile` handler after a user (or LLM) edits
/// a manifest on disk.
/// Serializes every reload+reconcile pass (chat setups, admin endpoint, git
/// import). Reconcile is global and non-transactional; concurrent passes can
/// interleave GC with upserts. One mutex, one writer at a time.
pub fn reconcile_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// The one blessed way to apply on-disk changes: reload the catalog then
/// reconcile rows. Serialization lives inside `reconcile_templates` itself,
/// so EVERY caller (this wrapper, the admin endpoint, git import, OAuth/
/// pairing fan-out, boot) is serialized — not just this path. `reload_catalog`
/// is an atomic in-memory swap and needs no lock.
pub async fn reload_and_reconcile(db: &PgPool) -> Result<usize> {
    reload_catalog();
    reconcile_templates(db).await
}

pub fn reload_catalog() {
    let fresh = load_catalog();
    let lock = catalog_lock();
    let mut guard = lock.write().expect("catalog rwlock poisoned");
    *guard = fresh;
}

/// Read and parse a single `manifest.toml`. Returns None on read errors (which
/// we log) and panics on parse errors (which are author bugs that must surface
/// loudly).
///
/// `dir` is the manifest's folder relative to the actions root and is the
/// **identity** of the action — `id_prefix` is derived from it as
/// `action_<dir>` with `/` rewritten to `__`. This is what keeps built-ins
/// from colliding with imports: built-ins have flat dirs (`morning_brief`),
/// imports always live under a slug (`team-pack/morning_brief`), and
/// from-chat user actions live under `user/` — so their derived ids never
/// clash unless a user explicitly hand-edits a built-in's path.
///
/// A manifest may still set `id_prefix` explicitly (legacy escape hatch); we
/// honor it, but the dir-derived form is the new default.
fn parse_template(manifest_path: &std::path::Path, dir: &str) -> Option<Template> {
    let text = match std::fs::read_to_string(manifest_path) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(
                path = %manifest_path.display(),
                error = %e,
                "failed to read action manifest; skipping"
            );
            return None;
        }
    };
    let mut tmpl: Template = match toml::from_str(&text) {
        Ok(t) => t,
        Err(e) => {
            // Log and skip, never panic. This used to abort the process, and
            // since imports land in the writable state root the folder survives
            // a restart — so one typo in a third party's manifest poisoned
            // `load_catalog` permanently, taking reconcile, boot, and the admin
            // Reconcile button (the only in-app lever) down with it. Recovery
            // required shell access. The sibling `read_sources_file` was already
            // hardened for exactly this; this path was missed.
            tracing::error!(
                path = %manifest_path.display(),
                error = %e,
                "skipping unparseable manifest.toml"
            );
            return None;
        }
    };
    if tmpl.id_prefix.is_none() {
        // Migration 0077 rewrote the stored ids to this prefix. `manifest.toml`
        // may still pin an explicit `id_prefix`; none currently does, and one
        // that did would be taken at its word.
        tmpl.id_prefix = Some(format!("applet_{}", dir.replace('/', "__")));
    }
    tmpl.dir = dir.to_string();
    Some(tmpl)
}

/// Read one `sources.toml`, tagging every row with the package folder it came
/// from. A parse failure is logged and skipped rather than fatal: a package
/// with a broken TOML must not take the whole catalog down with it. (The box's
/// own top-level file is still a panic — that one is ours and a typo in it is a
/// build error we want loudly.)
fn read_sources_file(path: &std::path::Path, dir: &str) -> Vec<Source> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    match toml::from_str::<ParsedTemplates>(&text) {
        Ok(doc) => doc
            .source
            .into_iter()
            .map(|mut s| {
                s.dir = dir.to_string();
                s
            })
            .collect(),
        Err(e) => {
            tracing::error!(path = %path.display(), error = %e, "skipping unparseable sources.toml");
            Vec::new()
        }
    }
}

/// Collect `[[source]]` rows contributed by packages under one root.
///
/// Same shape as `scan_root` does for manifests: a package is a folder, and it
/// may declare sources in its own `sources.toml`. One level only — a pack's
/// sources belong to the pack, not to each applet inside it.
fn scan_sources(root: &std::path::Path) -> Vec<Source> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut dirs: Vec<_> = entries.flatten().collect();
    dirs.sort_by_key(|e| e.file_name());
    for entry in dirs {
        if !entry.path().is_dir() {
            continue;
        }
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let candidate = entry.path().join("sources.toml");
        if candidate.is_file() {
            out.extend(read_sources_file(&candidate, &name));
        }
    }
    out
}

fn load_catalog() -> ParsedTemplates {
    let applets_dir = shipped_root();

    // 1. Sources. Three layers, last-wins-by-id, mirroring how actions merge
    //    below: the box's own catalog, then packages that shipped with it, then
    //    packages installed on this box. That last layer is what lets a new
    //    provider arrive as a directory instead of a release — and it is why a
    //    source now carries the `dir` that contributed it.
    let sources_path = applets_dir.join("sources.toml");
    let base: Vec<Source> = match std::fs::read_to_string(&sources_path) {
        Ok(text) => toml::from_str::<ParsedTemplates>(&text)
            .unwrap_or_else(|e| panic!("failed to parse {}: {e}", sources_path.display()))
            .source,
        Err(_) => toml::from_str::<ParsedTemplates>(SOURCES_TOML)
            .unwrap_or_else(|e| panic!("failed to parse baked sources.toml: {e}"))
            .source,
    };

    let mut sources: Vec<Source> = base;
    for contributed in scan_sources(&applets_dir)
        .into_iter()
        .chain(scan_sources(&state_root()))
    {
        match sources.iter().position(|s| s.id == contributed.id) {
            Some(i) => {
                tracing::info!(
                    source_id = %contributed.id,
                    shadowed_by = %contributed.dir,
                    "package source shadows an earlier definition"
                );
                sources[i] = contributed;
            }
            None => sources.push(contributed),
        }
    }
    let sources_doc = ParsedTemplates {
        source: sources,
        action: Vec::new(),
        shipped_count: 0,
    };

    // 2. Per-folder manifests. The scanner rule is intentionally flat:
    //
    //    - A subdir of `actions/` with a `manifest.toml` IS an action.
    //      `dir = <folder>` (e.g. `morning_brief`).
    //
    //    - A subdir of `actions/` WITHOUT a `manifest.toml` is treated as a
    //      namespace folder (a Git pack, `user/` for from-chat actions, etc).
    //      We descend one level and pick up any `<folder>/<child>/manifest.toml`.
    //      `dir = <folder>/<child>` (e.g. `team-pack/morning_brief`).
    //
    // No nested `actions/` is required inside a pack repo — the pack's own
    // top-level folders ARE its action folders. This keeps imported repos
    // shallow (`actions/team-pack/foo/bin/run`, not
    // `actions/team-pack/actions/foo/bin/run`).
    //
    // Reserved namespaces:
    //    - `user/`  — from-chat–authored actions live here.
    //    - any imported slug (e.g. `team-pack/`) — owned by that import.
    //
    // The `dir` recorded on each Template is the folder path relative to the
    // actions root; reconcile writes it onto every `app_applets` row, and
    // `id_prefix` defaults to `action_<dir-with-/-as-__>` so different
    // namespaces never collide.
    // Both roots, shipped first: the state root's entries override shipped
    // ones with the same `dir`, so an authored applet shadows a system applet
    // of that name and deleting it reverts to shipped.
    let mut actions = scan_root(&applets_dir);
    let shipped_count = actions.len();
    for t in scan_root(&state_root()) {
        match actions.iter().position(|e| e.dir == t.dir) {
            Some(i) => actions[i] = t,
            None => actions.push(t),
        }
    }

    // Reject duplicate id_prefixes loudly but without crashing the process.
    // The dir-based id derivation makes natural collisions almost impossible
    // — to land here someone has either hand-set `id_prefix = "..."` to the
    // same value in two manifests, or hand-edited `actions/<built-in>/` to
    // shadow a shipped action. Either way: surface, drop the offender, keep
    // the first occurrence, and let the rest of the catalog load.
    let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut deduped: Vec<Template> = Vec::with_capacity(actions.len());
    for t in actions {
        let id = match t.id_prefix.as_deref() {
            Some(s) => s.to_string(),
            None => {
                tracing::error!(dir = %t.dir, "manifest missing id_prefix; skipping");
                continue;
            }
        };
        if let Some(first_dir) = seen.get(&id) {
            tracing::error!(
                id_prefix = %id,
                first_dir = %first_dir,
                colliding_dir = %t.dir,
                "two manifests claim the same id_prefix; keeping the first, dropping this one. \
                 Rename `id_prefix` in one of the manifests, or change the folder so the dir-derived id differs."
            );
            continue;
        }
        seen.insert(id, t.dir.clone());
        deduped.push(t);
    }

    ParsedTemplates {
        source: sources_doc.source,
        action: deduped,
        shipped_count,
    }
}

/// Walk one applet root and parse every manifest under it. Same rule for both
/// roots — see the scanner notes in `load_catalog`.
fn scan_root(root: &std::path::Path) -> Vec<Template> {
    let mut actions: Vec<Template> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let folder_name = match path.file_name().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };

            // Single action at top level.
            let direct_manifest = path.join("manifest.toml");
            if direct_manifest.exists() {
                if let Some(t) = parse_template(&direct_manifest, &folder_name) {
                    actions.push(t);
                }
                continue;
            }

            // Namespace folder — descend one level.
            let inner_entries = match std::fs::read_dir(&path) {
                Ok(it) => it,
                Err(_) => continue,
            };
            for inner in inner_entries.flatten() {
                let inner_path = inner.path();
                if !inner_path.is_dir() {
                    continue;
                }
                let inner_name = match inner_path.file_name().and_then(|s| s.to_str()) {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                let inner_manifest = inner_path.join("manifest.toml");
                if !inner_manifest.exists() {
                    continue;
                }
                let dir = format!("{folder_name}/{inner_name}");
                if let Some(t) = parse_template(&inner_manifest, &dir) {
                    actions.push(t);
                }
            }
        }
    }

    actions
}

/// Mirror an ai-owned applet's enabled flag into its manifest
/// (`default_enabled`) so a DB rebuilt from disk restores the user's last
/// choice. Best-effort — only chat-authored folders (`user/` namespace) are
/// ever touched, and failures just log.
pub fn mirror_enabled_to_manifest(applet_id: &str, enabled: bool) {
    let Some(dir) = dir_for_applet_id(applet_id) else {
        return;
    };
    if !dir.starts_with("user/") {
        return;
    }
    let path = resolve_applet_dir(&dir).join("manifest.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(mut doc) = text.parse::<toml::Value>() else {
        return;
    };
    if let Some(table) = doc.as_table_mut() {
        table.insert("default_enabled".into(), toml::Value::Boolean(enabled));
        match toml::to_string_pretty(&doc) {
            Ok(out) => {
                if let Err(e) = std::fs::write(&path, out) {
                    tracing::warn!(applet_id, error = %e, "enabled mirror write failed");
                }
            }
            Err(e) => tracing::warn!(applet_id, error = %e, "enabled mirror serialize failed"),
        }
    }
}

/// Copy a shipped applet into the state root so the owner can change it.
///
/// This is what "fork" means on a box. It needs no git remote and no network:
/// the state root already shadows the shipped root by folder name, and deleting
/// the copy already reverts to the shipped version — that precedence is an
/// existing, documented feature. All this adds is the copy and the record of
/// what it diverged from.
///
/// Stamps `forked_from = "virtues@<version>"` into the copied manifest so the
/// answer survives a database rebuild and travels with the folder.
///
/// Returns the folder that was created. Errors if the applet is unknown, if it
/// did not come from the shipped root (nothing to fork — it is already yours),
/// or if a copy already exists.
pub async fn fork_applet(db: &PgPool, applet_id: &str) -> Result<String> {
    let dir = dir_for_applet_id(applet_id)
        .ok_or_else(|| Error::Other(format!("unknown applet: {applet_id}")))?;

    let shipped = shipped_root().join(&dir);
    if !shipped.is_dir() {
        return Err(Error::Other(format!(
            "applet {applet_id} has no shipped folder to fork"
        )));
    }
    if template_needs_vault_key(&dir) {
        return Err(Error::Other(format!(
            "{dir} cannot be forked: it re-encrypts stored secrets, and a fork \
             runs without the vault key — token refresh would stop working \
             silently. Change it in place or file an issue instead."
        )));
    }
    let target = state_root().join(&dir);
    if target.exists() {
        return Err(Error::Other(format!(
            "{dir} is already forked onto this box"
        )));
    }

    copy_tree(&shipped, &target)?;
    stamp_forked_from(
        &target.join("manifest.toml"),
        &format!("virtues@{}", crate::codename::version()),
    )?;

    // The copy only takes effect once the catalog re-reads both roots.
    reload_and_reconcile(db).await?;
    Ok(dir)
}

/// Whether the applet behind this id declares it needs the vault key. Public
/// twin of [`template_needs_vault_key`] for the runner, which holds an id
/// rather than a folder — and must resolve fan-out ids too.
pub fn declares_vault_key_need(applet_id: &str) -> bool {
    match dir_for_applet_id(applet_id) {
        Some(dir) => template_needs_vault_key(&dir),
        None => false,
    }
}

/// Whether the manifest at `dir` declares it needs the vault key.
fn template_needs_vault_key(dir: &str) -> bool {
    let guard = catalog_lock().read().expect("catalog rwlock poisoned");
    guard
        .action
        .iter()
        .any(|t| t.dir == dir && t.needs_vault_key)
}

/// Recursive copy, skipping dot-directories. `.git` in particular must not be
/// carried into a fork: it would point the copy at the upstream remote and, for
/// an authenticated clone, bring the credential in its URL along with it.
fn copy_tree(from: &std::path::Path, to: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(to)
        .map_err(|e| Error::Other(format!("create {}: {e}", to.display())))?;
    let entries = std::fs::read_dir(from)
        .map_err(|e| Error::Other(format!("read {}: {e}", from.display())))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        // Don't follow links out of the tree while copying.
        let Ok(meta) = entry.path().symlink_metadata() else {
            continue;
        };
        if meta.is_symlink() {
            continue;
        }
        let src = entry.path();
        let dst = to.join(&name);
        if meta.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)
                .map_err(|e| Error::Other(format!("copy {}: {e}", src.display())))?;
        }
    }
    Ok(())
}

/// Add (or replace) the `forked_from` key in a manifest, textually. Rewriting
/// the file through the TOML serializer would drop its comments, which for an
/// applet manifest are most of what a reader came for.
fn stamp_forked_from(manifest: &std::path::Path, origin: &str) -> Result<()> {
    let text = std::fs::read_to_string(manifest)
        .map_err(|e| Error::Other(format!("read {}: {e}", manifest.display())))?;
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim_start().starts_with("forked_from"))
        .collect();
    let stamped = format!("forked_from = \"{origin}\"\n{}", kept.join("\n"));
    std::fs::write(manifest, stamped)
        .map_err(|e| Error::Other(format!("write {}: {e}", manifest.display())))
}

/// Resolve the manifest folder (relative to an applet root) that produced
/// an action id. Matches the base id (`id_prefix`) and per-credential /
/// per-device fan-out ids (`<id_prefix>_<anchor>`). Used by the face server
/// to root static serving at the applet's folder.
pub fn dir_for_applet_id(applet_id: &str) -> Option<String> {
    let guard = catalog_lock().read().expect("catalog rwlock poisoned");
    guard
        .action
        .iter()
        .find(|t| {
            t.id_prefix.as_deref().is_some_and(|p| {
                applet_id == p || applet_id.strip_prefix(p).is_some_and(|r| r.starts_with('_'))
            })
        })
        .map(|t| t.dir.clone())
}

/// Which catalog sources can produce a given ontology, by source id.
///
/// Built from the `writes` each template declares, joined to the source that
/// template fans out from. Empty means nothing installed writes it — which is
/// a different fact from "connected but silent", and the difference is the
/// whole reason this exists.
pub fn sources_writing(ontology: &str) -> Vec<String> {
    let guard = catalog_lock().read().expect("catalog rwlock poisoned");
    let mut out: Vec<String> = guard
        .action
        .iter()
        .filter(|t| t.writes.iter().any(|w| w == ontology))
        .filter_map(|t| t.source.as_ref().map(|s| s.id.clone()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The inverse of [`sources_writing`]: which ontologies a source can produce.
///
/// Catalog-side question — "what would connecting this give me" — where
/// `sources_writing` answers the stream-side one, "what would fill this".
pub fn ontologies_written_by(source_id: &str) -> Vec<String> {
    let guard = catalog_lock().read().expect("catalog rwlock poisoned");
    let mut out: Vec<String> = guard
        .action
        .iter()
        .filter(|t| t.source.as_ref().is_some_and(|s| s.id == source_id))
        .flat_map(|t| t.writes.iter().cloned())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Look up a `[[source]]` entry by its id. Returns an owned clone so the
/// catalog rwlock isn't held across the caller's await points.
pub fn lookup_source(id: &str) -> Option<Source> {
    let guard = catalog_lock().read().expect("catalog rwlock poisoned");
    guard.source.iter().find(|s| s.id == id).cloned()
}

/// All `[[source]]` entries sorted by `display_name`. Used by the catalog API
/// for stable UI ordering. Returns owned clones (same lock-release rationale).
pub fn list_sources_sorted() -> Vec<Source> {
    let guard = catalog_lock().read().expect("catalog rwlock poisoned");
    let mut sorted: Vec<Source> = guard.source.iter().cloned().collect();
    sorted.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    sorted
}

// ─────────────────────────────────────────────────────────────────────────────
// Reconciliation
// ─────────────────────────────────────────────────────────────────────────────

/// Reconcile `app_applets` rows against the on-disk catalog.
///
/// Snapshots the catalog under a brief read lock, releases the lock, then
/// performs SQL writes against the snapshot. This avoids holding the
/// rwlock across `await` points.
///
/// Returns the number of rows upserted.
pub async fn reconcile_templates(db: &PgPool) -> Result<usize> {
    // Serialize the whole reconcile against every other caller — the pass is
    // global and non-transactional, so concurrent GC-deletes + upserts (a chat
    // setup racing an OAuth callback / admin reconcile / boot) would interleave.
    // The lock lives here, not in a wrapper, so bare callers are covered too.
    let _guard = reconcile_lock().lock().await;

    let templates: ParsedTemplates = {
        let guard = catalog_lock().read().expect("catalog rwlock poisoned");
        guard.clone()
    };

    // GC pass: delete fan-out action rows whose anchor is GONE — a deleted
    // credential row (OAuth/api actions) or a revoked/absent device (ingest
    // actions). The revoke paths handle this inline, but any state drift
    // (direct SQL, import, bug) leaves orphans. Nullify run FKs first so history
    // is preserved under `applet_id = NULL`.
    //
    // Deliberately NOT keyed on credential status: a recoverable blip
    // (`reauth_required`, refresh error) must not destroy the row's operational
    // state (archived_at, memory, sync cursors in config). Inactive-but-present
    // credentials just fail/skip at run time and surface in run status; the row
    // survives to resume when the credential recovers. Device revocation is
    // permanent, so the device clause still keys on revoked_at.
    const ORPHAN_PREDICATE: &str = "(credential_id IS NOT NULL \
             AND credential_id NOT IN (SELECT id FROM credentials)) \
          OR (device_id IS NOT NULL \
             AND device_id NOT IN (SELECT id FROM app_device WHERE revoked_at IS NULL))";
    let pruned = sqlx::query(&format!(
        "UPDATE app_applet_runs SET applet_id = NULL \
         WHERE applet_id IN (SELECT id FROM app_applets WHERE {ORPHAN_PREDICATE})"
    ))
    .execute(db)
    .await?
    .rows_affected();

    let deleted = sqlx::query(&format!("DELETE FROM app_applets WHERE {ORPHAN_PREDICATE}"))
        .execute(db)
        .await?
        .rows_affected();

    if deleted > 0 {
        tracing::info!(
            deleted,
            runs_nullified = pruned,
            "reconcile GC: removed fan-out actions for inactive credentials / revoked devices"
        );
    }

    let mut upserted = 0usize;
    // Every action id the current catalog expands to. Used by the template-GC
    // pass below to drop system rows whose template no longer exists.
    let mut live_ids: Vec<String> = Vec::new();

    for template in &templates.action {
        // The loader fills `id_prefix` from the folder name when missing, so
        // by this point it's always Some. Defensive check for non-globbed
        // sources (tests, future inline registries).
        let id_prefix = match template.id_prefix.as_deref() {
            Some(s) => s,
            None => {
                tracing::warn!("template missing id_prefix; skipping");
                continue;
            }
        };

        // A face-only applet is never invoked server-side, so an empty
        // `triggers` list is its canonical shape. For anything that DOES run,
        // empty triggers means it can never fire — fail reconcile so a
        // manifest typo surfaces immediately rather than silently dropping the
        // applet from the catalog.
        //
        // Derived from which fields are set, not from a declared `runtime`.
        // The two could disagree, and when they did the declaration won: a
        // manifest with a command and `runtime = "view"` passed this check and
        // then never ran. This is the same derivation the runner and the API
        // already use.
        let runs_server_side = template.command.as_ref().is_some_and(|c| !c.is_empty())
            || template.agent.as_deref().is_some_and(|a| !a.trim().is_empty());
        if template.triggers.is_empty() && runs_server_side {
            return Err(Error::Other(format!(
                "template {id_prefix} has an empty triggers list but does run (it has a \
                 command or an agent prompt) — give it at least one of: cron, manual, \
                 tool, api, webhook"
            )));
        }

        // Webhook invariant: any action accepting webhook posts MUST resolve to
        // an identity to authorize the post — a device_id (device sources) or a
        // credential_id (OAuth/api). Both come only from per_credential fan-out.
        if template.triggers.iter().any(|t| t == "webhook") && !template.per_credential {
            return Err(Error::Other(format!(
                "template {} has 'webhook' trigger but per_credential=false — webhook actions must fan out per device/credential",
                id_prefix
            )));
        }

        if template.per_credential {
            let source_id = template
                .source
                .as_ref()
                .map(|s| s.id.as_str())
                .ok_or_else(|| {
                    Error::Other(format!(
                        "template {} has per_credential = true but no source block",
                        id_prefix
                    ))
                })?;

            // Skip, don't abort. This used to `return Err`, which unwound the
            // whole pass — after the orphan GC had already deleted rows and
            // before the system GC ran — so every remaining template was
            // skipped and boot, admin reconcile, the OAuth callback and
            // pair-consume all failed together. That was survivable while the
            // source list shipped with the box; now that a package can
            // contribute (and remove) sources, one package that deletes a
            // source while leaving a manifest behind would brick reconcile for
            // everything else on the box.
            let Some(source) = lookup_source(source_id) else {
                tracing::error!(
                    template = %id_prefix,
                    source_id = %source_id,
                    "template references an unknown source; skipping it. The package \
                     that declares this source may be missing or failed to parse."
                );
                continue;
            };

            if matches!(source.auth, SourceAuth::SelfIssuedBearer) {
                // Device source (iOS/Mac/sensor): fan out per DEVICE. The device's
                // allowlisted iroh key authorizes its `/webhook/:applet_id` posts,
                // so the action is anchored on device_id — no credential/bearer.
                let device_ids: Vec<(String,)> = sqlx::query_as(
                    "SELECT id FROM app_device WHERE source_id = $1 AND revoked_at IS NULL",
                )
                .bind(source_id)
                .fetch_all(db)
                .await?;
                for (device_id,) in device_ids {
                    let applet_id = format!("{}_{}", id_prefix, device_id);
                    upsert_row(db, template, &applet_id, None, Some(&device_id)).await?;
                    live_ids.push(applet_id);
                    upserted += 1;
                }
            } else {
                // OAuth / API-key source: fan out per credential (the outbound
                // secret the action uses to call the provider).
                let credential_ids: Vec<(String,)> = sqlx::query_as(
                    "SELECT id FROM credentials WHERE source_id = $1 AND status = 'active'",
                )
                .bind(source_id)
                .fetch_all(db)
                .await?;
                for (cred_id,) in credential_ids {
                    let applet_id = format!("{}_{}", id_prefix, cred_id);
                    upsert_row(db, template, &applet_id, Some(&cred_id), None).await?;
                    live_ids.push(applet_id);
                    upserted += 1;
                }
            }
        } else {
            upsert_row(db, template, id_prefix, None, None).await?;

            // Bring the applet's own tables up to whatever its folder declares.
            // Only concrete (non-fan-out) applets own a schema — a per-credential
            // template expands to many rows and none of them owns tables.
            //
            // After the upsert, because the migrations table references the
            // applet row. A no-op when the box is already current, so this
            // costs one query per reconcile per schema-owning applet.
            if let Some(slug) = crate::scheduler::applets::applet_slug(id_prefix) {
                let dir = resolve_applet_dir(&template.dir);
                let ran = crate::tools::applet_schema::replay_pending(
                    db, id_prefix, &slug, &dir,
                )
                .await;
                if ran > 0 {
                    tracing::info!(
                        applet_id = id_prefix,
                        versions = ran,
                        "applied applet schema versions from disk"
                    );
                    // A table nobody may read is not a table. Boot grants run
                    // BEFORE this pass (server::mod), so on a fresh box the
                    // grant sweep saw no applet schemas at all and everything
                    // created here came out unreadable by the face role and
                    // unwritable by the applet role — until the next restart,
                    // and never at all for the Reconcile button. Re-grant
                    // whenever DDL actually ran.
                    if let Err(e) = crate::server::faces::ensure_applet_db_grants(db).await {
                        tracing::warn!(
                            applet_id = id_prefix,
                            error = %e,
                            "applied applet schema but failed to re-grant access to it"
                        );
                    }
                }
            }

            live_ids.push(id_prefix.to_string());
            upserted += 1;
        }
    }

    // GC pass 2: delete system (template-managed) action rows that the current
    // catalog no longer produces — a manifest that was removed or renamed (e.g.
    // the six `ios_*` actions collapsed into `ios_ingest`). Without this, a
    // deleted template leaves dead rows pointing at a binary that no longer
    // ships: legacy cruft, and a 404 surface if a stale client still posts to
    // it. `user`-owned rows are never touched, and the pass is guarded on a
    // non-empty SHIPPED catalog so a load failure can't wipe the table.
    // Run-history FKs are nullified first so history survives under
    // `applet_id = NULL`.
    //
    // The guard keys on the shipped root specifically, NOT on `live_ids`.
    // System rows come only from the shipped tree, so a shipped root that
    // failed to load must never delete them — and `live_ids` can't tell you
    // that, because one authored applet in the state root makes it non-empty.
    // Keying on the wrong one deletes every system applet on a box whose
    // shipped tree is briefly unreadable (mid-upgrade, bad mount, bad env).
    if templates.shipped_count > 0 {
        sqlx::query(
            r#"UPDATE app_applet_runs SET applet_id = NULL
               WHERE applet_id IN (
                   SELECT id FROM app_applets
                   WHERE owner = 'system' AND id <> ALL($1::text[])
               )"#,
        )
        .bind(&live_ids)
        .execute(db)
        .await?;

        let removed = sqlx::query(
            r#"DELETE FROM app_applets
               WHERE owner = 'system' AND id <> ALL($1::text[])"#,
        )
        .bind(&live_ids)
        .execute(db)
        .await?
        .rows_affected();

        if removed > 0 {
            tracing::info!(
                removed,
                "reconcile GC: removed system actions with no matching template"
            );
        }
    }

    tracing::info!(count = upserted, "reconciled action templates");
    Ok(upserted)
}

async fn upsert_row(
    db: &PgPool,
    template: &Template,
    applet_id: &str,
    credential_id: Option<&str>,
    device_id: Option<&str>,
) -> Result<()> {
    let triggers_json = serde_json::to_string(&template.triggers)
        .map_err(|e| Error::Other(format!("failed to serialize triggers: {e}")))?;

    // Optional polyglot command stored as JSON. Reconcile rewrites the
    // declarative `command` field on every system upsert (matches the rest of
    // the manifest-managed fields).
    let command_json = match &template.command {
        Some(cmd) => Some(
            serde_json::to_string(cmd)
                .map_err(|e| Error::Other(format!("failed to serialize command: {e}")))?,
        ),
        None => None,
    };

    // Manifest-supplied config (e.g. `[config.limits]`). Convert TOML → JSON
    // via serde round-trip. Empty manifest
    // config defaults to `{}` so we don't blow away user-customized config
    // on system reconcile of a manifest with no [config] block.
    let config_json: String = match &template.config {
        Some(cfg) => serde_json::to_string(cfg)
            .map_err(|e| Error::Other(format!("failed to serialize config: {e}")))?,
        None => "{}".to_string(),
    };

    // Owner determines reconcile semantics:
    //
    //   system: UPSERT with overwrite of template-managed fields (name, owner,
    //           agent, condition, triggers, credential_id, runtime, command).
    //           Preserves user-managed runtime state
    //           (schedule, enabled, config, memory).
    //
    //   user:   ON CONFLICT DO NOTHING. Factory defaults are seeded the first time
    //           the template is added; after that the row is fully owned by
    //           the user and reconcile is a no-op.
    //
    //   ai:     the third branch (authoring plan §E). The folder is written by
    //           setup_applet, so COMPILED fields (name, agent, condition,
    //           until, triggers, schedule) overwrite on reconcile — re-setup
    //           IS the edit path and must propagate. OPERATIONAL state is
    //           never touched: `enabled` (the gate lives there; the user
    //           toggle mirrors back into the manifest for restore fidelity),
    //           `memory`, and non-manifest config keys — manifest config
    //           merges OVER existing config so runtime keys survive.
    let sql = if template.owner == "user" {
        r#"
        INSERT INTO app_applets (
            id, name, owner, agent, schedule, enabled, config, condition,
            triggers, credential_id, command, device_id, until, description
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8, $9::jsonb, $10, $11, $12, $13, $14)
        ON CONFLICT(id) DO UPDATE SET
            device_id      = EXCLUDED.device_id,
            updated_at     = now()
        "#
    } else if template.owner == "ai" {
        r#"
        INSERT INTO app_applets (
            id, name, owner, agent, schedule, enabled, config, condition,
            triggers, credential_id, command, device_id, until, description
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8, $9::jsonb, $10, $11, $12, $13, $14)
        ON CONFLICT(id) DO UPDATE SET
            name           = EXCLUDED.name,
            agent          = EXCLUDED.agent,
            schedule  = EXCLUDED.schedule,
            config         = app_applets.config || EXCLUDED.config,
            condition      = EXCLUDED.condition,
            triggers       = EXCLUDED.triggers,
            until          = EXCLUDED.until,
            description    = EXCLUDED.description,
            updated_at     = now()
        "#
    } else {
        r#"
        INSERT INTO app_applets (
            id, name, owner, agent, schedule, enabled, config, condition,
            triggers, credential_id, command, device_id, until, description
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8, $9::jsonb, $10, $11, $12, $13, $14)
        ON CONFLICT(id) DO UPDATE SET
            name           = EXCLUDED.name,
            owner          = EXCLUDED.owner,
            agent          = EXCLUDED.agent,
            config         = EXCLUDED.config,
            condition      = EXCLUDED.condition,
            triggers       = EXCLUDED.triggers,
            credential_id  = EXCLUDED.credential_id,
            command        = EXCLUDED.command,
            device_id      = EXCLUDED.device_id,
            until          = EXCLUDED.until,
            description    = EXCLUDED.description,
            updated_at     = now()
        "#
    };

    sqlx::query(sql)
        .bind(applet_id)
        .bind(&template.name)
        .bind(&template.owner)
        .bind(&template.agent)
        .bind(&template.schedule)
        .bind(template.default_enabled)
        .bind(&config_json)
        .bind(&template.condition)
        .bind(&triggers_json)
        .bind(credential_id)
        .bind(&command_json)
        .bind(device_id)
        .bind(&template.until)
        .bind(&template.description)
        .execute(db)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both roots are scanned, and an authored applet in the state root
    /// Every `writes` entry must name a real ontology. A typo here fails
    /// silently and in the worst direction: the stream reports that *nothing*
    /// provides it, so the UI tells the user to connect something they already
    /// have. Cheap to assert, impossible to notice otherwise.
    #[test]
    fn declared_writes_name_real_ontologies() {
        let known: std::collections::HashSet<String> =
            virtues_registry::ontologies::registered_ontologies()
                .into_iter()
                .map(|o| o.name.to_string())
                .collect();

        let guard = catalog_lock().read().unwrap();
        let mut bad = Vec::new();
        for t in &guard.action {
            for w in &t.writes {
                if !known.contains(w) {
                    bad.push(format!("{} declares writes = \"{}\"", t.dir, w));
                }
            }
        }
        assert!(bad.is_empty(), "unknown ontologies in `writes`: {bad:#?}");
    }

    /// The map has to actually resolve, or the feature it exists for is a very
    /// well-tested no-op.
    #[test]
    fn ingest_applets_claim_their_streams() {
        assert!(
            sources_writing("health_sleep").contains(&"ios".to_string()),
            "iPhone should provide sleep"
        );
        assert!(
            sources_writing("communication_message").contains(&"mac".to_string()),
            "Mac should provide messages"
        );
        // Two sources can fill one stream, and both should be offered.
        let bookmarks = sources_writing("content_bookmark");
        assert!(bookmarks.contains(&"mac".to_string()) && bookmarks.contains(&"github".to_string()));
        // And a stream nothing writes must stay empty rather than guess.
        assert!(sources_writing("activity_listening").is_empty());
    }

    /// The keystone of the package model: a directory can contribute a
    /// `[[source]]` row, so a new provider no longer requires editing the box's
    /// own catalog and cutting a release.
    #[test]
    fn packages_contribute_and_shadow_sources() {
        let root = tempfile::tempdir().unwrap();
        let write = |dir: &str, body: &str| {
            let d = root.path().join(dir);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("sources.toml"), body).unwrap();
        };

        write(
            "todoist-pack",
            r#"[[source]]
id = "todoist"
display_name = "Todoist"
icon = "ri:list-check"
description = "Tasks."
auth = { kind = "api_key", fields = ["token"] }
"#,
        );
        // A package may also shadow a source the box ships, which is what makes
        // "fix the description without waiting for a release" possible.
        write(
            "my-google",
            r#"[[source]]
id = "google"
display_name = "Google (mine)"
icon = "ri:google-fill"
description = "Patched."
auth = { kind = "via_proxy", start_path = "/google/start" }
"#,
        );
        // Not a package: no sources.toml, and dot-dirs are skipped outright.
        std::fs::create_dir_all(root.path().join(".hidden")).unwrap();

        let found = scan_sources(root.path());
        assert_eq!(found.len(), 2, "one row per package sources.toml: {found:?}");

        let todoist = found.iter().find(|s| s.id == "todoist").unwrap();
        assert_eq!(todoist.display_name, "Todoist");
        assert_eq!(
            todoist.dir, "todoist-pack",
            "a source must be attributable to the package that added it"
        );

        // Merge order: base, then packages, last-wins-by-id.
        let mut merged = vec![Source {
            id: "google".into(),
            display_name: "Google".into(),
            icon: "ri:google-fill".into(),
            description: "Shipped.".into(),
            auth: SourceAuth::ViaProxy { start_path: "/google/start".into() },
            repo: None,
            repo_ref: None,
            dir: String::new(),
        }];
        for s in found {
            match merged.iter().position(|e| e.id == s.id) {
                Some(i) => merged[i] = s,
                None => merged.push(s),
            }
        }
        assert_eq!(merged.len(), 2, "shadowing must not duplicate an id");
        let google = merged.iter().find(|s| s.id == "google").unwrap();
        assert_eq!(google.display_name, "Google (mine)", "package wins");
        assert_eq!(google.dir, "my-google");
    }

    /// A package with a broken `sources.toml` must not take the catalog down
    /// with it — the whole point of the merge is that packages are independent.
    #[test]
    fn unparseable_package_sources_are_skipped_not_fatal() {
        let root = tempfile::tempdir().unwrap();
        let bad = root.path().join("broken");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("sources.toml"), "this is not = = toml [[[").unwrap();

        let good = root.path().join("fine");
        std::fs::create_dir_all(&good).unwrap();
        std::fs::write(
            good.join("sources.toml"),
            "[[source]]\nid = \"ok\"\ndisplay_name = \"Ok\"\nicon = \"i\"\ndescription = \"d\"\nauth = { kind = \"self_issued_bearer\" }\n",
        )
        .unwrap();

        let found = scan_sources(root.path());
        assert_eq!(found.len(), 1, "the good package still loads");
        assert_eq!(found[0].id, "ok");
    }

    /// A fork must not carry `.git` across. For an applet forked out of an
    /// imported package that directory points at the upstream remote, and for
    /// an authenticated clone it holds the credential in the remote's URL.
    /// Symlinks are skipped for the same reason the source reader skips them:
    /// a package controls its own layout and can link anywhere.
    #[test]
    fn copy_tree_skips_dotdirs_and_symlinks() {
        let base = std::env::temp_dir().join(format!("vfork-{}", std::process::id()));
        let (from, to, outside) = (base.join("from"), base.join("to"), base.join("outside"));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(from.join(".git")).unwrap();
        std::fs::create_dir_all(from.join("face")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(from.join("manifest.toml"), "name = \"x\"\nowner = \"system\"\n").unwrap();
        std::fs::write(from.join("face").join("index.html"), "<p>hi</p>").unwrap();
        std::fs::write(from.join(".git").join("config"), "url = https://tok@h/r\n").unwrap();
        std::fs::write(outside.join("secret"), "k").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.join("secret"), from.join("link")).unwrap();

        copy_tree(&from, &to).unwrap();

        assert!(to.join("manifest.toml").is_file(), "real files copy");
        assert!(to.join("face").join("index.html").is_file(), "subdirs copy");
        assert!(!to.join(".git").exists(), "`.git` must not be carried into a fork");
        #[cfg(unix)]
        assert!(!to.join("link").exists(), "symlinks must not be copied");

        let _ = std::fs::remove_dir_all(&base);
    }

    /// The stamp is textual so a manifest keeps its comments, which for an
    /// applet are most of what a reader opened the file for. Re-stamping must
    /// replace rather than accumulate.
    #[test]
    fn stamp_forked_from_replaces_and_keeps_comments() {
        let p = std::env::temp_dir().join(format!("vstamp-{}.toml", std::process::id()));
        std::fs::write(&p, "# why this exists\nname = \"x\"\n").unwrap();

        stamp_forked_from(&p, "virtues@v0.3.0").unwrap();
        stamp_forked_from(&p, "virtues@v0.4.0").unwrap();
        let out = std::fs::read_to_string(&p).unwrap();

        assert_eq!(out.matches("forked_from").count(), 1, "replaced, not appended");
        assert!(out.contains("virtues@v0.4.0"));
        assert!(out.contains("# why this exists"), "comments survive");
        assert!(out.contains("name = \"x\""));
        let _ = std::fs::remove_file(&p);
    }

    /// SHADOWS a shipped applet with the same dir rather than colliding with
    /// it. This is what makes "fork a system applet" work, and what makes
    /// deleting your copy revert cleanly to shipped.
    #[test]
    fn state_root_shadows_shipped_root() {
        let shipped = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();

        let write = |root: &std::path::Path, dir: &str, name: &str| {
            let d = root.join(dir);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(
                d.join("manifest.toml"),
                format!("name = \"{name}\"\nowner = \"system\"\n"),
            )
            .unwrap();
        };
        write(shipped.path(), "day_summary_eod", "Shipped Summary");
        write(shipped.path(), "weather_sync", "Weather");
        write(state.path(), "day_summary_eod", "My Summary");
        write(state.path(), "user/wife_week", "Wife Week");

        let mut merged = scan_root(shipped.path());
        let shipped_count = merged.len();
        for t in scan_root(state.path()) {
            match merged.iter().position(|e| e.dir == t.dir) {
                Some(i) => merged[i] = t,
                None => merged.push(t),
            }
        }

        assert_eq!(shipped_count, 2, "shipped root parsed independently");
        assert_eq!(merged.len(), 3, "shadowing must not duplicate a dir");

        let by_dir = |d: &str| merged.iter().find(|t| t.dir == d).unwrap().name.clone();
        assert_eq!(by_dir("day_summary_eod"), "My Summary", "state wins");
        assert_eq!(by_dir("weather_sync"), "Weather", "unshadowed shipped survives");
        assert_eq!(by_dir("user/wife_week"), "Wife Week", "authored applet is found");
    }

    /// The state root must never resolve to a path the process cannot create.
    /// It previously returned `/var/lib/virtues/applets` unconditionally, so
    /// on a dev machine (no `/var/lib/virtues`, and `/var/lib` root-owned)
    /// every authoring attempt died with a permission error.
    #[test]
    fn state_root_is_writable_on_this_machine() {
        // Env override would mask what we're testing.
        if std::env::var("VIRTUES_APPLET_STATE_DIR").is_ok() {
            return;
        }
        let root = state_root();
        let probe = root.join(".writability-probe");
        std::fs::create_dir_all(&probe).unwrap_or_else(|e| {
            panic!("state_root() resolved to {} which is not creatable: {e}", root.display())
        });
        let _ = std::fs::remove_dir(&probe);
    }

    /// A missing state root is the normal case on a box that has never
    /// authored anything — it must not disturb the shipped catalog.
    #[test]
    fn absent_state_root_is_not_an_error() {
        let shipped = tempfile::tempdir().unwrap();
        let d = shipped.path().join("weather_sync");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("manifest.toml"), "name = \"W\"\nowner = \"system\"\n").unwrap();

        assert_eq!(scan_root(shipped.path()).len(), 1);
        assert!(scan_root(&shipped.path().join("nope")).is_empty());
    }

    /// Golden test: the baked templates.toml must parse cleanly with the
    /// current struct shape. If this fails, a TOML edit broke schema compat.
    #[test]
    fn baked_templates_parse() {
        // Force initialization. Panics if any manifest is malformed.
        // Bind the guard so clippy doesn't flag the held-lock.
        let _guard = catalog_lock().read().expect("catalog rwlock poisoned");
    }

    #[test]
    fn ios_source_present() {
        let ios = lookup_source("ios").expect("ios source must exist in templates.toml");
        assert_eq!(ios.display_name, "iPhone");
        assert!(matches!(ios.auth, SourceAuth::SelfIssuedBearer));
        assert_eq!(ios.auth.kind_str(), "self_issued_bearer");
    }

    #[test]
    fn source_ids_unique() {
        let snapshot: ParsedTemplates = {
            let guard = catalog_lock().read().expect("catalog rwlock poisoned");
            guard.clone()
        };
        let mut seen = std::collections::HashSet::new();
        for s in &snapshot.source {
            assert!(
                seen.insert(s.id.clone()),
                "duplicate source id in sources.toml: {}",
                s.id
            );
        }
    }

    #[test]
    fn list_sorted_is_stable() {
        let sources = list_sources_sorted();
        let names: Vec<&str> = sources.iter().map(|s| s.display_name.as_str()).collect();
        let mut expected = names.clone();
        expected.sort();
        assert_eq!(names, expected);
    }

    #[test]
    fn every_per_credential_applet_references_known_source() {
        let snapshot: ParsedTemplates = {
            let guard = catalog_lock().read().expect("catalog rwlock poisoned");
            guard.clone()
        };
        for tmpl in &snapshot.action {
            if tmpl.per_credential {
                let id = tmpl.id_prefix.as_deref().unwrap_or("?");
                let src = tmpl
                    .source
                    .as_ref()
                    .unwrap_or_else(|| panic!("{} per_credential needs source", id));
                assert!(
                    lookup_source(&src.id).is_some(),
                    "{} references unknown source '{}'",
                    id,
                    src.id
                );
            }
        }
    }

    #[test]
    fn no_manifest_uses_legacy_connector_field() {
        // The legacy `connector = { id = "..." }` field is not in the Template
        // struct anymore. If a manifest still uses it, deserialization with
        // serde's default `deny_unknown_fields = false` will silently ignore
        // it and `per_credential` validation will fail. This test scans every
        // per-folder manifest + sources.toml for the offending substring.
        let core_manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let applets_dir = core_manifest.join(ACTIONS_DIR_FROM_CORE);

        // Sources file
        if let Ok(text) = std::fs::read_to_string(applets_dir.join("sources.toml")) {
            assert!(
                !text.contains("connector = {"),
                "sources.toml still references legacy `connector = {{ id = ... }}` field"
            );
        }

        // Per-action manifests
        if let Ok(entries) = std::fs::read_dir(&applets_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let manifest = path.join("manifest.toml");
                if !manifest.exists() {
                    continue;
                }
                let text = std::fs::read_to_string(&manifest).unwrap_or_default();
                assert!(
                    !text.contains("connector = {"),
                    "{} still references legacy `connector = {{ id = ... }}` field",
                    manifest.display()
                );
            }
        }
    }

    /// Reconcile must be idempotent: a second back-to-back call against the
    /// same DB and templates produces zero `app_applets` row diffs.
    ///
    /// This is the precondition for triggering reconcile from auth handlers
    /// (Phase 3 + Phase 4). If reconcile churns rows, every double-callback
    /// or refresh sweep would mutate state needlessly and break the
    /// dual-path verification window in Phase 6.
    // Requires a live Postgres: `#[sqlx::test]` provisions a scratch DB and
    // applies `core/migrations` automatically, so this runs against the real
    // schema. Set DATABASE_URL when running. `triggers` is JSONB, so we cast it
    // to text for a stable string snapshot comparison.
    /// Every shipped manifest carries a description, and reconcile must land
    /// it. It did not for the life of the project: `Template` had no such
    /// field, serde discarded the key without complaint, and there was no
    /// column to bind it to — so the sentence the list row, the detail
    /// headline, and the authoring gate all read from was empty everywhere.
    #[sqlx::test]
    async fn reconcile_carries_the_description_to_the_row(pool: sqlx::PgPool) {
        reconcile_templates(&pool).await.expect("reconcile");

        let missing: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM app_applets \
             WHERE owner = 'system' AND (description IS NULL OR btrim(description) = '') \
             ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(
            missing.is_empty(),
            "shipped applets with no description on the row: {missing:?}"
        );
    }

    /// The sentence is reconcile's to own, like `name` — editing a shipped
    /// manifest and reconciling has to change what the user reads, or fixing
    /// a description would require a database migration.
    #[sqlx::test]
    async fn an_edited_description_overwrites_on_reconcile(pool: sqlx::PgPool) {
        reconcile_templates(&pool).await.expect("reconcile");
        sqlx::query("UPDATE app_applets SET description = 'stale' WHERE owner = 'system'")
            .execute(&pool)
            .await
            .unwrap();

        reconcile_templates(&pool).await.expect("second reconcile");

        let stale: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM app_applets WHERE description = 'stale'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stale, 0, "reconcile must restore the manifest sentence");
    }

    /// End-to-end for the schema-replay path: a folder on disk carrying
    /// `schema/NNNN_*.sql` must reach real tables through reconcile alone.
    /// Nothing had ever exercised this — the versioned-migration work shipped
    /// with unit tests over the collapse logic and no applet that used it.
    #[sqlx::test]
    async fn a_shipped_applet_gets_its_tables_from_disk(pool: sqlx::PgPool) {
        reconcile_templates(&pool).await.expect("reconcile");

        let table: Option<String> = sqlx::query_scalar(
            "SELECT table_name FROM information_schema.tables \
              WHERE table_schema = 'applet_calorie_tracker' AND table_name = 'entries'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert_eq!(
            table.as_deref(),
            Some("entries"),
            "the tracker's schema/0001 did not reach the database"
        );

        // Recorded, so a second reconcile does not run it again.
        let applied: Vec<(i32, String)> = sqlx::query_as(
            "SELECT version, name FROM app_applet_schema_migrations \
              WHERE applet_id = 'applet_calorie_tracker' ORDER BY version",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].0, 1);

        reconcile_templates(&pool).await.expect("second reconcile");
        let after: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM app_applet_schema_migrations \
              WHERE applet_id = 'applet_calorie_tracker'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(after, 1, "a version must be applied exactly once");

        // A table nobody may read is not a table. Boot grants the roles BEFORE
        // reconcile runs, so everything created by the pass above arrives
        // ungranted unless the pass re-grants — the face would see permission
        // denied on its own applet's table, and sql_write could not log a meal.
        for (role, priv_) in [
            ("virtues_face_reader", "SELECT"),
            ("virtues_applet_writer", "INSERT"),
        ] {
            let ok: bool = sqlx::query_scalar(
                "SELECT has_table_privilege($1, 'applet_calorie_tracker.entries', $2)",
            )
            .bind(role)
            .bind(priv_)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert!(ok, "{role} cannot {priv_} the table reconcile just created");
        }
    }

    #[sqlx::test]
    async fn reconcile_is_idempotent(pool: sqlx::PgPool) {
        // Seed an active iOS credential so per_credential templates fan out.
        sqlx::query(
            "INSERT INTO credentials (id, source_id, name, status, secrets_ciphertext) \
             VALUES ($1, 'ios', 'test ios', 'active', 'x')",
        )
        .bind("cred_test_ios")
        .execute(&pool)
        .await
        .unwrap();

        // First reconcile: populates rows.
        let first = reconcile_templates(&pool).await.expect("first reconcile");
        assert!(first > 0, "first reconcile should populate some rows");

        let snapshot_before: Vec<(String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, name, owner, triggers::text, credential_id FROM app_applets ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        // Second reconcile: must produce identical row set.
        let second = reconcile_templates(&pool).await.expect("second reconcile");
        assert_eq!(
            second, first,
            "second reconcile upsert count must match first (idempotent)"
        );

        let snapshot_after: Vec<(String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, name, owner, triggers::text, credential_id FROM app_applets ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(
            snapshot_before, snapshot_after,
            "row set must be byte-identical across back-to-back reconciles"
        );
    }
}
