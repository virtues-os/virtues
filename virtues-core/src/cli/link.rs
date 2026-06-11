//! Output helpers for `virtues link` / `virtues init`.
//!
//! With the localhost-daemon trust model (see [[localhost-daemon-trust]] in
//! MEMORY.md), there are no certs to install on the client. The box exposes
//! plain HTTP on :8000 and the only browser that can reach it without a
//! daemon is the Jetson's own Chromium hitting `http://localhost:8000`. For
//! other devices we point at the Virtues client (v0.2 work).
//!
//! This module builds the URL list shown by `virtues link` / `virtues init`.

use std::net::{IpAddr, SocketAddr, UdpSocket};

use crate::wireguard::INTERNAL_PORT;

/// One reachable pair URL we print to the user. The label is a short tag
/// ("Local", "LAN") and the `url` is what they paste into a browser.
pub struct ReachableUrl {
    pub label: &'static str,
    pub url: String,
}

/// Discover the box's primary outbound-facing IP address. Bind a UDP socket
/// to a public address (no traffic is sent — `connect()` on UDP only sets
/// the route); the OS then assigns us the local address that would be used
/// for that route. This is the address a client laptop on the same LAN
/// would reach the box on.
///
/// Returns `None` if we can't determine an address (rare; e.g. no network
/// configured at all). The caller falls back to printing only the loopback URL.
pub fn primary_ip() -> Option<IpAddr> {
    // 198.51.100.1 is documented test address space (RFC 5737) — we won't
    // actually send anything, but using a TEST-NET address avoids any
    // possibility of confusion with real traffic.
    let target: SocketAddr = "198.51.100.1:1".parse().ok()?;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect(target).ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}

/// Build the URLs `virtues link` / `virtues init` print for the user.
///
/// `Local` (loopback) always works in Chromium on the Jetson itself — it's a
/// W3C Secure Context without needing TLS. `LAN` is the box's IP, useful for
/// developer scenarios where you're SSH'd in but want to use a browser on a
/// different machine on the same LAN. In production the LAN URL is more of a
/// diagnostic — the canonical path for other devices is the Virtues client.
pub fn reachable_pair_urls(token: &str, is_dev: bool, web_port: &str) -> Vec<ReachableUrl> {
    if is_dev {
        return vec![ReachableUrl {
            label: "Local",
            url: format!("http://localhost:{web_port}/pair#t={token}"),
        }];
    }
    let mut urls = vec![ReachableUrl {
        label: "Local",
        url: format!("http://localhost:{INTERNAL_PORT}/pair#t={token}"),
    }];
    if let Some(ip) = primary_ip() {
        let host = match ip {
            IpAddr::V4(v4) => v4.to_string(),
            IpAddr::V6(v6) => format!("[{v6}]"),
        };
        urls.push(ReachableUrl {
            label: "LAN",
            url: format!("http://{host}:{INTERNAL_PORT}/pair#t={token}"),
        });
    }
    urls
}
