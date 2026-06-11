//! # virtues-client
//!
//! The desktop daemon. Pairs to a Virtues box over WireGuard and runs a local
//! HTTP reverse proxy on `127.0.0.1:8000` so any browser on this machine sees
//! the box at a Secure-Context-eligible loopback URL — no per-box CA install,
//! no cert warnings, no trust dance.
//!
//! See [[localhost-daemon-trust]] and [[v02-plan]] in MEMORY.md for the
//! architectural commitment this implements.
//!
//! ## Subcommands (this commit)
//!
//! - `pair <pair-url>` — consume a one-time pair token from the box; receive a
//!   [`virtues_protocol::PairingBundle`] (WG keys, box endpoint, rendezvous
//!   params, session bearer); store it in the OS keychain.
//!
//! ## Subcommands (stubs — fill in next commits)
//!
//! - `up` — bring the WG tunnel up (via GotaTun) and start the localhost proxy.
//! - `status` — report tunnel state, last handshake, proxy port.
//! - `revoke` — clear local creds + tell the box to drop this device's peer.
//!
//! ## Why this binary exists
//!
//! Until this ships, only the Jetson's own Chromium can use Virtues. The
//! desktop daemon is the v0.2 unblock-everyone milestone — see
//! [[v02-plan]] Track A in MEMORY.md.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod keychain;
mod pair;
mod proxy;
mod punch;
mod tunnel;
mod wg_keys;

#[derive(Parser)]
#[command(name = "virtues-client")]
#[command(version, about = "Desktop daemon for connecting to a Virtues box", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Consume a one-time pair URL from the box and store the resulting
    /// PairingBundle in this machine's OS keychain.
    ///
    /// The pair URL is what `virtues link` prints on the box. Copy it from the
    /// box's terminal output (or scan its QR if your box has a screen) and
    /// paste it here. The token expires in 15 minutes and is single-use.
    Pair {
        /// The full pair URL, e.g.
        /// `http://10.0.0.5:8000/pair#t=<token>&ep=...&fpr=...`.
        pair_url: String,
    },

    /// Bring the WG tunnel up and start the localhost HTTP proxy. Runs in the
    /// foreground until interrupted.
    Up,

    /// Report tunnel state, last handshake age, and proxy port.
    Status,

    /// Clear local creds + remove this device's WG peer from the box.
    Revoke,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();
    match cli.command {
        Command::Pair { pair_url } => pair::run(&pair_url).await,
        Command::Up => run_up().await,
        Command::Status => print_status(),
        Command::Revoke => revoke().await,
    }
}

/// `virtues-client up` — bring the tunnel up + start the local HTTP proxy.
///
/// Returns only on fatal error (listener refused, etc.) or on Ctrl-C.
async fn run_up() -> Result<()> {
    let bundle = keychain::load_bundle()
        .context("read paired bundle from OS keychain")?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no paired box — run `virtues-client pair <pair-url>` first"
            )
        })?;

    // Bring the WG tunnel up (Linux only today; macOS / Windows are
    // platform-specific impls in `tunnel/` and follow). Errors here are
    // honest fatals — without a tunnel the proxy can't reach the box.
    // The handle MUST stay alive for the proxy's lifetime; dropping it
    // tears the WG state machine down.
    let _tunnel = tunnel::start(&bundle).await.context("bring tunnel up")?;

    // Start the proxy and run until Ctrl-C / signal.
    let cfg = proxy::ProxyConfig::from_bundle(&bundle)?;
    let result = tokio::select! {
        result = proxy::run(cfg) => result,
        _ = tokio::signal::ctrl_c() => {
            eprintln!();
            eprintln!("shutting down…");
            Ok(())
        }
    };

    // Graceful tunnel teardown before returning. `stop` is a no-op on
    // non-Linux today and a tasks-cleanup on Linux.
    _tunnel.stop().await;

    result
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

fn print_status() -> Result<()> {
    use virtues_protocol::spki_fingerprint;

    let bundle = match keychain::load_bundle()? {
        Some(b) => b,
        None => {
            println!("paired:           no");
            println!();
            println!("Run `virtues-client pair <pair-url>` to pair with your box.");
            println!("The pair URL is printed by `virtues link` on the box itself.");
            return Ok(());
        }
    };

    // Box identity. SPKI fingerprint is computed from the box's WG public
    // key — same primitive we'd verify out-of-band against what the box
    // shows on its own screen.
    let box_pub_bytes = match decode_b64_32(&bundle.wg.server_public_key) {
        Ok(b) => Some(b),
        Err(e) => {
            eprintln!("warning: could not decode box WG pubkey: {e}");
            None
        }
    };
    let spki = box_pub_bytes.as_ref().map(spki_fingerprint);

    // Local WG identity. The presence of a stored private key tells us
    // whether the tunnel CAN come up at all.
    let has_wg_private = keychain::load_wg_private()?.is_some();

    println!("paired:           yes");
    println!("box id:           {}", bundle.rendezvous.publish_id);
    if let Some(fp) = &spki {
        println!("box fingerprint:  {fp}");
    }
    println!("box address:      {} (port {})", bundle.internal_ip, bundle.http_port);
    println!("box endpoint:     {}", bundle.wg.server_endpoint);
    println!("client address:   {}", bundle.wg.client_address);
    println!();
    println!("wg private key:   {}", if has_wg_private { "present" } else { "MISSING (re-pair to regenerate)" });
    println!("local proxy:      http://localhost:{}", virtues_protocol::INTERNAL_PORT);
    println!();
    println!("tunnel state:     run `virtues-client up` to bring it up");
    println!("                  (Linux: needs CAP_NET_ADMIN — see linux/virtues-client.service)");

    Ok(())
}

fn decode_b64_32(s: &str) -> Result<[u8; 32]> {
    use base64::Engine as _;
    let v = base64::engine::general_purpose::STANDARD
        .decode(s)
        .context("base64 decode")?;
    v.try_into().map_err(|v: Vec<u8>| {
        anyhow::anyhow!("expected 32 bytes, got {}", v.len())
    })
}

async fn revoke() -> Result<()> {
    // TODO: POST to box's revoke endpoint; clear keychain entry.
    keychain::delete_bundle()?;
    println!("local creds cleared.");
    println!("warning: this only removes credentials on THIS machine. To also");
    println!("remove this device from the box's Devices list, open the box's web");
    println!("UI on another paired device and revoke it there.");
    Ok(())
}
