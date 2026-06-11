//! macOS WireGuard tunnel — **STUB** (planned, not yet implemented).
//!
//! This is the WG *data plane* for macOS: the same role as
//! [`super::linux`], just with a macOS `utun` device instead of the Linux
//! `tun` crate. It has nothing to do with the (deleted) hole-punch
//! coordinator — it's purely "bring up a userspace WireGuard tunnel to the
//! box."
//!
//! ## Planned implementation
//!
//! - **GotaTun** drives the WG state machine (same as Linux).
//! - **utun**: create a `utun` interface (via `SYSPROTO_CONTROL` /
//!   `UTUN_CONTROL_NAME`, or a Network Extension `NEPacketTunnelProvider` for
//!   the App Store path). The CLI/dev path can open `/dev/utunN` directly with
//!   the right entitlement; the shipping app wraps it in a Network Extension.
//! - **Routing**: `route add -inet6 <server_addr> -interface utunN` +
//!   `ifconfig utunN inet6 <client_addr>` (the macOS analogue of the `ip`
//!   commands in `linux.rs`).
//!
//! Until this lands, reach the box over a BYO transport instead:
//! `virtues-client up --no-tunnel --upstream <addr:port>` (Tailscale / VPS /
//! direct IPv6) — see `docs/byo-networking.md`.

use anyhow::Result;
use virtues_protocol::PairingBundle;

/// Placeholder handle. Mirrors [`super::linux::TunnelHandle`]'s API so the
/// caller in `main.rs` is platform-agnostic.
pub struct TunnelHandle;

impl TunnelHandle {
    pub async fn stop(self) {}
}

pub async fn start(bundle: &PairingBundle) -> Result<TunnelHandle> {
    let _ = bundle;
    anyhow::bail!(
        "macOS WireGuard tunnel isn't implemented yet (planned: utun + GotaTun). \
         For now reach your box over your own transport: \
         `virtues-client up --no-tunnel --upstream <addr:port>` \
         (Tailscale / VPS / direct IPv6). See docs/byo-networking.md."
    )
}
