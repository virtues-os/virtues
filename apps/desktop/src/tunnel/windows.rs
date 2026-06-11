//! Windows WireGuard tunnel — **STUB** (planned, not yet implemented).
//!
//! The WG *data plane* for Windows: same role as [`super::linux`], with a
//! `wintun` adapter instead of the Linux `tun` crate. Nothing to do with the
//! (deleted) hole-punch coordinator — purely "bring up a userspace WireGuard
//! tunnel to the box."
//!
//! ## Planned implementation
//!
//! - **GotaTun** drives the WG state machine (same as Linux).
//! - **wintun**: load WireGuard's signed `wintun.dll` (bundle the upstream
//!   signed binary — do NOT ship our own driver) and create an adapter.
//! - **Routing**: configure the adapter's IPv6 address + a route to the box's
//!   address via the Windows IP Helper API (`netsh interface ipv6 ...` or the
//!   `iphlpapi` bindings) — the analogue of the `ip` commands in `linux.rs`.
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
        "Windows WireGuard tunnel isn't implemented yet (planned: wintun + GotaTun). \
         For now reach your box over your own transport: \
         `virtues-client up --no-tunnel --upstream <addr:port>` \
         (Tailscale / VPS / direct IPv6). See docs/byo-networking.md."
    )
}
