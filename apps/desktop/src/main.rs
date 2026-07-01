//! # virtues-client
//!
//! A small CLI for pairing this machine to a Virtues box and opening it in the
//! browser.
//!
//! In the iroh model the box is an iroh `Endpoint` reached by its Ed25519
//! EndpointId — LAN-direct, hole-punched, or via our relay. At pairing this
//! client generates its own device iroh key, submits its EndpointId (so the box
//! allowlists it), and stores the box's reach ticket (`{box_node_id, relay_url}`)
//! plus a bearer. `up` then runs a local `:7117` proxy that dials the box over
//! iroh and serves it to the browser on loopback (same-origin cookies intact).
//!
//! ## Subcommands
//!
//! - `pair <pair-url>` — consume a one-time pair URL; generate a device key, send
//!   its EndpointId, store the reach ticket + bearer in the OS keychain.
//! - `pair-code <code>` — same, via the short code the box prints (box found over
//!   mDNS unless `--server` is given).
//! - `discover` — list Virtues boxes found on the LAN via mDNS (`--json`).
//! - `up` — serve the paired box at `http://localhost:7117` over iroh.
//! - `open` — open the box in the default browser (via the `:7117` helper).
//! - `status` — report the paired box + whether the `:7117` helper reaches it.
//! - `revoke` (alias `reset`) — clear local creds + drop this credential on the box.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod discover;
mod keychain;
mod pair;
mod proxy;

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

    /// Serve the paired box at http://localhost:7117 over iroh (the local helper
    /// the browser and Tauri app talk to). Runs until stopped.
    Up,

    /// Open the paired box in the default browser (via the `:7117` helper).
    /// Run `virtues-client up` first so the helper is serving.
    Open,

    /// Report the paired box + whether the `:7117` helper can reach it.
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
        Command::Up => proxy::run().await,
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

/// The local helper (`virtues-client up`) serves the box here over iroh. Since
/// the box no longer has a public URL, every "reach the box" command goes through
/// this loopback address rather than dialing the box directly.
const HELPER_URL: &str = "http://localhost:7117";

fn run_open() -> Result<()> {
    keychain::load_box()?.ok_or_else(|| {
        anyhow::anyhow!("no paired box — run `virtues-client pair <pair-url>` first")
    })?;
    open_in_browser(HELPER_URL)?;
    println!("opening {HELPER_URL} …");
    println!("(if the page doesn't load, run `virtues-client up` first)");
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
    match (&rec.box_node_id, &rec.relay_url) {
        (Some(n), Some(r)) => println!("iroh reach:  {n} via {r}"),
        _ => println!("reach:       LAN only ({})", rec.box_url),
    }
    println!("bearer:      present");

    // Reachability probe — hit the box's health endpoint *through the `:7117`
    // helper* (the box has no public URL; the helper dials it over iroh).
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()?;
    let health = format!("{HELPER_URL}/api/health");
    match client.get(&health).send().await {
        Ok(r) if r.status().is_success() => println!("reachable:   yes, via helper ({})", r.status()),
        Ok(r) => println!("reachable:   helper responded {} (box up, endpoint differs)", r.status()),
        Err(_) => println!("reachable:   helper not running — run `virtues-client up`, then retry"),
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

    // Best-effort: tell the box to drop this device's credential row, through the
    // `:7117` helper (the box has no public URL). The DELETE endpoint matches on
    // the *credential* id, not the device id. If the helper isn't running we still
    // clear local creds — the owner can finish removal from the Devices page.
    if let Some(credential_id) = &rec.credential_id {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let url = format!("{HELPER_URL}/api/credentials/{credential_id}");
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
            Err(_) => eprintln!(
                "warning: `:7117` helper not running — clearing local creds anyway. \
                 Remove this device from the box's Devices page to fully de-authorize it."
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
