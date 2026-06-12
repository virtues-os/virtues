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
use crate::preflight;
use crate::steps;
use crate::ui;

pub struct Config {
    pub version: Option<String>,
    pub dry_run: bool,
    pub no_init: bool,
    pub assume_yes: bool,
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

    let mut cfg = InstallConfig::recommended_defaults();
    cfg.pinned_version = cli.version.clone();

    if cli.dry_run {
        ui::skip("dry-run — system would be modified by the following steps");
        ui::skip("  • System packages (Postgres 18, WireGuard, Avahi)");
        ui::skip(&format!(
            "  • Inference sidecars (llama-server): {} + {}",
            cfg.embed_gguf, cfg.rerank_gguf
        ));
        ui::skip("  • mDNS (hostname → virtues, _http._tcp on :8000)");
        ui::skip("  • System user 'virtues' + data dir + Postgres role/db/pgvector");
        ui::skip(&format!("  • Virtues binary → {}", cfg.binary_path().display()));
        ui::skip(&format!("  • Env file at {}", cfg.env_file_path().display()));
        ui::skip("  • virtues bringup + systemd unit");
        return Ok(());
    }

    // ─── System packages ────────────────────────────────────────────────
    ui::section("System packages");
    install::install_deps(&target).await?;
    install::configure_mdns().await?;
    install::create_user(&cfg).await?;
    install::provision_db().await?;

    // ─── Virtues ────────────────────────────────────────────────────────
    ui::section("Virtues");
    download::download_binary(&mut cfg, target.arch).await?;
    // After the tarball: the sidecars need the llama-server binary it ships.
    install::install_inference(&cfg).await?;
    install::write_env_file(&cfg).await?;
    install::run_bringup(&cfg).await?;
    install::install_systemd_unit(&cfg).await?;

    // Start the service so init's pair-token mint sees a running daemon.
    let mut start = tokio::process::Command::new("systemctl");
    start.args(["enable", "--now", "virtues"]);
    steps::run_step("Enable + start virtues service", start).await?;

    // Start virtues-wireguard after virtues is up so the reconciler reads a
    // populated DB (server keypair + any pre-existing peer rows). No-op if
    // the WG binary wasn't in the tarball.
    install::enable_wireguard_unit(&cfg).await?;

    // ─── Verifying ──────────────────────────────────────────────────────
    ui::section("Verifying");
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
