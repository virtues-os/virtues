//! Virtues installer — single-binary first-boot bootstrap.
//!
//! Architecture:
//!
//!   curl virtues.com/sh | sh        ← Caddy 302 → bootstrap.sh
//!         ↓
//!   bootstrap.sh                     ← tiny bash: detects arch, fetches us
//!         ↓
//!   virtues-installer                ← this binary: real TUI install flow
//!         ↓
//!   virtues init                     ← chain-exec at the end
//!
//! Why a binary instead of bash:
//!
//!   - Real TUI (cliclack rail-connected prompts) instead of a stream of
//!     disjoint dialog boxes.
//!   - Real progress bars for slow steps (GGUF model downloads, binary
//!     download) that stream live throughput instead of "spinning ⠋".
//!   - Themed copy in brand voice without escape-character gymnastics.
//!   - Real types + error handling instead of `set -euo pipefail` prayer.
//!   - Reusable: the same binary backs `virtues upgrade` later.
//!
//! Everything that mutates the host (apt install, systemctl, createdb)
//! is a shell-out to the underlying CLI. We're an orchestration layer,
//! not a re-implementation of apt.

mod brand;
mod config;
mod download;
mod flow;
mod install;
mod preflight;
mod steps;
mod ui;

use anyhow::Result;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(long, value_name = "vX.Y.Z")]
    version: Option<String>,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    no_init: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install the ring CryptoProvider as the process-wide default. Without
    // this, `reqwest::Client::new()` panics on first use because rustls
    // 0.23 (transitively via reqwest's rustls-tls-no-provider feature)
    // requires the provider to be installed before any TLS work. Mirrors
    // the fix in virtues-core and atlas main.rs.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install rustls ring CryptoProvider");

    let cli = Cli::parse();

    if !nix_check_root() {
        ui::die("virtues-installer must run as root. Re-run with sudo.");
    }

    brand::print_header();

    let outcome = flow::run(flow::Config {
        version: cli.version,
        dry_run: cli.dry_run,
        no_init: cli.no_init,
    })
    .await;

    match outcome {
        Ok(_) => Ok(()),
        Err(e) => ui::die(&format!("{e:#}")),
    }
}

fn nix_check_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}
