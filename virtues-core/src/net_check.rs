//! On-box network self-assessment for the IPv6-direct doctrine.
//!
//! Answers one question honestly: **can a device reach this box directly?**
//! The box is a real computer on the real internet (see
//! `[[project_networking_doctrine]]`) — reachable directly when it has a
//! globally-routable address. This module detects that locally via the
//! outbound-socket trick (which source address the kernel picks for the
//! default route, per family) and classifies the result into actionable,
//! doctrine-aware guidance.
//!
//! What it can say for certain: whether the box has a global address to be
//! reached AT. What it deliberately does NOT claim: that inbound actually
//! works — a box cannot test its own firewall/NAT from the inside; only an
//! external echo can confirm that, which is a follow-on. So the headline is
//! "you have a global IPv6, direct should work — verify the pinhole," never a
//! false "you're reachable."

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, UdpSocket};

/// The box's WireGuard listen port — the inbound port that must be reachable.
/// Reads `VIRTUES_WG_LISTEN_PORT` (mirroring `virtues_wg::manager::wg_listen_port`),
/// else the WireGuard default. Re-read here rather than importing the WG crate
/// so this module stays free of the (Linux-only) WG crate and runs on any host.
fn wg_port() -> u16 {
    std::env::var("VIRTUES_WG_LISTEN_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|&p| p != 0)
        .unwrap_or(51820)
}

/// How the box sits on the network, most → least directly reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetClass {
    /// Global IPv6 egress — the doctrine's happy path. Direct works once the
    /// inbound pinhole is open.
    Ipv6Direct,
    /// Global IPv4 egress, no IPv6 — direct works over v4 (rare: a static home
    /// IP or a VPS). Pinhole still required.
    Ipv4Public,
    /// Behind NAT (private v4 source) with no global IPv6. Direct needs a
    /// forwarded port on a router you control; otherwise (dorm/corporate/CGNAT)
    /// a BYO overlay. Locally indistinguishable from CGNAT — an external echo
    /// is needed to tell which.
    NatNoIpv6,
    /// No egress address detected at all (no network).
    Unknown,
}

impl NetClass {
    /// Human label for `virtues doctor`.
    pub fn label(self) -> &'static str {
        match self {
            NetClass::Ipv6Direct => "ipv6-direct (recommended)",
            NetClass::Ipv4Public => "ipv4-public",
            NetClass::NatNoIpv6 => "behind-nat (no global IPv6)",
            NetClass::Unknown => "unknown (no egress)",
        }
    }

    /// Stable machine value for `status --json` / beacons.
    pub fn as_str(self) -> &'static str {
        match self {
            NetClass::Ipv6Direct => "ipv6_direct",
            NetClass::Ipv4Public => "ipv4_public",
            NetClass::NatNoIpv6 => "behind_nat",
            NetClass::Unknown => "unknown",
        }
    }
}

/// The result of an on-box network assessment.
#[derive(Debug, Clone)]
pub struct NetStatus {
    pub class: NetClass,
    /// The box's global IPv6, if any — the address a remote device dials.
    pub ipv6_global: Option<Ipv6Addr>,
    /// The box's egress IPv4 source (may be a private/LAN address behind NAT).
    pub ipv4_source: Option<Ipv4Addr>,
    /// One-line verdict.
    pub headline: String,
    /// What the user should do, doctrine-aware.
    pub guidance: String,
}

/// Assess the box's direct-reachability from on-box signals alone.
pub fn compute_net_status() -> NetStatus {
    let ipv6_global = probe_source("[2606:4700:4700::1111]:53", "[::]:0").and_then(|ip| match ip {
        IpAddr::V6(v) if is_global_v6(v) => Some(v),
        _ => None,
    });
    let ipv4_source = probe_source("1.1.1.1:53", "0.0.0.0:0").and_then(|ip| match ip {
        IpAddr::V4(v) => Some(v),
        _ => None,
    });

    let class = if ipv6_global.is_some() {
        NetClass::Ipv6Direct
    } else if ipv4_source.map(is_global_v4).unwrap_or(false) {
        NetClass::Ipv4Public
    } else if ipv4_source.is_some() {
        NetClass::NatNoIpv6
    } else {
        NetClass::Unknown
    };

    let port = wg_port();
    let (headline, guidance) = match class {
        NetClass::Ipv6Direct => {
            let addr = ipv6_global.expect("ipv6_global is Some in this arm");
            (
                format!("Global IPv6 detected ({addr}) — direct access works here."),
                format!(
                    "Highly recommended: reach your box directly over IPv6. The box tries to \
                     open inbound udp/{port} automatically; if your router/firewall is \
                     default-deny you may need to allow it. No third party, no overlay."
                ),
            )
        }
        NetClass::Ipv4Public => (
            "Global IPv4 detected — direct access works over IPv4.".to_string(),
            format!(
                "Allow/forward inbound udp/{port} to this box on your router. \
                 (IPv6 would be simpler if your ISP offers it.)"
            ),
        ),
        NetClass::NatNoIpv6 => (
            "No global IPv6 — this box is behind NAT.".to_string(),
            format!(
                "If you control the router (home), forward udp/{port} to this box. \
                 If you do NOT control the network (dorm/office/CGNAT), direct access isn't \
                 possible — host the box where you control the network, or add a BYO overlay \
                 you run yourself (Tailscale/Headscale/your own VPS). Virtues never runs or \
                 requires one."
            ),
        ),
        NetClass::Unknown => (
            "No network egress detected.".to_string(),
            "The box can't reach the internet. Check its network connection.".to_string(),
        ),
    };

    NetStatus {
        class,
        ipv6_global,
        ipv4_source,
        headline,
        guidance,
    }
}

/// Outcome of the external inbound-reachability echo.
pub enum InboundResult {
    /// virtues-api reached us — inbound is confirmed open on the tested path.
    Reachable,
    /// Could not confirm. NOT a definitive "blocked": a timeout is ambiguous
    /// (the box's firewall, OR api having no IPv6 yet, OR a transient drop), so
    /// we never claim "blocked" — only "couldn't confirm" + a short reason.
    Inconclusive(String),
}

/// Ask virtues-api to fire a UDP nonce back at us over IPv6 and see if it
/// arrives — the one honest inbound test (a box can't test its own firewall
/// from inside). Returns [`InboundResult::Reachable`] only on a positive
/// confirmation; everything else is `Inconclusive` (we never assert "blocked").
///
/// Forces the probe request out over the box's global IPv6 (`local_address`) so
/// api observes our v6 source and fires at v6 — testing the doctrine's primary
/// path. Requires api to have an AAAA; until then this is always inconclusive,
/// which is honest (no false negatives).
pub async fn verify_inbound(global_v6: Ipv6Addr, api_base: &str) -> InboundResult {
    use std::time::Duration;

    let sock = match tokio::net::UdpSocket::bind("[::]:0").await {
        Ok(s) => s,
        Err(e) => return InboundResult::Inconclusive(format!("could not bind probe socket: {e}")),
    };
    let port = match sock.local_addr() {
        Ok(a) => a.port(),
        Err(e) => return InboundResult::Inconclusive(format!("local_addr: {e}")),
    };

    // A short random nonce so we can match the echoed datagram to this request.
    let nonce: String = {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..16).map(|_| format!("{:x}", rng.gen_range(0u8..16))).collect()
    };

    // Pin egress to the global v6 so api observes our v6 (not a happy-eyeballs v4).
    let client = match reqwest::Client::builder()
        .local_address(IpAddr::V6(global_v6))
        .timeout(Duration::from_secs(4))
        .build()
    {
        Ok(c) => c,
        Err(e) => return InboundResult::Inconclusive(format!("http client: {e}")),
    };
    let url = format!("{}/v1/net/probe", api_base.trim_end_matches('/'));
    let body = serde_json::json!({ "port": port, "nonce": nonce });
    if let Err(e) = client.post(&url).json(&body).send().await {
        return InboundResult::Inconclusive(format!(
            "could not reach virtues-api over IPv6 ({e}) — api may not have an IPv6 address yet"
        ));
    }

    // Wait up to 3s for the echoed nonce.
    let mut buf = [0u8; 256];
    match tokio::time::timeout(Duration::from_secs(3), sock.recv_from(&mut buf)).await {
        Ok(Ok((n, _))) if buf[..n] == *nonce.as_bytes() => InboundResult::Reachable,
        Ok(Ok(_)) => InboundResult::Inconclusive("received an unexpected datagram".into()),
        Ok(Err(e)) => InboundResult::Inconclusive(format!("recv error: {e}")),
        Err(_) => InboundResult::Inconclusive(
            "no packet arrived within 3s — inbound may be blocked (open udp/51820), \
             or api has no IPv6 yet"
                .into(),
        ),
    }
}

impl NetStatus {
    /// Print a human-readable report (used by `virtues doctor`).
    pub fn print_report(&self) {
        println!("Virtues network reachability");
        println!("  class:         {}", self.class.label());
        match self.ipv6_global {
            Some(a) => println!("  global IPv6:   {a}"),
            None => println!("  global IPv6:   none"),
        }
        match self.ipv4_source {
            Some(a) => println!("  IPv4 source:   {a}"),
            None => println!("  IPv4 source:   none"),
        }
        println!("  {}", self.headline);
        println!("  → {}", self.guidance);
    }
}

/// Outbound-socket trick: `connect()` a UDP socket to a public address (no
/// packets sent — only a kernel route lookup) and read back the source address
/// the kernel picked. `None` if no route / no address on that family.
fn probe_source(dest: &str, bind: &str) -> Option<IpAddr> {
    let sock = UdpSocket::bind(bind).ok()?;
    sock.connect(dest).ok()?;
    let ip = sock.local_addr().ok()?.ip();
    if ip.is_loopback() || ip.is_unspecified() {
        None
    } else {
        Some(ip)
    }
}

/// Globally-routable IPv6: not loopback/unspecified/multicast/link-local/ULA.
/// (`Ipv6Addr::is_unique_local`/`is_unicast_link_local` are unstable on stable
/// Rust, so the ranges are open-coded — kept in sync with the WG daemon.)
fn is_global_v6(v: Ipv6Addr) -> bool {
    !v.is_loopback()
        && !v.is_unspecified()
        && !v.is_multicast()
        && (v.segments()[0] & 0xffc0) != 0xfe80 // not fe80::/10 link-local
        && (v.segments()[0] & 0xfe00) != 0xfc00 // not fc00::/7 unique-local
}

/// Globally-routable IPv4: not loopback/unspecified/private/link-local/
/// broadcast/multicast/CGNAT.
fn is_global_v4(v: Ipv4Addr) -> bool {
    !v.is_loopback()
        && !v.is_unspecified()
        && !v.is_private()
        && !v.is_link_local()
        && !v.is_broadcast()
        && !v.is_multicast()
        && !is_cgnat_v4(v)
}

/// 100.64.0.0/10 (RFC 6598) — carrier-grade NAT, never internet-routable.
fn is_cgnat_v4(v: Ipv4Addr) -> bool {
    let o = v.octets();
    o[0] == 100 && (o[1] & 0xc0) == 0x40
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v6_classification() {
        assert!(is_global_v6("2606:4700:4700::1111".parse().unwrap()));
        assert!(is_global_v6("2001:db8::1".parse().unwrap()));
        assert!(!is_global_v6("::1".parse().unwrap()));
        assert!(!is_global_v6("fe80::1".parse().unwrap())); // link-local
        assert!(!is_global_v6("fc00::1".parse().unwrap())); // ULA
        assert!(!is_global_v6("fd00:5654::1".parse().unwrap())); // ULA (our WG range)
        assert!(!is_global_v6("ff02::1".parse().unwrap())); // multicast
    }

    #[test]
    fn v4_classification() {
        assert!(is_global_v4("12.34.56.78".parse().unwrap()));
        assert!(is_global_v4("8.8.8.8".parse().unwrap()));
        assert!(!is_global_v4("10.0.0.5".parse().unwrap()));
        assert!(!is_global_v4("192.168.1.9".parse().unwrap()));
        assert!(!is_global_v4("172.16.0.1".parse().unwrap()));
        assert!(!is_global_v4("169.254.1.1".parse().unwrap())); // link-local
        assert!(!is_global_v4("100.64.0.1".parse().unwrap())); // CGNAT
        assert!(!is_global_v4("100.127.255.1".parse().unwrap())); // CGNAT edge
        assert!(is_global_v4("100.128.0.1".parse().unwrap())); // just outside CGNAT
    }
}
