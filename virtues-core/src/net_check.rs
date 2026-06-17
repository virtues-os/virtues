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
    /// A user-run overlay transport noticed on this box (Tailscale, a foreign
    /// WireGuard, …). Auto-noticed, never auto-enabled: Virtues never starts,
    /// configures, or recommends one (docs/byo-networking.md) — but a box that
    /// IS on one is reachable there, and the verdict should say so.
    pub byo: Option<ByoTransport>,
    /// One-line verdict.
    pub headline: String,
    /// What the user should do, doctrine-aware.
    pub guidance: String,
}

/// Assess the box's direct-reachability from on-box signals alone.
///
/// Cheap by design — two UDP `connect()`s (kernel route lookups, no packets
/// on the wire) plus one `getifaddrs` syscall. It is called per-poll from
/// `/api/setup/state`, so keep it allocation-light and never add network I/O
/// here; the active inbound echo lives in [`verify_inbound`], doctor-only.
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

    let (headline, guidance) = verdict_strings(class, ipv6_global, wg_port());

    NetStatus {
        class,
        ipv6_global,
        ipv4_source,
        byo: detect_byo(),
        headline,
        guidance,
    }
}

/// Headline + guidance per class. The split is a copy doctrine
/// (docs/onboarding.md): the **headline is a weather report** — it reaches
/// the setup handoff and `/api/setup/state`, so it states a fact about the
/// network and never instructs, never blames, never says "wait". The
/// **guidance carries the instructions** and only renders in `virtues
/// doctor`, where the user has asked for them.
fn verdict_strings(class: NetClass, ipv6: Option<Ipv6Addr>, port: u16) -> (String, String) {
    match class {
        NetClass::Ipv6Direct => {
            let addr = ipv6.expect("ipv6 is Some for Ipv6Direct");
            (
                format!("Global IPv6 detected ({addr}) — reachable directly from anywhere."),
                format!(
                    "Your server has a public IPv6 address, which is the ideal path. \
                     It tries to open inbound udp/{port} automatically; if your router \
                     is default-deny you may need to allow it. No third party, no overlay."
                ),
            )
        }
        NetClass::Ipv4Public => (
            "Global IPv4 detected — reachable directly over IPv4.".to_string(),
            format!(
                "Forward inbound udp/{port} to your server on your router. \
                 (IPv6 would be simpler if your ISP offers it.)"
            ),
        ),
        NetClass::NatNoIpv6 => (
            "Behind NAT, no global IPv6 — local + LAN access work. \
             Remote access depends on whether you control the router."
                .to_string(),
            format!(
                "If you control the router, forward udp/{port} to your server — that's all it \
                 takes. If you don't control the network, remote access needs a tunnel you run \
                 yourself (Tailscale/Headscale/your own VPS). Virtues never installs or \
                 requires one. Run `virtues doctor` for a network diagnosis."
            ),
        ),
        NetClass::Unknown => (
            "No internet connection detected.".to_string(),
            "Your server can't reach the internet. Check its network connection.".to_string(),
        ),
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
        let mut rng = rand::rng();
        (0..16).map(|_| format!("{:x}", rng.random_range(0u8..16))).collect()
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
    /// The one-line verdict consumers print (setup handoff, `/api/setup/state`).
    /// Prefers concrete reachability over the class headline: a NAT'd box on a
    /// user-run overlay IS reachable — at the overlay address.
    pub fn verdict_line(&self) -> String {
        if self.ipv6_global.is_some() {
            return self.headline.clone();
        }
        match &self.byo {
            Some(b) => format!("Available via your own network ({}).", b.ifname),
            None => self.headline.clone(),
        }
    }

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
        if let Some(b) = &self.byo {
            let addr_paren = b.addr.map(|a| format!(" ({a})")).unwrap_or_default();
            println!(
                "  BYO transport: {}{addr_paren} — your devices can reach your server at that address",
                b.ifname
            );
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

// ─── BYO transport auto-notice ──────────────────────────────────────────────
//
// "Auto-enable nothing, auto-notice everything" (docs/onboarding.md): Virtues
// never runs, configures, or recommends an overlay — but the box answers on
// every interface it has ([::]:8000), so a user-run Tailscale/WireGuard IS a
// working path to it, and pretending otherwise would make the verdict a lie.
// Detection is name+address heuristics over the interface list; report-only.

/// A user-run overlay transport noticed on this box.
#[derive(Debug, Clone)]
pub struct ByoTransport {
    /// Interface name, e.g. "tailscale0".
    pub ifname: String,
    /// The overlay address devices would dial, if one was found on the
    /// interface (the CGNAT v4 for Tailscale; first non-link-local otherwise).
    pub addr: Option<IpAddr>,
}

/// The box's own tunnel interface — never a "BYO" finding. Mirrors
/// `virtues_wg::manager::WG_IFNAME` rather than importing it, same reasoning
/// as [`wg_port`]: that crate's `manager` is Linux-only and this module (and
/// its tests) must build on any host. Keep in sync.
const BUILTIN_WG_IFNAME: &str = "wg0";

/// Detect a user-run overlay on this box. Linux-only signal; `None` elsewhere.
fn detect_byo() -> Option<ByoTransport> {
    classify_byo(&list_interfaces())
}

/// Pure classification over (ifname, addrs) — unit-testable on any OS.
/// Rules in priority order (lowest rank wins):
///   0. ifname starts with "tailscale"
///   1. any non-loopback, non-builtin interface carrying a CGNAT (100.64/10)
///      address — catches Tailscale on a renamed tun + other CGNAT overlays
///   2. a WireGuard interface that isn't the built-in one
///   3. other known overlay names: NetBird (nb-*/wt0), Nebula, ZeroTier (zt*)
fn classify_byo(ifaces: &[(String, Vec<IpAddr>)]) -> Option<ByoTransport> {
    let mut best: Option<(u8, &str, &[IpAddr])> = None;
    for (name, addrs) in ifaces {
        if name == "lo" || name == BUILTIN_WG_IFNAME {
            continue;
        }
        let rank = if name.starts_with("tailscale") {
            Some(0)
        } else if addrs.iter().any(|a| matches!(a, IpAddr::V4(v) if is_cgnat_v4(*v))) {
            Some(1)
        } else if name.starts_with("wg") {
            Some(2)
        } else if name.starts_with("nb-")
            || name == "wt0"
            || name.starts_with("nebula")
            || name.starts_with("zt")
        {
            Some(3)
        } else {
            None
        };
        if let Some(r) = rank {
            if best.map(|(b, _, _)| r < b).unwrap_or(true) {
                best = Some((r, name, addrs));
            }
        }
    }
    best.map(|(_, name, addrs)| ByoTransport {
        ifname: name.to_string(),
        addr: pick_dial_addr(addrs),
    })
}

/// The address a device would dial: prefer the CGNAT v4 (Tailscale's stable
/// per-node address), else the first non-link-local address of any family.
fn pick_dial_addr(addrs: &[IpAddr]) -> Option<IpAddr> {
    addrs
        .iter()
        .find(|a| matches!(a, IpAddr::V4(v) if is_cgnat_v4(*v)))
        .or_else(|| {
            addrs.iter().find(|a| match a {
                IpAddr::V4(v) => !v.is_link_local(),
                IpAddr::V6(v) => (v.segments()[0] & 0xffc0) != 0xfe80,
            })
        })
        .copied()
}

/// Enumerate (ifname, addresses) via `getifaddrs(3)`. Interfaces appear even
/// when they carry no INET address yet (an unconfigured `wg1` still matters
/// for the name rules). All unsafe lives here.
#[cfg(target_os = "linux")]
fn list_interfaces() -> Vec<(String, Vec<IpAddr>)> {
    use std::collections::BTreeMap;
    use std::ffi::CStr;

    let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs allocates the list into ifap on success; freed below.
    if unsafe { libc::getifaddrs(&mut ifap) } != 0 {
        return Vec::new();
    }
    let mut map: BTreeMap<String, Vec<IpAddr>> = BTreeMap::new();
    let mut cur = ifap;
    while !cur.is_null() {
        // SAFETY: cur is a valid node of the list returned by getifaddrs.
        let ifa = unsafe { &*cur };
        cur = ifa.ifa_next;
        if ifa.ifa_name.is_null() {
            continue;
        }
        // SAFETY: ifa_name is a NUL-terminated C string owned by the list.
        let name = unsafe { CStr::from_ptr(ifa.ifa_name) }
            .to_string_lossy()
            .into_owned();
        let entry = map.entry(name).or_default();
        // ifa_addr is NULL for AF_PACKET-only/point-to-point entries — the
        // one real segfault risk here; check before any deref.
        if ifa.ifa_addr.is_null() {
            continue;
        }
        // SAFETY: ifa_addr is non-null; sa_family tells us the concrete type.
        match unsafe { (*ifa.ifa_addr).sa_family } as i32 {
            libc::AF_INET => {
                // SAFETY: sa_family == AF_INET guarantees sockaddr_in layout.
                let sa = unsafe { &*(ifa.ifa_addr as *const libc::sockaddr_in) };
                entry.push(IpAddr::V4(Ipv4Addr::from(u32::from_be(sa.sin_addr.s_addr))));
            }
            libc::AF_INET6 => {
                // SAFETY: sa_family == AF_INET6 guarantees sockaddr_in6 layout.
                let sa = unsafe { &*(ifa.ifa_addr as *const libc::sockaddr_in6) };
                entry.push(IpAddr::V6(Ipv6Addr::from(sa.sin6_addr.s6_addr)));
            }
            _ => {}
        }
    }
    // SAFETY: ifap came from getifaddrs and is freed exactly once.
    unsafe { libc::freeifaddrs(ifap) };
    map.into_iter().collect()
}

#[cfg(not(target_os = "linux"))]
fn list_interfaces() -> Vec<(String, Vec<IpAddr>)> {
    Vec::new()
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
    fn headlines_are_weather_reports() {
        // Most headlines must state facts, never instruct (instructions live in
        // guidance, rendered by doctor only). NatNoIpv6 is intentionally
        // actionable — it names the remediation path directly in the headline
        // since the setup wizard surfaces it without a doctor step.
        for (class, ipv6) in [
            (NetClass::Ipv6Direct, Some("2001:db8::1".parse().unwrap())),
            (NetClass::Ipv4Public, None),
            (NetClass::Unknown, None),
        ] {
            let (headline, _) = verdict_strings(class, ipv6, 51820);
            for instruction in ["forward", "router", "open udp", "allow", "run ", "check "] {
                assert!(
                    !headline.to_lowercase().contains(instruction),
                    "{class:?} headline instructs ({instruction:?}): {headline}"
                );
            }
        }
    }

    #[test]
    fn headline_copy_exact() {
        let (h, _) = verdict_strings(NetClass::NatNoIpv6, None, 51820);
        assert_eq!(
            h,
            "Behind NAT, no global IPv6 — local + LAN access work. \
             Remote access depends on whether you control the router."
        );
        let (h, _) = verdict_strings(NetClass::Unknown, None, 51820);
        assert_eq!(h, "No internet connection detected.");
        let (h, _) =
            verdict_strings(NetClass::Ipv6Direct, Some("2001:db8::1".parse().unwrap()), 51820);
        assert_eq!(h, "Global IPv6 detected (2001:db8::1) — reachable directly from anywhere.");
    }

    fn iface(name: &str, addrs: &[&str]) -> (String, Vec<IpAddr>) {
        (name.to_string(), addrs.iter().map(|a| a.parse().unwrap()).collect())
    }

    #[test]
    fn byo_classification() {
        // Tailscale by name, CGNAT addr preferred for dialing.
        let found = classify_byo(&[
            iface("lo", &["127.0.0.1"]),
            iface("tailscale0", &["fe80::1", "100.101.102.103"]),
        ])
        .unwrap();
        assert_eq!(found.ifname, "tailscale0");
        assert_eq!(found.addr, Some("100.101.102.103".parse().unwrap()));

        // CGNAT address on a renamed tun still counts.
        let found = classify_byo(&[iface("tun0", &["100.64.0.7"])]).unwrap();
        assert_eq!(found.ifname, "tun0");

        // The built-in wg0 is never a BYO finding; a foreign wg1 is.
        assert!(classify_byo(&[iface("wg0", &["fd00:5654::1"])]).is_none());
        let found = classify_byo(&[iface("wg1", &["10.9.0.2"])]).unwrap();
        assert_eq!(found.ifname, "wg1");
        assert_eq!(found.addr, Some("10.9.0.2".parse().unwrap()));

        // Other known overlay names, even with no address yet.
        assert!(classify_byo(&[iface("wt0", &[])]).is_some());
        assert!(classify_byo(&[iface("nebula1", &["192.168.100.2"])]).is_some());
        assert!(classify_byo(&[iface("zt7nnplfqx", &["10.147.17.5"])]).is_some());

        // Ordinary interfaces never match.
        assert!(classify_byo(&[
            iface("eth0", &["192.168.1.20"]),
            iface("docker0", &["172.17.0.1"]),
            iface("veth1a2b", &[]),
            iface("lo", &["127.0.0.1"]),
        ])
        .is_none());

        // Priority: tailscale name beats a foreign wg.
        let found = classify_byo(&[
            iface("wg1", &["10.9.0.2"]),
            iface("tailscale0", &["100.64.0.9"]),
        ])
        .unwrap();
        assert_eq!(found.ifname, "tailscale0");
    }

    #[test]
    fn verdict_line_prefers_concrete_reachability() {
        let mut status = NetStatus {
            class: NetClass::NatNoIpv6,
            ipv6_global: None,
            ipv4_source: Some("192.168.1.20".parse().unwrap()),
            byo: None,
            headline: "Remote access isn't available from this network — everything else \
                       works. The box re-checks wherever it lives."
                .to_string(),
            guidance: String::new(),
        };
        assert_eq!(status.verdict_line(), status.headline);
        status.byo = Some(ByoTransport {
            ifname: "tailscale0".to_string(),
            addr: Some("100.64.0.9".parse().unwrap()),
        });
        assert_eq!(status.verdict_line(), "Available via your own network (tailscale0).");
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
