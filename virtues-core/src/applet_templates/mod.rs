//! Applet catalog, source catalog, and reconciler.
//!
//! Two on-disk inputs:
//!
//! 1. `applets/sources.toml`: the source catalog, `[[source]]` entries, one
//!    tile per provider on the Sources UI. Baked into the binary as a
//!    fallback for when the file is missing at runtime.
//! 2. Applet folders, each with a `SKILL.md` ([`skill`]): the shipped tree
//!    ([`shipped_root`]) and the state root ([`state_root`]: your own
//!    applets under `user/`, imports under their slug). A `manifest.toml`
//!    folder in the state root converts to `SKILL.md` first ([`convert`]).
//!
//! `reconcile_templates` upserts one `app_applets` row per folder, or one
//! per matching credential for an applet with a `source`. How much of the
//! row follows the file depends on who owns it; `upsert_row` says.

use std::sync::{OnceLock, RwLock};

mod convert;

pub use convert::convert_state_root;
mod skill;

pub(crate) use skill::{
    key_value, overlay, override_between, render as render_skill_md, watch_in, with_keys, SkillMd,
    ALWAYS_ALLOWED_TOOLS, SETUP_KEYS,
};

/// An applet folder written in the Agent Skills format. See [`skill`].
const SKILL_FILE: &str = "SKILL.md";

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;


// ─────────────────────────────────────────────────────────────────────────────
// TOML schema
// ─────────────────────────────────────────────────────────────────────────────

/// One `[[source]]` entry in `actions/sources.toml`. Catalog tile.
#[derive(Debug, Deserialize, Clone)]
pub struct Source {
    /// Stable id used as `credentials.key` (kind `source`) and as `[[action]].source.id`.
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
    /// A login page whose session is the credential, for sources with no API a
    /// personal box can use (X, Instagram). The app opens `url` in a browser
    /// window, the owner logs in, and once every cookie in `cookies` is set the
    /// app fills the `api_key` fields from the cookie jar. Only meaningful for
    /// `api_key` sources; the paste form stays the fallback.
    #[serde(default)]
    pub login: Option<SourceLogin>,
    /// Which package contributed this row, relative to whichever root it was
    /// found in. Empty for the box's own top-level `sources.toml`. Populated by
    /// the loader from the on-disk path, never read from TOML — a file cannot
    /// know its own location. Without it a source cannot be attributed to the
    /// thing that would have to be uninstalled to remove it.
    #[serde(skip)]
    pub dir: String,
}

/// See [`Source::login`].
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct SourceLogin {
    /// Where the owner logs in.
    pub url: String,
    /// Cookies that exist only once the owner is logged in. The login is done
    /// when all of them are set.
    pub cookies: Vec<String>,
    /// Absent: each name in `cookies` is also an `api_key` field, filled with
    /// that cookie's value. Present: this one field gets the site's whole
    /// cookie jar as a `Cookie:` header (`a=1; b=2`), for a site that checks
    /// more of the jar than a fixed list (Instagram).
    #[serde(default)]
    pub jar_field: Option<String>,
    /// Other domains the sign-in sets cookies on, whose cookies the jar
    /// includes (ChatGPT signs in through openai.com).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domains: Vec<String>,
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

/// One applet as the catalog holds it, filled by the SKILL.md reader
/// ([`skill::parse`]). It still derives `Deserialize` because the converter
/// reads an old `manifest.toml` into it.
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
    /// For shipped applets, reconcile overwrites the row's copy on every
    /// start. For your own and imported ones it merges (`upsert_row`).
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
    /// Named keys this applet needs, such as `OPENAI_API_KEY`. The runner
    /// sets each as an environment variable for the applet's runs, from the
    /// owner's `env` secrets (`crate::secrets`), and fails a run whose key is
    /// not set. An undeclared key is never passed. Names the box uses itself
    /// are refused when the manifest loads (`reserved_env_name`).
    #[serde(default)]
    secrets: Vec<String>,
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
    /// `/name` in chat starts it: a `SKILL.md` whose `invoke` includes
    /// `chat`. A `manifest.toml` has no way to say so.
    #[serde(skip)]
    invoke_chat: bool,
    /// A `SKILL.md`'s `icon`, an `ri:` id, for the `/` menu.
    #[serde(skip)]
    icon: Option<String>,
    /// A `SKILL.md`'s `run: sql …`, or the text of its `.sql` file: read-only
    /// SQL run before the prompt (`applet_runner::run_sql`).
    #[serde(skip)]
    run_sql: Option<String>,
    /// A `SKILL.md`'s data trigger (`crate::scheduler::watch`).
    #[serde(skip)]
    watch: Option<crate::scheduler::watch::Watch>,
    /// The folder on disk, set by the reader.
    #[serde(skip)]
    folder: Option<std::path::PathBuf>,
    /// The files under `references/` and `assets/`, relative to the folder:
    /// what its prompt may read with `use_applet` (the spec's third stage).
    #[serde(skip)]
    files: Vec<String>,
    /// Manifest folder relative to the actions root. Populated by the loader
    /// from the on-disk path, never read from TOML — a file can't know its own
    /// location. Examples: `morning_brief`, `team-pack/actions/foo`.
    #[serde(skip)]
    dir: String,

    /// Every key in the manifest this struct does not name.
    ///
    /// Kept because serde discards unknown keys in silence, and this file
    /// already carries the scar: `description` was in every manifest in the
    /// tree from the beginning and had no field here, so it was thrown away on
    /// every load for months and nobody could see it. `runtime = "function"`
    /// sat in a shipped manifest the same way.
    ///
    /// Not `deny_unknown_fields`, deliberately. That would turn an author's
    /// typo into a hard load failure for an applet that is otherwise fine, and
    /// authored applets are written by people through chat, not reviewed. A
    /// warning names the key, keeps the applet working, and is loud enough to
    /// find — which silence never was.
    #[serde(flatten)]
    unknown: std::collections::BTreeMap<String, toml::Value>,
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
/// Only the installer writes here. Everything else is in the state root
/// ([`state_root`]), and a folder there with a shipped folder's name is read
/// as unshipped, which is what keeps `sudo` and the other shipped-only grants
/// to the applets that shipped.
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

/// The state root folder that holds changes to shipped applets, one file per
/// applet (`override_path`). Not an applet namespace.
const OVERRIDES_DIR: &str = "overrides";

/// Where the owner's change to the shipped applet at `dir` is kept: a
/// `SKILL.md` holding only what they changed, which the catalog lays over the
/// shipped file (`skill::overlay`). The shipped tree is replaced on every
/// upgrade, so the change lives in the state root, and a shipped fix still
/// reaches every key it does not name.
fn override_path(dir: &str) -> std::path::PathBuf {
    override_in(&state_root().join(OVERRIDES_DIR), dir)
}

fn override_in(overrides: &std::path::Path, dir: &str) -> std::path::PathBuf {
    overrides.join(format!("{}.md", dir.replace('/', "__")))
}

/// Whether the owner has changed this shipped applet.
pub fn has_override(applet_id: &str) -> bool {
    dir_for_applet_id(applet_id).is_some_and(|d| override_path(&d).is_file())
}

/// Write `text` to `path` by renaming a finished copy into place, so a reader
/// never sees half a file.
fn write_atomic(path: &std::path::Path, text: &str) -> Result<()> {
    let dir = path.parent().ok_or_else(|| Error::Other(format!("{} has no folder", path.display())))?;
    std::fs::create_dir_all(dir).map_err(|e| Error::Other(format!("could not create {}: {e}", dir.display())))?;
    let tmp = dir.join(format!(".{}.tmp", path.file_name().and_then(|n| n.to_str()).unwrap_or("file")));
    std::fs::write(&tmp, text).map_err(|e| Error::Other(format!("could not write {}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, path).map_err(|e| Error::Other(format!("could not write {}: {e}", path.display())))
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
    convert::convert_state_root(db).await;
    reload_catalog();
    reconcile_templates(db).await
}

pub fn reload_catalog() {
    let fresh = load_catalog();
    let lock = catalog_lock();
    let mut guard = lock.write().expect("catalog rwlock poisoned");
    *guard = fresh;
}

/// Read and parse a single `manifest.toml`, for the converter. Returns None,
/// logged, when the file cannot be read or parsed.
///
/// `dir` is the folder relative to its root and is the applet's identity:
/// the id is `applet_<dir>` with `/` as `__`, which is what the converter
/// pins when the SKILL.md reader would derive another.
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
    // Say what was thrown away. See `Template::unknown`.
    if !tmpl.unknown.is_empty() {
        let keys: Vec<&str> = tmpl.unknown.keys().map(String::as_str).collect();
        tracing::warn!(
            path = %manifest_path.display(),
            keys = %keys.join(", "),
            "manifest.toml has keys this build does not understand — they are \
             being ignored. Either the key is misspelled, or it is from a newer \
             manifest format than this box runs."
        );
    }

    let (kept, refused): (Vec<String>, Vec<String>) =
        std::mem::take(&mut tmpl.secrets).into_iter().partition(|n| valid_secret_name(n));
    if !refused.is_empty() {
        tracing::warn!(
            path = %manifest_path.display(),
            names = %refused.join(", "),
            "manifest.toml declares secrets with names an applet cannot have: they \
             are being ignored. A name is CAPITALS_AND_UNDERSCORES and not one the \
             box sets itself."
        );
    }
    tmpl.secrets = kept;

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
    let mut actions = scan_root(&applets_dir, true, Some(&state_root().join(OVERRIDES_DIR)));
    let shipped_count = actions.len();
    for t in scan_root(&state_root(), false, None) {
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

/// Walk one applet root and parse every applet folder under it. Same rule for
/// both roots — see the scanner notes in `load_catalog`. `shipped` is whether
/// this is the shipped root, which decides a `SKILL.md` applet's owner, and
/// `overrides` is the folder of the owner's changes to read on top of it.
fn scan_root(root: &std::path::Path, shipped: bool, overrides: Option<&std::path::Path>) -> Vec<Template> {
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

            // Skills saved before skills were applets, in the old format,
            // until `chat_applets::move_saved_skills` has moved them.
            // TODO(2026-10-27): delete with that move.
            if !shipped && (folder_name == crate::api::chat_applets::SAVED_SKILLS_DIR || folder_name == OVERRIDES_DIR) {
                continue;
            }

            // Single action at top level.
            if let Some(parsed) = read_applet_folder(&path, &folder_name, shipped, overrides) {
                actions.extend(parsed);
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
                let dir = format!("{folder_name}/{inner_name}");
                if let Some(parsed) = read_applet_folder(&inner_path, &dir, shipped, overrides) {
                    actions.extend(parsed);
                }
            }
        }
    }

    actions
}

/// Read one folder's `SKILL.md`. `None` when it has none, so the caller can
/// treat it as a namespace (or, in the shipped root, a folder of Rust
/// sources); `Some(None)` when the file was refused, which is logged. A
/// `manifest.toml` is not read here: `convert::convert_state_root` turns it
/// into a `SKILL.md` first, and one that could not convert is not loaded.
fn read_applet_folder(
    path: &std::path::Path,
    dir: &str,
    shipped: bool,
    overrides: Option<&std::path::Path>,
) -> Option<Option<Template>> {
    let skill_md = path.join(SKILL_FILE);
    if !skill_md.is_file() {
        if path.join("manifest.toml").is_file() {
            tracing::warn!(dir, "an applet folder still has only a manifest.toml; it is not loaded");
            return Some(None);
        }
        return None;
    }
    let mut text = match std::fs::read_to_string(&skill_md) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(path = %skill_md.display(), error = %e, "skipping unreadable SKILL.md");
            return Some(None);
        }
    };
    if let Some(overrides) = overrides {
        if let Ok(over) = std::fs::read_to_string(override_in(overrides, dir)) {
            match skill::overlay(&text, &over) {
                Ok(merged) => text = merged,
                Err(why) => tracing::error!(dir, why = %why, "an applet's override cannot apply; the shipped file is used"),
            }
        }
    }
    Some(match skill::parse(&text, path, dir, shipped) {
        Ok(t) => Some(t),
        Err(problems) => {
            tracing::error!(
                path = %skill_md.display(),
                problems = %problems.join(" "),
                "skipping SKILL.md the box refuses"
            );
            None
        }
    })
}

/// Write an edit to an applet into its file: the folder's own `SKILL.md` in
/// the state root (yours, the assistant's, a fork), or, for a shipped applet,
/// its override. The file is the applet, so this is the only way an edit to
/// what it is lands; the caller reloads and reconciles to rebuild the row.
/// Leaving a shipped applet exactly as it ships removes its override.
pub fn write_edit(applet_id: &str, patch: &serde_json::Map<String, serde_json::Value>) -> Result<()> {
    let no_file = || Error::InvalidInput(format!("{applet_id} has no SKILL.md, so it cannot be edited. Set it up again."));
    let dir = dir_for_applet_id(applet_id).ok_or_else(no_file)?;
    let own = state_root().join(&dir);
    if own.join(SKILL_FILE).is_file() {
        let skill_md = own.join(SKILL_FILE);
        let text = std::fs::read_to_string(&skill_md)
            .map_err(|e| Error::Other(format!("could not read {}: {e}", skill_md.display())))?;
        let edited = skill::apply_patch(&text, patch).map_err(Error::InvalidInput)?;
        // What is written must load, or the next reconcile would skip it.
        skill::parse(&edited, &own, &dir, false).map_err(|problems| Error::InvalidInput(problems.join(" ")))?;
        return write_atomic(&skill_md, &edited);
    }
    let shipped = shipped_root().join(&dir);
    let shipped_text = std::fs::read_to_string(shipped.join(SKILL_FILE)).map_err(|_| no_file())?;
    let over = override_path(&dir);
    let existing = std::fs::read_to_string(&over).ok();
    match edit_shipped(&shipped_text, existing.as_deref(), patch, &shipped, &dir)? {
        Some(text) => write_atomic(&over, &text),
        None => remove_override(&dir),
    }
}

/// The override a shipped applet has after `patch`, given the shipped file
/// and its current override, or `None` when it is back to what ships.
fn edit_shipped(
    shipped_text: &str,
    existing: Option<&str>,
    patch: &serde_json::Map<String, serde_json::Value>,
    folder: &std::path::Path,
    dir: &str,
) -> Result<Option<String>> {
    let current = match existing {
        Some(o) => skill::overlay(shipped_text, o).map_err(Error::InvalidInput)?,
        None => shipped_text.to_string(),
    };
    let edited = skill::apply_patch(&current, patch).map_err(Error::InvalidInput)?;
    skill::parse(&edited, folder, dir, true).map_err(|problems| Error::InvalidInput(problems.join(" ")))?;
    skill::override_between(shipped_text, &edited).map_err(Error::InvalidInput)
}

/// The file setup writes for the applet named `spec`.
pub(crate) enum SetupTarget {
    /// A folder in the state root: the assistant's own (`user/<spec>`, which
    /// may not exist yet) or a fork of a shipped applet (`<spec>`).
    Own { rel: String, folder: std::path::PathBuf },
    /// A shipped applet, changed through its override.
    Shipped { dir: String, folder: std::path::PathBuf },
}

/// Which file a setup for `spec` changes. The same name is the same applet:
/// one the assistant made, then a fork, then a shipped applet; a name none of
/// them has is a new applet under `user/`.
pub(crate) fn setup_target(spec: &str) -> SetupTarget {
    let own = format!("user/{spec}");
    for rel in [own.clone(), spec.to_string()] {
        let folder = state_root().join(&rel);
        if folder.join(SKILL_FILE).is_file() {
            return SetupTarget::Own { rel, folder };
        }
    }
    let shipped = shipped_root().join(spec);
    if shipped.join(SKILL_FILE).is_file() {
        return SetupTarget::Shipped { dir: spec.to_string(), folder: shipped };
    }
    SetupTarget::Own { folder: state_root().join(&own), rel: own }
}

/// A shipped applet's file and its override, if it has one.
pub(crate) fn shipped_and_override(dir: &str) -> Result<(String, Option<String>)> {
    let path = shipped_root().join(dir).join(SKILL_FILE);
    let shipped = std::fs::read_to_string(&path).map_err(|e| Error::Other(format!("could not read {}: {e}", path.display())))?;
    Ok((shipped, std::fs::read_to_string(override_path(dir)).ok()))
}

/// Write, or with `None` remove, a shipped applet's override.
pub(crate) fn set_override(dir: &str, text: Option<&str>) -> Result<()> {
    match text {
        Some(t) => write_atomic(&override_path(dir), t),
        None => remove_override(dir),
    }
}

/// Write a state-root applet's `SKILL.md`, whole.
pub(crate) fn write_skill_md(folder: &std::path::Path, text: &str) -> Result<()> {
    write_atomic(&folder.join(SKILL_FILE), text)
}

/// Return a shipped applet to exactly what ships.
pub fn reset_to_shipped(applet_id: &str) -> Result<()> {
    let dir = dir_for_applet_id(applet_id).ok_or_else(|| Error::NotFound(format!("Applet not found: {applet_id}")))?;
    remove_override(&dir)
}

fn remove_override(dir: &str) -> Result<()> {
    match std::fs::remove_file(override_path(dir)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(Error::Other(format!("could not remove the override of {dir}: {e}")))
        }
        _ => Ok(()),
    }
}

/// The problems a `SKILL.md` would be refused for at `dir` (relative to the
/// state root) in `folder`, so a writer can check before it writes.
pub(crate) fn check_skill_md(text: &str, folder: &std::path::Path, dir: &str) -> std::result::Result<(), Vec<String>> {
    skill::parse(text, folder, dir, false).map(|_| ())
}

/// What a `SKILL.md` text reads as, for setup's reply: the applet's id, its
/// schedule as cron, its `until`, and whether `/name` starts it.
pub(crate) fn read_skill_md(
    text: &str,
    folder: &std::path::Path,
    dir: &str,
    shipped: bool,
) -> std::result::Result<(String, Option<String>, Option<String>, bool), Vec<String>> {
    let t = skill::parse(text, folder, dir, shipped)?;
    Ok((t.id_prefix.unwrap_or_default(), t.schedule, t.until, t.invoke_chat))
}

/// A `SKILL.md` folder as the `setup` arguments that would write it, for
/// `virtues applet put`: the same keys by the same names. A key setup does
/// not write (`source`, `secrets`, `icon`, …) or a `run` that is code is
/// refused with what to change, since `put` would drop it.
pub(crate) fn skill_md_as_setup_args(folder: &std::path::Path) -> std::result::Result<serde_json::Value, String> {
    let name = folder
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("{} has no folder name", folder.display()))?;
    let text = std::fs::read_to_string(folder.join(SKILL_FILE))
        .map_err(|e| format!("{}: {e}", folder.join(SKILL_FILE).display()))?;
    skill::parse(&text, folder, &format!("user/{name}"), false).map_err(|problems| problems.join(" "))?;
    let unsupported: Vec<String> = skill::metadata_keys(&text)?
        .into_iter()
        .filter(|k| !SETUP_KEYS.contains(&k.as_str()))
        .collect();
    if !unsupported.is_empty() {
        return Err(format!("`put` cannot carry {}; remove them or set the applet up by hand", unsupported.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", ")));
    }
    let value = |k: &str| skill::key_value(&text, k);
    if value("run").is_some_and(|r| !r.starts_with("sql ")) {
        return Err("`run` names code, which `put` cannot carry".into());
    }
    let mut args = serde_json::json!({ "name": name });
    for key in ["description", "prompt", "trigger", "run", "limit", "model"] {
        if let Some(v) = value(key) {
            args[key] = serde_json::json!(v);
        }
    }
    if let Some(v) = value("invoke") {
        args["invoke"] = serde_json::json!(v.split(',').map(str::trim).filter(|i| *i != "none").collect::<Vec<_>>());
    }
    if let Some(v) = value("allowed-tools") {
        args["tools"] = serde_json::json!(v.split_whitespace().collect::<Vec<_>>());
    }
    Ok(args)
}

/// An applet `/name` in chat starts: a `SKILL.md` whose `invoke` includes
/// `chat`. The chat runs it as a turn with the applet's body as the prompt
/// (`api::chat_applets`), not as a headless run.
#[derive(Debug, Clone)]
pub struct ChatApplet {
    /// The spec's `name`, which is its folder's: what follows the `/`.
    pub name: String,
    pub applet_id: String,
    pub description: String,
    pub body: String,
    /// Its `allowed-tools` tool ids, or `None` for the default set.
    pub tools: Option<Vec<String>>,
    /// `limit`'s `$N per run`, in dollars.
    pub max_cost_usd: Option<f64>,
    /// `limit`'s duration, in minutes, rounded up.
    pub max_minutes: Option<u64>,
    /// `model: deep`.
    pub deep: bool,
    pub icon: Option<String>,
    /// It ships with the box.
    pub shipped: bool,
    /// Its `references/` and `assets/` files (`Template::files`).
    pub files: Vec<String>,
}

/// Every applet `/name` in chat can start, shipped ones first, each group by
/// name.
pub fn chat_applets() -> Vec<ChatApplet> {
    let guard = catalog_lock().read().expect("catalog rwlock poisoned");
    let mut out: Vec<ChatApplet> = guard
        .action
        .iter()
        .filter(|t| t.invoke_chat)
        .filter_map(|t| {
            let config = t.config.as_ref();
            let get = |k: &str| config.and_then(|c| c.get(k));
            let limits = get("limits");
            Some(ChatApplet {
                name: t.dir.rsplit('/').next()?.to_string(),
                applet_id: t.id_prefix.clone()?,
                description: t.description.clone().unwrap_or_default(),
                body: t.agent.clone()?,
                tools: get("allowed_tools").and_then(|v| v.as_array()).map(|a| {
                    a.iter().filter_map(|v| v.as_str().map(String::from)).collect()
                }),
                max_cost_usd: limits.and_then(|l| l.get("max_llm_cost")).and_then(|v| v.as_float()),
                max_minutes: limits
                    .and_then(|l| l.get("timeout_s"))
                    .and_then(|v| v.as_integer())
                    .map(|s| (s.max(0) as u64).div_ceil(60)),
                deep: get("model_slot").and_then(|v| v.as_str()) == Some("deep"),
                icon: t.icon.clone(),
                shipped: t.owner == "system",
                files: t.files.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| (!a.shipped, &a.name).cmp(&(!b.shipped, &b.name)));
    // `/name` and the wire carry only the name, which resolves to the shipped
    // applet, so an unshipped one with a shipped one's name would show in the
    // menu as itself and start the shipped one (a pack's `admin` would open
    // the root shell). It is left out instead.
    let shipped: std::collections::HashSet<String> =
        out.iter().filter(|a| a.shipped).map(|a| a.name.clone()).collect();
    out.retain(|a| {
        let shadow = !a.shipped && shipped.contains(&a.name);
        if shadow {
            tracing::warn!(name = %a.name, applet_id = %a.applet_id, "an applet has a built-in applet's name, so /name leaves it out; rename its folder");
        }
        !shadow
    });
    out
}

/// The applet `/name` starts, shipped first.
pub fn chat_applet_named(name: &str) -> Option<ChatApplet> {
    chat_applets().into_iter().find(|a| a.name == name)
}

/// An applet as `use_applet` reaches it: its prompt, loaded into the turn
/// that calls it, and the files that prompt may read.
#[derive(Debug, Clone)]
pub struct UsableApplet {
    /// The spec's `name`, its folder's.
    pub name: String,
    pub description: String,
    pub body: String,
    /// Paths relative to `folder`, under `references/` and `assets/`.
    pub files: Vec<String>,
    pub folder: std::path::PathBuf,
    /// Its `invoke` includes `assistant` and it is a prompt alone, so the
    /// assistant may load it. An applet with a `run` stage starts through
    /// the `applet` tool's `run` instead, which runs the whole applet.
    pub for_assistant: bool,
}

/// Every applet with a prompt, shipped first, each group by name.
fn usable_applets() -> Vec<UsableApplet> {
    let guard = catalog_lock().read().expect("catalog rwlock poisoned");
    let mut out: Vec<(bool, UsableApplet)> = guard
        .action
        .iter()
        .filter_map(|t| {
            Some((
                t.owner == "system",
                UsableApplet {
                    name: t.dir.rsplit('/').next()?.to_string(),
                    description: t.description.clone().unwrap_or_default(),
                    body: t.agent.clone()?,
                    files: t.files.clone(),
                    folder: t.folder.clone()?,
                    // A disabled applet is not offered.
                    for_assistant: t.triggers.iter().any(|x| x == "tool")
                        && t.command.is_none()
                        && t.run_sql.is_none()
                        && t.default_enabled,
                },
            ))
        })
        .collect();
    out.sort_by(|(a_shipped, a), (b_shipped, b)| (!a_shipped, &a.name).cmp(&(!b_shipped, &b.name)));
    out.into_iter().map(|(_, a)| a).collect()
}

/// The applets the assistant may load by name, in the order `use_applet`
/// lists them.
pub fn assistant_applets() -> Vec<UsableApplet> {
    // A name loads the first applet that has it, the shipped one, so a second
    // with the same name is not listed as if it were its own.
    let mut seen = std::collections::HashSet::new();
    usable_applets().into_iter().filter(|a| seen.insert(a.name.clone())).filter(|a| a.for_assistant).collect()
}

/// The applet named `name` that has a prompt, shipped first.
pub fn usable_applet_named(name: &str) -> Option<UsableApplet> {
    usable_applets().into_iter().find(|a| a.name == name)
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

    // The copy must load from the state root, where it no longer counts as
    // shipped: an applet that runs a compiled binary by name does not, and has
    // nothing in its folder to change.
    let text = std::fs::read_to_string(shipped.join(SKILL_FILE))
        .map_err(|e| Error::Other(format!("could not read {dir}'s SKILL.md: {e}")))?;
    let stamped = skill::with_metadata(&text, "forked-from", &format!("virtues@{}", crate::codename::version()))
        .map_err(Error::Other)?;
    skill::parse(&stamped, &shipped, &dir, false).map_err(|problems| {
        Error::Other(format!("{dir} cannot be forked: {}", problems.join(" ")))
    })?;
    copy_tree(&shipped, &target)?;
    std::fs::write(target.join(SKILL_FILE), stamped)
        .map_err(|e| Error::Other(format!("could not write the fork's SKILL.md: {e}")))?;

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

/// The named keys the applet behind this id declares (`secrets = [...]`),
/// already checked by [`valid_secret_name`]. Resolves fan-out ids too.
pub fn declared_secrets(applet_id: &str) -> Vec<String> {
    let Some(dir) = dir_for_applet_id(applet_id) else {
        return Vec::new();
    };
    let guard = catalog_lock().read().expect("catalog rwlock poisoned");
    guard
        .action
        .iter()
        .find(|t| t.dir == dir)
        .map(|t| t.secrets.clone())
        .unwrap_or_default()
}

/// Every declared key name, with the names of the applets that declare it,
/// sorted. What Settings shows as a key's "used by".
pub fn secret_declarers() -> std::collections::BTreeMap<String, Vec<String>> {
    let guard = catalog_lock().read().expect("catalog rwlock poisoned");
    let mut out: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for t in &guard.action {
        for name in &t.secrets {
            out.entry(name.clone()).or_default().push(t.name.clone());
        }
    }
    for names in out.values_mut() {
        names.sort();
        names.dedup();
    }
    out
}

/// A name an applet may declare: an environment variable name in capitals
/// (`OPENAI_API_KEY`) that the box does not set itself. A declared
/// `DATABASE_URL` or `PATH` would otherwise replace the box's own.
pub fn valid_secret_name(name: &str) -> bool {
    let mut chars = name.chars();
    let starts = chars.next().is_some_and(|c| c.is_ascii_uppercase());
    starts
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && !crate::applet_runner::reserved_env_name(name)
}

/// What the folder at `dir` was forked from, if its manifest records it —
/// the read side of the fork's `forked-from` stamp. The source pane shows it, so "what
/// did this diverge from" is answered where the code is read.
pub fn forked_from_for_dir(dir: &str) -> Option<String> {
    let guard = catalog_lock().read().expect("catalog rwlock poisoned");
    guard
        .action
        .iter()
        .find(|t| t.dir == dir)
        .and_then(|t| t.forked_from.clone())
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

/// The template that produced an applet id: the one whose `id_prefix` is the
/// id, or one that fans out per credential or device and whose
/// `<id_prefix>_<anchor>` it is. An exact id wins, then the longest prefix.
///
/// Matching the first prefix in catalog order, which is unsorted, let
/// `applet_daily_2` resolve to `applet_daily`'s folder, so deleting one
/// applet could remove another's folder. A template that does not fan out
/// never claims a suffixed id, so an id whose own folder is gone resolves to
/// nothing instead of to a neighbor.
fn template_for_applet_id<'a>(templates: &'a [Template], applet_id: &str) -> Option<&'a Template> {
    if let Some(exact) = templates.iter().find(|t| t.id_prefix.as_deref() == Some(applet_id)) {
        return Some(exact);
    }
    templates
        .iter()
        .filter(|t| t.per_credential)
        .filter_map(|t| {
            let p = t.id_prefix.as_deref()?;
            applet_id.strip_prefix(p)?.starts_with('_').then_some((p.len(), t))
        })
        .max_by_key(|(len, _)| *len)
        .map(|(_, t)| t)
}

/// Resolve the manifest folder (relative to an applet root) that produced
/// an action id, by [`template_for_applet_id`]. Used by the face server to
/// root static serving at the applet's folder.
pub fn dir_for_applet_id(applet_id: &str) -> Option<String> {
    let guard = catalog_lock().read().expect("catalog rwlock poisoned");
    template_for_applet_id(&guard.action, applet_id).map(|t| t.dir.clone())
}

/// Which ontologies a live applet instance writes, resolved from its id.
///
/// The instance-id → template match is [`template_for_applet_id`].
///
/// This is the join that lets a stream say *why* it is empty. Freshness alone
/// cannot: a stream whose writing applet has failed on every run for three days
/// has exactly the same row counts as one with nothing to report, and was
/// therefore reported as `idle`.
pub fn ontologies_written_by_applet_id(applet_id: &str) -> Vec<String> {
    let guard = catalog_lock().read().expect("catalog rwlock poisoned");
    template_for_applet_id(&guard.action, applet_id)
        .map(|t| t.writes.clone())
        .unwrap_or_default()
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
             AND credential_id NOT IN (SELECT id FROM credentials WHERE kind = 'source')) \
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
            || template.run_sql.is_some()
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
                let devices: Vec<(String, Option<String>)> = sqlx::query_as(
                    "SELECT id, label FROM app_device
                      WHERE source_id = $1 AND revoked_at IS NULL
                      ORDER BY created_at",
                )
                .bind(source_id)
                .fetch_all(db)
                .await?;
                let qualify = devices.len() > 1;
                for (device_id, label) in &devices {
                    let applet_id = format!("{}_{}", id_prefix, device_id);
                    let anchor = qualify.then(|| label.as_deref()).flatten();
                    upsert_row(db, template, &applet_id, None, Some(device_id), anchor).await?;
                    live_ids.push(applet_id);
                    upserted += 1;
                }
            } else {
                // OAuth / API-key source: fan out per credential (the outbound
                // secret the action uses to call the provider).
                let credentials: Vec<(String, Option<String>)> = sqlx::query_as(
                    "SELECT id, name FROM credentials
                      WHERE kind = 'source' AND key = $1 AND status = 'active'
                      ORDER BY created_at",
                )
                .bind(source_id)
                .fetch_all(db)
                .await?;
                let qualify = credentials.len() > 1;
                for (cred_id, name) in &credentials {
                    let applet_id = format!("{}_{}", id_prefix, cred_id);
                    let anchor = qualify.then(|| name.as_deref()).flatten();
                    upsert_row(db, template, &applet_id, Some(cred_id), None, anchor).await?;
                    live_ids.push(applet_id);
                    upserted += 1;
                }
            }
        } else {
            upsert_row(db, template, id_prefix, None, None, None).await?;

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

/// The row's display name: the manifest's, qualified by the thing it fans out
/// from when that is the only way to tell two rows apart.
///
/// A `per_credential` manifest has ONE name and produces one row per account,
/// so connecting a second bank rendered "Plaid Transactions" twice — same
/// name, same description, same schedule, differing only in a "last run"
/// column, which reads as a rendering bug rather than as two banks. Four
/// Plaid manifests × two institutions = eight rows, four visible names.
/// The device branch has the same shape latent in it: a second iPhone would
/// duplicate "iOS Ingest".
///
/// Qualified only when the source actually has more than one anchor, so a box
/// with one Google account keeps "Google Mail" rather than carrying an email
/// address around in a list. The caller decides that from the same fetch it
/// fans out over, so it is one pass and deterministic — and because system
/// rows take `name = EXCLUDED.name` on conflict, adding or removing an account
/// re-qualifies every sibling on the next reconcile with no migration.
fn row_name(template: &Template, anchor_label: Option<&str>) -> String {
    match anchor_label.map(str::trim).filter(|s| !s.is_empty()) {
        Some(label) => format!("{} — {}", template.name, label),
        None => template.name.clone(),
    }
}

async fn upsert_row(
    db: &PgPool,
    template: &Template,
    applet_id: &str,
    credential_id: Option<&str>,
    device_id: Option<&str>,
    anchor_label: Option<&str>,
) -> Result<()> {
    let name = row_name(template, anchor_label);
    let triggers_json = serde_json::to_string(&template.triggers)
        .map_err(|e| Error::Other(format!("failed to serialize triggers: {e}")))?;

    // The command, stored as a JSON array.
    let command_json = match &template.command {
        Some(cmd) => Some(
            serde_json::to_string(cmd)
                .map_err(|e| Error::Other(format!("failed to serialize command: {e}")))?,
        ),
        None => None,
    };

    // The file's config (limits, model, tools), TOML to JSON. None is `{}`.
    let config_json: String = match &template.config {
        Some(cfg) => serde_json::to_string(cfg)
            .map_err(|e| Error::Other(format!("failed to serialize config: {e}")))?,
        None => "{}".to_string(),
    };

    // One rule for every applet, because the file is the applet: what it is
    // (name, owner, prompt, when it runs, what it runs, its limits and tools)
    // is rewritten from the file on every reconcile, and the owner's edits
    // reach it by changing the file (`write_edit`). What the box keeps beside
    // the file is never touched here: `enabled` (set from the file only when
    // the row is first made), `memory`, the code's `state`, run history, and
    // config keys the file does not own, such as the chat an applet was made
    // in.
    let watch_json = match &template.watch {
        Some(w) => Some(serde_json::to_string(w).map_err(|e| Error::Other(format!("failed to serialize watch: {e}")))?),
        None => None,
    };

    // A changed data trigger starts watching afresh: `watched_through` goes,
    // and the next poll sets it to now (`scheduler::watch`). The label is
    // only how the table is named, so a new one keeps it.
    let sql = r#"
        INSERT INTO app_applets (
            id, name, owner, agent, schedule, enabled, config, triggers,
            credential_id, command, device_id, until, description, run_sql, watch
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8::jsonb, $9, $10, $11, $12, $13, $14, $15::jsonb)
        ON CONFLICT(id) DO UPDATE SET
            name           = EXCLUDED.name,
            owner          = EXCLUDED.owner,
            agent          = EXCLUDED.agent,
            schedule       = EXCLUDED.schedule,
            config         = (app_applets.config - 'limits' - 'model_slot' - 'allowed_tools' - 'net_hosts')
                             || EXCLUDED.config,
            triggers       = EXCLUDED.triggers,
            credential_id  = EXCLUDED.credential_id,
            command        = EXCLUDED.command,
            device_id      = EXCLUDED.device_id,
            until          = EXCLUDED.until,
            description    = EXCLUDED.description,
            run_sql        = EXCLUDED.run_sql,
            watch          = EXCLUDED.watch,
            watched_through = CASE
                WHEN app_applets.watch - 'label' IS DISTINCT FROM EXCLUDED.watch - 'label' THEN NULL
                ELSE app_applets.watched_through
            END,
            updated_at     = now()
    "#;

    sqlx::query(sql)
        .bind(applet_id)
        .bind(&name)
        .bind(&template.owner)
        .bind(&template.agent)
        .bind(&template.schedule)
        .bind(template.default_enabled)
        .bind(&config_json)
        .bind(&triggers_json)
        .bind(credential_id)
        .bind(&command_json)
        .bind(device_id)
        .bind(&template.until)
        .bind(&template.description)
        .bind(&template.run_sql)
        .bind(&watch_json)
        .execute(db)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    /// An unknown manifest key is CAPTURED, not silently dropped — and the
    /// known ones still parse around it.
    ///
    /// The bug this guards has happened twice: `description` sat in every
    /// manifest in the tree with no field to receive it, and `runtime =
    /// "function"` shipped in `bookmark_enrichment`. Both were thrown away on
    /// every load, in silence, for months. The warning that now fires is only
    /// as good as the capture, so the capture is what is asserted here.
    #[test]
    fn an_unknown_manifest_key_is_kept_and_visible() {
        let dir = std::env::temp_dir().join(format!("tmpl-unknown-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("manifest.toml");
        std::fs::write(
            &path,
            "name = \"One\"\nowner = \"user\"\nruntime = \"function\"\nnot_a_key = 3\n",
        )
        .unwrap();

        let t = super::parse_template(&path, "one").expect("manifest should still load");

        assert_eq!(t.name, "One", "known keys must still parse");
        let mut unknown: Vec<&str> = t.unknown.keys().map(String::as_str).collect();
        unknown.sort();
        assert_eq!(
            unknown,
            vec!["not_a_key", "runtime"],
            "unknown keys must be captured so the loader can name them"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A declared key the box sets itself would replace the box's own value
    /// in the applet's environment, so the loader drops it, and keeps the rest.
    #[test]
    fn declared_secrets_keep_only_names_an_applet_may_have() {
        let dir = std::env::temp_dir().join(format!("tmpl-secrets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("manifest.toml");
        std::fs::write(
            &path,
            "name = \"Keys\"\nowner = \"user\"\nsecrets = [\"OPENAI_API_KEY\", \"DATABASE_URL\", \"VIRTUES_ENCRYPTION_KEY\", \"lower_case\", \"GITHUB_TOKEN\", \"9LIVES\"]\n",
        )
        .unwrap();

        let t = super::parse_template(&path, "keys").expect("manifest should still load");
        assert_eq!(t.secrets, ["OPENAI_API_KEY", "GITHUB_TOKEN"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

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
        // (data_activity_listening itself was deleted 2026-08-28 — no
        // collector ever existed — so the probe uses another absent name.)
        assert!(sources_writing("activity_never_existed").is_empty());
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
            login: None,
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

    /// SHADOWS a shipped applet with the same dir rather than colliding with
    /// it. This is what makes "fork a system applet" work, and what makes
    /// deleting your copy revert cleanly to shipped.
    #[test]
    fn state_root_shadows_shipped_root() {
        let shipped = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();

        let write = |root: &std::path::Path, dir: &str, description: &str| {
            let d = root.join(dir);
            std::fs::create_dir_all(&d).unwrap();
            let name = dir.rsplit('/').next().unwrap();
            std::fs::write(d.join(SKILL_FILE), format!("---\nname: {name}\ndescription: {description}\n---\n")).unwrap();
        };
        write(shipped.path(), "day-summary-eod", "Shipped Summary");
        write(shipped.path(), "weather-sync", "Weather");
        write(state.path(), "day-summary-eod", "My Summary");
        write(state.path(), "user/garden-log", "Garden Log");

        let mut merged = scan_root(shipped.path(), true, None);
        let shipped_count = merged.len();
        for t in scan_root(state.path(), false, None) {
            match merged.iter().position(|e| e.dir == t.dir) {
                Some(i) => merged[i] = t,
                None => merged.push(t),
            }
        }

        assert_eq!(shipped_count, 2, "shipped root parsed independently");
        assert_eq!(merged.len(), 3, "shadowing must not duplicate a dir");

        let by_dir = |d: &str| merged.iter().find(|t| t.dir == d).unwrap().description.clone().unwrap();
        assert_eq!(by_dir("day-summary-eod"), "My Summary", "state wins");
        assert_eq!(by_dir("weather-sync"), "Weather", "unshadowed shipped survives");
        assert_eq!(by_dir("user/garden-log"), "Garden Log", "authored applet is found");
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
        let d = shipped.path().join("weather-sync");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(SKILL_FILE), "---\nname: weather-sync\ndescription: W\n---\n").unwrap();

        assert_eq!(scan_root(shipped.path(), true, None).len(), 1);
        assert!(scan_root(&shipped.path().join("nope"), true, None).is_empty());
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

    /// A login must be able to fill the form it replaces, or the app would hand
    /// the box a credential the connect route then rejects as incomplete.
    #[test]
    fn source_logins_fill_their_fields() {
        let snapshot: ParsedTemplates = {
            let guard = catalog_lock().read().expect("catalog rwlock poisoned");
            guard.clone()
        };
        for s in &snapshot.source {
            let Some(login) = &s.login else { continue };
            let SourceAuth::ApiKey { fields } = &s.auth else {
                panic!("source {} declares a login but is not api_key", s.id);
            };
            assert!(!login.cookies.is_empty(), "source {}: login names no cookies", s.id);
            let filled: Vec<&String> = match &login.jar_field {
                Some(f) => vec![f],
                None => login.cookies.iter().collect(),
            };
            for f in fields {
                assert!(filled.contains(&f), "source {}: login does not fill field {f}", s.id);
            }
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

    /// A change to a shipped applet is an override: the catalog reads it on
    /// top of the shipped file, an edit back to what ships removes it, and
    /// an override cannot change what the applet runs or may use.
    #[test]
    fn a_shipped_applet_changes_through_its_override() {
        let shipped = tempfile::tempdir().unwrap();
        let overrides = tempfile::tempdir().unwrap();
        let folder = shipped.path().join("morning-note");
        std::fs::create_dir_all(&folder).unwrap();
        let text = "---\nname: morning-note\ndescription: Writes a short morning note. Use when asked.\n\
                    allowed-tools: sql_query\nmetadata:\n  trigger: every day at 07:00\n---\nWrite the note.\n";
        std::fs::write(folder.join(SKILL_FILE), text).unwrap();

        let patch = serde_json::json!({ "schedule": "0 0 9 * * *", "agent": "Write a shorter note." });
        let over = edit_shipped(text, None, patch.as_object().unwrap(), &folder, "morning-note").unwrap().expect("an override");
        assert!(!over.contains("allowed-tools") && !over.contains("description"), "only what changed: {over}");
        std::fs::write(override_in(overrides.path(), "morning-note"), &over).unwrap();

        let t = &scan_root(shipped.path(), true, Some(overrides.path()))[0];
        assert_eq!(t.schedule.as_deref(), Some("0 0 9 * * *"));
        assert_eq!(t.agent.as_deref(), Some("Write a shorter note."));
        assert_eq!(t.owner, "system", "it is still the shipped applet");
        assert!(scan_root(shipped.path(), true, None)[0].agent.as_deref() == Some("Write the note."));

        // Edited back to what ships, it needs no override.
        let back = serde_json::json!({ "schedule": "0 0 7 * * *", "agent": "Write the note." });
        assert_eq!(edit_shipped(text, Some(&over), back.as_object().unwrap(), &folder, "morning-note").unwrap(), None);

        // An override that reaches for the tools is not applied.
        std::fs::write(override_in(overrides.path(), "morning-note"), "---\nallowed-tools: sudo\n---\n").unwrap();
        assert_eq!(scan_root(shipped.path(), true, Some(overrides.path()))[0].agent.as_deref(), Some("Write the note."));
    }

    fn template(id_prefix: &str, dir: &str, per_credential: bool) -> Template {
        let mut t: Template = toml::from_str(&format!(
            "id_prefix = \"{id_prefix}\"\nname = \"{dir}\"\nowner = \"ai\"\nper_credential = {per_credential}"
        ))
        .expect("template toml");
        t.dir = dir.to_string();
        t
    }

    /// An applet's own folder wins over a neighbor whose id is a prefix of
    /// its id, in either catalog order. Before, the first prefix in an
    /// unsorted catalog won, so deleting `daily_2` could remove `daily`.
    #[test]
    fn an_id_resolves_to_its_own_folder_and_never_a_prefix_neighbors() {
        let daily = template("applet_daily", "user/daily", false);
        let daily_2 = template("applet_daily_2", "user/daily_2", false);
        for catalog in [vec![daily.clone(), daily_2.clone()], vec![daily_2.clone(), daily.clone()]] {
            let found = template_for_applet_id(&catalog, "applet_daily_2").map(|t| t.dir.as_str());
            assert_eq!(found, Some("user/daily_2"));
        }
        // Its folder gone, the id resolves to nothing, not to `daily`.
        assert!(template_for_applet_id(&[daily], "applet_daily_2").is_none());
    }

    /// A fan-out id still finds its template, and the longest prefix wins.
    #[test]
    fn a_fan_out_id_finds_the_longest_matching_template() {
        let sync = template("applet_mail", "mail", true);
        let sync_work = template("applet_mail_work", "mail_work", true);
        let catalog = [sync, sync_work];
        let dir = |id: &str| template_for_applet_id(&catalog, id).map(|t| t.dir.clone());
        assert_eq!(dir("applet_mail_cred_1").as_deref(), Some("mail"));
        assert_eq!(dir("applet_mail_work_cred_1").as_deref(), Some("mail_work"));
        assert_eq!(dir("applet_mailbox"), None);
    }

    /// A key the manifest adds reaches a box that seeded before it existed,
    /// and a key the row already holds is never touched.
    /// The row follows the file. A limit or prompt changed in the row alone
    /// goes back to the file's on the next reconcile, and what the box keeps
    /// beside the file (on/off, memory, the chat it was made in) stays.
    #[sqlx::test]
    async fn the_row_follows_the_file_and_keeps_the_boxs_own_state(pool: sqlx::PgPool) {
        reconcile_templates(&pool).await.expect("reconcile");
        sqlx::query(
            "UPDATE app_applets SET config = '{\"limits\": {\"max_llm_cost\": 5}, \"chat_id\": \"chat_x\"}'::jsonb, \
             agent = 'Changed in the row.', enabled = FALSE, memory = 'Kept.' WHERE id = 'applet_morning_examen'",
        )
        .execute(&pool)
        .await
        .unwrap();
        reconcile_templates(&pool).await.expect("reconcile");

        let (config, agent, enabled, memory): (serde_json::Value, Option<String>, bool, Option<String>) = sqlx::query_as(
            "SELECT config, agent, enabled, memory FROM app_applets WHERE id = 'applet_morning_examen'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(config["model_slot"], "lite");
        assert_eq!(config["limits"]["max_llm_cost"], 0.1, "the file's limit");
        assert_eq!(config["chat_id"], "chat_x", "the box's own keys stay");
        assert_ne!(agent.as_deref(), Some("Changed in the row."), "the prompt is the file's");
        assert!(!enabled, "on/off is the box's");
        assert_eq!(memory.as_deref(), Some("Kept."));
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

    /// The schema-replay path: a folder carrying `schema/NNNN_*.sql` must
    /// reach real tables, record each version exactly once, and re-grant the
    /// applet roles. Exercised against a synthetic fixture dir rather than a
    /// shipped applet — the last shipped applet with a schema/ was deleted
    /// 2026-08-26, and pointing VIRTUES_APPLETS_DIR at a fixture would race
    /// every other reconcile test in the process. `replay_pending` is the
    /// exact call reconcile makes per template dir.
    #[sqlx::test]
    async fn schema_replay_applies_records_and_grants(pool: sqlx::PgPool) {
        let dir = std::env::temp_dir().join(format!(
            "virtues-replay-probe-{}",
            std::process::id()
        ));
        let schema_dir = dir.join("schema");
        std::fs::create_dir_all(&schema_dir).unwrap();
        std::fs::write(
            schema_dir.join("0001_entries.sql"),
            "CREATE SCHEMA IF NOT EXISTS applet_replay_probe;\n\
             CREATE TABLE IF NOT EXISTS applet_replay_probe.entries (\n\
                 id text PRIMARY KEY DEFAULT gen_random_uuid()::text\n\
             );\n",
        )
        .unwrap();
        // The migrations table references the applet row, as on a real box.
        sqlx::query("INSERT INTO app_applets (id, name) VALUES ($1, $2)")
            .bind("applet_replay_probe")
            .bind("Replay Probe")
            .execute(&pool)
            .await
            .unwrap();

        let ran = crate::tools::applet_schema::replay_pending(
            &pool,
            "applet_replay_probe",
            "replay_probe",
            &dir,
        )
        .await;
        assert_eq!(ran, 1, "the fixture's 0001 did not run");

        let table: Option<String> = sqlx::query_scalar(
            "SELECT table_name FROM information_schema.tables \
              WHERE table_schema = 'applet_replay_probe' AND table_name = 'entries'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert_eq!(table.as_deref(), Some("entries"));

        // Recorded, so a second replay does not run it again.
        let ran_again = crate::tools::applet_schema::replay_pending(
            &pool,
            "applet_replay_probe",
            "replay_probe",
            &dir,
        )
        .await;
        assert_eq!(ran_again, 0, "a version must be applied exactly once");
        let recorded: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM app_applet_schema_migrations \
              WHERE applet_id = 'applet_replay_probe'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(recorded, 1);

        // A table nobody may read is not a table: the grant sweep must reach
        // schemas created after boot, or the face sees permission denied on
        // its own applet's table.
        crate::server::faces::ensure_applet_db_grants(&pool)
            .await
            .expect("grant sweep");
        for (role, priv_) in [
            ("virtues_face_reader", "SELECT"),
            ("virtues_applet_writer", "INSERT"),
        ] {
            let ok: bool = sqlx::query_scalar(
                "SELECT has_table_privilege($1, 'applet_replay_probe.entries', $2)",
            )
            .bind(role)
            .bind(priv_)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert!(ok, "{role} cannot {priv_} the replayed table");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[sqlx::test]
    async fn reconcile_is_idempotent(pool: sqlx::PgPool) {
        // Seed an active iOS credential so per_credential templates fan out.
        sqlx::query(
            "INSERT INTO credentials (id, key, name, status, secrets_ciphertext) \
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

    /// Two accounts on one source must not produce two rows with one name.
    ///
    /// Connecting a second bank made every Plaid manifest render twice —
    /// identical name, description and schedule — so the applets page showed
    /// four names for eight rows and the only thing separating them was a
    /// timestamp. A single account stays unqualified: a box with one Google
    /// account should read "Google Mail", not carry an email address around.
    #[sqlx::test]
    async fn a_second_account_makes_its_applets_tell_themselves_apart(pool: sqlx::PgPool) {
        let seed = |id: &'static str, name: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO credentials (id, key, name, status, secrets_ciphertext) \
                     VALUES ($1, 'plaid', $2, 'active', 'x')",
                )
                .bind(id)
                .bind(name)
                .execute(&pool)
                .await
                .unwrap();
            }
        };

        // One account: the manifest name stands alone.
        seed("cred_one", "Brex").await;
        reconcile_templates(&pool).await.expect("reconcile one");
        let solo: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM app_applets WHERE credential_id = 'cred_one' ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(!solo.is_empty(), "plaid templates should fan out");
        assert!(
            solo.iter().all(|n| !n.contains('—')),
            "a lone account must not be qualified: {solo:?}"
        );

        // Second account: every row on the source says which one it is.
        seed("cred_two", "SoFi").await;
        reconcile_templates(&pool).await.expect("reconcile two");
        let names: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM app_applets WHERE credential_id IN ('cred_one', 'cred_two')",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "names must be distinct: {names:?}");
        assert!(
            names.iter().any(|n| n.ends_with("— Brex"))
                && names.iter().any(|n| n.ends_with("— SoFi")),
            "each row should name its institution: {names:?}"
        );
    }

    /// A folder holding only `SKILL.md` is read by the catalog scan and
    /// reconciles into a row with its wiring.
    #[sqlx::test]
    async fn a_folder_with_only_skill_md_reconciles_into_a_row(pool: sqlx::PgPool) {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("weekly-review");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("SKILL.md"),
            "---\nname: weekly-review\ndescription: Looks back over your week.\n\
             allowed-tools: sql_query create_page\nmetadata:\n  trigger: every sunday at 18:00\n  \
             limit: $1 per run\n---\nWrite a short review of the seven days before today.\n",
        )
        .unwrap();
        // A sibling the box refuses is skipped, and does not take the scan down.
        let bad = root.path().join("broken");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("SKILL.md"), "---\nname: other\n---\n").unwrap();

        let found = scan_root(root.path(), false, None);
        assert_eq!(found.len(), 1, "only the valid folder loads");
        let t = &found[0];
        upsert_row(&pool, t, t.id_prefix.as_deref().unwrap(), None, None, None)
            .await
            .expect("upsert");

        let (name, owner, schedule, agent, triggers, config): (
            String,
            String,
            Option<String>,
            Option<String>,
            serde_json::Value,
            serde_json::Value,
        ) = sqlx::query_as(
            "SELECT name, owner, schedule, agent, triggers, config FROM app_applets \
             WHERE id = 'applet_weekly_review'",
        )
        .fetch_one(&pool)
        .await
        .expect("the row exists");
        assert_eq!(name, "Weekly review");
        assert_eq!(owner, "user");
        assert_eq!(schedule.as_deref(), Some("0 0 18 * * 0"));
        assert!(agent.unwrap().starts_with("Write a short review"));
        assert_eq!(triggers, serde_json::json!(["manual", "message", "tool", "cron"]));
        assert_eq!(config["limits"]["max_llm_cost"], 1.0);
        assert_eq!(config["allowed_tools"], serde_json::json!(["sql_query", "create_page"]));
    }

    /// Reconcile writes a data trigger into the row and keeps how far its
    /// rows have gone to runs, until the trigger itself changes.
    #[sqlx::test]
    async fn a_changed_data_trigger_starts_watching_afresh(pool: sqlx::PgPool) {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("meeting-notes");
        std::fs::create_dir_all(&folder).unwrap();
        let reconcile = |trigger: &str| {
            std::fs::write(
                folder.join("SKILL.md"),
                format!("---\nname: meeting-notes\ndescription: Asks how a meeting went.\nmetadata:\n  trigger: {trigger}\n---\nAsk how it went.\n"),
            )
            .unwrap();
            scan_root(root.path(), false, None).remove(0)
        };
        let watched = || async {
            sqlx::query_as::<_, (serde_json::Value, Option<chrono::DateTime<chrono::Utc>>)>(
                "SELECT watch, watched_through FROM app_applets WHERE id = 'applet_meeting_notes'",
            )
            .fetch_one(&pool)
            .await
            .unwrap()
        };

        let t = reconcile("15 minutes after data_calendar_event ends");
        upsert_row(&pool, &t, "applet_meeting_notes", None, None, None).await.unwrap();
        let (watch, _) = watched().await;
        assert_eq!(watch["kind"], "after_end");
        sqlx::query("UPDATE app_applets SET watched_through = now() WHERE id = 'applet_meeting_notes'")
            .execute(&pool)
            .await
            .unwrap();

        upsert_row(&pool, &t, "applet_meeting_notes", None, None, None).await.unwrap();
        assert!(watched().await.1.is_some(), "the same trigger keeps its place");

        let t = reconcile("30 minutes after data_calendar_event ends");
        upsert_row(&pool, &t, "applet_meeting_notes", None, None, None).await.unwrap();
        let (watch, through) = watched().await;
        assert_eq!((watch["minutes"].as_i64(), through), (Some(30), None), "a changed one starts again");
    }

    /// A chat-made applet that is retimed keeps its new schedule through a
    /// reconcile, and so does the rest of the edit.
    ///
    /// Reconcile's `ai` branch rewrites the prompt, schedule, triggers, limits
    /// and model from the file. The edit path wrote only the row, so the next
    /// boot or Reconcile put the old values back without a word.
    ///
    /// Drives the row through `update_applet` and the file through
    /// `skill::apply_patch` on a temp copy, because the catalog that resolves
    /// an id to its folder is global to the test process.
    #[sqlx::test]
    async fn a_retimed_applet_survives_a_reconcile(pool: sqlx::PgPool) {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("retime-probe");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(SKILL_FILE);
        std::fs::write(
            &path,
            "---\nname: retime-probe\ndescription: Says good morning.\nmetadata:\n  trigger: every day at 07:00\n  \
             limit: $0.5 per run\n  invoke: none\n---\nSay good morning.\n",
        )
        .unwrap();
        let id = "applet_user__retime_probe";
        let reconcile = |pool: sqlx::PgPool| {
            let (path, folder) = (path.clone(), folder.clone());
            async move {
                let text = std::fs::read_to_string(&path).unwrap();
                let t = skill::parse(&text, &folder, "user/retime-probe", false).expect("SKILL.md loads");
                upsert_row(&pool, &t, id, None, None, None).await.expect("upsert");
            }
        };
        reconcile(pool.clone()).await;
        sqlx::query("UPDATE app_applets SET config = config || '{\"chat_id\": \"chat_a\"}'::jsonb WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();

        let patch = serde_json::json!({
            "schedule": "0 30 9 * * *",
            "agent": "Say good evening.",
            "config": { "limits": { "max_llm_cost": 0.25 } },
        });
        let edited = skill::apply_patch(&std::fs::read_to_string(&path).unwrap(), patch.as_object().unwrap()).unwrap();
        std::fs::write(&path, edited).unwrap();

        reconcile(pool.clone()).await;

        let (schedule, agent, config): (Option<String>, Option<String>, serde_json::Value) =
            sqlx::query_as("SELECT schedule, agent, config FROM app_applets WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(schedule.as_deref(), Some("0 30 9 * * *"), "the new schedule must survive");
        assert_eq!(agent.as_deref(), Some("Say good evening."));
        assert_eq!(config["limits"]["max_llm_cost"], 0.25, "the file's limit takes the edit");
        assert_eq!(config["chat_id"], "chat_a", "a runtime key stays in the row");
    }

}
