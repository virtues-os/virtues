//! ULA address allocation for WG peers.
//!
//! The box owns a private ULA `/64` (`fd00:5654::/64`). The box itself is `::1`;
//! each paired device gets the next free `::N` (N ≥ 2) as its `/128`. These
//! addresses never appear on the public internet — they're the tunnel-internal
//! identities the device dials (`virtues.internal` → the box's `::1`).

use std::collections::HashSet;
use std::net::Ipv6Addr;

/// The box's ULA `/64` prefix (first four hextets). `fd00:5654::/64`.
const ULA_PREFIX: [u16; 4] = [0xfd00, 0x5654, 0, 0];
/// Host id of the box itself.
const SERVER_HOST_ID: u16 = 1;
/// Highest assignable host id (`::ffff` reserved as the broadcast-ish ceiling).
const MAX_HOST_ID: u16 = 0xfffe;

fn ula_addr(host_id: u16) -> Ipv6Addr {
    Ipv6Addr::new(
        ULA_PREFIX[0],
        ULA_PREFIX[1],
        ULA_PREFIX[2],
        ULA_PREFIX[3],
        0,
        0,
        0,
        host_id,
    )
}

/// The box's own WG address (`fd00:5654::1`) — the tunnel peer devices talk to,
/// and what `virtues.internal` resolves to.
pub fn server_address() -> Ipv6Addr {
    ula_addr(SERVER_HOST_ID)
}

/// The pool's network address (`fd00:5654::`) and prefix length (`64`).
///
/// The box installs a kernel route for this whole `/64` via `wg0` so that
/// in-tunnel *reply* traffic to any device address routes back through the
/// tunnel. WireGuard's per-peer `allowed-ips` only drive crypto-routing; they
/// don't add a kernel route, and the interface address is a `/128`, so without
/// this the kernel sends replies (e.g. an HTTP SYN-ACK to a device) out the
/// default/WAN route instead of `wg0`.
pub fn pool_network() -> Ipv6Addr {
    ula_addr(0)
}

/// Prefix length of the box's ULA pool (`/64`).
pub const POOL_PREFIX_LEN: u8 = 64;

/// The pool as a CIDR string (`fd00:5654::/64`) for `ip route` / config.
pub fn pool_cidr() -> String {
    format!("{}/{}", pool_network(), POOL_PREFIX_LEN)
}

/// Allocate the lowest free device address (`::2` and up) not already assigned.
/// `assigned` is the set of addresses currently handed out to paired devices.
/// Returns `None` only if the (enormous) `/64` host space is exhausted.
pub fn allocate(assigned: &[Ipv6Addr]) -> Option<Ipv6Addr> {
    let used: HashSet<u16> = assigned.iter().map(|a| a.segments()[7]).collect();
    (2..=MAX_HOST_ID)
        .find(|h| !used.contains(h))
        .map(ula_addr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn server_is_host_one() {
        assert_eq!(server_address(), Ipv6Addr::from_str("fd00:5654::1").unwrap());
    }

    #[test]
    fn first_device_is_two() {
        assert_eq!(
            allocate(&[]),
            Some(Ipv6Addr::from_str("fd00:5654::2").unwrap())
        );
    }

    #[test]
    fn allocates_next_free() {
        let a2 = Ipv6Addr::from_str("fd00:5654::2").unwrap();
        let a3 = Ipv6Addr::from_str("fd00:5654::3").unwrap();
        assert_eq!(allocate(&[a2]), Some(a3));
        assert_eq!(
            allocate(&[a2, a3]),
            Some(Ipv6Addr::from_str("fd00:5654::4").unwrap())
        );
    }

    #[test]
    fn reuses_gaps() {
        // ::3 freed (device removed) → next allocation fills the gap.
        let a2 = Ipv6Addr::from_str("fd00:5654::2").unwrap();
        let a4 = Ipv6Addr::from_str("fd00:5654::4").unwrap();
        assert_eq!(
            allocate(&[a2, a4]),
            Some(Ipv6Addr::from_str("fd00:5654::3").unwrap())
        );
    }
}
