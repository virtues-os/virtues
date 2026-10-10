//! CLI argument types and command structures

use clap::{Parser, Subcommand};

/// Default port: reads NOMAD_PORT_http env var (Nomad host networking),
/// falling back to 8000 for local development.
pub(crate) fn default_port() -> u16 {
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

/// `virtues device <action>` — the allowlist as a CLI.
#[derive(Subcommand)]
pub enum DeviceCommands {
    /// List the devices currently allowed to reach this box (non-revoked).
    #[command(alias = "list")]
    Ls,

    /// Revoke a device by id — de-allowlists its iroh key so its next dial is
    /// refused, and revokes any credential rows it owns.
    #[command(alias = "revoke")]
    Rm {
        /// The device id (as shown by `virtues device ls`).
        id: String,
    },

    /// Print the pair code to bring a new device onto the allowlist.
    /// Alias for `virtues pair` scoped to the allowlist framing.
    Add,
}

/// `virtues agent-key <action>` — SSH keys for outside agents.
#[derive(Subcommand)]
pub enum AgentKeyCmd {
    /// Let an agent (Claude Code, Codex) reach this box with a key.
    ///
    /// The key goes to its own user, `virtues-agent`, and can run only the
    /// data verbs: query, search, schema, write, applet, page. No shell, no
    /// forwarding, nothing else. Run with sudo.
    Add {
        /// The public key file (`.pub`), or `-` for stdin.
        key: String,
        /// A name for it, shown in `ls` and in the server's audit lines.
        #[arg(long)]
        name: String,
    },

    /// List agent keys.
    #[command(alias = "list")]
    Ls,

    /// Remove an agent key. Its next connection is refused.
    #[command(alias = "revoke")]
    Rm {
        name: String,
    },
}

/// `virtues applet <action>` — read and change the box's applets.
#[derive(Subcommand)]
pub enum AppletCmd {
    /// List applets, with each one's last run.
    #[command(alias = "list")]
    Ls {
        /// Include archived applets (their `until` has been met).
        #[arg(long)]
        all: bool,
        #[command(flatten)]
        out: OutputArgs,
    },

    /// Show one applet and its last ten runs.
    Get {
        /// The applet id, as `virtues applet ls` shows it.
        id: String,
        #[command(flatten)]
        out: OutputArgs,
    },

    /// Create or update an applet from a folder, and turn it on.
    ///
    /// Runs the check first and creates nothing when it finds anything. A
    /// scheduled applet is turned on unless `--off`; one with a prompt and no
    /// daily spend limit gets `max_llm_cost_per_day = 1.00`. Needs the virtues
    /// server running on this machine.
    Put {
        /// The applet folder, or `-` for `setup_applet`'s JSON on stdin.
        path: String,
        /// Leave it turned off.
        #[arg(long)]
        off: bool,
        #[command(flatten)]
        out: OutputArgs,
    },

    /// Turn an applet on.
    On {
        id: String,
    },

    /// Turn an applet off.
    Off {
        id: String,
    },

    /// Run an applet now. Count limits do not apply; spend limits do.
    Run {
        id: String,
        /// For an applet that works on one day: YYYY-MM-DD.
        #[arg(long)]
        date: Option<String>,
        #[command(flatten)]
        out: OutputArgs,
    },

    /// Check an applet against this box without creating anything.
    ///
    /// The same check chat runs before it creates an applet: cron shape, SQL
    /// conditions EXPLAINed, schema DDL dry-run and drift, table names in the
    /// prompt, limit keys. Takes an applet folder (`manifest.toml`,
    /// `face/index.html`, `schema/NNNN_*.sql`) or `-` for `setup_applet`'s
    /// JSON arguments on stdin. Exits 1 when there are findings.
    Check {
        /// The applet folder, or `-` for JSON on stdin.
        path: String,
        #[command(flatten)]
        out: OutputArgs,
    },
}

/// `virtues page <action>` — read and write pages.
#[derive(Subcommand)]
pub enum PageCmd {
    /// Create a page. Its markdown comes from stdin when stdin is piped.
    New {
        /// The page title.
        title: String,
        #[command(flatten)]
        out: OutputArgs,
    },

    /// Edit a page through the live document, so an open editor sees the
    /// change.
    ///
    /// A markdown page takes `--find` and `--replace`. A block page takes
    /// ops on the block ids `page get --blocks` prints, with the base it
    /// prints: `--ops` for a JSON array of {op, id, html}, or one op with
    /// `--op`, `--block` and `--html`. An append needs no base. All or
    /// none: a refused edit writes nothing, says why, and exits 1.
    #[command(group(clap::ArgGroup::new("change").required(true).args(["find", "ops", "op"])))]
    Edit {
        /// The page id.
        id: String,
        /// A markdown page: the exact text to find, as the markdown reads.
        #[arg(long, requires = "replace")]
        find: Option<String>,
        /// What to put in its place, or `-` for stdin.
        #[arg(long, requires = "find")]
        replace: Option<String>,
        /// A block page: the base from `page get --blocks` or the last edit.
        #[arg(long)]
        base: Option<String>,
        /// A block page: a file holding the ops as a JSON array, or `-` for
        /// stdin.
        #[arg(long, conflicts_with_all = ["op", "find"])]
        ops: Option<String>,
        /// A block page: one op. replace, insert-after, insert-before,
        /// delete or append.
        #[arg(long, value_parser = parse_op, conflicts_with = "find")]
        op: Option<String>,
        /// The id of the block the op names.
        #[arg(long, requires = "op", conflicts_with_all = ["ops", "find"])]
        block: Option<String>,
        /// The op's HTML, whole blocks such as <p>…</p>, or `-` for stdin.
        #[arg(long, requires = "op", conflicts_with_all = ["ops", "find"])]
        html: Option<String>,
        #[command(flatten)]
        out: OutputArgs,
    },

    /// Print a page's markdown.
    #[command(group(clap::ArgGroup::new("read_on").args(["after", "ids"])))]
    Get {
        /// The page id.
        id: String,
        /// A block page: its HTML with block ids (or, when long, its
        /// markdown with each block's id above it, as far as one read
        /// holds), and on stderr the base `page edit` takes.
        #[arg(long)]
        blocks: bool,
        /// A long block page: read on after this block, the one the last
        /// read named. Takes that read's `--base`.
        #[arg(long, requires_all = ["blocks", "base"])]
        after: Option<String>,
        /// A block page: these blocks by id, comma-separated, as HTML to
        /// edit: a block shown only as markdown, or one inside a list or
        /// table too long for one read, the list cut down to the way to it.
        /// Takes the last read's `--base`.
        #[arg(long, value_delimiter = ',', requires_all = ["blocks", "base"])]
        ids: Vec<String>,
        /// The base from the last read, for `--after` or `--ids`.
        #[arg(long, requires = "read_on")]
        base: Option<String>,
        #[command(flatten)]
        out: OutputArgs,
    },
}

/// An op's name as `edit_page` spells it, from the CLI's (`insert-after`)
/// or the tool's (`insert_after`).
fn parse_op(name: &str) -> Result<String, String> {
    let op = name.trim().replace('-', "_");
    match op.as_str() {
        "replace" | "insert_after" | "insert_before" | "delete" | "append" => Ok(op),
        _ => Err("use replace, insert-after, insert-before, delete or append".to_string()),
    }
}

/// The output switch every data verb takes. Without it the verb prints a
/// table on a terminal and tab-separated lines when piped.
#[derive(clap::Args, Clone, Copy)]
pub struct OutputArgs {
    /// Print the tool's JSON result instead of a table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Finish a fresh install: run migrations, mint a pair code, print the
    /// handoff. Idempotent — safe to re-run at any time.
    ///
    /// PLUMBING, not a wizard. The account/subscribe/naming conversation this
    /// used to host has moved to the app, because a TTY is the worst available
    /// medium for billing and OAuth. `virtues-installer` execs this as the last
    /// step of `curl virtues.com/sh | sudo sh`; it writes no config of its own
    /// and touches no `.env`.
    ///
    /// (Its help used to describe an interactive wizard that backed up `.env`
    /// before overwriting it, and pointed at an `install.sh` that no longer
    /// exists. It did none of those things.)
    #[command(hide = true)]
    Init,

    /// Pair a device with your box: print the standing code to type into the
    /// app, then wait until it's used.
    ///
    /// Prints the box's multi-use standing code (minting one if none is live),
    /// NOT a fresh one-time token, and NOT a URL or QR — a browser cannot pair
    /// (it holds no iroh key) and the desktop app has no camera, so the code is
    /// typed by hand. No `.env` touching, no prompts. Idempotent — run as often
    /// as needed. THE one human verb for connecting a device to the box
    /// (agents/build/onboarding.md). `login` and `link` survive as aliases (this used
    /// to be `virtues login`).
    #[command(alias = "login", alias = "link")]
    Pair {
        /// Print the code and exit immediately instead of waiting for it
        /// to be used (scripts, copy-paste workflows).
        #[arg(long)]
        no_wait: bool,
    },

    /// Approve a pending sudo request from the box.
    ///
    /// A "sudo request" is the confirmation step a paired web client triggers
    /// when it wants to do one of the 5 high-sensitivity actions (export all
    /// data, swap BYO AI key, wipe the box, revoke the last remaining other
    /// device, install a third party's applet package — that last one runs
    /// their code on this box). Running `virtues sudo` proves physical access
    /// — a thief with your laptop can't do it from outside, but you can sit
    /// at the box and approve.
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

    /// Manage the devices allowed to reach this box.
    ///
    /// A paired device = a row in `app_device` holding an allowlisted iroh
    /// EndpointId. The allowlist IS the auth boundary: `ls` shows who can reach
    /// the box, `rm` de-allowlists a device (its next dial is refused at the
    /// handshake), and `add` prints a pair code to bring a new device on.
    Device {
        #[command(subcommand)]
        action: DeviceCommands,
    },

    /// Run database migrations
    Migrate {
        /// Check lineage only — apply NOTHING. Diffs the DB's applied
        /// migrations against this binary's embedded set and exits non-zero
        /// on divergence (applied-but-missing or checksum drift). The upgrade
        /// preflight runs this under the STAGED binary before any swap, so a
        /// lineage mismatch is a clean refusal instead of a mid-swap brick.
        #[arg(long)]
        check: bool,
    },

    /// Snapshot the box's state into a single tarball.
    ///
    /// Includes the Postgres database (full `pg_dump`), the data-lake (action
    /// stream archives + drive files at `/var/lib/virtues/lake/`), and the env
    /// file holding the encryption key — required to decrypt credentials in
    /// the DB. Refuses to produce a backup when no env file can be found,
    /// since the result would be an undecryptable dump.
    ///
    /// Because the env file is included, the tarball is **as sensitive as
    /// the box itself**. Store backups with the same care.
    Backup {
        /// Output path. Defaults to
        /// `/var/lib/virtues/backups/virtues-<utc-iso>.tar.gz.age` — the
        /// archive is `tar -> gzip -> age`, so the `.age` is not decoration.
        #[arg(long)]
        output: Option<std::path::PathBuf>,

        /// Overwrite an existing file at the output path.
        #[arg(long)]
        force: bool,

        /// Produce a backup even when no encryption key can be found. The
        /// resulting tarball CANNOT decrypt its own database — only useful
        /// for dev boxes that keep the key elsewhere.
        #[arg(long)]
        allow_missing_key: bool,

        /// Mint this box's backup key and print the recovery secret.
        ///
        /// Run it from a terminal you are watching: the secret is shown once
        /// and cannot be recovered. Nothing else creates it — not a first
        /// backup, and never a scheduled run — because a key minted where
        /// nobody is reading produces archives nobody can ever open.
        #[arg(long, conflicts_with_all = ["verify", "volume", "output"])]
        init_key: bool,

        /// Verify an existing archive instead of writing one: decrypt it,
        /// re-hash every member, and compare against its manifest.
        ///
        /// Reads nothing else and writes nothing. A backup nobody has ever
        /// opened is a hope; this is the cheap way to stop it being one.
        #[arg(long, value_name = "ARCHIVE", conflicts_with_all = ["output", "volume"])]
        verify: Option<std::path::PathBuf>,

        /// Recovery key file, for `--verify` on an encrypted archive.
        #[arg(long, requires = "verify")]
        key_file: Option<std::path::PathBuf>,

        /// Back up to a registered volume instead of a local file.
        ///
        /// Writes a full archive plus an increment carrying only the lake files
        /// that volume has not already received. `all` targets every registered
        /// volume; one that is not attached is skipped, not an error.
        #[arg(long, value_name = "ID|all")]
        volume: Option<String>,
    },

    /// Restore the box's state from a backup tarball.
    ///
    /// Destructive. The current Postgres DB is dropped + recreated; the lake
    /// directory is replaced; the env file is overwritten. Refuses to run if
    /// `systemctl is-active virtues` returns active (unless `--force`), or
    /// if the tarball was produced by a binary newer than this one (upgrade
    /// the binary first; we never restore-into-older-schema).
    Restore {
        /// Path to the archive. Omit when using `--from-volume`.
        path: Option<std::path::PathBuf>,

        /// Bypass the "service is running" check. The schema-version + sha256
        /// checks are never bypassable.
        #[arg(long)]
        force: bool,

        /// Restore from a backup drive rather than a single archive: its
        /// newest full archive, then every increment in order.
        ///
        /// Takes a PATH — the mount point, or the box directory on it. Not a
        /// registered volume id: the registry lives in the database being
        /// restored, so on replacement hardware there is nothing to look up.
        #[arg(long, value_name = "PATH", conflicts_with = "path")]
        from_volume: Option<std::path::PathBuf>,

        /// File holding the age recovery key printed when this box took its
        /// first backup.
        ///
        /// Required for encrypted archives. The box keeps only the public half
        /// of that keypair, so it cannot decrypt its own backups — which is
        /// exactly what stops a stolen box from reading them, and why this
        /// cannot be recovered from the box if you lose it.
        #[arg(long)]
        key_file: Option<std::path::PathBuf>,
    },

    /// Register and inspect backup destinations.
    Volumes {
        #[command(subcommand)]
        cmd: VolumesCmd,
    },

    /// Remove Virtues from this machine (box installs; requires root).
    ///
    /// Probes for every artifact the installer creates and prints the exact
    /// manifest before touching anything. Confirmation = typing this box's
    /// hostname. Shared infra (Postgres server, Avahi) stays; the inference
    /// sidecars (llama-server + units) are ours and go.
    Uninstall {
        /// Keep all data: /var/lib/virtues (env + ENCRYPTION KEY + lake),
        /// the Postgres db/role, and the system user. A later reinstall
        /// picks the box back up. This is the dev-loop tier.
        #[arg(long)]
        keep_data: bool,

        /// Also remove the downloaded GGUF models (/var/lib/virtues/models)
        /// when using --keep-data. They re-download on reinstall.
        #[arg(long)]
        purge_models: bool,

        /// Skip the typed-hostname confirmation (scripts/CI). Root is
        /// still required.
        #[arg(long)]
        force: bool,
    },

    /// Wipe this box back to a fresh state (HIDDEN; testing only).
    ///
    /// Default (full): drops all app tables — all data AND the box's identity
    /// (CA, WireGuard keys, paired devices, subscription link) — re-runs
    /// migrations, then clears the data lake. The encryption key + the `vector`
    /// extension are kept. Refuses if the service is running (unless `--force`);
    /// confirmation = typing this box's hostname (unless `--yes`).
    ///
    /// `--keep-data`: just RE-OPEN onboarding — revoke paired devices so the
    /// setup wizard reappears and you re-pair. Keeps your indexed data, sources,
    /// subscription, identity, and schema. Safe to run on a live box.
    #[command(hide = true)]
    Reset {
        /// Re-open onboarding without deleting data: revoke devices only.
        #[arg(long)]
        keep_data: bool,

        /// Skip the confirmation prompt (scripts/CI).
        #[arg(long)]
        yes: bool,

        /// Bypass the "service is running" check (full reset only).
        #[arg(long)]
        force: bool,
    },

    /// Strip every per-unit identity so this box's disk can be imaged and
    /// cloned. The LAST command before a box ships or its boot card is `dd`'d.
    ///
    /// Not a reset and not an uninstall — the software stays installed and
    /// configured. It removes only what must be unique per unit: the database
    /// (which holds the iroh secret that IS the box's network identity), the
    /// lake, `VIRTUES_ENCRYPTION_KEY`, machine-id, SSH host keys, and saved
    /// wifi. Each clone re-mints them on first boot.
    ///
    /// Cloning without this ships every unit as the same box: one EndpointId
    /// across the fleet, and one data-at-rest key. Neither is visible on the
    /// bench and neither is fixable in the field.
    #[command(hide = true)]
    Deprovision {
        /// Skip the typed-hostname confirmation (scripts/CI).
        #[arg(long)]
        yes: bool,

        /// Bypass the "service is running" check.
        #[arg(long)]
        force: bool,
    },

    /// Is this disk safe to image and clone? Read-only; changes nothing.
    ///
    /// The gate between `virtues deprovision` and `dd`. Deprovision reports
    /// success and tells you to power off, and until now that was the whole
    /// assurance — nothing ever re-checked the result. A per-unit secret that
    /// survived into a master image is invisible on the bench and catastrophic
    /// in the field: the iroh secret IS the box's identity, so clones of an
    /// un-deprovisioned master are literally the same box, and one leaked
    /// encryption key decrypts every unit ever shipped.
    ///
    /// Exits non-zero on any finding, so it can be the last line of a
    /// manufacturing script. See agents/build/appliance-image.md.
    #[command(name = "image-check")]
    ImageCheck,

    /// Self-update from the latest GitHub Release, via atomic release slots.
    ///
    /// Stages the whole release into `releases/<slot>/`, preflights it (the
    /// staged binary must pass `migrate --check` + a version smoke test),
    /// then activates by flipping the `current` symlink — binary + web +
    /// actions move together. Failures before the flip leave the box
    /// untouched; failures after flip straight back.
    Upgrade {
        /// Report the available version without changing anything.
        #[arg(long)]
        check: bool,

        /// Pin to a specific tag (e.g. `v0.1.3`). Defaults to `latest`.
        #[arg(long)]
        version: Option<String>,

        /// Track the staging channel: upgrade to the newest *prerelease* instead
        /// of the latest stable. (Once staging tags are marked prerelease, the
        /// default `latest` follows stable; `--pre` opts into staging.)
        #[arg(long)]
        pre: bool,

        /// Allow installing a version older than the one currently running.
        /// Without this, a downgrade is refused so a stale or tampered "latest"
        /// can't roll the box back to a known-vulnerable build. (No effect with
        /// `--pre`, where the prerelease channel is an explicit opt-in.)
        #[arg(long)]
        force: bool,

        /// Refresh only the named components (comma-separated: `web`,
        /// `actions`) in the CURRENT release — no binary swap, no migration,
        /// no restart. The safe fast path for UI iteration.
        #[arg(long)]
        only: Option<String>,
    },

    /// Show or set the release channel this box follows.
    ///
    /// With no argument, prints the current channel. With one, persists it to
    /// the state root, so `virtues upgrade` follows that line from then on
    /// without needing `--pre` every time — which is the whole point: `--pre`
    /// is a one-off override and forgets itself, so a box meant to track
    /// staging silently drifted back to stable the first time anyone typed a
    /// bare `virtues upgrade`.
    ///
    /// Accepts `stable` or `prerelease` (`pre`, `edge` and `nightly` are taken
    /// as prerelease — they are what people actually type).
    Channel {
        /// The channel to follow. Omit to print the current one.
        channel: Option<String>,
    },

    /// Flip back to the previous release slot and restart.
    ///
    /// The atomic inverse of `upgrade`: one symlink flip restores binary +
    /// web + actions together. Schema is not rolled back (migrations only go
    /// forward); the previous binary tolerates a newer schema.
    Rollback,

    /// Download, verify, and preflight the newest release — without installing.
    ///
    /// The first half of `upgrade`, on its own: the release is staged into its
    /// slot and made to prove itself (`--version` smoke + `migrate --check`),
    /// but `current` is not touched, so the box is byte-identical afterwards
    /// whether this succeeds or fails. `virtues activate` installs the result.
    ///
    /// Costs nothing when there is nothing to do — two small API calls settle
    /// "already on it" before any transfer starts. The box runs this on a
    /// schedule on the stable channel; running it by hand is the same work.
    Prepare {
        /// Re-fetch and re-stage even if the newest build is already staged.
        /// Also skips the migration lineage gate.
        ///
        /// Does NOT re-stage the release this box is already RUNNING: that
        /// resolves to the live slot, and staging starts by deleting it. Use
        /// `upgrade --force` to reinstall in place, which restarts into the
        /// result rather than leaving it half-written underneath a live box.
        #[arg(long)]
        force: bool,
    },

    /// Install the release `prepare` staged: flip, migrate, restart.
    ///
    /// The second half of `upgrade`. Preflights the staged release again first,
    /// because the box's schema may have moved since it was prepared. Fails
    /// cleanly if nothing is staged.
    Activate,

    /// The nightly update pass: `prepare`, then `activate`, on this box's
    /// channel. The server runs this every night unless automatic updates are
    /// turned off in Settings; running it by hand does the same work.
    ///
    /// A release that failed to activate is not retried here — the box waits
    /// for a newer build, or for `virtues activate` by hand.
    AutoUpdate,

    /// Start the HTTP server
    #[command(hide = true)]
    Server {
        /// Host to bind to. Default `[::]` is dual-stack — it accepts IPv4 AND
        /// IPv6, including the WG tunnel's ULA the pairing bundle advertises.
        /// (`0.0.0.0` would be IPv4-only and unreachable over the tunnel.)
        #[arg(long, default_value = "[::]")]
        host: String,

        /// Port to bind to (defaults to NOMAD_PORT_http env var, or 8000)
        #[arg(long, default_value_t = default_port())]
        port: u16,
    },

    /// Seed the database with demo data (people, places, events, etc.)
    #[command(hide = true)]
    Seed,

    /// Show box health: identity (WG keypair), subscription, and paired devices.
    /// The deployment substrate's status command.
    ///
    /// `--json` emits a stable machine-readable summary instead of the human
    /// dashboard. Hand someone this output ("paste me `virtues status --json`")
    /// when triaging — it's the boring-but-complete diagnostic.
    Status {
        /// Emit machine-readable JSON instead of the human-friendly dashboard.
        #[arg(long)]
        json: bool,
    },

    /// Run a read-only SQL query over your data.
    ///
    /// Runs as the same restricted database role chat uses: `data_*` and
    /// `wiki_*` tables, no secrets, 25 seconds at most. Pass `-` to read the
    /// SQL from stdin. `virtues schema` lists what you can query.
    Query {
        /// The SELECT to run, or `-` for stdin.
        sql: String,
        /// Rows to return (max 200).
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[command(flatten)]
        out: OutputArgs,
    },

    /// Search your data by meaning.
    Search {
        /// What to look for.
        #[arg(required = true, num_args = 1..)]
        text: Vec<String>,
        /// Only records linked to this entity id: `person_…`, `place_…` or
        /// `org_…` (repeatable).
        #[arg(long = "entity")]
        entities: Vec<String>,
        /// Only these domains: document, email, message, calendar, chat,
        /// transaction, transcription, page (repeatable).
        #[arg(long = "domain")]
        domains: Vec<String>,
        /// Only records on or after this date (ISO 8601).
        #[arg(long)]
        after: Option<String>,
        /// Only records before this date (ISO 8601).
        #[arg(long)]
        before: Option<String>,
        /// Results to return (max 50).
        #[arg(long, default_value_t = 10)]
        limit: u32,
        #[command(flatten)]
        out: OutputArgs,
    },

    /// List the tables you can query, or describe some of them.
    Schema {
        /// Tables to describe. With none, lists every queryable table.
        tables: Vec<String>,
        #[command(flatten)]
        out: OutputArgs,
    },

    /// Insert, update or delete rows in an applet's own tables.
    ///
    /// Runs as the applet writer role: `applet_*` schemas only, never your
    /// record. Pass `-` to read the SQL from stdin. Needs the virtues server
    /// running on this machine.
    Write {
        /// The statement to run, or `-` for stdin.
        sql: String,
        #[command(flatten)]
        out: OutputArgs,
    },

    /// Manage the SSH keys outside agents use to reach this box.
    AgentKey {
        #[command(subcommand)]
        cmd: AgentKeyCmd,
    },

    /// The forced command behind an agent key. sshd runs it; never by hand.
    #[command(hide = true)]
    AgentExec {
        /// The key's name, from its `authorized_keys` line.
        #[arg(long)]
        key: String,
    },

    /// Read and change the box's applets.
    Applet {
        #[command(subcommand)]
        cmd: AppletCmd,
    },

    /// Read and write pages.
    Page {
        #[command(subcommand)]
        cmd: PageCmd,
    },

    /// Report a crash to the Virtues cloud diagnostic endpoint.
    ///
    /// Invoked by systemd's `ExecStopPost=` hook. Reads `$EXIT_STATUS` and
    /// `$EXIT_CODE` from the unit environment, tails the last 50 journal
    /// lines, and POSTs JSON to `https://atlas.virtues.com/diag/crash`.
    /// Honors `VIRTUES_DIAG=off` in the env file systemd loads
    /// (`/var/lib/virtues/virtues.env` on installer-built boxes) — when
    /// disabled, exits silently with code 0 so systemd doesn't log a failed
    /// post-stop.
    ///
    /// Never run this by hand. Service-internal hook.
    #[command(hide = true)]
    ReportCrash,

    /// First-boot bringup (non-interactive): run migrations + ensure the box's
    /// identity exists. Idempotent; the appliance runs this headless, DIY too.
    #[command(hide = true)]
    Bringup,

    /// Connect this box to a paid Virtues subscription (device-authorization
    /// flow). Prints a QR + URL and waits for you to complete checkout on a
    /// phone or browser; the box never holds a Stripe key.
    ///
    /// A power-user hatch for re-subscribing or dev iteration. In the normal
    /// flow the app carries the account grant to the box over Bluetooth and
    /// nobody runs this. (It used to point at `virtues init` as the "full
    /// first-run wizard: config + subscribe + migrate" — init has been
    /// plumbing-only since the account conversation left the TTY.)
    #[command(alias = "claim", hide = true)]
    Subscribe,

    /// Attach this box to an existing Virtues subscription via the
    /// magic-link login flow. Standalone, for retries.
    ///
    /// Hidden power-user command, and the distinction it turns on is worth
    /// keeping straight: `virtues pair` attaches a DEVICE to this box, this
    /// attaches this box to an ACCOUNT. In the normal flow the app carries the
    /// account grant over Bluetooth (agents/build/onboarding-paradigm.md §7) and
    /// neither is typed.
    ///
    /// (It used to describe itself as pairing with `virtues init`'s "[1] Log
    /// in" branch — a menu that no longer exists.)
    #[command(name = "account-login", hide = true)]
    AccountLogin,

    /// Pre-download ML models (embedding, etc.) for offline/Docker use
    #[command(hide = true)]
    WarmModels,

    /// Point search at a different embedding server, or re-validate the current
    /// one after its model changed. With `--embed-url`, switches the box to
    /// that server (e.g. llama.cpp on your GPU) and keeps the index when the
    /// server runs the same model. Without it, re-probes the configured
    /// endpoint and, on a model change, re-embeds from source.
    #[command(name = "configure-inference")]
    ConfigureInference {
        /// Switch search to this embedding server (an OpenAI-style
        /// /v1/embeddings endpoint on this machine, your LAN, or your VPN).
        #[arg(long)]
        embed_url: Option<String>,
        /// Rerank server to record with the switch. Reranking stays off
        /// unless VIRTUES_RERANK_GAP opts in.
        #[arg(long)]
        rerank_url: Option<String>,
        /// Model name sent with every request; Ollama routes by it.
        #[arg(long)]
        embed_model: Option<String>,
        /// Re-embed without the interactive confirmation if the model changed.
        #[arg(long)]
        reembed: bool,
        /// Skip confirmation prompts (scripts/CI).
        #[arg(long)]
        yes: bool,
    },

    /// Adopt orphaned media into the lake: recordings written before the lake
    /// existed live outside it (a cwd-relative path bug), so they are invisible to
    /// lake accounting and to any GC. Copies them in, registers them, rewrites the
    /// pointers. Idempotent; leaves the originals in place for you to verify first.
    LakeAdopt {
        /// Report what would be adopted without copying or rewriting anything.
        #[arg(long)]
        dry_run: bool,
    },

    /// Rebuild the derived search index from source with the current model.
    /// Wipes the vector + BM25 index (source data is untouched), resizes the
    /// vector columns to match the model, and re-embeds. Use after an index
    /// schema change (e.g. the halfvec/BM25 upgrade) or to recover a stale index.
    Reindex {
        /// Skip the confirmation prompt (scripts/CI).
        #[arg(long)]
        yes: bool,
    },

    /// Report the inference stack's hardware resolution without downloading:
    /// detected accelerator, whether this build links CUDA, the chosen ONNX
    /// precision, and whether each model is baked or would be downloaded. The
    /// DB-free composability check for appliance-vs-DIY (web status reads the
    /// same `inference_report::resolution_report`).
    Doctor,

    /// Compute novelty scores for all days with events
    #[command(hide = true)]
    ComputeNovelty,

    /// Compute autonomic z-scores for all days with avg_hr data
    #[command(hide = true)]
    ComputeAutonomic,

    /// Annotate events from their own time windows: avg_hr, entities,
    /// source_ontologies. Backfills history; safe to re-run (idempotent).
    #[command(hide = true)]
    AnnotateEvents,

    /// Roll a day's 5-minute audio chunks up into coherent context sessions
    /// (changepoint on loudness + speaker count). Idempotent per day.
    #[command(hide = true)]
    SessionizeAudio {
        /// Date to sessionize (YYYY-MM-DD). Omit for all days with audio.
        #[arg(long)]
        date: Option<String>,
    },

    /// Generate the day's page and 24h event timeline for a date.
    ///
    /// Runs the full nightly chain locally, in production order: roll audio chunks
    /// into sessions → the DETECTIVE (`segment_day_events`, best model) fuses the
    /// dossier of clean rollups into a gapless timeline → scoring (sleep, annotate,
    /// novelty, autonomic, topic) → narration (`narrate_day`) writes the day's
    /// page from its record. Writes `wiki_events` (clearing existing auto events
    /// first; manual events are preserved) and the day's article page. Gaps are
    /// backfilled as "Unknown" to guarantee 00:00–24:00 coverage. Narration writes
    /// a day once and refuses a day that is not over; it says which.
    #[command(hide = true)]
    DaySummary {
        /// Date to summarize (YYYY-MM-DD). Defaults to today in the user's
        /// profile timezone (or local time if no timezone is set).
        #[arg(long)]
        date: Option<String>,
        /// Run ONLY narration (`narrate_day`) — skip sessionize / detective /
        /// scoring, so no NPU or embedder is needed. A day already written
        /// reports so and is not written again; "Rewrite this page", in the
        /// day page's ⋯ menu, is what writes it again.
        #[arg(long)]
        narrate_only: bool,
        /// Force a re-cut of the event timeline (the DETECTIVE) and print it, then
        /// stop — no scoring, no narrative, no embedder. Clears the day's sources
        /// fingerprint so segmentation actually re-runs even if sources are
        /// unchanged. For inspecting detective output / variance in isolation.
        /// With --from and --to, re-cuts a backlog instead and then scores it.
        #[arg(long, conflicts_with = "narrate_only")]
        segment_only: bool,
        /// With --segment-only: the first day of a backlog to re-cut
        /// (YYYY-MM-DD). Every day from --from to --to, oldest first, is re-cut
        /// (one Standard call for each day with enough to cut), then each re-cut
        /// day is annotated and scored again, which needs the embedder.
        #[arg(long, requires_all = ["segment_only", "to"], conflicts_with = "date")]
        from: Option<String>,
        /// With --segment-only: the last day of the backlog, included.
        #[arg(long, requires_all = ["segment_only", "from"], conflicts_with = "date")]
        to: Option<String>,
    },

    /// Run entity resolution (places + people) over the last N hours.
    ///
    /// This is the bridge while the legacy transform-chaining pipeline still
    /// owns clustering. The new actions path (ios_location, etc.) writes
    /// `data_location_point` rows but doesn't chain into place resolution,
    /// so visits don't get created. Use this to manually backfill.
    #[command(hide = true)]
    ResolveEntities {
        /// Lookback window in hours (default: 24)
        #[arg(long, default_value_t = 24)]
        hours: i64,
    },
}

/// `virtues volumes <action>` — the backup destinations, as a CLI.
#[derive(Subcommand)]
pub enum VolumesCmd {
    /// Show every registered destination, whether it is attached, and how old
    /// its newest good backup is.
    #[command(alias = "list")]
    Ls,

    /// Register a mounted filesystem as a backup destination.
    ///
    /// Identity is the filesystem UUID, read from the given path — not the path
    /// itself, which moves between boots. Nothing outside the box's own
    /// subdirectory on that volume is ever read, written, or removed, so the
    /// drive stays usable for whatever else lives on it and never needs
    /// formatting.
    Add {
        /// Any path on the mounted volume, e.g. `/media/backup`.
        path: std::path::PathBuf,

        /// Human label shown in `volumes ls`. Defaults to the mount point.
        #[arg(long)]
        name: Option<String>,
    },
}
