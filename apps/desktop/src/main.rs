//! # virtues-client
//!
//! The desktop daemon. Pairs to a Virtues box over WireGuard and runs a local
//! HTTP reverse proxy on `127.0.0.1:7117` so any browser on this machine sees
//! the box at a Secure-Context-eligible loopback URL — no per-box CA install,
//! no cert warnings, no trust dance.
//!
//! See [[localhost-daemon-trust]] and [[v02-plan]] in MEMORY.md for the
//! architectural commitment this implements.
//!
//! ## Subcommands
//!
//! - `pair <pair-url>` — consume a one-time pair URL from the server; receive a
//!   [`virtues_protocol::PairingBundle`] (WG keys, server endpoint, session
//!   bearer); store it in the OS keychain.
//! - `pair-code <code>` — same, but via the short 6-char code the server prints
//!   (server discovered over mDNS unless `--server` is given).
//! - `discover` — list Virtues servers found on the LAN via mDNS (`--json`).
//! - `up` — bring the WG tunnel up (via GotaTun) and start the localhost proxy.
//! - `status` — report pairing + tunnel state and the proxy port.
//! - `revoke` — clear local creds + tell the server to drop this credential.
//! - `daemon` (hidden) — privileged root service: WG tunnel + `.virtues` DNS.
//!
//! ## Why this binary exists
//!
//! Until this ships, only the Jetson's own Chromium can use Virtues. The
//! desktop daemon is the v0.2 unblock-everyone milestone — see
//! [[v02-plan]] Track A in MEMORY.md.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod daemon;
mod discover;
mod dns;
mod install;
mod keychain;
mod pair;
mod proxy;
mod tunnel;

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
    /// The pair URL is what `virtues pair` prints on the box. Copy it from the
    /// box's terminal output (or scan its QR if your box has a screen) and
    /// paste it here. The token expires in 15 minutes and is single-use.
    Pair {
        /// The full pair URL, e.g.
        /// `http://10.0.0.5:8000/pair#t=<token>&ep=...&fpr=...`.
        pair_url: String,
    },

    /// Bring the WG tunnel up and start the localhost HTTP proxy. Runs in the
    /// foreground until interrupted.
    Up {
        /// Skip Virtues' built-in WireGuard tunnel and proxy to a box you can
        /// already reach over your OWN transport (Tailscale, Headscale, a VPS,
        /// direct IPv6, …). The box authenticates at the app layer, so any
        /// transport works — see docs/byo-networking.md. Requires --upstream.
        #[arg(long)]
        no_tunnel: bool,

        /// Override the upstream box address the proxy forwards to, e.g.
        /// `100.64.0.2:8000` or `[2606:4700::1]:8000`. Defaults to the paired
        /// box's WG-internal address (reached through the built-in tunnel).
        /// Set this to the box's address on your BYO transport.
        #[arg(long)]
        upstream: Option<String>,
    },

    /// Pair using a short 6-character code printed by `virtues pair` or
    /// `virtues init` on the server. Discovers the server via mDNS if --server
    /// is not given.
    PairCode {
        /// The 6-character code shown by the server (spaces optional, e.g. "ABC DEF").
        code: String,
        /// Server origin to pair with, e.g. `http://adam.local:8000`.
        /// If omitted, discovered automatically via mDNS.
        #[arg(long)]
        server: Option<String>,
    },

    /// Discover Virtues servers on the local network via mDNS.
    Discover {
        /// Emit JSON array of found servers instead of human-readable output.
        #[arg(long)]
        json: bool,
    },

    /// Privileged background daemon: brings up the WireGuard tunnel and runs
    /// the `.virtues` DNS server. Invoked by the LaunchDaemon as root.
    #[command(hide = true)]
    Daemon {
        /// Path to the bundle JSON file written by `virtues-client pair`.
        /// Required when running as root (LaunchDaemon) because the OS keychain
        /// is user-specific and not accessible to the root process.
        /// The installer embeds the actual user's home path in the plist.
        #[arg(long)]
        bundle_path: Option<std::path::PathBuf>,
    },

    /// Report tunnel state, last handshake age, and proxy port.
    Status,

    /// Clear local creds + remove this device's WG peer from the box.
    ///
    /// Aliased as `reset` — the one-step "my tunnel is wedged, start clean"
    /// command: wipes the keychain entries AND the on-disk fallbacks
    /// (`bundle.json`, `wg-private.key`), then re-pair to reconnect.
    #[command(alias = "reset")]
    Revoke,

    /// Install the user-level LaunchAgent (localhost proxy). No root needed.
    /// Run after pairing. The Tauri app calls this on the bundled sidecar.
    #[command(hide = true)]
    Install {
        /// Box address the proxy forwards to (e.g. `100.104.55.76:8000` or
        /// `adam.local:8000`) — normally the address you paired against. When
        /// omitted, falls back to the bundle's WG-internal address.
        #[arg(long)]
        upstream: Option<String>,
    },

    /// Remove the user-level LaunchAgent + the ~/.virtues/bin binary.
    #[command(hide = true)]
    Uninstall,

    /// Install the root LaunchDaemon (WG tunnel + .virtues DNS). Must run as
    /// root — invoked once via `osascript … with administrator privileges`.
    #[command(hide = true)]
    InstallSystem {
        /// The login user whose ~/.virtues/bundle.json the daemon reads.
        #[arg(long)]
        user: String,
        /// Path to that user's bundle.json.
        #[arg(long)]
        bundle: std::path::PathBuf,
    },

    /// Remove the root LaunchDaemon + /usr/local/bin binary. Must run as root.
    #[command(hide = true)]
    UninstallSystem,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();
    match cli.command {
        Command::Pair { pair_url } => pair::run(&pair_url).await,
        Command::PairCode { code, server } => run_pair_code(code, server).await,
        Command::Discover { json } => run_discover(json).await,
        Command::Daemon { bundle_path } => daemon::run(bundle_path).await,
        Command::Up { no_tunnel, upstream } => run_up(no_tunnel, upstream).await,
        Command::Status => print_status(),
        Command::Revoke => revoke().await,
        Command::Install { upstream } => install::run_user(upstream.as_deref()),
        Command::Uninstall => install::uninstall_user(),
        Command::InstallSystem { user, bundle } => install::run_system(&user, &bundle),
        Command::UninstallSystem => install::uninstall_system(),
    }
}

/// Load the paired bundle, resilient to macOS's per-binary keychain ACL.
///
/// The keychain item is scoped to whatever binary created it — the app's
/// *bundled* sidecar at pair time. A *different* binary (e.g. this one running
/// as the installed LaunchAgent at `~/.virtues/bin/virtues-client`) usually
/// can't read it, so `keychain::load_bundle()` comes back empty and the proxy
/// would die with "no paired box". `pair` also writes `~/.virtues/bundle.json`
/// (mode 600), readable by any of the user's processes regardless of code
/// identity — so fall back to that.
fn load_paired_bundle() -> Result<Option<virtues_protocol::PairingBundle>> {
    if let Some(b) = keychain::load_bundle().context("read bundle from keychain")? {
        return Ok(Some(b));
    }
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return Ok(None),
    };
    let path = std::path::PathBuf::from(home).join(".virtues").join("bundle.json");
    match std::fs::read_to_string(&path) {
        Ok(json) => Ok(Some(
            serde_json::from_str(&json)
                .with_context(|| format!("decode {}", path.display()))?,
        )),
        Err(_) => Ok(None),
    }
}

async fn run_pair_code(code: String, server: Option<String>) -> Result<()> {
    let origin = match server {
        Some(s) => s,
        None => {
            eprintln!("searching for Virtues servers on the local network…");
            let servers = discover::discover_servers(5).await;
            if servers.is_empty() {
                anyhow::bail!(
                    "no Virtues servers found via mDNS. Pass --server <origin> explicitly, \
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

/// `virtues-client up` — bring the tunnel up + start the local HTTP proxy.
///
/// Returns only on fatal error (listener refused, etc.) or on Ctrl-C.
///
/// With `--no-tunnel` the built-in WireGuard tunnel is skipped and the proxy
/// forwards to `--upstream` instead — the box's address on a BYO transport
/// (Tailscale/VPS/direct IPv6). The proxy itself is transport-agnostic, so this
/// is the only change needed; the box still authenticates at the app layer.
async fn run_up(no_tunnel: bool, upstream: Option<String>) -> Result<()> {
    if no_tunnel && upstream.is_none() {
        anyhow::bail!(
            "--no-tunnel needs --upstream <addr:port> — the box's address on your \
             own transport, e.g. `--upstream 100.64.0.2:8000`. See docs/byo-networking.md."
        );
    }

    let bundle = load_paired_bundle()?.ok_or_else(|| {
        anyhow::anyhow!("no paired box — run `virtues-client pair` first")
    })?;

    // Transport selection. The handle (if any) MUST stay alive for the proxy's
    // lifetime; dropping it tears the WG state machine down.
    //  - `--no-tunnel`: forced BYO — skip WG entirely, forward to `--upstream`
    //    (the box's address on the user's own transport: Tailscale/VPS/IPv6).
    //  - default: bring up the userspace WireGuard tunnel (SPKI trust). If the
    //    box's WG endpoint isn't reachable from here (no IPv6 / NAT / hostile
    //    network) and an `--upstream` was supplied, fall back to BYO so the
    //    proxy still works; otherwise the WG failure is fatal.
    let (tunnel, cfg) = if no_tunnel {
        eprintln!("⤳ --no-tunnel: skipping WireGuard; forwarding over your own transport");
        let addr = upstream.as_ref().expect("checked above");
        let cfg = proxy::ProxyConfig::from_bundle_with_upstream(&bundle, addr)
            .with_context(|| format!("parse --upstream `{addr}`"))?;
        (None, cfg)
    } else {
        match tunnel::start(&bundle).await {
            Ok(handle) => {
                // Proxy → the loopback forwarder → over WG → box.
                let cfg = proxy::ProxyConfig {
                    upstream_addr: handle.forwarder_addr,
                    upstream_host: bundle.internal_host.clone(),
                    bind_port: proxy::LOCAL_PROXY_PORT,
                    bearer: bundle.bearer.clone(),
                };
                (Some(handle), cfg)
            }
            Err(e) => match &upstream {
                Some(addr) => {
                    eprintln!("⚠ WireGuard tunnel unavailable ({e});");
                    eprintln!("  falling back to direct upstream {addr}");
                    let cfg = proxy::ProxyConfig::from_bundle_with_upstream(&bundle, addr)
                        .with_context(|| format!("parse --upstream `{addr}`"))?;
                    (None, cfg)
                }
                None => {
                    return Err(e).context(
                        "WireGuard tunnel failed and no --upstream fallback was given",
                    )
                }
            },
        }
    };
    let result = tokio::select! {
        result = proxy::run(cfg) => result,
        _ = tokio::signal::ctrl_c() => {
            eprintln!();
            eprintln!("shutting down…");
            Ok(())
        }
    };

    // Graceful tunnel teardown before returning (no-op when --no-tunnel).
    if let Some(t) = tunnel {
        t.stop().await;
    }

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

    let bundle = match load_paired_bundle()? {
        Some(b) => b,
        None => {
            println!("paired:           no");
            println!();
            println!("Run `virtues-client pair <pair-url>` to pair with your box.");
            println!("The pair URL is printed by `virtues pair` on the box itself.");
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
    if let Some(fp) = &spki {
        println!("box fingerprint:  {fp}");
    }
    println!("box address:      {} (port {})", bundle.internal_ip, bundle.http_port);
    println!("box endpoint:     {}", bundle.wg.server_endpoint);
    println!("client address:   {}", bundle.wg.client_address);
    println!();
    println!("wg private key:   {}", if has_wg_private { "present" } else { "MISSING (re-pair to regenerate)" });
    println!("local proxy:      http://localhost:{}", proxy::LOCAL_PROXY_PORT);
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
    let bundle = match load_paired_bundle()? {
        Some(b) => b,
        None => {
            println!("not paired — nothing to revoke.");
            return Ok(());
        }
    };

    // Best-effort: tell the server to drop this device's credential row.
    // NB: the DELETE endpoint matches on the *credential* id, not the device
    // id — they're different id spaces. Older pairings (pre-credential_id)
    // won't have it stored; those revoke locally only.
    if let Some(credential_id) = keychain::load_credential_id()? {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let url = format!(
            "http://localhost:{}/api/credentials/{credential_id}",
            proxy::LOCAL_PROXY_PORT
        );
        match client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", bundle.bearer))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => {
                eprintln!("✓ removed device from server");
            }
            Ok(r) => {
                eprintln!(
                    "warning: server returned {} — clearing local creds anyway",
                    r.status()
                );
            }
            Err(e) => {
                eprintln!(
                    "warning: could not reach server ({e}) — clearing local creds anyway"
                );
            }
        }
    } else {
        eprintln!(
            "note: no stored credential id (paired before revoke support) — \
             clearing local creds only. Remove this device from the server's \
             Devices page to fully de-authorize it."
        );
    }

    keychain::delete_bundle()?;
    println!("local creds cleared.");
    Ok(())
}
