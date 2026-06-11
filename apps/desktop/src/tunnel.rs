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

// Linux is the only wired implementation; macOS/Windows are sketched stubs
// (the WG data plane — utun/wintun + GotaTun) that bail with a clear
// "use --no-tunnel + BYO" message until implemented.
#[cfg(target_os = "linux")]
pub use self::linux::TunnelHandle;
#[cfg(target_os = "macos")]
pub use self::macos::TunnelHandle;
#[cfg(target_os = "windows")]
pub use self::windows::TunnelHandle;

// Fallback for any other target (BSD, etc.) — a unit handle so the proxy-only
// `--no-tunnel` path still compiles everywhere.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub struct TunnelHandle;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
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
    #[cfg(target_os = "macos")]
    {
        return macos::start(bundle).await;
    }
    #[cfg(target_os = "windows")]
    {
        return windows::start(bundle).await;
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = bundle;
        anyhow::bail!(
            "tunnel: no built-in WireGuard implementation for this platform. \
             Reach the box over a BYO transport: `virtues-client up --no-tunnel \
             --upstream <addr:port>`. See docs/byo-networking.md."
        )
    }
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
