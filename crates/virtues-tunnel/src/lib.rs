//! # virtues-tunnel
//!
//! In-app **userspace** WireGuard for paired Virtues clients.
//!
//! The desktop daemon runs WireGuard over a real kernel/`utun` device. iOS
//! can't: a `NEPacketTunnelProvider` would seize the single system-VPN slot and
//! disable the user's iCloud Private Relay / Nord / etc. So this crate runs the
//! whole tunnel **inside the app process** — no system VPN, no entitlement, no
//! permission prompt, coexisting with any other VPN — and exposes a tiny
//! [`dial`](Tunnel::dial) API the app uses to issue plain HTTP to the box.
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
mod rendezvous;
mod tunnel;
mod wg;

// uniffi library mode: generates the FFI scaffolding consumed by the Swift
// bindings (no UDL file). Must be called exactly once in the crate root.
uniffi::setup_scaffolding!();

pub use keys::{generate_keypair, Keypair};
pub use rendezvous::{fetch_endpoint, EndpointBlob};
pub use tunnel::{Tunnel, TunnelStatus, TunnelStream};

// Re-export the shared bundle types so consumers (and the FFI) have one import.
pub use virtues_protocol::{spki_fingerprint, PairingBundle, RendezvousParams, WgParams};

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

    #[error("rendezvous: {0}")]
    Rendezvous(String),

    #[error("wireguard: {0}")]
    WireGuard(String),
}

impl From<std::io::Error> for TunnelError {
    fn from(e: std::io::Error) -> Self {
        TunnelError::Io(e.to_string())
    }
}
