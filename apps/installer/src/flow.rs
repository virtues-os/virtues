//! Top-level install orchestration.
//!
//! Phase order (this is a literal pipeline; later phases depend on earlier
//! phases having succeeded):
//!
//!     pre-flight  → installing  → configuring  → verifying  → handoff
//!
//! Each phase prints a header (`∴ <Phase>`), runs its steps via `steps::*`,
//! and either succeeds (continue) or returns an error (which we surface at
//! the top level with full context).
//!
//! Phase-3-scope: this lays the visible flow structure. Step bodies are
//! the same shell-outs the bash install.sh ran, ported to typed
//! `tokio::process::Command` invocations through `steps::run_step` /
//! `steps::run_streaming`. Real implementations land in follow-up commits
//! as we port each section.

use anyhow::{anyhow, Result};
use cliclack::{intro, log, outro, select, spinner};

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

pub async fn run(cfg: Config) -> Result<()> {
    // ─── Pre-flight ─────────────────────────────────────────────────────
    ui::section("Pre-flight");
    let target = steps::detect()?;
    ui::ok(&format!("Linux {} on {}", target.arch, target.distro));
    ui::ok(&format!("{} {}", target.distro, target.distro_version));

    // ─── Mode selector (interactive only) ───────────────────────────────
    let mode = if cfg.assume_yes {
        Mode::Recommended
    } else {
        intro("∴ Setup")?;
        let choice = select("How do you want to install?")
            .item(Mode::Recommended, "Recommended", "what most people want")
            .item(Mode::Advanced, "Advanced", "override defaults")
            .interact()?;
        choice
    };

    // ─── Installing ─────────────────────────────────────────────────────
    ui::section("Installing");

    if cfg.dry_run {
        ui::skip("dry-run — system unchanged");
    } else {
        // TODO(phase-3-follow-up): port each install_deps step from
        // scripts/install.sh into `steps::*`. For now we run a single
        // ceremonial spinner so the visual flow is end-to-end testable.
        let sp = spinner();
        sp.start("System dependencies (placeholder — port from install.sh)");
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        sp.stop("System dependencies queued");
    }

    // ─── Configuring ────────────────────────────────────────────────────
    ui::section("Configuring");
    ui::thinking("Forging your box's identity…");
    if cfg.dry_run {
        ui::skip("dry-run — config unchanged");
    } else {
        // TODO(phase-3-follow-up): write env file, generate encryption
        // key (only if no existing file), set hostname, drop mDNS service.
        let sp = spinner();
        sp.start("Box identity (placeholder)");
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        sp.stop("Box identity queued");
    }

    // ─── Verifying ──────────────────────────────────────────────────────
    ui::section("Verifying");
    ui::skip(
        "Health checks (placeholder — port from install.sh post_install_health)",
    );

    // ─── Handoff ────────────────────────────────────────────────────────
    if cfg.no_init {
        ui::section("Done");
        ui::ok("Install complete. Run `virtues init` when you're ready.");
        return Ok(());
    }

    outro(format!(
        "{}  Install complete. Continuing to setup…",
        crate::brand::mark()
    ))?;

    // Hand off to `virtues init`. Phase 3 will plumb this through a real
    // exec via std::os::unix::process::CommandExt so the wizard inherits
    // the terminal cleanly. For now we just print the command so an
    // operator can run it manually.
    log::info("Run: sudo -u virtues virtues init")?;

    let _ = cfg.version;
    Ok(())
}
