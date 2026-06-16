//! mDNS discovery of Virtues servers on the local network.
//!
//! Browses for `_http._tcp` services with TXT record `service=virtues` —
//! the same service the installer advertises via Avahi. Returns a list of
//! found servers, each with a name, host, port, and pre-formed origin URL.
//!
//! The browse runs synchronously on a thread (mdns-sd uses std channels
//! internally) and is wrapped in `spawn_blocking` for async callers.

use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct FoundServer {
    pub name: String,
    pub host: String,
    pub port: u16,
    /// Full origin URL, e.g. `http://adam.local:8000`
    pub origin: String,
}

/// Browse the local network for Virtues servers. Waits up to `timeout_secs`
/// seconds for responses, then returns whatever was found.
pub async fn discover_servers(timeout_secs: u64) -> Vec<FoundServer> {
    tokio::task::spawn_blocking(move || discover_blocking(timeout_secs))
        .await
        .unwrap_or_default()
}

fn discover_blocking(timeout_secs: u64) -> Vec<FoundServer> {
    use mdns_sd::{ServiceDaemon, ServiceEvent};
    use std::time::{Duration, Instant};

    let mdns = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("mDNS daemon start failed: {e}");
            return vec![];
        }
    };

    let receiver = match mdns.browse("_http._tcp.local.") {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("mDNS browse failed: {e}");
            return vec![];
        }
    };

    let mut found: Vec<FoundServer> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        // Cap each recv at 100ms so we keep checking the deadline.
        let wait = remaining.min(Duration::from_millis(100));
        match receiver.recv_timeout(wait) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                let props = info.get_properties();
                let is_virtues = props.iter().any(|p| {
                    p.key().eq_ignore_ascii_case("service")
                        && p.val_str().eq_ignore_ascii_case("virtues")
                });
                if !is_virtues {
                    continue;
                }
                // Strip trailing dot from mDNS-style hostname (e.g. "adam.local.")
                let host = info.get_hostname().trim_end_matches('.').to_string();
                let port = info.get_port();
                let origin = format!("http://{host}:{port}");
                found.push(FoundServer {
                    name: info.get_fullname().to_string(),
                    host,
                    port,
                    origin,
                });
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    let _ = mdns.stop_browse("_http._tcp.local.");
    found
}

/// Print discovered servers to stdout for the CLI `discover` subcommand.
pub fn print_servers(servers: &[FoundServer]) {
    if servers.is_empty() {
        println!("No Virtues servers found on the local network.");
        println!();
        println!("Make sure your server is running and on the same network.");
        println!("Run `virtues doctor` on the server to check mDNS.");
        return;
    }
    println!("Found {} server(s):", servers.len());
    println!();
    for (i, s) in servers.iter().enumerate() {
        println!("  {}. {} ({})", i + 1, s.name, s.origin);
    }
}

/// Prompt the user to pick a server when multiple are found. Returns the
/// chosen server or an error if stdin is not interactive.
pub fn pick_server(servers: &[FoundServer]) -> Result<&FoundServer> {
    use std::io::{self, Write};

    if servers.len() == 1 {
        return Ok(&servers[0]);
    }

    print_servers(servers);
    print!("Choose server [1-{}]: ", servers.len());
    io::stdout().flush()?;

    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let n: usize = line
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid selection"))?;
    servers
        .get(n.wrapping_sub(1))
        .ok_or_else(|| anyhow::anyhow!("selection out of range"))
}
