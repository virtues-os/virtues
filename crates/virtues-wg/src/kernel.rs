//! Kernel WireGuard capability probe.
//!
//! Answers one question: *can this host bring up a kernel WireGuard interface?*
//! Stock Ubuntu/Debian kernels ship `CONFIG_WIREGUARD` and the answer is yes; a
//! stripped vendor kernel (NVIDIA Jetson/Tegra is the one we've hit) ships it
//! disabled and the answer is no — in which case `virtues-wireguard` can't bring
//! up `wg0` and remote access is unavailable until the module is supplied (see
//! `docs/jetson-wg.md`).
//!
//! The probe is definitive but needs `NET_ADMIN` (it actually creates + removes a
//! throwaway interface); without that privilege it can't tell, hence [`Unknown`].
//! Cross-platform: the enum is always available; the real probe is Linux-only and
//! everything else reports [`Unknown`].
//!
//! [`Unknown`]: WgSupport::Unknown

/// Whether this host can run kernel WireGuard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WgSupport {
    /// Kernel WireGuard works — a throwaway interface was created and removed.
    Supported,
    /// The kernel has no WireGuard support (module missing / not built in).
    Unsupported,
    /// Couldn't determine — no `NET_ADMIN` to run the probe, or non-Linux host.
    Unknown,
}

/// Probe kernel WireGuard support by creating and immediately removing a
/// throwaway interface. Definitive, but needs `NET_ADMIN`: without it the create
/// fails with `EPERM` and we return [`WgSupport::Unknown`] rather than guessing.
#[cfg(target_os = "linux")]
pub fn kernel_wg_supported() -> WgSupport {
    use defguard_wireguard_rs::{Kernel, WGApi, WireguardInterfaceApi};

    const PROBE_IF: &str = "vwgprobe0";

    let api = match WGApi::<Kernel>::new(PROBE_IF.to_string()) {
        Ok(api) => api,
        // Opening the netlink handle itself failed — can't tell why; don't guess.
        Err(_) => return WgSupport::Unknown,
    };

    match api.create_interface() {
        Ok(()) => {
            // Clean up the throwaway interface; ignore the (unlikely) error.
            let _ = api.remove_interface();
            WgSupport::Supported
        }
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            if msg.contains("exists") {
                // A leftover/racing probe interface — its existence *proves* the
                // kernel supports the wireguard type. Clean it up and report yes.
                let _ = api.remove_interface();
                WgSupport::Supported
            } else if msg.contains("permitted") || msg.contains("permission") || msg.contains("eperm")
            {
                // A permission error means the kernel *might* support WG — we just
                // lack the privilege to find out.
                WgSupport::Unknown
            } else {
                // No such device / unknown device type / not supported → no WG.
                WgSupport::Unsupported
            }
        }
    }
}

/// Non-Linux hosts (the macOS dev box) have no kernel WireGuard engine here.
#[cfg(not(target_os = "linux"))]
pub fn kernel_wg_supported() -> WgSupport {
    WgSupport::Unknown
}
