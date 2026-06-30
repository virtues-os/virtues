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
/// Idle timeout for a spliced client↔box connection. The timer resets on any
/// byte movement, so a long-lived stream carrying periodic heartbeats lives
/// indefinitely — but a half-open connection (peer vanished with no FIN, e.g. a
/// NAT/middlebox blackhole) is reaped instead of pinning a task + two FDs forever.
pub const SPLICE_IDLE: Duration = Duration::from_secs(600);
/// Max time a freshly-accepted box connection has to send its hello line. Closes
/// a slowloris on the control/work port: connect, then send the hello one byte
/// at a time (or never) to pin a task + socket. The browser side is already
/// bounded by the SNI peek timeout; this is its box-side counterpart.
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
/// Max age of a control connection before the relay drops it, forcing the box to
/// re-register. The token is only checked at `Register`, so this is what makes
/// revocation bite within a bounded time: a revoked box, on its forced reconnect,
/// presents a now-stale (un-re-minted) token and is rejected. Kept below one
/// token bucket so re-verification happens at least once per bucket.
pub const MAX_CONN_AGE: Duration = Duration::from_secs(20 * 3600);

pub struct Config {
    /// Browser/client-facing listener (TLS passthrough — peek SNI, splice
    /// ciphertext, never terminate). Production fronts this on TCP/443.
    pub client_addr: String,
    /// Box-facing listener: boxes dial out here (control + work connections).
    pub control_addr: String,
    /// Per-SNI HMAC secret (`VIRTUES_RELAY_SECRET`). When set, a box must present
    /// `derive_token(secret, sni, bucket)` to `Register`, so a box can register only its
    /// own SNI — closing the cross-tenant hijack a flat shared bearer allows.
    /// **Strongly recommended in production.** When unset, the relay falls back
    /// to the shared [`Self::token`] bearer (dev/single-tenant).
    pub secret: Option<String>,
    /// Shared bearer fallback used only when [`Self::secret`] is unset (v1 dev
    /// auth; blinded tokens in P3).
    pub token: String,
}

impl Config {
    pub fn from_env() -> Self {
        let secret = std::env::var("VIRTUES_RELAY_SECRET")
            .ok()
            .filter(|s| !s.is_empty());
        let token = std::env::var("VIRTUES_RELAY_TOKEN")
            .ok()
            .filter(|s| !s.is_empty());

        // Fail closed. With neither a per-SNI secret nor an explicit shared
        // bearer, the relay would otherwise authenticate every box against a
        // source-known default token — letting anyone register any SNI and
        // intercept that box's inbound TLS (the exact cross-tenant hijack the
        // HMAC path exists to close). Refuse to boot in that state unless the
        // operator explicitly opts into the insecure dev fallback.
        let token = match (&secret, token) {
            // Per-SNI HMAC governs auth; the shared bearer is unused.
            (Some(_), _) => String::new(),
            (None, Some(t)) => {
                tracing::warn!(
                    "VIRTUES_RELAY_SECRET unset — using a flat shared bearer; set a secret \
                     for per-SNI HMAC auth in production"
                );
                t
            }
            (None, None) => {
                if std::env::var("VIRTUES_RELAY_ALLOW_INSECURE").is_ok() {
                    tracing::warn!(
                        "INSECURE: no VIRTUES_RELAY_SECRET/VIRTUES_RELAY_TOKEN set — accepting the \
                         well-known 'dev-token' for ANY SNI. Dev only."
                    );
                    "dev-token".to_string()
                } else {
                    eprintln!(
                        "FATAL: relay has no auth configured. Set VIRTUES_RELAY_SECRET \
                         (recommended) or VIRTUES_RELAY_TOKEN, or VIRTUES_RELAY_ALLOW_INSECURE=1 \
                         for local dev. Refusing to start with a default token."
                    );
                    std::process::exit(1);
                }
            }
        };

        Self {
            client_addr: std::env::var("VIRTUES_RELAY_CLIENT_ADDR")
                .unwrap_or_else(|_| "[::]:8443".to_string()),
            control_addr: std::env::var("VIRTUES_RELAY_CONTROL_ADDR")
                .unwrap_or_else(|_| "[::]:9443".to_string()),
            secret,
            token,
        }
    }
}
