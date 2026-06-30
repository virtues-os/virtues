//! Relay configuration, read from the environment at boot.

use std::time::Duration;

/// Relay keepalive cadence on the control connection (relay → box `Ping`).
pub const KEEPALIVE: Duration = Duration::from_secs(25);
/// If a box sends no traffic (no `Pong`) within this window, it's declared dead
/// and its registry entry is evicted. Must exceed [`KEEPALIVE`].
pub const PONG_DEADLINE: Duration = Duration::from_secs(35);
/// How long an inbound client waits for the box to dial its work connection
/// before we give up and close the client.
pub const WORK_DEADLINE: Duration = Duration::from_secs(10);

pub struct Config {
    /// Browser/client-facing listener (TLS passthrough — peek SNI, splice
    /// ciphertext, never terminate). Production fronts this on TCP/443.
    pub client_addr: String,
    /// Box-facing listener: boxes dial out here (control + work connections).
    pub control_addr: String,
    /// Shared bearer a box must present to `Register` (v1 auth; blinded tokens
    /// in P3). Required in production; defaults to a dev value locally.
    pub token: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            client_addr: std::env::var("VIRTUES_RELAY_CLIENT_ADDR")
                .unwrap_or_else(|_| "[::]:8443".to_string()),
            control_addr: std::env::var("VIRTUES_RELAY_CONTROL_ADDR")
                .unwrap_or_else(|_| "[::]:9443".to_string()),
            token: std::env::var("VIRTUES_RELAY_TOKEN")
                .unwrap_or_else(|_| "dev-token".to_string()),
        }
    }
}
