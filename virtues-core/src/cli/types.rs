//! CLI argument types and command structures

use clap::{Parser, Subcommand};

/// Default port: reads NOMAD_PORT_http env var (Nomad host networking),
/// falling back to 8000 for local development.
fn default_port() -> u16 {
    std::env::var("NOMAD_PORT_http")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8000)
}

#[derive(Parser)]
#[command(name = "virtues")]
#[command(version, about = "Virtues personal data platform CLI", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Interactive setup wizard. Mostly historical — fresh hardware boots use
    /// `install.sh` which writes `.env` non-interactively. Kept for niche manual
    /// setups. Safe by default: backs up an existing `.env` to `.env.bak.<ts>`
    /// before overwriting.
    Init,

    /// Print a one-time URL for the human to open in their browser and land in
    /// a logged-in web session. Mints a fresh pair token in the DB; no `.env`
    /// touching, no prompts. Idempotent — run as often as needed.
    ///
    /// Honors `ENVIRONMENT=dev` to print `http://localhost:<VIRTUES_WEB_PORT>/...`
    /// (vite dev server) instead of `http://localhost:8000/...` (the production
    /// HTTP server on the box).
    Link,

    /// Approve a pending sudo request from the box.
    ///
    /// A "sudo request" is the confirmation step a paired web client triggers
    /// when it wants to do one of the 4 high-sensitivity actions (export all
    /// data, swap BYO AI key, wipe the box, revoke the last remaining other
    /// device). Running `virtues sudo` proves physical access — a thief with
    /// your laptop can't do it from outside, but you can sit at the box and
    /// approve.
    ///
    /// With no args: lists open requests and prompts for each.
    /// With `--id <REQ>`: targets one specific request id (scripting hook).
    Sudo {
        /// Approve a specific request id directly (skip the interactive list).
        #[arg(long)]
        id: Option<String>,

        /// Deny instead of approve.
        #[arg(long, conflicts_with = "id")]
        deny: bool,
    },

    /// Run database migrations
    Migrate,

    /// Snapshot the box's state into a single tarball.
    ///
    /// Includes the Postgres database (full `pg_dump`), the data-lake (action
    /// stream archives + drive files at `/var/lib/virtues/lake/`), and
    /// `/etc/virtues/env` (the encryption key — required to decrypt
    /// credentials in the DB).
    ///
    /// Because the env file is included, the tarball is **as sensitive as
    /// the box itself**. Store backups with the same care.
    Backup {
        /// Output path. Defaults to `/var/lib/virtues/backups/virtues-<utc-iso>.tar.gz`.
        #[arg(long)]
        output: Option<std::path::PathBuf>,

        /// Overwrite an existing file at the output path.
        #[arg(long)]
        force: bool,
    },

    /// Restore the box's state from a backup tarball.
    ///
    /// Destructive. The current Postgres DB is dropped + recreated; the lake
    /// directory is replaced; the env file is overwritten. Refuses to run if
    /// `systemctl is-active virtues` returns active (unless `--force`), or
    /// if the tarball was produced by a binary newer than this one (upgrade
    /// the binary first; we never restore-into-older-schema).
    Restore {
        /// Path to the tarball.
        path: std::path::PathBuf,

        /// Bypass the "service is running" check. The schema-version + sha256
        /// checks are never bypassable.
        #[arg(long)]
        force: bool,
    },

    /// Self-update from the latest GitHub Release.
    ///
    /// Stops the service, swaps `/usr/local/bin/virtues` with the new binary
    /// (keeping one `.bak` for rollback), runs `virtues migrate` to apply
    /// any schema changes, restarts the service.
    Upgrade {
        /// Report the available version without changing anything.
        #[arg(long)]
        check: bool,

        /// Pin to a specific tag (e.g. `v0.1.3`). Defaults to `latest`.
        #[arg(long)]
        version: Option<String>,
    },

    /// Start the HTTP server
    Server {
        /// Host to bind to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,

        /// Port to bind to (defaults to NOMAD_PORT_http env var, or 8000)
        #[arg(long, default_value_t = default_port())]
        port: u16,
    },

    /// Seed the database with demo data (people, places, events, etc.)
    Seed,

    /// Show box health: identity (CA / WG keypair / rendezvous), subscription,
    /// and paired devices. The deployment substrate's status command.
    ///
    /// `--json` emits a stable machine-readable summary instead of the human
    /// dashboard. Hand someone this output ("paste me `virtues status --json`")
    /// when triaging — it's the boring-but-complete diagnostic.
    Status {
        /// Emit machine-readable JSON instead of the human-friendly dashboard.
        #[arg(long)]
        json: bool,
    },

    /// Report a crash to the Virtues cloud diagnostic endpoint.
    ///
    /// Invoked by systemd's `ExecStopPost=` hook. Reads `$EXIT_STATUS` and
    /// `$EXIT_CODE` from the unit environment, tails the last 50 journal
    /// lines, and POSTs JSON to `https://atlas.virtues.com/diag/crash`.
    /// Honors `VIRTUES_DIAG=off` in `/etc/virtues/env` — when disabled,
    /// exits silently with code 0 so systemd doesn't log a failed
    /// post-stop.
    ///
    /// Never run this by hand. Service-internal hook.
    ReportCrash,

    /// First-boot bringup (non-interactive): run migrations + ensure the box's
    /// identity exists. Idempotent; the appliance runs this headless, DIY too.
    Bringup,

    /// Connect this box to a paid Virtues subscription (device-authorization
    /// flow). Prints a QR + URL and waits for you to complete checkout on a
    /// phone or browser; the box never holds a Stripe key.
    ///
    /// Most users want `virtues init` instead (full first-run wizard: config
    /// + subscribe + migrate). `subscribe` is the lower-level subscribe-only
    /// command for re-subscribing or dev iteration.
    #[command(alias = "claim")]
    Subscribe,

    /// Attach this box to an existing Virtues subscription via the
    /// magic-link login flow. Pairs with `virtues init`'s [1] Log in
    /// branch — same code path, just standalone for retries.
    Login,

    /// Pre-download ML models (embedding, etc.) for offline/Docker use
    WarmModels,

    /// Report the inference stack's hardware resolution without downloading:
    /// detected accelerator, whether this build links CUDA, the chosen ONNX
    /// precision, and whether each model is baked or would be downloaded. The
    /// DB-free composability check for appliance-vs-DIY (web status reads the
    /// same `model_cache::resolution_report`).
    Doctor,

    /// Compute novelty scores for all days with events
    ComputeNovelty,

    /// Compute autonomic z-scores for all days with avg_hr data
    ComputeAutonomic,

    /// Pair an iOS device manually (dev shortcut — bypasses the QR flow).
    ///
    /// Mints a `credentials` row with a **server-issued random bearer** (printed
    /// for you to paste into the app's keychain) and fans out the per-device iOS
    /// `app_actions`. The device id is stored only as a label, never as the
    /// bearer.
    PairIos {
        /// Device label (e.g. the app's install id). Stored as metadata only —
        /// NOT used as the auth token.
        device_id: String,

        /// Friendly name for the device
        #[arg(long, default_value = "iPhone")]
        name: String,
    },

    /// Diagnose token encryption: pull stored device tokens, try to decrypt
    /// them with the current `VIRTUES_ENCRYPTION_KEY`, and report what happens
    /// for each. Pass an optional bearer token to compare against the
    /// decrypted plaintext.
    VerifyTokens {
        /// Optional bearer token (raw, no "Bearer " prefix) to match against
        bearer: Option<String>,
    },

    /// Generate the day summary (autobiography + 24h event timeline) for a date.
    ///
    /// Calls `api::day_summary::generate_day_summary`, which gathers the day's
    /// ontology data, prompts the user's chat model via virtues-api, and writes
    /// the results to `wiki_days` (autobiography/epigraph/data_quality) and
    /// `wiki_events` (clearing existing auto events first; manual events are
    /// preserved). Gaps in the LLM-emitted timeline are backfilled as "Unknown"
    /// to guarantee 00:00–24:00 coverage.
    DaySummary {
        /// Date to summarize (YYYY-MM-DD). Defaults to today in the user's
        /// profile timezone (or local time if no timezone is set).
        #[arg(long)]
        date: Option<String>,
    },

    /// Run entity resolution (places + people) over the last N hours.
    ///
    /// This is the bridge while the legacy transform-chaining pipeline still
    /// owns clustering. The new actions path (ios_location, etc.) writes
    /// `data_location_point` rows but doesn't chain into place resolution,
    /// so visits don't get created. Use this to manually backfill.
    ResolveEntities {
        /// Lookback window in hours (default: 24)
        #[arg(long, default_value_t = 24)]
        hours: i64,
    },
}
