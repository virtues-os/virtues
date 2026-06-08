//! Virtues CLI - Command-line interface for the Virtues personal data platform

use clap::Parser;
use std::env;
use virtues::cli::types::{Cli, Commands};
use virtues::search::Embedder;
use virtues::VirtuesBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Install the ring CryptoProvider as the process-wide default.
    // We use rustls with `default-features = false, features = ["ring"]` to
    // avoid aws-lc-rs (and aws-lc-sys, which doesn't cross-compile under
    // GCC 11). Rustls 0.23 requires the provider to be installed once at
    // startup before any TLS work; otherwise the axum-server TLS task
    // panics on first connection.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install rustls ring CryptoProvider");

    // Load environment variables from .env file
    // Try current directory first, then parent directory (for running from core/)
    if dotenv::dotenv().is_err() {
        let _ = dotenv::from_path("../.env");
    }

    // Initialize tracing
    // Use RUST_LOG env var, falling back to INFO if not set
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    // Initialize observability (metrics)
    // If OTEL_EXPORTER_OTLP_ENDPOINT is set, metrics will be exported
    if let Err(e) =
        virtues::observability::init(virtues::observability::ObservabilityConfig::default())
    {
        tracing::warn!(error = %e, "Failed to initialize observability, continuing without metrics");
    }

    let cli = Cli::parse();

    // Handle Doctor early (no database — pure hardware/model resolution report)
    if matches!(cli.command, Some(Commands::Doctor)) {
        print_resolution_report();
        return Ok(());
    }

    // Handle WarmModels early (no database needed — just downloads ML models)
    if matches!(cli.command, Some(Commands::WarmModels)) {
        // Show what will be fetched (accelerator, precision, baked vs download)
        // before pulling anything — same report `virtues doctor` prints.
        print_resolution_report();
        println!();

        let embedder = virtues::search::get_embedder().await?;
        println!("✅ Embedder ready (dim={})", embedder.dimension());

        let _reranker = virtues::search::get_reranker().await?;
        println!("✅ Reranker ready");

        return Ok(());
    }

    // Handle Init command early (doesn't need Virtues client).
    //
    // `virtues init` is the all-in-one first-run wizard:
    //   1. Local config: DB URL, server URL, storage path, encryption key
    //   2. Save to .env
    //   3. Migrations (required before subscribe — box_secrets table)
    //   4. Subscribe ($20/mo via QR + URL on the same screen)
    //   5. Done — print next-step hints
    //
    // Power users still have `virtues subscribe` / `virtues migrate` separately
    // when they want granular control.
    if matches!(cli.command, Some(Commands::Init)) {
        let config = virtues::setup::run_init().await?;

        // Save configuration
        virtues::setup::save_config(&config)?;

        // Migrations are functionally required before the subscribe step
        // (box vault stores billing_token in `box_secrets`). If the user
        // declined run_migrations we still run them here unless they're
        // explicitly skipping subscribe too.
        println!();
        println!("📊 Running migrations...");
        let db = virtues::database::Database::new(&config.database_url)?;
        db.initialize().await?;
        println!("✅ Migrations complete");

        // Subscribe step — prompt before launching the browser/QR flow so
        // CI/dev users can decline. They can run `virtues subscribe` later.
        let do_subscribe = dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
            .with_prompt("Subscribe to Virtues now? ($20/mo)")
            .default(true)
            .interact()
            .unwrap_or(true);

        if do_subscribe {
            // Build a minimal Virtues client just for the subscribe step —
            // the wizard already ran migrations so the schema is ready.
            use virtues::VirtuesBuilder;
            let virtues = VirtuesBuilder::new()
                .database(&config.database_url)
                .build()
                .await?;
            // Soft-fail — let the user retry with `virtues subscribe` if
            // the link expires or atlas is unreachable.
            if let Err(e) = virtues::cli::commands::deploy::handle_subscribe(&virtues).await {
                println!();
                println!("  ⚠  subscribe step did not finish: {e}");
                println!("     Run `virtues subscribe` later when you're ready.");
            }
        } else {
            println!();
            println!("  Skipped subscribe. Run `virtues subscribe` when you're ready.");
        }

        // Mint a CLI-origin pair token and print the URL.  The fragment-token
        // form (`/pair#t=…`) never leaks the token to server logs or referers.
        {
            let db = virtues::database::Database::new(&config.database_url)?;
            match virtues::api::pair::mint_pair_token(db.pool(), None, Some("browser")).await {
                Ok(minted) => print_link_output(&minted.token),
                Err(e) => {
                    println!();
                    println!("  ⚠  could not mint pair token: {e}");
                    println!("     Run `virtues link` later to get a fresh URL.");
                }
            }
        }

        virtues::setup::display_completion();
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

    // ─── `virtues link` ─────────────────────────────────────────────────────
    // Mints a CLI-origin pair token (authorized immediately because typing
    // this command IS proof of physical access) and prints a one-time URL.
    // The URL puts the token in a `#t=` fragment, so it never hits server
    // logs or referer headers. Open it in any browser to land in a paired
    // session.
    if matches!(cli.command, Some(Commands::Link)) {
        let database_url = virtues::database::normalize_database_url()?;
        let db = virtues::database::Database::new(&database_url)?;
        match virtues::api::pair::mint_pair_token(db.pool(), None, Some("browser")).await {
            Ok(minted) => {
                print_link_output(&minted.token);
                return Ok(());
            }
            Err(e) => {
                eprintln!("error: could not mint pair token: {e}");
                eprintln!("hint: is the database reachable? DATABASE_URL={}", database_url);
                std::process::exit(1);
            }
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
    if let Some(Commands::Backup { output, force }) = &cli.command {
        let database_url = virtues::database::normalize_database_url()?;
        let db = virtues::database::Database::new(&database_url)?;
        match virtues::cli::backup::run(db.pool(), output.clone(), *force).await {
            Ok(_) => return Ok(()),
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
    if let Some(Commands::Restore { path, force }) = &cli.command {
        match virtues::cli::restore::run(path.clone(), *force).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("error: restore failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // ─── `virtues upgrade` ──────────────────────────────────────────────────
    // Self-update from the latest GitHub Release (or a pinned --version
    // tag). Stops the service, swaps the binary, applies migrations,
    // restarts. Detailed in `virtues::cli::upgrade`.
    if let Some(Commands::Upgrade { check, version }) = &cli.command {
        match virtues::cli::upgrade::run(*check, version.clone()).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("error: upgrade failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // DATABASE_URL (Postgres) must be set — no default. Fail loudly if missing.
    // It's already in the process env, so subprocess actions inherit it as-is.
    let database_url = virtues::database::normalize_database_url()?;

    // Initialize Virtues client
    // Storage path: STORAGE_PATH env var or ./data/lake default
    let mut builder = VirtuesBuilder::new().database(&database_url);

    // Configure storage path if specified
    if let Ok(storage_path) = env::var("STORAGE_PATH") {
        builder = builder.storage_path(&storage_path);
    }

    let virtues = builder.build().await?;

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
                host: "0.0.0.0".to_string(),
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

/// Print the inference stack's hardware-resolution plan: which accelerator is
/// active, whether this build links CUDA, the chosen ONNX precision, and where
/// each model's files come from (baked vs download). Pure — no DB, no network,
/// Print the `virtues link` + `virtues init` output: the pair URL(s), then
/// the per-OS CA-trust recipes. Honors `ENVIRONMENT=dev` (plain HTTP, no CA
/// step) and falls back from `virtues.local` to the box's primary IP for
/// clients on which mDNS isn't resolving. Shared between `Link` and `Init`
/// so the user sees identical output regardless of which command they ran.
fn print_link_output(token: &str) {
    use virtues::cli::link::{ca_recipe_host, ca_recipes, reachable_pair_urls};
    let is_dev = std::env::var("ENVIRONMENT").map(|v| v == "dev").unwrap_or(false);
    let web_port = std::env::var("VIRTUES_WEB_PORT").unwrap_or_else(|_| "5173".to_string());
    let urls = reachable_pair_urls(token, is_dev, &web_port);

    println!();
    println!("─────────────────────────────────────────────────────────");
    println!("  Open this in your browser to log in:");
    println!();
    for url in &urls {
        println!("    {:<18}  {}", format!("{}:", url.label), url.url);
    }
    println!();
    if is_dev {
        println!("  Notes:");
        println!("    • Dev mode (ENVIRONMENT=dev): plain HTTP, no CA trust needed.");
        println!("    • Link expires in 15 minutes. Single-use.");
    } else {
        println!("  First visit only: trust the box's CA root (one-time).");
        println!("  Run the line for your client OS:");
        println!();
        for recipe in ca_recipes(&ca_recipe_host()) {
            println!("    {}:", recipe.os);
            println!("      {}", recipe.command);
            println!();
        }
        println!("  Notes:");
        println!("    • If `virtues.local` doesn't resolve on your laptop, use the IP URL above.");
        println!("    • Linux clients can also install `libnss-mdns` (Debian/Ubuntu) or");
        println!("      `nss-mdns` (Fedora) to make `.local` work natively.");
        println!("    • Link expires in 15 minutes. Single-use.");
    }
    println!("─────────────────────────────────────────────────────────");
}

/// no session construction.
fn print_resolution_report() {
    use virtues::search::model_cache::{resolution_report, ModelSource};

    let r = resolution_report();
    println!("Virtues inference resolution");
    println!("  accelerator:   {}", r.accelerator);
    println!("  precision:     {}", r.precision);
    println!(
        "  cuda in build: {}",
        if r.cuda_compiled { "yes" } else { "no (CPU-only image)" }
    );
    match &r.models_dir {
        Some(d) => println!("  models dir:    {} (baked)", d.display()),
        None => println!("  models dir:    none — models download from HuggingFace on first use"),
    }
    if r.accelerator == "cuda" && !r.cuda_compiled {
        // reconcile() already downgraded to CPU + warned; never reached, but
        // kept as a guard if policy changes.
        println!("  note:          GPU detected but no CUDA EP linked — running on CPU");
    }
    println!("  models:");
    for m in &r.models {
        let source = match &m.source {
            ModelSource::Baked(p) => format!("baked @ {}", p.display()),
            ModelSource::Download => "download".to_string(),
        };
        println!("    - {:<9} {} :: {} [{}]", m.name, m.repo, m.onnx_file, source);
    }
}
