//! Virtues installer — single-binary first-boot bootstrap.
//!
//! Architecture:
//!
//!   curl get.virtues.com | sh        ← CloudFront 302 → bootstrap.sh
//!         ↓
//!   bootstrap.sh                     ← tiny bash: detects arch, fetches us
//!         ↓
//!   virtues-installer                ← this binary: real TUI install flow
//!         ↓
//!   virtues init                     ← chain-exec at the end
//!
//! Why a binary instead of bash polish on `install.sh`:
//!
//!   - Real TUI (cliclack rail-connected prompts) instead of a stream of
//!     disjoint dialog boxes.
//!   - Real progress bars for slow steps (Ollama model pull, binary
//!     download) that stream live throughput instead of "spinning ⠋".
//!   - Themed copy in brand voice without escape-character gymnastics.
//!   - Real types + error handling instead of `set -euo pipefail` prayer.
//!   - Reusable: the same binary backs `virtues upgrade` later.
//!
//! Everything that mutates the host (apt install, systemctl, ollama pull,
//! createdb) is a shell-out to the underlying CLI. We're an orchestration
//! layer, not a re-implementation of apt.

mod brand;
mod flow;
mod steps;
mod ui;

use anyhow::Result;
use clap::Parser;

/// Virtues installer.
///
/// Usage: this is normally invoked by the bootstrap.sh that
/// `curl get.virtues.com | sh` downloads. You usually shouldn't run it
/// by hand unless you're debugging the install path.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Pin a specific virtues release tag (default: latest).
    #[arg(long, value_name = "vX.Y.Z")]
    version: Option<String>,

    /// Print every step without modifying the system.
    #[arg(long)]
    dry_run: bool,

    /// Don't chain-exec `virtues init` at the end.
    #[arg(long)]
    no_init: bool,

    /// Assume yes everywhere; bypass interactive prompts. Used by
    /// scripted/headless deploys. Forces auto-init in non-interactive
    /// mode with recommended defaults.
    #[arg(short = 'y', long)]
    yes: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if !nix_check_root() {
        ui::die("virtues-installer must run as root. Re-run with sudo.");
    }

    brand::print_header();

    let outcome = flow::run(flow::Config {
        version: cli.version,
        dry_run: cli.dry_run,
        no_init: cli.no_init,
        assume_yes: cli.yes,
    })
    .await;

    match outcome {
        Ok(_) => Ok(()),
        Err(e) => {
            ui::die(&format!("{e:#}"));
        }
    }
}

/// EUID == 0 check without pulling the full nix crate.
fn nix_check_root() -> bool {
    // SAFETY: geteuid is a trivial syscall that always succeeds.
    unsafe { libc::geteuid() == 0 }
}
