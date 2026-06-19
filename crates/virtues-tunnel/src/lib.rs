//! # virtues-tunnel
//!
//! In-app **userspace** WireGuard — the **one client tunnel engine** for every
//! paired Virtues device (iOS today; desktop after the gotatun retirement).
//!
//! It runs the whole tunnel **inside the app process** (no kernel module, no
//! `utun`, no root, no system-VPN slot — coexists with the user's iCloud Private
//! Relay / other VPN) and exposes a tiny [`dial`](Tunnel::dial) API for issuing
//! plain HTTP to the box. iOS binds the FFI; desktop uses the native API
//! directly and bridges [`TunnelStream`] (via [`TunnelStream::into_split`]) to
//! its async localhost reverse proxy. This is why the previous desktop path
//! (a kernel/`utun` tunnel via gotatun, needing root) was retired: this engine
//! is userspace everywhere and avoids seizing iOS's single system-VPN slot.
//!
//! Architecture (the [onetun](https://github.com/aramperes/onetun) pattern):
//!
//! ```text
//!  app HTTP  ──>  Tunnel::dial(ip, port) ──> smoltcp TCP socket
//!                                              │  IP packets
//!                                     defguard_boringtun Tunn (Noise)
//!                                              │  encrypted datagrams
//!                                          UDP socket  ──> box [global v6]:51820
//! ```
//!
//! Inside the tunnel the box serves plain HTTP on its ULA (`internal_ip` /
//! `http_port` from the bundle) — WireGuard already provides confidentiality,
//! so no TLS is needed in-tunnel (same model as the desktop reverse proxy).
//!
//! The FFI surface (see `ffi`) is what the iOS XCFramework binds; Rust callers
//! (tests, a future desktop adoption) use the native API directly.

mod ffi;
mod keys;
mod netstack;
mod tunnel;
mod wg;

// uniffi library mode: generates the FFI scaffolding consumed by the Swift
// bindings (no UDL file). Must be called exactly once in the crate root.
uniffi::setup_scaffolding!();

pub use keys::{generate_keypair, Keypair};
pub use tunnel::{Tunnel, TunnelReadHalf, TunnelStatus, TunnelStream, TunnelWriteHalf};

// Re-export the shared bundle types so consumers (and the FFI) have one import.
pub use virtues_protocol::{spki_fingerprint, PairingBundle, WgParams};

/// Everything that can go wrong bringing up or using the tunnel.
#[derive(Debug, thiserror::Error)]
pub enum TunnelError {
    #[error("invalid key: {0}")]
    BadKey(String),

    #[error("invalid bundle: {0}")]
    BadBundle(String),

    #[error("network: {0}")]
    Io(String),

    #[error("handshake did not complete within timeout")]
    HandshakeTimeout,

    #[error("tunnel is not connected")]
    NotConnected,

    #[error("dial {addr} failed: {reason}")]
    Dial { addr: String, reason: String },

    #[error("wireguard: {0}")]
    WireGuard(String),
}

impl From<std::io::Error> for TunnelError {
    fn from(e: std::io::Error) -> Self {
        TunnelError::Io(e.to_string())
    }
}
