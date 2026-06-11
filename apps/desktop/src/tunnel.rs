//! WireGuard tunnel manager.
//!
//! Brings up a userspace WG tunnel to the box using **GotaTun** (Mullvad's
//! actively-maintained boringtun fork — see [[remote-access-decision]]).
//!
//! Platform implementations live in `tunnel/{linux,macos,windows}.rs` and
//! are selected at compile time. `linux.rs` is wired up today; macOS and
//! Windows return a clear "platform pending" error so the caller fails
//! cleanly rather than half-starts a proxy with no tunnel behind it.

use anyhow::Result;
use virtues_protocol::PairingBundle;

#[cfg(target_os = "linux")]
pub use self::linux::TunnelHandle;

#[cfg(not(target_os = "linux"))]
pub struct TunnelHandle;

#[cfg(not(target_os = "linux"))]
impl TunnelHandle {
    pub async fn stop(self) {}
}

/// Bring the tunnel up. Returns once the WG state machine is running; the
/// caller is responsible for holding the returned [`TunnelHandle`] for the
/// lifetime of the daemon (dropping it tears the tunnel down).
pub async fn start(bundle: &PairingBundle) -> Result<TunnelHandle> {
    #[cfg(target_os = "linux")]
    {
        return linux::start(bundle).await;
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = bundle;
        anyhow::bail!(
            "tunnel: only Linux is supported in this build. macOS userspace \
             WG (via utun + GotaTun) and Windows (via wintun + GotaTun) are \
             the next platform milestones — code lives in `tunnel/macos.rs` \
             and `tunnel/windows.rs` once wired."
        )
    }
}

#[cfg(target_os = "linux")]
mod linux;
