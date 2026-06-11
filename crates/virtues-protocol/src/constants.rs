//! Shared constants every component references.

/// Hostname the box advertises *inside* the WG tunnel. The daemon on the paired
/// device maps this to `wg.server_address` in its own resolver / Host header.
/// Never appears in public DNS.
pub const INTERNAL_HOST: &str = "virtues.internal";

/// Hostname the box advertises on the LAN via mDNS / Avahi. Used by daemons to
/// discover the box on first pair before any WG tunnel is up.
pub const LAN_HOST: &str = "virtues.local";

/// Port the box's HTTP server listens on. Reached by paired daemons over the
/// WG tunnel, and by the box's own browser via the loopback bypass. No TLS —
/// the WG tunnel provides encryption + authentication, and loopback is a
/// Secure Context per W3C.
///
/// Changing this is a hard wire-protocol break: every paired device has it
/// baked into its `http_port` field in the [`crate::PairingBundle`].
pub const INTERNAL_PORT: u16 = 8000;
