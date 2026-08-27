//! Virtues CLI - Command-line interface for the Virtues personal data platform

use std::env;
use virtues::cli::types::{Cli, Commands};
use virtues::VirtuesBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Trivial subcommands (--version, --help) skip the heavy startup path:
    // no tracing init, no observability, no .env loading, no rustls provider.
    // Without this, `virtues --version` prints two lines of OTel/observability
    // noise before the version itself — visibly broken when install.sh's
    // post-install health check captures the output. Detect on raw argv so
    // we don't pay clap's parse cost either.
    //
    // Anything matching is handled by clap below as normal; we just skip the
    // setup that wouldn't have produced useful output for a one-shot probe.
    let is_trivial = std::env::args()
        .nth(1)
        .map(|a| matches!(a.as_str(), "--version" | "-V" | "--help" | "-h"))
        .unwrap_or(false);

    if !is_trivial {
        // Install the ring CryptoProvider as the process-wide default.
        // We use rustls with `default-features = false, features = ["ring"]` to
        // avoid aws-lc-rs (and aws-lc-sys, which doesn't cross-compile under
        // GCC 11). Rustls 0.23 requires the provider to be installed once at
        // startup before any TLS work — the box serves no TLS itself, but
        // outbound HTTPS to atlas (via reqwest with rustls-tls-webpki-roots)
        // still needs the provider installed.
        rustls::crypto::ring::default_provider()
            .install_default()
            .expect("install rustls ring CryptoProvider");

        // Load env in this priority order:
        //   1. Existing process env (set by systemd's EnvironmentFile or the
        //      operator's `env VAR=… virtues …` invocation) — never override.
        //   2. /var/lib/virtues/virtues.env — the canonical production env
        //      file install.sh writes. Lets `sudo -u virtues virtues init`
        //      work without `env $(cat /var/lib/virtues/virtues.env | xargs)`.
        //   3. ./.env in CWD (Mac/dev convention).
        //   4. ../.env (for running from virtues-core/ during dev).
        let _ = dotenv::from_path("/var/lib/virtues/virtues.env");
        if dotenv::dotenv().is_err() {
            let _ = dotenv::from_path("../.env");
        }

        // On a box install, DB-touching CLI commands must run as the `virtues`
        // service user (Unix-socket peer auth maps OS user → Postgres role).
        // Running `virtues init` as adam/root used to burn a fake 30s
        // "Postgres did not accept connections" timeout. Self-correct instead.
        #[cfg(unix)]
        maybe_reexec_as_service_user();

        // Initialize tracing.
        //
        // Interactive subcommands share stdout with cliclack/dialoguer wizards
        // — INFO log lines collide with the TUI and break the carefully drawn
        // rail-connected prompts. So:
        //   • Default to `warn` for interactive subcommands; full `info` for
        //     `server` and the background/daemon commands.
        //   • Always write tracing output to stderr so cliclack owns stdout
        //     cleanly, even when RUST_LOG bumps the filter to debug.
        // RUST_LOG still overrides for debugging.
        let interactive = matches!(
            std::env::args().nth(1).as_deref(),
            Some("init")
                | Some("pair")
                | Some("link")
                | Some("login")
                | Some("subscribe")
                | Some("status")
                | Some("doctor")
                | Some("upgrade")
                | Some("backup")
                | Some("restore")
                | Some("reset")
                | Some("configure-inference")
                | Some("reindex")
                | Some("sudo")
                | Some("warm-models")
        );
        let default_filter = if interactive { "warn" } else { "info" };
        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_filter));

        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .init();

        // No metrics exporter. Virtues collects no central telemetry — all
        // observability is box-local (see api/system_telemetry.rs + the
        // app_ai_calls / app_system_samples tables). The old OpenTelemetry
        // OTLP exporter was removed: it was egress and dropped-on-restart.
    }

    // Inject the rich version (semver + codename + date + sha) into clap's
    // `--version`/`-V`. The derive uses CARGO_PKG_VERSION; override it at the
    // parse site since the codename is computed from the baked git sha.
    let cli = {
        use clap::{CommandFactory, FromArgMatches};
        // clap wants a &'static str; the version is fixed for the process, so
        // leak the one-time String.
        let version: &'static str =
            Box::leak(virtues::codename::long_version().into_boxed_str());
        let matches = Cli::command().version(version).get_matches();
        match Cli::from_arg_matches(&matches) {
            Ok(c) => c,
            Err(e) => e.exit(),
        }
    };

    // Handle Doctor early (no app stack — the report opens its own DB pool
    // best-effort, and an unreadable DB is itself a finding, not a crash).
    if matches!(cli.command, Some(Commands::Doctor)) {
        std::process::exit(virtues::cli::doctor::run().await);
    }

    // Handle WarmModels early (no database needed — just downloads ML models)
    if matches!(cli.command, Some(Commands::WarmModels)) {
        use virtues::cli::ui;
        // Show what will be exercised (accelerator, precision, on-disk state)
        // before pulling anything — the same ledger `virtues doctor` prints.
        // A missing GGUF is a hard stop here: warming would fail against it
        // anyway, so fail with the remedy instead of a sidecar error.
        let mut issues = ui::Issues::new();
        ui::section("Warm models");
        virtues::cli::doctor::print_inference(
            &virtues::inference_report::resolution_report(),
            &mut issues,
        );
        if issues.has_errors() {
            std::process::exit(issues.verdict());
        }
        println!();

        let embedder = virtues::search::get_embedder().await?;
        // Actually embed once, don't just connect: this runs the sidecar's
        // native-dim validation, so a wrong GGUF (e.g. a 1024-dim model vs
        // EmbeddingGemma's 768-dim native) fails HERE instead of "passing" a connect-only
        // check and silently corrupting the index.
        let probe = embedder.embed_query_async("virtues warm-up probe").await?;
        ui::ok(&format!(
            "embedder ready (stored dim={}, native validated)",
            probe.len()
        ));

        let _reranker = virtues::search::get_reranker().await?;
        ui::ok("reranker ready");
        println!();

        return Ok(());
    }

    // Handle Init command early (doesn't need Virtues client).
    //
    // `virtues init` is PLUMBING, not a wizard (docs/onboarding.md): resolve
    // config from the env the installer wrote, run migrations, mint a pair
    // token, print the handoff. The account/subscribe conversation lives in
    // the web setup wizard (/setup) — a TTY is the worst possible medium for
    // billing and OAuth, so the interactive middle this command used to have
    // is gone. The installer execs this at the end of `curl virtues.com/sh`;
    // re-running it by hand is always safe (everything here is idempotent).
    //
    // Power users keep `virtues subscribe` / `account-login` / `migrate` as
    // hidden standalone commands.
    if matches!(cli.command, Some(Commands::Init)) {
        // recommended_config() reads DATABASE_URL, VIRTUES_ENCRYPTION_KEY,
        // STATIC_DIR, STORAGE_PATH, etc. from the process env (systemd
        // EnvironmentFile + dotenv both populate them). Operators who need
        // to override can edit /var/lib/virtues/virtues.env first; there is
        // deliberately no second wizard here — the Recommended/Advanced
        // choice lives one level up, in the installer.
        let config = virtues::setup::recommended_config()?;

        let db = virtues::database::Database::new(&config.database_url)?;
        db.initialize().await?;
        // Match the installer's step iconography (it just printed its own
        // "✓" steps right above us via `ui::ok`), so the handoff reads as one
        // continuous checklist rather than switching to emoji mid-stream.
        println!();
        println!("  {}  {}", console::style("✓").green(), "Database ready");

        // Print the handoff pair code. On a fresh box (install time) this is the
        // standing setup code; if the box is already claimed it is a one-time
        // code — see `api::pair::cli_pair_code`.
        match virtues::api::pair::cli_pair_code(db.pool()).await {
            Ok(minted) => print_link_output(&minted),
            Err(e) => {
                println!();
                println!("  ⚠  could not produce a pair code: {e}");
                println!("     Run `virtues pair` later to get one.");
            }
        }

        return Ok(());
    }

    // ─── `virtues sudo` ─────────────────────────────────────────────────────
    // Confirmation gate for high-sensitivity actions. Lists open sudo
    // requests; on approval, the corresponding web action unlocks. The
    // person running this command IS the second factor (proves physical
    // access to the box).
    if let Some(Commands::Sudo { id, deny }) = &cli.command {
        let database_url = virtues::database::normalize_database_url()?;
        let db = virtues::database::Database::new(&database_url)?;
        let pool = db.pool();

        // If a specific id was provided, fast-path it.
        if let Some(id) = id {
            let res = if *deny {
                virtues::api::sudo::deny_from_cli(pool, id).await
            } else {
                virtues::api::sudo::approve_from_cli(pool, id).await
            };
            match res {
                Ok(()) => {
                    println!("✓ {}", if *deny { "denied" } else { "approved" });
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
        }

        // Interactive: list open requests, prompt for each.
        let pending = virtues::api::sudo::list_pending(pool).await?;
        if pending.is_empty() {
            println!();
            println!("  No pending sudo requests.");
            println!();
            return Ok(());
        }
        for req in pending {
            let remaining = (req.expires_at - chrono::Utc::now()).num_seconds().max(0);
            println!();
            println!("─────────────────────────────────────────────────────────");
            println!("  Pending action:  {}", req.action);
            println!("  Requested by:    {}", req.requesting_device_label);
            if let Some(ip) = req.requested_ip.as_deref() {
                println!("  From IP:         {ip}");
            }
            println!("  Requested at:    {}", req.created_at.to_rfc3339());
            println!("  Expires in:      {remaining}s");
            println!("─────────────────────────────────────────────────────────");

            let approve = dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                .with_prompt("Approve?")
                .default(false)
                .interact()
                .unwrap_or(false);

            if approve {
                if let Err(e) = virtues::api::sudo::approve_from_cli(pool, &req.id).await {
                    eprintln!("  ⚠  {e}");
                } else {
                    println!("  ✓ approved. Action will complete in the requesting browser.");
                }
            } else {
                if let Err(e) = virtues::api::sudo::deny_from_cli(pool, &req.id).await {
                    eprintln!("  ⚠  {e}");
                } else {
                    println!("  ✗ denied.");
                }
            }
        }
        return Ok(());
    }

    // ─── `virtues pair` (aliases: `login`, `link`) ──────────────────────────
    // THE human verb for connecting a device to the box (docs/onboarding.md). Mints a
    // CLI-origin pair token (authorized immediately because typing this
    // command IS proof of physical access), prints the one-time URL + QR,
    // then waits until the link is opened — the wait is also the
    // client-isolation detector (a printed link nobody opens is the only
    // box-side signal for a network that blocks device-to-device traffic).
    // The URL puts the token in a `#t=` fragment, so it never hits server
    // logs or referer headers.
    if let Some(Commands::Pair { no_wait }) = &cli.command {
        let database_url = virtues::database::normalize_database_url()?;
        let db = virtues::database::Database::new(&database_url)?;
        // On an UNCLAIMED box this shows the standing setup code (multi-use,
        // what the panel and BLE flow use). On a CLAIMED box it mints a FRESH
        // ONE-TIME code, consumed on use — the always-live standing code is
        // retired at claim. See `api::pair::cli_pair_code`.
        match virtues::api::pair::cli_pair_code(db.pool()).await {
            Ok(minted) => {
                print_link_output(&minted);
                if !*no_wait {
                    use virtues::cli::link::wait_for_new_device;
                    println!("  Waiting for a device to connect… (Ctrl+C to exit)");
                    match wait_for_new_device(db.pool()).await {
                        Ok(()) => {
                            println!();
                            println!("  ✓ connected — finish setup in the app.");
                        }
                        Err(e) => eprintln!("  (stopped waiting: {e})"),
                    }
                }
                return Ok(());
            }
            Err(e) => {
                eprintln!("error: could not produce a pair code: {e}");
                eprintln!("hint: is the database reachable? DATABASE_URL={}", database_url);
                std::process::exit(1);
            }
        }
    }

    // ─── `virtues device <ls|rm|add>` ───────────────────────────────────────
    // The allowlist as a CLI — "who can reach this box?" is one list. `ls`
    // shows non-revoked devices, `rm` de-allowlists one (next dial refused at
    // the handshake), `add` prints the standing pair code (same as `pair`).
    // Bare-pool: the on-box operator is the owner (physical access = you).
    if let Some(Commands::Device { action }) = &cli.command {
        use virtues::cli::types::DeviceCommands;
        let database_url = virtues::database::normalize_database_url()?;
        let db = virtues::database::Database::new(&database_url)?;
        let pool = db.pool();
        match action {
            DeviceCommands::Ls => {
                use virtues::cli::ui;
                let devices = virtues::api::devices::list_devices_cli(pool).await?;
                if devices.is_empty() {
                    println!();
                    ui::skip("no devices on the allowlist — run `virtues device add` to pair one");
                    println!();
                    return Ok(());
                }
                println!();
                println!(
                    "  {}",
                    console::style(format!(
                        "{:<30}  {:<12}  {:<22}  {:<14}  {:<14}  {}",
                        "ID", "KIND", "LABEL", "KEY", "VERSION", "LAST SEEN"
                    ))
                    .dim()
                );
                for (id, kind, label, node_id, last_seen, version) in &devices {
                    let key = node_id
                        .as_deref()
                        .map(|n| ui::ellipsize_middle(n, 14))
                        .unwrap_or_else(|| "—".to_string());
                    // Relative on a TTY (a ledger you scan); absolute RFC 3339
                    // when piped (a log you correlate).
                    let seen = match last_seen {
                        Some(t) if ui::tty() => ui::rel_time(*t),
                        Some(t) => t.to_rfc3339(),
                        None => "never".to_string(),
                    };
                    let label = if label.chars().count() > 22 {
                        format!("{}…", label.chars().take(21).collect::<String>())
                    } else {
                        label.clone()
                    };
                    let version = version.as_deref().unwrap_or("—");
                    println!("  {id:<30}  {kind:<12}  {label:<22}  {key:<14}  {version:<14}  {seen}");
                }
                println!();
                return Ok(());
            }
            DeviceCommands::Rm { id } => match virtues::api::devices::revoke_device_cli(pool, id).await {
                Ok(true) => {
                    virtues::cli::ui::ok(&format!(
                        "revoked {id} — its key is de-allowlisted; the next dial is refused"
                    ));
                    return Ok(());
                }
                Ok(false) => {
                    eprintln!("error: no active device with id {id} (already revoked or unknown)");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("error: revoke failed: {e}");
                    std::process::exit(1);
                }
            },
            DeviceCommands::Add => match virtues::api::pair::cli_pair_code(pool).await {
                Ok(minted) => {
                    print_link_output(&minted);
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("error: could not produce a pair code: {e}");
                    std::process::exit(1);
                }
            },
        }
    }

    // ─── `virtues report-crash` ─────────────────────────────────────────────
    // systemd post-stop hook. ALWAYS exits 0 — a failed diag post must
    // never cascade into a "post-stop hook failed" event. Handled here
    // (not in `cli::run`) so we don't depend on a healthy DB pool; the
    // crash may BE the DB pool going down.
    if matches!(cli.command, Some(Commands::ReportCrash)) {
        let _ = virtues::cli::report_crash::run().await;
        return Ok(());
    }

    // ─── `virtues backup` ───────────────────────────────────────────────────
    // Produces a single tarball of the full box state (DB + lake + env +
    // manifest). Detailed in `virtues::cli::backup`. Runs against a bare DB
    // pool — does not need the full app stack.
    if let Some(Commands::Backup {
        output,
        force,
        allow_missing_key,
        verify,
        key_file,
        volume,
        init_key,
    }) = &cli.command
    {
        if *init_key {
            match virtues::cli::backup::init_key() {
                Ok(()) => return Ok(()),
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
        }
        // Verification needs no database at all — it reads one file.
        if let Some(archive) = verify {
            match virtues::cli::restore::verify(archive.clone(), key_file.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    eprintln!("error: verification failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        let database_url = virtues::database::normalize_database_url()?;
        let db = virtues::database::Database::new(&database_url)?;
        let result = match volume {
            Some(target) => {
                virtues::cli::backup_volume::run_cli(db.pool().clone(), target, *allow_missing_key).await
            }
            None => virtues::cli::backup::run(
                db.pool(),
                output.clone(),
                *force,
                *allow_missing_key,
            )
            .await
            .map(|_| ()),
        };
        match result {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("error: backup failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // ─── `virtues restore` ──────────────────────────────────────────────────
    // Destructive. Refuses if the service is running (unless --force), if
    // the manifest's schema is newer than this binary's, or if any sha256
    // doesn't match. Detailed in `virtues::cli::restore`.
    if let Some(Commands::Restore {
        path,
        force,
        from_volume,
        key_file,
    }) = &cli.command
    {
        match virtues::cli::restore::run(
            path.clone(),
            *force,
            from_volume.clone(),
            key_file.clone(),
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("error: restore failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // ─── `virtues uninstall` ────────────────────────────────────────────────
    // Destructive, root-gated, typed-hostname confirm. Handled here (not in
    // `cli::run`) because it must not require a healthy DB pool — uninstall
    // is exactly what you reach for when the install is broken.
    if let Some(Commands::Uninstall { keep_data, purge_models, force }) = &cli.command {
        match virtues::cli::uninstall::run(*keep_data, *purge_models, *force).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("error: uninstall failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // ─── `virtues reset` (HIDDEN, testing) ──────────────────────────────────
    // Destructive: wipes the box (DB + lake) back to fresh state. Handled here
    // (not in `cli::run`) because it manages the schema itself and runs against
    // a bare pool, like restore/uninstall.
    if let Some(Commands::Reset { keep_data, yes, force }) = &cli.command {
        match virtues::cli::reset::run(*keep_data, *yes, *force).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("error: reset failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // ─── `virtues deprovision` ──────────────────────────────────────────────
    // Prepares the box to be imaged. Wraps `reset` (for the DB + lake) and then
    // strips the host-level identity, so like reset it runs against a bare pool
    // before any app stack exists.
    if let Some(Commands::Deprovision { yes, force }) = &cli.command {
        match virtues::cli::deprovision::run(*yes, *force).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("error: deprovision failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // ─── `virtues image-check` ──────────────────────────────────────────────
    // The gate between deprovision and `dd`. Read-only, and handled here with
    // the other bare-pool commands because a deprovisioned box has no app
    // stack left to build — which is the state it is meant to be run in.
    if matches!(cli.command, Some(Commands::ImageCheck)) {
        std::process::exit(virtues::cli::image_check::run().await);
    }

    // ─── `virtues configure-inference` ──────────────────────────────────────
    // Recover after a manual endpoint's model changed. Runs BEFORE the app
    // builds the guarded embedder — which would itself fail on the very
    // fingerprint mismatch this command exists to fix.
    if let Some(Commands::ConfigureInference { reembed, yes }) = &cli.command {
        match virtues::cli::configure_inference::run(*reembed, *yes).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("error: configure-inference failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // ─── `virtues reindex` ──────────────────────────────────────────────────
    // Rebuild the derived search index from source. Runs BEFORE normal init:
    // its ensure_embedding_dims refuses the width change (e.g. halfvec 256→384)
    // while the index is populated — reindex is the wedge-clearer.
    if let Some(Commands::Reindex { yes }) = &cli.command {
        match virtues::cli::reindex::run(*yes).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("error: reindex failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // ─── `virtues lake-adopt` ───────────────────────────────────────────────
    // Pull the recordings that predate the lake into it. Needs only a pool (and
    // STORAGE_PATH from the box env), so it runs here rather than paying for the
    // whole client stack.
    if let Some(Commands::LakeAdopt { dry_run }) = &cli.command {
        match virtues::cli::lake_adopt::run(*dry_run).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("error: lake-adopt failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // ─── `virtues upgrade` ──────────────────────────────────────────────────
    // Self-update from the latest GitHub Release (or a pinned --version
    // tag). Stops the service, swaps the binary, applies migrations,
    // restarts. Detailed in `virtues::cli::upgrade`.
    if let Some(Commands::Upgrade { check, version, pre, force, only }) = &cli.command {
        match virtues::cli::upgrade::run(*check, version.clone(), *pre, *force, only.clone()).await
        {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("error: upgrade failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // ─── `virtues channel` ──────────────────────────────────────────────────
    // Read or write the followed release channel. Sits with upgrade/rollback
    // above the DB setup because it must work on a box whose database is
    // unhealthy — the channel file is in the state root precisely so that
    // upgrading a broken box doesn't depend on the broken part.
    if let Some(Commands::Channel { channel }) = &cli.command {
        use virtues::cli::channel::{self, Channel};
        match channel {
            None => {
                println!("{}", channel::current());
                return Ok(());
            }
            Some(raw) => match Channel::parse(raw) {
                Some(c) => match channel::set(c) {
                    Ok(()) => {
                        virtues::cli::ui::ok(&format!("following the {c} channel"));
                        if c == Channel::Prerelease {
                            virtues::cli::ui::skip(
                                "`virtues upgrade` now takes prereleases without --pre",
                            );
                        } else {
                            // Going back to stable does not roll anything back;
                            // say so here rather than letting it look like the
                            // next upgrade silently did nothing.
                            virtues::cli::ui::skip(
                                "a box ahead of stable stays put until stable catches up",
                            );
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        eprintln!("error: could not set channel: {e}");
                        std::process::exit(1);
                    }
                },
                None => {
                    eprintln!("error: unknown channel {raw:?} — expected 'stable' or 'prerelease'");
                    std::process::exit(1);
                }
            },
        }
    }

    // ─── `virtues rollback` ─────────────────────────────────────────────────
    // Flip `current` back to the previous release slot and restart — the
    // atomic inverse of an upgrade's activation. Like Upgrade, needs no DB.
    if let Some(Commands::Rollback) = &cli.command {
        match virtues::cli::upgrade::rollback().await {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("error: rollback failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // ─── `virtues prepare` ──────────────────────────────────────────────────
    // Fetch + stage + preflight the next release WITHOUT installing it. Needs
    // no DB (and must run without one — a box whose database is unhealthy is a
    // box that wants the next release ready to go).
    if let Some(Commands::Prepare { force }) = &cli.command {
        match virtues::cli::upgrade::prepare(*force).await {
            Ok(virtues::cli::upgrade::Prepared::UpToDate) => {
                virtues::cli::ui::ok("already on the newest build for this channel");
                return Ok(());
            }
            Ok(virtues::cli::upgrade::Prepared::Already { slot_id }) => {
                virtues::cli::ui::ok(&format!(
                    "{slot_id} is already staged — `virtues activate` installs it"
                ));
                return Ok(());
            }
            Ok(virtues::cli::upgrade::Prepared::Staged { slot_id }) => {
                virtues::cli::ui::ok(&format!(
                    "{slot_id} staged — `virtues activate` installs it"
                ));
                return Ok(());
            }
            Err(e) => {
                eprintln!("error: prepare failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // ─── `virtues activate` ─────────────────────────────────────────────────
    // Install what `prepare` staged. Like Upgrade, deliberately does NOT touch
    // the DB here; the new binary's `migrate` does that after the flip.
    if let Some(Commands::Activate) = &cli.command {
        match virtues::cli::upgrade::activate_prepared().await {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("error: activate failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // DATABASE_URL (Postgres) must be set — no default. Fail loudly if missing.
    // It's already in the process env, so subprocess actions inherit it as-is.
    let database_url = virtues::database::normalize_database_url()?;

    // Initialize Virtues client. The lake location resolves inside the builder,
    // via `storage::lake::lake_root`. Reading STORAGE_PATH here as well would be
    // a second expression of the same rule — exactly the divergence that
    // resolver exists to prevent.
    let virtues = VirtuesBuilder::new().database(&database_url).build().await?;

    // Default to server with auto-migrate if no command specified
    let cli = if cli.command.is_none() {
        println!("🚀 Starting Virtues (auto-setup mode)...");
        println!();

        // Run migrations first
        println!("📊 Running migrations...");
        virtues.database.initialize().await?;
        println!("✅ Migrations complete");
        println!();

        // Seed production defaults (models, agents, etc.)
        println!("🌱 Seeding defaults...");
        virtues::seeding::prod_seed::seed_production_data(&virtues.database).await?;
        println!("✅ Seeding complete");
        println!();

        // In production (Nomad), NOMAD_PORT_http is set to the dynamically allocated port.
        // Fall back to 8000 for local development.
        let port = env::var("NOMAD_PORT_http")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(8000);

        Cli {
            command: Some(Commands::Server {
                // Dual-stack: `[::]` accepts both IPv4 and IPv6 (incl. the WG
                // tunnel's ULA fd00:5654::1 that pairing bundles advertise).
                // `0.0.0.0` would be IPv4-only and unreachable over the tunnel.
                host: "[::]".to_string(),
                port,
            }),
        }
    } else {
        cli
    };

    // Run CLI commands
    virtues::cli::run(cli, virtues).await?;

    Ok(())
}

// NOTE: the first-boot trust pitch (`print_account_intro`) that used to live
// here moved to the web setup wizard's account step — the user now makes the
// account decision in a browser, so that's where the pitch belongs. Its
// design rules travel with it (kept in the wizard's comments): first-person
// claims only, no named-competitor comparisons (Lanham/trade-libel exposure),
// the virtues-api sunset commitment as the closer, and every claim must stay
// true in lockstep with shipped features.

/// The Virtues wordmark — a serif figlet ("Georgia11") that opens the CLI
/// journey. Plain text on purpose: this output is frequently piped, captured,
/// and read over SSH, so no ANSI styling that would garble in a log.
fn print_banner() {
    const WORDMARK: &str = r#"
              ,,
`7MMF'   `7MF'db             mm
  `MA     ,V                 MM
   VM:   ,V `7MM  `7Mb,od8 mmMMmm `7MM  `7MM  .gP"Ya  ,pP"Ybd
    MM.  M'   MM    MM' "'   MM     MM    MM ,M'   Yb 8I   `"
    `MM A'    MM    MM       MM     MM    MM 8M"""""" `YMMMa.
     :MM;     MM    MM       MM     MM    MM YM.    , L.   I8
      VF    .JMML..JMML.     `Mbmo  `Mbod"YML.`Mbmmd' M9mmmP'
"#;
    println!("{WORDMARK}");
    println!("   This is technology that helps you be the person you ought to become.");
    println!();
}

/// Print the boxed "Your server is ready" call-to-action — the one thing the
/// user must act on, so it's the only block with a border. The pair code is the
/// single most important glyph on screen: bright + bold on a TTY, plain when
/// piped/captured. The box itself uses Unicode line-drawing (renders fine in
/// terminals and logs); only the colour is TTY-gated to avoid garbling logs.
fn print_pair_hero(display: &str) {
    use console::style;
    const W: usize = 54; // inner width between the borders (fits the downloads URL line)
    let tty = console::Term::stdout().is_term();

    let top = format!("  ┌{}┐", "─".repeat(W));
    let bot = format!("  └{}┘", "─".repeat(W));
    let blank = format!("  │{}│", " ".repeat(W));
    // A left-padded content line, right-padded to the inner width.
    let line = |content: &str| {
        let pad = W.saturating_sub(content.chars().count());
        format!("  │{}{}│", content, " ".repeat(pad))
    };

    println!("{top}");
    println!("{blank}");
    println!("{}", line("   Your server is ready."));
    println!("{blank}");
    println!("{}", line("   1.  Desktop app     https://virtues.com/downloads"));

    // The code line: pad on the *visible* length (ANSI is zero-width), then
    // wrap just the code in colour so the right border still aligns.
    let prefix = "   2.  Enter code      ";
    let pad = W.saturating_sub(prefix.chars().count() + display.chars().count());
    let code = if tty {
        style(display).cyan().bold().to_string()
    } else {
        display.to_string()
    };
    println!("  │{prefix}{code}{}│", " ".repeat(pad));

    println!("{blank}");
    println!("{}", line("   Rotates automatically · valid while shown"));
    println!("{blank}");
    println!("{bot}");
}

fn print_link_output(minted: &virtues::api::pair::MintedToken) {
    use virtues::cli::link::{reachable_box_origins, ssh_context, ssh_forward_host, ssh_handoff_block};
    let is_dev = std::env::var("ENVIRONMENT").map(|v| v == "dev").unwrap_or(false);
    let web_port = std::env::var("VIRTUES_WEB_PORT").unwrap_or_else(|_| "5173".to_string());
    let token = &minted.token;
    let display = minted.display_code();

    println!();

    if is_dev {
        println!("─────────────────────────────────────────────────────────");
        println!("  [dev] http://localhost:{web_port}/pair#t={token}");
        println!("─────────────────────────────────────────────────────────");
        return;
    }

    // Skip the wordmark when the installer chained straight into `init` — it
    // already printed the serif banner at the top of `curl … | sh`. Standalone
    // `virtues pair` / `virtues init` runs still get it.
    if std::env::var_os("VIRTUES_NO_BANNER").is_none() {
        print_banner();
    }
    print_pair_hero(&display);

    // On SSH: the desktop app needs a route to the box. On isolated networks
    // (office/hotel) mDNS is blocked, so the existing SSH session is the
    // reliable path — print the forward recipe ("auto-notice everything").
    if let Some(ssh) = ssh_context() {
        println!();
        let host = ssh_forward_host();
        for line in ssh_handoff_block(&ssh, &host) {
            println!("{line}");
        }
    }

    // WHERE TO GET THE APP — not a browser URL to pair in.
    //
    // This block used to read "No app yet? Open in a browser on your network:"
    // followed by the box's `/pair#t=…` URLs. Those URLs resolve to a page
    // whose entire job is to say you are on the wrong surface: an allowlisted
    // iroh key is the credential, a browser tab holds none, and
    // `/api/pair/consume` rejects `kind: "browser"` outright. So the one line
    // offered to someone who does NOT have the app sent them to a dead end,
    // and it was the last thing the DIY installer printed.
    //
    // The honest fallback for "no app yet" is where to get one.
    println!();
    println!("  Don't have the app yet?  https://virtues.com/downloads");
    println!("    Enter the code above in it. A browser cannot pair — pairing is");
    println!("    a held key, and a browser tab has none.");

    // The box's addresses, for the app's "enter its address" field when mDNS
    // does not carry — an isolated office LAN, or a box on another subnet.
    let urls = reachable_box_origins(is_dev, &web_port);
    if !urls.is_empty() {
        println!();
        println!("  If the app can't find this box, give it an address:");
        for url in &urls {
            println!("    {}", url.url);
        }
        if let Some(v6) = virtues::net_check::compute_net_status().ipv6_global {
            println!("    http://[{v6}]:8000");
        }
    }

    println!("─────────────────────────────────────────────────────────");
}

/// Re-exec `sudo -u virtues <argv>` when a DB-touching CLI command runs as the
/// wrong OS user on a box install.
///
/// Box installs talk to Postgres over the Unix socket with peer auth, so the
/// OS user IS the database identity: only the `virtues` service user (and the
/// systemd unit, which runs as it) can connect. A human SSH'd in as adam (or
/// root) typing `virtues init` would get an instant-but-permanent auth error.
/// Rather than telling them to retype the command, become the right user.
///
/// Guards (all must hold, so dev machines are never touched):
///   - argv[1] is one of the DB-touching interactive commands
///   - `/var/lib/virtues/virtues.env` exists (the box-install marker)
///   - the current user isn't already `virtues`
///
/// On exec failure (no sudo rights, etc.) we print the one-line manual hint
/// and exit — never fall through to the misleading Postgres timeout.
#[cfg(unix)]
fn maybe_reexec_as_service_user() {
    // Every command that opens a Postgres pool or reads the `virtues`-owned env
    // file. Miss one and it runs as the login user, whose role does not exist in
    // the cluster: `virtues reindex` died with `role "root" does not exist`
    // after an hour-long restore (2026-08-27), which is a confusing way to say
    // "wrong user".
    //
    // THIS LIST DRIFTED FROM THE ONE AT THE TOP OF `main()`. That one decides
    // the log level for "interactive" commands and already knew about
    // `reindex`, `configure-inference`, `restore` and `warm-models`; this one
    // decides who they run AS, and had never been updated. Two hand-synced
    // lists, one true — the same shape as the ACL lattice (`plugins/lockstep`)
    // and the applet env allowlist. When you add a subcommand that touches the
    // database, add it to BOTH.
    //
    // Deliberately absent: the root-only lifecycle verbs — `upgrade`,
    // `prepare`, `activate`, `rollback`, `deprovision`, `uninstall`,
    // `image-check`, `bringup` — which drive systemd and must NOT drop
    // privilege.
    const DB_COMMANDS: &[&str] = &[
        "init", "pair", "link", "login", "subscribe", "sudo", "backup", "reset", "status",
        "migrate", "seed",
        // `doctor` reads the env file (inference mode) and the DB (reach legs),
        // both `virtues`-owned. Run as another user it can read neither, so it
        // would render the default llama-server guess + "DB unknown" — a
        // confident, wrong report. Re-exec so it reports this box's real config.
        "doctor",
        // Each of these opens its own pool before the app path (main.rs) or
        // rides the shared one in `cli::run`.
        "device",
        "reindex",
        "configure-inference",
        "lake-adopt",
        "volumes",
    ];
    let Some(cmd) = std::env::args().nth(1) else { return };
    if !DB_COMMANDS.contains(&cmd.as_str()) {
        return;
    }
    if !std::path::Path::new("/var/lib/virtues/virtues.env").exists() {
        return;
    }
    let user = std::process::Command::new("id")
        .arg("-un")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if user.is_empty() || user == "virtues" {
        return;
    }
    eprintln!("(running as '{user}' — switching to the 'virtues' service user)");
    use std::os::unix::process::CommandExt;
    let mut reexec = std::process::Command::new("sudo");
    reexec.arg("-u").arg("virtues");
    // `SSH_CONNECTION` is set in this (login-user) process but stripped by
    // sudo's env_reset. It carries the box-side IP the client reached us on —
    // the only provably-reachable address for the SSH-forward handoff (the LAN
    // IP is unreachable on client-isolated wifi). Thread it across via an `env`
    // prefix (run as the virtues user, after the privilege drop), which
    // sidesteps any sudoers env policy. Absent over a console login → omitted,
    // and the handoff falls back to the overlay/LAN address.
    if let Some(ip) = std::env::var("SSH_CONNECTION")
        .ok()
        .and_then(|c| c.split_whitespace().nth(2).map(str::to_string))
        .filter(|s| !s.is_empty())
    {
        reexec.arg("env").arg(format!("VIRTUES_SSH_SERVER_IP={ip}"));
    }
    let err = reexec.args(std::env::args()).exec();
    eprintln!("could not switch user: {err}");
    eprintln!("hint: run it as the service user yourself: sudo -u virtues virtues {cmd}");
    std::process::exit(1);
}

// NOTE: the doctor report (inference + reach ledgers + verdict) lives in
// `virtues::cli::doctor`; `virtues warm-models` shares its Inference ledger.
