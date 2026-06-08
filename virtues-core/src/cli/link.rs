//! Output helpers for `virtues link` / `virtues init`.
//!
//! Two things live here:
//!
//! 1. **CA install recipes** — per-OS commands for trusting the box's
//!    self-signed CA, so the browser doesn't show a scary warning. We print
//!    all four (macOS / Debian-Ubuntu / Fedora / Windows) and let the user
//!    pick. No client-OS detection magic.
//!
//! 2. **Reachable URL set** — the box can advertise itself as
//!    `https://virtues.local` (mDNS), but `.local` doesn't resolve on Linux
//!    clients without `nss-mdns` or on Windows without Bonjour. As a
//!    fallback we also print `https://<box-ip>/...` for each non-loopback
//!    interface address. The user opens whichever works.
//!
//! Both are pure functions — the actual minting + printing happens in
//! `main.rs`. Testable, swappable, no state.

use std::net::{IpAddr, SocketAddr, UdpSocket};

/// One line of advice the user can copy-paste to trust the box CA. The
/// `box_host` placeholder is substituted with the actual hostname / IP the
/// user is hitting (so e.g. the Linux command points at the same host they
/// just opened in the browser).
pub struct CaRecipe {
    pub os: &'static str,
    pub command: String,
}

pub fn ca_recipes(box_host: &str) -> Vec<CaRecipe> {
    vec![
        CaRecipe {
            os: "macOS",
            command: format!(
                "curl -k https://{box_host}/ca-cert -o virtues-ca.crt && \
                 sudo security add-trusted-cert -d -r trustRoot \
                 -k /Library/Keychains/System.keychain virtues-ca.crt"
            ),
        },
        CaRecipe {
            os: "Linux (Debian/Ubuntu)",
            command: format!(
                "curl -k https://{box_host}/ca-cert | \
                 sudo tee /usr/local/share/ca-certificates/virtues.crt >/dev/null && \
                 sudo update-ca-certificates"
            ),
        },
        CaRecipe {
            os: "Linux (Fedora)",
            command: format!(
                "curl -k https://{box_host}/ca-cert -o /tmp/virtues-ca.crt && \
                 sudo trust anchor /tmp/virtues-ca.crt"
            ),
        },
        CaRecipe {
            os: "Windows (PowerShell, run as Administrator)",
            command: format!(
                "Invoke-WebRequest -Uri https://{box_host}/ca-cert \
                 -OutFile virtues-ca.crt; \
                 Import-Certificate -FilePath virtues-ca.crt \
                 -CertStoreLocation Cert:\\LocalMachine\\Root"
            ),
        },
    ]
}

/// Discover the box's primary outbound-facing IP address. Bind a UDP socket
/// to a public address (no traffic is sent — `connect()` on UDP only sets
/// the route); the OS then assigns us the local address that would be used
/// for that route. This is the address a client laptop on the same LAN
/// would reach the box on.
///
/// Returns `None` if we can't determine an address (rare; e.g. no network
/// configured at all). The caller falls back to printing only the mDNS URL.
pub fn primary_ip() -> Option<IpAddr> {
    // 198.51.100.1 is documented test address space (RFC 5737) — we won't
    // actually send anything, but using a TEST-NET address avoids any
    // possibility of confusion with real traffic.
    let target: SocketAddr = "198.51.100.1:1".parse().ok()?;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect(target).ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}

/// Build the list of URLs to print in `virtues link` output. Always
/// includes the canonical mDNS URL; adds an IP-based fallback when we can
/// figure one out. In dev mode (`ENVIRONMENT=dev`) we only print the
/// localhost form — the IP fallback is for production LANs where `.local`
/// might not resolve on the client.
pub struct ReachableUrl {
    pub label: &'static str,
    pub url: String,
}

pub fn reachable_pair_urls(token: &str, is_dev: bool, web_port: &str) -> Vec<ReachableUrl> {
    if is_dev {
        return vec![ReachableUrl {
            label: "Local",
            url: format!("http://localhost:{web_port}/pair#t={token}"),
        }];
    }
    let mut urls = vec![ReachableUrl {
        label: "Primary (mDNS)",
        url: format!("https://virtues.local/pair#t={token}"),
    }];
    if let Some(ip) = primary_ip() {
        let host = match ip {
            IpAddr::V4(v4) => v4.to_string(),
            IpAddr::V6(v6) => format!("[{v6}]"),
        };
        urls.push(ReachableUrl {
            label: "Fallback (IP)",
            url: format!("https://{host}/pair#t={token}"),
        });
    }
    urls
}

/// Pick a representative `box_host` to use in CA-trust recipe URLs. Prefers
/// `virtues.local` (the user typically tries this first); recipes still work
/// when the user reaches the box via IP since the CA-cert endpoint is
/// served on every interface.
pub fn ca_recipe_host() -> String {
    "virtues.local".to_string()
}
