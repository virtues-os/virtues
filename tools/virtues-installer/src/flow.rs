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
    let target = steps::detect()?;
    ui::ok(&format!("Linux {} · {} {}", target.arch, target.distro, target.distro_version));
    preflight::run().await?;

    let mut cfg = InstallConfig::recommended_defaults();
    cfg.pinned_version = cli.version.clone();

    if cli.dry_run {
        ui::skip("dry-run — system would be modified by the following steps");
        ui::skip("  • Inference mode resolution (Dragon board auto-detect, else manual endpoint prompts + validation)");
        ui::skip("  • System locale → C.UTF-8 (when not already UTF-8)");
        ui::skip("  • System packages (Postgres 18, Avahi)");
        ui::skip(&format!(
            "  • Inference sidecars (Dragon mode only, llama-server): {} + {}",
            cfg.embed_gguf, cfg.rerank_gguf
        ));
        ui::skip("  • mDNS (hostname → virtues, _http._tcp on :8000)");
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
    let inference = InferenceMode::resolve()?;
    let validation = match &inference {
        InferenceMode::Dragon => None,
        InferenceMode::Manual { embed_url, embed_model, rerank_url, hf_repo } => Some(
            mode::validate_manual(
                embed_url,
                embed_model,
                rerank_url.as_deref(),
                hf_repo.as_deref(),
            )
            .await?,
        ),
    };

    // ─── System packages ────────────────────────────────────────────────
    ui::section("System packages");
    install::ensure_utf8_locale().await?;
    install::install_deps(&target).await?;
    install::configure_mdns().await?;
    install::create_user(&cfg).await?;
    install::provision_db().await?;

    // ─── Virtues ────────────────────────────────────────────────────────
    ui::section("Virtues");
    download::download_binary(&mut cfg, target.arch).await?;
    // Dragon: after the tarball, provision the sidecars (they need the
    // llama-server binary it ships). Manual: the user's endpoints were
    // already validated above — no llama-server, no GGUF fetch, no units.
    match &inference {
        InferenceMode::Dragon => install::install_inference(&cfg).await?,
        InferenceMode::Manual { .. } => {
            ui::skip("Manual inference — skipping local sidecar provisioning")
        }
    }
    install::write_env_file(&cfg, &inference, validation.as_ref()).await?;
    install::run_bringup(&cfg).await?;
    install::install_systemd_unit(&cfg).await?;

    // Start the service so init's pair-token mint sees a running daemon.
    let mut start = tokio::process::Command::new("systemctl");
    start.args(["enable", "--now", "virtues"]);
    steps::run_step("Enable + start virtues service", start).await?;

    // ─── Verifying ──────────────────────────────────────────────────────
    ui::section("Verifying");
    let issues = install::health_check(&cfg, matches!(inference, InferenceMode::Dragon)).await?;
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
    let err = StdCommand::new("sudo")
        .args(["-u", "virtues", "env", "VIRTUES_NO_BANNER=1", "virtues", "init"])
        .exec();
    // If we reach this line, exec failed.
    Err(anyhow::anyhow!("failed to exec `virtues init`: {err}"))
}
