//! Top-level install orchestration.
//!
//! Pipeline:
//!
//!     pre-flight  →  installing  →  configuring  →  verifying  →  handoff
//!
//! Each phase prints `∴ <Phase>` and runs its steps. Failures bubble up
//! with full anyhow context so the user sees what actually went wrong.
//! Handoff `exec`s into `virtues init` so the same terminal/session
//! becomes the wizard — no copy-paste tax for the user.

use anyhow::Result;
use cliclack::outro;
use std::os::unix::process::CommandExt;
use std::process::Command as StdCommand;

use crate::brand;
use crate::config::InstallConfig;
use crate::download;
use crate::install;
use crate::mode::{self, InferenceMode};
use crate::preflight;
use crate::steps;
use crate::storage;
use crate::ui;

pub struct Config {
    pub version: Option<String>,
    pub dry_run: bool,
    pub no_init: bool,
}

pub async fn run(cli: Config) -> Result<()> {
    // ─── Intro ──────────────────────────────────────────────────────────
    // One sentence so the user knows where this ends: the installer sets up
    // the box, then the desktop app completes setup via a short code.
    println!("  Installing Virtues on this machine. At the end you'll connect");
    println!("  the desktop app (virtues.com/downloads) to finish setup.");
    println!();

    // ─── Pre-flight ─────────────────────────────────────────────────────
    ui::section("Pre-flight");
    let mut target = steps::detect()?;
    ui::ok(&format!("Linux {} · {} {}", target.arch, target.distro, target.distro_version));
    preflight::run(&mut target).await?;

    let mut cfg = InstallConfig::recommended_defaults();
    cfg.pinned_version = cli.version.clone();

    // Storage quality is decided by the DATA_DIR medium, which cfg has just
    // resolved (default /var/lib/virtues, honoring the DATA_DIR env override).
    // Runs inside pre-flight, after preflight::run, because a slow/lying/NFS
    // disk is exactly the kind of thing the user should learn about before we
    // start provisioning Postgres onto it. Non-blocking, like the rest of
    // pre-flight — it warns with numbers, it never aborts.
    storage::report(&cfg.data_dir).await?;

    if cli.dry_run {
        ui::skip("dry-run — system would be modified by the following steps");
        ui::skip("  • Inference: this machine's CPU (recommended, and a Dragon's without asking) or an embedding server you run");
        ui::skip("  • System locale → C.UTF-8 (when not already UTF-8)");
        ui::skip("  • System packages (Postgres 18, Avahi)");
        ui::skip("  • Automatic security updates (apt systems)");
        ui::skip(&format!(
            "  • Inference sidecar (CPU only, llama-server): {}",
            cfg.embed_gguf
        ));
        ui::skip("  • mDNS (hostname → virtues, _http._tcp on :8000)");
        ui::skip("  • Firewall, when firewalld or ufw is on: 8000/tcp, mDNS, 51820/udp");
        ui::skip("  • System user 'virtues' + data dir + Postgres role/db/pgvector");
        ui::skip(&format!("  • Virtues binary → {}", cfg.binary_path().display()));
        ui::skip(&format!("  • Env file at {}", cfg.env_file_path().display()));
        ui::skip("  • virtues bringup + systemd unit");
        return Ok(());
    }

    // ─── Inference mode ─────────────────────────────────────────────────
    // Resolved (and, for manual, validated) BEFORE anything mutates the
    // system: a user whose endpoint is broken should learn that before we
    // start installing packages, not after.
    ui::section("Inference");
    let inference = InferenceMode::resolve(&cfg.env_file_path())?;
    let validation = match &inference {
        // Bundled provisions our own local sidecar — nothing to validate (we
        // control the endpoint).
        InferenceMode::Bundled => None,
        InferenceMode::Manual { embed_url, embed_model } => Some(
            mode::validate_manual(embed_url, embed_model).await?,
        ),
    };

    // ─── System packages ────────────────────────────────────────────────
    ui::section("System packages");
    install::ensure_utf8_locale().await?;
    install::install_deps(&target).await?;
    install::enable_security_updates(&target).await?;
    install::configure_mdns().await?;
    let firewall = install::open_firewall().await?;

    install::create_user(&cfg).await?;
    install::provision_db().await?;

    // ─── Virtues ────────────────────────────────────────────────────────
    ui::section("Virtues");
    download::download_binary(&mut cfg, target.arch).await?;
    // After the tarball, provision local inference. Bundled → the portable CPU
    // llama-server sidecar (paced on a Dragon). Manual → nothing (the user's
    // endpoints were validated above).
    match &inference {
        InferenceMode::Bundled if install::npu_still_serving() => {
            ui::skip("This Dragon's search moves from its NPU to the CPU at the end of the install")
        }
        InferenceMode::Bundled => install::install_inference(&cfg).await?,
        InferenceMode::Manual { .. } => {
            ui::skip("Manual inference — skipping local sidecar provisioning")
        }
    }
    // libpdfium — document text extraction. Mode-independent (CPU parse):
    // every path gets it, Dragon and DIY alike.
    install::install_pdfium(&cfg).await?;

    install::write_install_manifest(&cfg, &inference, &firewall)?;
    install::write_env_file(&cfg, &inference, validation.as_ref()).await?;
    install::run_bringup(&cfg).await?;
    install::install_systemd_unit(&cfg).await?;

    // Start the service so init's pair-token mint sees a running daemon.
    //
    // `enable --now` is a NO-OP on a unit that is already active, which is the
    // normal case for a reinstall/upgrade — so the box kept running the OLD
    // binary against a schema `run_bringup` had just migrated forward. Observed
    // on a real upgrade: the previous process stayed alive with its exe showing
    // `/usr/local/bin/virtues (deleted)`, and every applet query failed with
    // `column t.supervise does not exist` (dropped by the migration the new
    // binary shipped) plus `cached plan must not change result type` from the
    // stale connection pool. The install reported success throughout.
    //
    // `enable` (no --now) then `restart` is unconditional: restart starts a
    // stopped unit and replaces a running one, so both fresh installs and
    // upgrades end up on the binary that was just installed.
    let mut enable = tokio::process::Command::new("systemctl");
    enable.args(["enable", "virtues"]);
    steps::run_step("Enable virtues service", enable).await?;

    let mut restart = tokio::process::Command::new("systemctl");
    restart.args(["restart", "virtues"]);
    steps::run_step("Start virtues service on the new binary", restart).await?;

    if matches!(inference, InferenceMode::Bundled) && install::npu_still_serving() {
        install::move_off_npu(&cfg).await;
    }

    // ─── Verifying ──────────────────────────────────────────────────────
    ui::section("Verifying");
    let issues = install::health_check(&cfg, &inference).await?;
    if issues > 0 {
        ui::warn(&format!("{issues} post-install issue(s) — run `virtues doctor` for details"));
    }

    // ─── Handoff ────────────────────────────────────────────────────────
    if cli.no_init {
        ui::section("Done");
        ui::ok("Install complete. Run `virtues init` when you're ready.");
        return Ok(());
    }

    outro(format!("{} Install complete. Continuing to setup…", brand::mark()))?;
    println!();

    // `exec` replaces this process with `sudo -u virtues virtues init`.
    // dialoguer/cliclack read /dev/tty internally, so even when our parent
    // was `curl | sh` (stdin = pipe), the wizard still reads input.
    // The installer already printed the serif wordmark at the top, so tell
    // `init` to skip its banner and avoid showing it a second time. sudo resets
    // the environment (env_reset), so pass the var through an `env` prefix that
    // runs after the privilege drop rather than via Command::env (stripped).
    // The binary goes by its full path: sudo searches only its `secure_path`,
    // which leaves out /usr/local/bin on RHEL, Rocky and Alma.
    let bin = cfg.binary_path();
    let err = StdCommand::new("sudo")
        .args(["-u", "virtues", "env", "VIRTUES_NO_BANNER=1"])
        .arg(&bin)
        .arg("init")
        .exec();
    // If we reach this line, exec failed.
    Err(anyhow::anyhow!("failed to exec `virtues init`: {err}"))
}
