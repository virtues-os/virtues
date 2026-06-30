//! # virtues-client
//!
//! A small CLI for pairing this machine to a Virtues box and opening it in the
//! browser.
//!
//! In the relay model the box is reachable from any browser at its own HTTPS URL
//! — `https://<boxhash>.boxes.virtues.com` via the blind relay, or the LAN
//! dashed-IP name on-network — with a browser-trusted cert the box holds itself.
//! So there is no tunnel to bring up and no localhost proxy: this client just
//! pairs (exchanges a one-time token for a bearer + the box URL) and opens that
//! URL. The old WireGuard tunnel + reverse-proxy daemon is gone.
//!
//! ## Subcommands
//!
//! - `pair <pair-url>` — consume a one-time pair URL from the box; store the
//!   bearer + box URL in the OS keychain.
//! - `pair-code <code>` — same, via the short code the box prints (box found over
//!   mDNS unless `--server` is given).
//! - `discover` — list Virtues boxes found on the LAN via mDNS (`--json`).
//! - `open` — open the paired box in the default browser.
//! - `status` — report the paired box URL + reachability.
//! - `revoke` (alias `reset`) — clear local creds + drop this credential on the box.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod discover;
mod keychain;
mod pair;

#[derive(Parser)]
#[command(name = "virtues-client")]
#[command(version, about = "Pair this machine to a Virtues box and open it in the browser", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Consume a one-time pair URL from the box and store the bearer + box URL.
    ///
    /// The pair URL is what `virtues pair` prints on the box. Copy it from the
    /// box's terminal (or scan its QR), paste it here. The token expires in 15
    /// minutes and is single-use.
    Pair {
        /// The full pair URL, e.g. `http://10.0.0.5:8000/pair#t=<token>`.
        pair_url: String,
    },

    /// Pair using the short code printed by `virtues pair` / `virtues init`.
    /// Discovers the box via mDNS if `--server` is not given.
    PairCode {
        /// The code shown by the box (spaces optional, e.g. "ABC DEF").
        code: String,
        /// Box origin to pair with, e.g. `http://adam.local:8000`.
        /// If omitted, discovered automatically via mDNS.
        #[arg(long)]
        server: Option<String>,
    },

    /// Discover Virtues boxes on the local network via mDNS.
    Discover {
        /// Emit a JSON array of found boxes instead of human-readable output.
        #[arg(long)]
        json: bool,
    },

    /// Open the paired box in the default browser.
    Open,

    /// Report the paired box URL and whether it's reachable.
    Status,

    /// Clear local creds + remove this device's credential from the box.
    ///
    /// Aliased as `reset` — the one-step "start clean" command.
    #[command(alias = "reset")]
    Revoke,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();
    match cli.command {
        Command::Pair { pair_url } => pair::run(&pair_url).await,
        Command::PairCode { code, server } => run_pair_code(code, server).await,
        Command::Discover { json } => run_discover(json).await,
        Command::Open => run_open(),
        Command::Status => run_status().await,
        Command::Revoke => revoke().await,
    }
}

async fn run_pair_code(code: String, server: Option<String>) -> Result<()> {
    let origin = match server {
        Some(s) => s,
        None => {
            eprintln!("searching for Virtues boxes on the local network…");
            let servers = discover::discover_servers(5).await;
            if servers.is_empty() {
                anyhow::bail!(
                    "no Virtues boxes found via mDNS. Pass --server <origin> explicitly, \
                     e.g. `--server http://adam.local:8000`."
                );
            }
            discover::pick_server(&servers)?.origin.clone()
        }
    };
    pair::run_with_code(&origin, &code).await
}

async fn run_discover(json: bool) -> Result<()> {
    let servers = discover::discover_servers(3).await;
    if json {
        println!("{}", serde_json::to_string(&servers)?);
    } else {
        discover::print_servers(&servers);
    }
    Ok(())
}

fn run_open() -> Result<()> {
    let rec = keychain::load_box()?.ok_or_else(|| {
        anyhow::anyhow!("no paired box — run `virtues-client pair <pair-url>` first")
    })?;
    open_in_browser(&rec.box_url)?;
    println!("opening {} …", rec.box_url);
    Ok(())
}

/// Open a URL in the platform's default browser. No external crate — one shell
/// command per OS keeps the dep list minimal.
fn open_in_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };

    let status = cmd.status().context("launch browser")?;
    if !status.success() {
        anyhow::bail!("browser launcher exited with {status}");
    }
    Ok(())
}

async fn run_status() -> Result<()> {
    let rec = match keychain::load_box()? {
        Some(r) => r,
        None => {
            println!("paired:      no");
            println!();
            println!("Run `virtues-client pair <pair-url>` to pair with your box.");
            println!("The pair URL is printed by `virtues pair` on the box itself.");
            return Ok(());
        }
    };

    println!("paired:      yes");
    println!("box url:     {}", rec.box_url);
    println!("bearer:      present");

    // Lightweight reachability probe — hit the box's health endpoint.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()?;
    let health = format!("{}/api/health", rec.box_url.trim_end_matches('/'));
    match client.get(&health).send().await {
        Ok(r) if r.status().is_success() => println!("reachable:   yes ({})", r.status()),
        Ok(r) => println!("reachable:   responded {} (box up, endpoint differs)", r.status()),
        Err(e) => println!("reachable:   no ({e})"),
    }
    Ok(())
}

/// Default tracing setup — stderr only, `RUST_LOG` controls level, defaults to
/// `warn` so casual users don't see internals.
fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .init();
}

async fn revoke() -> Result<()> {
    let rec = match keychain::load_box()? {
        Some(r) => r,
        None => {
            println!("not paired — nothing to revoke.");
            return Ok(());
        }
    };

    // Best-effort: tell the box to drop this device's credential row. The DELETE
    // endpoint matches on the *credential* id, not the device id.
    if let Some(credential_id) = &rec.credential_id {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let url = format!(
            "{}/api/credentials/{credential_id}",
            rec.box_url.trim_end_matches('/')
        );
        match client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", rec.bearer))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => eprintln!("✓ removed device from the box"),
            Ok(r) => eprintln!(
                "warning: box returned {} — clearing local creds anyway",
                r.status()
            ),
            Err(e) => eprintln!(
                "warning: could not reach the box ({e}) — clearing local creds anyway"
            ),
        }
    } else {
        eprintln!(
            "note: no stored credential id — clearing local creds only. Remove this \
             device from the box's Devices page to fully de-authorize it."
        );
    }

    keychain::delete_box()?;
    println!("local creds cleared.");
    Ok(())
}
