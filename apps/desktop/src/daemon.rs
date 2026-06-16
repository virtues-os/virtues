//! `virtues-client daemon` — privileged background service for macOS.
//!
//! Runs as root via a LaunchDaemon
//! (`/Library/LaunchDaemons/com.virtues.daemon.plist`). Responsible for
//! the two operations that require root on macOS:
//!
//! 1. **WireGuard tunnel** — `ifconfig`/`route` commands to configure utun
//! 2. **`.virtues` DNS** — writes `/etc/resolver/virtues` and runs the DNS
//!    server on `127.0.0.1:5354`
//!
//! The HTTP reverse proxy (`virtues-client up --no-tunnel`) runs separately
//! as the current user via a LaunchAgent — no root needed for that part.

use std::net::IpAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::{dns, keychain, tunnel};

const RESOLVER_PATH: &str = "/etc/resolver/virtues";
const DNS_PORT: u16 = 5354;

pub async fn run(bundle_path: Option<PathBuf>) -> Result<()> {
    let bundle = load_bundle(bundle_path)?;

    let server_ip: IpAddr = bundle
        .internal_ip
        .parse()
        .with_context(|| format!("parse internal_ip `{}`", bundle.internal_ip))?;

    eprintln!("virtues-client daemon starting (server IP: {server_ip})");

    // Write /etc/resolver/virtues so mDNSResponder delegates .virtues queries
    // to our local DNS server. Requires root, which the LaunchDaemon provides.
    write_resolver_file(server_ip)?;

    // Bring up the WireGuard tunnel.
    let _tunnel = tunnel::start(&bundle)
        .await
        .context("bring WireGuard tunnel up")?;

    // Run the .virtues DNS server. This future never returns unless the socket
    // bind fails; we hold _tunnel alive for the same lifetime.
    dns::run_dns_server(server_ip).await?;

    Ok(())
}

/// Load the PairingBundle for the daemon.
///
/// When a `--bundle-path` was given (the LaunchDaemon case, running as root),
/// read from that file. Otherwise fall back to the OS keychain, which works
/// when the CLI is run as the paired user.
fn load_bundle(bundle_path: Option<PathBuf>) -> Result<virtues_protocol::PairingBundle> {
    if let Some(path) = bundle_path {
        let json = std::fs::read_to_string(&path)
            .with_context(|| format!("read bundle file {}", path.display()))?;
        return serde_json::from_str(&json)
            .with_context(|| format!("decode bundle file {}", path.display()));
    }
    keychain::load_bundle()
        .context("read paired bundle from OS keychain")?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no paired server — run `virtues-client pair <pair-url>` first, \
                 then restart the daemon. If running as root (LaunchDaemon), \
                 pass --bundle-path ~/.virtues/bundle.json"
            )
        })
}

fn write_resolver_file(server_ip: IpAddr) -> Result<()> {
    let content = format!(
        "# Written by virtues-client daemon. Do not edit.\n\
         nameserver 127.0.0.1\n\
         port {DNS_PORT}\n"
    );

    // Ensure /etc/resolver/ exists (it usually does on macOS but may be absent
    // in minimal/container environments).
    std::fs::create_dir_all("/etc/resolver")
        .context("create /etc/resolver")?;

    std::fs::write(RESOLVER_PATH, &content)
        .with_context(|| format!("write {RESOLVER_PATH}"))?;

    eprintln!("✓ wrote {RESOLVER_PATH} (*.virtues → 127.0.0.1:{DNS_PORT} → {server_ip})");
    tracing::info!(
        path = RESOLVER_PATH,
        server_ip = %server_ip,
        "wrote .virtues resolver stub"
    );
    Ok(())
}
