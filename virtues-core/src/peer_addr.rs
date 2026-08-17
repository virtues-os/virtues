//! Canonicalizing the address a request actually arrived from.
//!
//! Every "where is this caller standing" gate on the box — `middleware::auth`'s
//! loopback console trust, `api::display`'s box-local rule, `api::provision`'s
//! setup-AP rule — starts by looking at the peer address. On this box that
//! address is **not** what you would guess.
//!
//! The server binds `*:8000`, a dual-stack IPv6 socket. Linux accepts IPv4
//! connections on it and reports them as **IPv4-mapped IPv6** addresses:
//! `127.0.0.1` arrives as `::ffff:127.0.0.1`, and `10.42.0.169` arrives as
//! `::ffff:10.42.0.169`. `Ipv6Addr::is_loopback()` matches only `::1`, and a
//! `match` on `IpAddr` sends every one of these down the `V6` arm — so an
//! address that is plainly a v4 address in every other sense fails every v4
//! test written against it.
//!
//! Found on hardware 2026-08-10, and it had silently closed the door on the
//! entire appliance flow: `/api/provision/*` gates on the caller being inside
//! `10.42.0.0/24`, that check lived in the `IpAddr::V4` arm, and no phone on the
//! setup AP ever reached it. Every request 404'd, which is exactly what the gate
//! looks like when it is working correctly, so it read as "the box is fine, the
//! venue is bad" for days.
//!
//! Call [`canonical_ip`] before any comparison. It is the difference between a
//! gate that is closed to strangers and one that is closed to everyone.

use std::net::{IpAddr, SocketAddr};

/// Unwrap an IPv4-mapped IPv6 address to the IPv4 address it carries.
///
/// **Deliberately `to_ipv4_mapped` and not `to_ipv4`.** The latter also unwraps
/// *IPv4-compatible* addresses (`::a.b.c.d`), a deprecated format that is not
/// how any real client reaches us and that lets a caller dress an arbitrary v4
/// address in a v6 shape. Only the `::ffff:0:0/96` block is a genuine
/// dual-stack artifact, so only that block is unwrapped here.
pub fn canonical_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        },
        v4 => v4,
    }
}

/// [`canonical_ip`] for a peer socket address.
pub fn canonical_peer(peer: &SocketAddr) -> IpAddr {
    canonical_ip(peer.ip())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn v4_mapped_loopback_is_recognized_as_loopback() {
        // THE BUG. This is what `curl http://127.0.0.1:8000` looks like to a
        // process listening on `*:8000`, and it was refused as "not local".
        let mapped: IpAddr = "::ffff:127.0.0.1".parse().unwrap();
        assert!(!mapped.is_loopback(), "precondition: raw form is not loopback");
        assert!(canonical_ip(mapped).is_loopback());
    }

    #[test]
    fn v4_mapped_ap_client_lands_in_the_v4_arm() {
        // The severe case: a phone on the setup AP. Before canonicalizing, this
        // matched `IpAddr::V6` and never reached the 10.42.0.0/24 test at all.
        let mapped: IpAddr = "::ffff:10.42.0.169".parse().unwrap();
        assert!(matches!(mapped, IpAddr::V6(_)), "precondition");
        assert_eq!(canonical_ip(mapped), IpAddr::V4(Ipv4Addr::new(10, 42, 0, 169)));
    }

    #[test]
    fn real_v6_is_left_alone() {
        assert_eq!(canonical_ip("::1".parse().unwrap()), IpAddr::V6(Ipv6Addr::LOCALHOST));
        let global: IpAddr = "2603:8080:1500:1d00::1".parse().unwrap();
        assert_eq!(canonical_ip(global), global);
    }

    #[test]
    fn plain_v4_is_left_alone() {
        let v4: IpAddr = "192.168.1.44".parse().unwrap();
        assert_eq!(canonical_ip(v4), v4);
    }

    #[test]
    fn deprecated_v4_compatible_is_not_unwrapped() {
        // `::10.42.0.169` is IPv4-COMPATIBLE, not IPv4-mapped. Unwrapping it
        // would let a caller present an arbitrary v4 address in v6 clothing and
        // walk through a subnet gate. `to_ipv4` would; `to_ipv4_mapped` does not.
        let compat: IpAddr = "::10.42.0.169".parse().unwrap();
        assert!(matches!(canonical_ip(compat), IpAddr::V6(_)), "must stay v6");
    }
}
