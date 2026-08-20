//! Which addresses the box is willing to fetch.
//!
//! This is the first place in the system where the box makes an HTTP request to
//! an arbitrary **user-supplied** URL, and that is a different risk from every
//! outbound call we already make (atlas, the gateway, the sidecars — all fixed
//! hosts we chose). A bookmark URL is attacker-influenced in the ordinary case:
//! browser bookmark files sync wholesale, and a developer's bookmarks contain
//! `http://localhost:3000` as a matter of course.
//!
//! Fetching those would be both a data problem (a dev server's HTML is not a
//! saved article) and a security one. The box runs Postgres on 5432, the QNN
//! sidecars on 18181/18182, and its own API on loopback; `169.254.169.254` is
//! the cloud metadata endpoint on the EC2 hosts where atlas and virtues-api
//! live. A fetch of any of those, whose body then lands in a database row and
//! gets summarized by a model, is an exfiltration path.
//!
//! So: public unicast destinations only, checked per redirect hop.
//!
//! **Known residual risk, stated rather than papered over.** We vet the
//! resolved addresses and then let reqwest resolve the name again when it
//! connects, so a DNS entry that changes between those two moments (classic
//! rebinding) is not closed by this guard. Closing it properly means pinning
//! the connection to the vetted IP. That is worth doing if this path ever takes
//! URLs from a less trusted place than the user's own saves; it is deliberately
//! not done here, because the mitigation costs a client rebuild per fetch and
//! the current threat is overwhelmingly "the user bookmarked their own dev
//! server", not an attacker racing our resolver.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::error::{Error, Result};

/// Carrier-grade NAT space (100.64.0.0/10). `Ipv4Addr::is_shared` is unstable,
/// so it is spelled out.
fn is_shared_v4(ip: &Ipv4Addr) -> bool {
    ip.octets()[0] == 100 && (ip.octets()[1] & 0b1100_0000) == 0b0100_0000
}

/// Benchmarking space (198.18.0.0/15). `is_benchmarking` is unstable too.
fn is_benchmarking_v4(ip: &Ipv4Addr) -> bool {
    ip.octets()[0] == 198 && (ip.octets()[1] & 0xfe) == 18
}

/// Unique local addresses (fc00::/7) — the v6 equivalent of RFC1918.
fn is_unique_local_v6(ip: &Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

/// Link-local unicast (fe80::/10), which includes the v6 metadata address.
fn is_link_local_v6(ip: &Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

/// Is this an address on the public internet?
///
/// Everything not provably public is refused — the default is "no", so a range
/// nobody thought about fails closed.
pub fn is_public(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_multicast()
                || v4.is_unspecified()
                || is_shared_v4(v4)
                || is_benchmarking_v4(v4)
                // 0.0.0.0/8 and the 240/4 reserved block.
                || v4.octets()[0] == 0
                || v4.octets()[0] >= 240)
        }
        IpAddr::V6(v6) => {
            // A v4-mapped v6 address is a v4 address wearing a hat; judge the
            // address it actually reaches, or ::ffff:127.0.0.1 walks straight
            // through the v6 arm.
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_public(&IpAddr::V4(mapped));
            }
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || is_unique_local_v6(v6)
                || is_link_local_v6(v6))
        }
    }
}

/// Resolve `host:port` and return the addresses, or refuse if any of them is
/// not public.
///
/// Refusing when *any* resolved address is private — rather than filtering down
/// to the public ones — is deliberate. A name that answers with both a public
/// and a loopback address is not a site we want to fetch under a race; it is a
/// name behaving oddly, and the honest response is to decline.
pub async fn resolve_public(host: &str, port: u16) -> Result<Vec<SocketAddr>> {
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| Error::Network(format!("cannot resolve {host}: {e}")))?
        .collect();

    if addrs.is_empty() {
        return Err(Error::Network(format!("{host} resolved to no addresses")));
    }
    if let Some(bad) = addrs.iter().find(|a| !is_public(&a.ip())) {
        return Err(Error::InvalidInput(format!(
            "refusing to fetch {host}: resolves to the non-public address {}",
            bad.ip()
        )));
    }
    Ok(addrs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn refuses_the_addresses_that_matter() {
        // Loopback: Postgres, the sidecars, and our own API live here.
        assert!(!is_public(&v4("127.0.0.1")));
        // Cloud metadata — the reason link-local is not merely tidy.
        assert!(!is_public(&v4("169.254.169.254")));
        // RFC1918, the LAN the box sits on.
        assert!(!is_public(&v4("192.168.1.10")));
        assert!(!is_public(&v4("10.0.0.5")));
        assert!(!is_public(&v4("172.16.0.1")));
        // CGNAT and benchmarking space.
        assert!(!is_public(&v4("100.64.0.1")));
        assert!(!is_public(&v4("198.18.0.1")));
        assert!(!is_public(&v4("0.0.0.0")));
    }

    #[test]
    fn allows_ordinary_public_addresses() {
        assert!(is_public(&v4("1.1.1.1")));
        assert!(is_public(&v4("93.184.216.34")));
        assert!(is_public(&"2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn v4_mapped_v6_cannot_smuggle_loopback() {
        // The bug this exists to prevent: ::ffff:127.0.0.1 is v6-shaped but
        // reaches v4 loopback, so judging it by the v6 rules would pass it.
        assert!(!is_public(&"::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_public(&"::ffff:169.254.169.254".parse().unwrap()));
        assert!(is_public(&"::ffff:1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn refuses_v6_private_ranges() {
        assert!(!is_public(&"::1".parse().unwrap()));
        assert!(!is_public(&"fd00::1".parse().unwrap()));
        assert!(!is_public(&"fe80::1".parse().unwrap()));
    }

    #[tokio::test]
    async fn resolve_public_refuses_localhost() {
        let err = resolve_public("localhost", 80).await.unwrap_err();
        assert!(
            err.to_string().contains("non-public"),
            "expected a non-public refusal, got: {err}"
        );
    }
}
