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

/// The box's mDNS name (`<hostname>.local`). The installer registers the box
/// with Avahi, so this resolves on the LAN — it's the name we lead with in
/// every cross-device handoff (onboarding doctrine: `virtues.local`, never
/// `localhost`, for anything meant to be opened on another device).
pub fn mdns_host() -> String {
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "virtues".to_string());
    format!("{host}.local")
}

/// Build the URLs `virtues link` / `virtues init` print for the user.
///
/// Order matters — it's a UX statement:
///   1. `Any device` — the mDNS name. The URL a human should actually use,
///      from a phone or laptop on the same network.
///   2. `(if .local fails)` — the raw LAN IP. mDNS is flaky on some clients
///      (notably Android) and filtered on some networks; the IP is the
///      universal fallback.
///   3. `This machine` — loopback, for a browser on the box itself (a W3C
///      Secure Context without TLS). Last because almost nobody runs a
///      browser on the box; the kiosk panel is the exception and doesn't
///      read this output.
pub fn reachable_pair_urls(token: &str, is_dev: bool, web_port: &str) -> Vec<ReachableUrl> {
    if is_dev {
        return vec![ReachableUrl {
            label: "Local",
            url: format!("http://localhost:{web_port}/pair#t={token}"),
        }];
    }
    let mut urls = vec![ReachableUrl {
        label: "Any device",
        url: format!("http://{}:{INTERNAL_PORT}/pair#t={token}", mdns_host()),
    }];
    if let Some(ip) = primary_ip() {
        let host = match ip {
            IpAddr::V4(v4) => v4.to_string(),
            IpAddr::V6(v6) => format!("[{v6}]"),
        };
        urls.push(ReachableUrl {
            label: "(if .local fails)",
            url: format!("http://{host}:{INTERNAL_PORT}/pair#t={token}"),
        });
    }
    urls.push(ReachableUrl {
        label: "This machine",
        url: format!("http://localhost:{INTERNAL_PORT}/pair#t={token}"),
    });
    urls
}
