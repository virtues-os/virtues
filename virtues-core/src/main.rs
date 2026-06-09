//! Virtues CLI - Command-line interface for the Virtues personal data platform

use clap::Parser;
use std::env;
use virtues::cli::types::{Cli, Commands};
use virtues::search::Embedder;
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
        use dialoguer::{theme::ColorfulTheme, Select};

        // First-boot mode selector. 99.99% of users want Recommended
        // (zero config questions — install.sh already wrote the env file
        // with sane defaults). Advanced is the override-everything wizard
        // for the rare operator running a custom deployment.
        let mode_idx = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("How do you want to set up?")
            .items(&[
                "Recommended — takes care of everything (what you want)",
                "Advanced — override defaults (DB URL, storage, encryption key, …)",
            ])
            .default(0)
            .interact()
            .unwrap_or(0);

        let config = if mode_idx == 0 {
            // Recommended: zero questions. Pull DATABASE_URL from the
            // environment (install.sh wrote /var/lib/virtues/virtues.env;
            // the systemd unit + this binary both load it). Fall back to
            // the production peer-auth URL if env is unset.
            virtues::setup::recommended_config()?
        } else {
            // Advanced: full interactive wizard.
            let cfg = virtues::setup::run_init().await?;
            virtues::setup::save_config(&cfg)?;
            cfg
        };

        // Migrations are functionally required before the subscribe step
        // (box vault stores billing_token in `box_secrets`). Idempotent —
        // safe to re-run on every `virtues init` invocation.
        println!();
        println!("📊 Running migrations...");
        let db = virtues::database::Database::new(&config.database_url)?;
        db.initialize().await?;
        println!("✅ Migrations complete");

        // Privacy framing before the account prompt. The user is *deciding*
        // whether to attach a Virtues account — they need the trust pitch
        // now, not buried in a Settings page later.
        print_account_intro();

        // Account selector. Replaces the old binary Confirm.
        //
        // [1] Log in: lands when atlas /init/login (Stripe Customer Portal
        //     magic link → mint billing_token for verified customer) ships.
        //     Until then, the option exists in the UI for honesty + sets
        //     expectations.
        // [2] Create new: existing device-authorization → Stripe Checkout
        //     flow that's been working since v0.1.0.
        let account_idx = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Account")
            .items(&[
                "Log in to existing Virtues account",
                "Create a new Virtues account ($20/mo, $15 wallet)",
            ])
            .default(1)
            .interact()
            .unwrap_or(1);

        // Two completely different "log ins" exist here:
        //   - account login   = attaching the box to your Virtues account
        //                       via atlas (Stripe Customer Portal magic
        //                       link). Lands in v0.1.1.
        //   - box login       = your laptop browser pairing with the box
        //                       via the URL printed at the end of this
        //                       function. Works today.
        //
        // The original code printed a "coming soon" message for account
        // login and then fell straight through to mint the pair URL,
        // making the two look conflated. We now explicitly call out the
        // difference so the user understands what just happened.
        if account_idx == 0 {
            println!();
            println!(
                "  {} The account-login flow lands in v0.1.1 (atlas /init/login",
                console::style("ⓘ").bold().yellow()
            );
            println!("    + Stripe Customer Portal magic link).");
            println!();
            println!("  Your box is fully set up — pair URL below logs you into the");
            println!("  box itself. Chat + cloud sources stay disabled until the");
            println!("  account link works in the next release. Re-run `virtues init`");
            println!("  then to attach your account.");
            println!();
            println!("  In the meantime you can:");
            println!("    • Browse the web UI, set per-source preferences, explore");
            println!("    • Set a BYO AI key under Settings → AI Provider Key for chat");
            println!("    • Use `virtues status` / `virtues doctor` to inspect the box");
        } else {
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

/// Print the first-boot trust pitch shown right before the account prompt
/// in `virtues init`.
///
/// Design notes (deliberate, not stylistic):
///
/// - **No named competitor comparisons.** Earlier drafts had a table
///   ("Google reads your content; we don't") — visually punchy but legally
///   exposed (Lanham false-advertising; trade libel; named-trademark use).
///   The cost-to-defend a single bad-faith C&D outweighs the rhetorical
///   win. Replaced with pure self-statements + one positive Plex analogy.
///
/// - **First-person, not comparative.** "What stays on your box" / "What
///   we see" lets the reader supply their own contrast from their own
///   life — usually more damning than ours.
///
/// - **Sunset commitment elevated to the closer.** The strongest trust
///   signal isn't a state (today's privacy) but a direction (where we're
///   going). It lands last so it's the thought the user holds when they
///   make the [1] / [2] choice.
///
/// - **Every claim has to remain true.** If we add a feature that breaks
///   one ("anything semantic about who you are stays on your box" implies
///   no telemetry of behavior), this copy needs updating in lockstep.
fn print_account_intro() {
    use console::style;
    let line = style("─────────────────────────────────────────────────────────").dim();
    let sep  = style("═════════════════════════════════════════════════════════").dim();

    println!();
    println!("{line}");
    println!();
    println!("  {}", style("Your data lives on this Linux device. Never our cloud.").bold());
    println!();
    println!("  {}", style("What stays on your box:").bold());
    println!("    • Every message, photo, file, calendar event, note");
    println!("    • Every prompt you type and every response");
    println!("    • Your encryption keys");
    println!("    • Anything semantic about who you are");
    println!();
    println!("  {}", style("What we see (the strict minimum):").bold());
    println!("    • A Stripe customer ID  ({})",
        style("Stripe holds your card and email").dim());
    println!("    • Token counts on AI calls  ({})",
        style("for billing").dim());
    println!("    • OAuth callbacks for ~200ms  ({})",
        style("so Google / Notion / Plaid will talk to your box at all").dim());
    println!();
    println!("  We never see content, conversations, who you talk to, or what");
    println!("  you ask. No metadata, no contact graph, no semantic shape of");
    println!("  your life.");
    println!();
    println!("{sep}");
    println!();
    println!("  {}",
        style("Two things still require a Virtues account").bold());
    println!("  — the smallest remaining surface, shrinking every release:");
    println!();
    println!("    1. {}.  Google, Notion, Plaid etc. require a registered",
        style("OAuth callbacks").bold());
    println!("       HTTPS URL. virtues.com hosts it, forwards to your box");
    println!("       in ~200ms, discards the auth code.");
    println!();
    println!("    2. {}.  Your provider key (yours or Virtues wallet) stays",
        style("AI proxy").bold());
    println!("       server-side as a short-lived bearer instead of living on");
    println!("       every device. Calls are passthrough — providers log");
    println!("       nothing of our traffic; we see only token counts.");
    println!();
    println!("{sep}");
    println!();
    println!("  {}",
        style("Our north star: make virtues-api extinct.").bold().green());
    println!();
    println!("  Every line of code there is something we'd rather you ran at");
    println!("  home. The roadmap is structured around shrinking this layer:");
    println!();
    println!("    • {}.  Edge models close the gap with cloud LLMs every",
        style("Local inference").bold());
    println!("      quarter. We route embeddings on-device today; chat-tier");
    println!("      moves home as Ollama-class models reach parity.");
    println!();
    println!("    • {}.  IETF is standardizing flows that don't need",
        style("Device-bound OAuth").bold());
    println!("      a hosted callback. When providers adopt them, the");
    println!("      callback-hosting role disappears.");
    println!();
    println!("    • {}.  Replaces atlas's role in box",
        style("Self-coordinated rendezvous").bold());
    println!("      discovery — signaling-only, then nothing.");
    println!();
    println!("  Every release that removes us from your data path is one we");
    println!("  ship harder than features.");
    println!();
    println!("{line}");
    println!();
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
