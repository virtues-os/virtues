//! Transport — the network seam between client and server.
//!
//! Two profiles, selected at compile time:
//!
//! - `real` (always present): production transport. The server binds a
//!   plain TCP listener; WireGuard runs at the OS network layer below
//!   us, and clients dial via `virtues.internal` through the tunnel.
//!   TLS uses a per-pair private CA provisioned at QR pairing (WS-2).
//!
//! - `dev_local` (feature `dev-transport`): development transport. The
//!   server binds loopback, clients dial `http://localhost:PORT`. No
//!   WG, no pairing, no `.internal`, no per-pair CA. The `dev-transport`
//!   feature is **never compiled into release binaries** — gating is at
//!   compile time, not runtime, so a misconfigured release cannot
//!   accidentally fall back to the dev path.
//!
//! WS-1 introduces the seam. WS-2 fills in WireGuard, pairing, and the
//! per-pair CA on the `real` profile.

use std::io;
use tokio::net::TcpListener;

/// Server-side transport: how the HTTP server obtains its listener.
///
/// Today both profiles return a plain `TcpListener`; the abstraction
/// exists so WS-2 can swap in TLS-terminating listeners on `real`
/// without churning every caller.
#[async_trait::async_trait]
pub trait ServerTransport: Send + Sync {
    async fn bind(&self) -> io::Result<TcpListener>;
    fn describe(&self) -> String;
}

pub mod real;
pub use real::RealServerTransport;

// Box-held-cert TLS termination for the LAN-direct listener (relay model).
#[cfg(feature = "tls")]
pub mod tls;

#[cfg(feature = "dev-transport")]
pub mod dev_local;
#[cfg(feature = "dev-transport")]
pub use dev_local::DevLocalServerTransport;
