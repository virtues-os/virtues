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
use cliclack::{intro, outro, select};
use std::os::unix::process::CommandExt;
use std::process::Command as StdCommand;

use crate::brand;
use crate::config::InstallConfig;
use crate::download;
use crate::install;
use crate::preflight;
use crate::steps;
use crate::ui;

pub struct Config {
    pub version: Option<String>,
    pub dry_run: bool,
    pub no_init: bool,
    pub assume_yes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Recommended,
    Advanced,
}

pub async fn run(cli: Config) -> Result<()> {
    // ─── Pre-flight ─────────────────────────────────────────────────────
    ui::section("Pre-flight");
    let target = steps::detect()?;
    ui::ok(&format!("Linux {} · {} {}", target.arch, target.distro, target.distro_version));
    let pf = preflight::run().await?;
    if pf.warnings > 0 {
        ui::warn(&format!("{} pre-flight issue(s) — continuing in 5s (Ctrl+C to abort)…", pf.warnings));
        if !cli.dry_run && !cli.assume_yes {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }

    // ─── Mode selector ──────────────────────────────────────────────────
    let mode = if cli.assume_yes || !brand::is_tty() {
        Mode::Recommended
    } else {
        intro("∴ Setup")?;
        select("How do you want to install?")
            .item(Mode::Recommended, "Recommended", "what most people want")
            .item(Mode::Advanced, "Advanced", "override defaults (coming v0.1.2)")
            .interact()?
    };

    let mut cfg = InstallConfig::recommended_defaults();
    cfg.pinned_version = cli.version.clone();
    // Advanced mode prompts for INSTALL_PREFIX, DATA_DIR, embed model, etc.
    // Land that in a follow-up; for v0.1.1 Recommended path is the locked one.
    let _ = mode;

    if cli.dry_run {
        ui::skip("dry-run — system would be modified by the following steps");
        ui::skip("  • System dependencies (Postgres 18, WireGuard, Avahi, Ollama)");
        ui::skip(&format!("  • Pull embedding model ({})", cfg.embed_model));
        ui::skip("  • Configure mDNS (hostname → virtues, _https._tcp on :443)");
        ui::skip("  • Create system user 'virtues' + data dir");
        ui::skip("  • Postgres role + database + pgvector extension");
        ui::skip(&format!("  • Download virtues binary → {}", cfg.binary_path().display()));
        ui::skip(&format!("  • Write env file at {}", cfg.env_file_path().display()));
        ui::skip("  • Run virtues bringup (migrations + box identity)");
        ui::skip("  • Install systemd unit");
        return Ok(());
    }

    // ─── Installing system deps ─────────────────────────────────────────
    ui::section("Installing system dependencies");
    install::install_deps(&target).await?;

    ui::section("Installing Ollama + embedding model");
    install::ensure_ollama(&cfg).await?;

    // ─── Configuring the box ────────────────────────────────────────────
    ui::section("Configuring");
    ui::thinking("Forging your box's identity…");
    install::configure_mdns().await?;
    install::create_user(&cfg).await?;
    install::provision_db().await?;

    // ─── Downloading the binary ─────────────────────────────────────────
    ui::section("Downloading virtues");
    download::download_binary(&mut cfg, target.arch).await?;

    // ─── Env + bringup + systemd ────────────────────────────────────────
    ui::section("Sealing your sovereignty");
    install::write_env_file(&cfg).await?;
    install::run_bringup(&cfg).await?;
    install::install_systemd_unit(&cfg).await?;

    // Start the service so init's pair-token mint sees a running daemon.
    let mut start = tokio::process::Command::new("systemctl");
    start.args(["enable", "--now", "virtues"]);
    steps::run_step("Enable + start virtues service", start).await?;

    // ─── Verifying ──────────────────────────────────────────────────────
    ui::section("Health check");
    let issues = install::health_check(&cfg).await?;
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
    let err = StdCommand::new("sudo")
        .args(["-u", "virtues", "virtues", "init"])
        .exec();
    // If we reach this line, exec failed.
    Err(anyhow::anyhow!("failed to exec `virtues init`: {err}"))
}
