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
mod mode;
mod preflight;
mod qairt;
mod steps;
mod storage;
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

    /// Provision this machine as a Virtues appliance rather than someone's
    /// general-purpose Linux server.
    ///
    /// The DIY installer is a guest on a machine the owner uses for other
    /// things, so it changes as little as possible. An appliance is the
    /// opposite: the box exists only to be Virtues, its only interface is the
    /// attached display, and anything else competing for the screen or the
    /// boot is a defect. So this turns off the desktop session, stops the boot
    /// blocking on a network the box does not have yet, and installs the
    /// display kiosk.
    ///
    /// Implied on our own hardware; the flag exists for building an appliance
    /// image on a board we haven't taught the detector about yet.
    #[arg(long)]
    appliance: bool,
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
        appliance: cli.appliance,
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
