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

/// Max concurrent inbound client connections per SNI (abuse floor, CGNAT-safe —
/// keyed on the box's tunnel identity, not source IP). Generous: a single box
/// legitimately fans out to many browser tabs + long-lived WS/SSE streams, so
/// this only trips on genuine flooding of one box's name.
pub const MAX_INFLIGHT_PER_SNI: u32 = 256;
/// New-connection token bucket per SNI: burst capacity + steady refill/sec.
/// Absorbs a normal page load's parallel connections while capping sustained
/// connect churn against one box.
pub const RATE_BURST_PER_SNI: f64 = 64.0;
pub const RATE_REFILL_PER_SEC: f64 = 32.0;

pub struct Config {
    /// Browser/client-facing listener (TLS passthrough — peek SNI, splice
    /// ciphertext, never terminate). Production fronts this on TCP/443.
    pub client_addr: String,
    /// Box-facing listener: boxes dial out here (control + work connections).
    pub control_addr: String,
    /// atlas's Ed25519 **public** key (`VIRTUES_RELAY_PUBLIC_KEY`, hex). When set,
    /// a box must present an atlas-signed `sign_token(sni, bucket)` at `Register`,
    /// which the relay *verifies* with this key — so a box can register only its
    /// own SNI, and the relay holds **nothing that can mint** (a compromise leaks
    /// only a public key). **Required in production.** When unset, the relay falls
    /// back to the shared [`Self::token`] bearer (dev/single-tenant only).
    pub public_key: Option<ed25519_dalek::VerifyingKey>,
    /// Previous atlas public key (`VIRTUES_RELAY_PUBLIC_KEY_PREV`, hex), accepted
    /// alongside [`Self::public_key`] during a **zero-downtime key rotation**: set
    /// it to the old public key while atlas switches to a new signing key, and the
    /// relay admits tokens signed by *either* until the fleet re-fetches. Clear it
    /// once rolled over (≥1 token-refresh interval). Non-secret, like all pubkeys.
    pub public_key_prev: Option<ed25519_dalek::VerifyingKey>,
    /// Shared bearer fallback used only when [`Self::public_key`] is unset (v1 dev
    /// auth; blinded tokens in P3).
    pub token: String,
}

/// Parse a hex Ed25519 public key from `name`, or `None` if unset/empty. A set
/// but malformed key is a fatal misconfig (exit) — never silently ignored, which
/// would drop the relay to the insecure fallback.
fn pubkey_from_env(name: &str) -> Option<ed25519_dalek::VerifyingKey> {
    let raw = std::env::var(name).ok().filter(|s| !s.is_empty())?;
    match virtues_protocol::relay::parse_verifying_key(&raw) {
        Some(k) => Some(k),
        None => {
            eprintln!(
                "FATAL: {name} is set but is not a valid hex-encoded 32-byte Ed25519 public key."
            );
            std::process::exit(1);
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let public_key = pubkey_from_env("VIRTUES_RELAY_PUBLIC_KEY");
        let public_key_prev = pubkey_from_env("VIRTUES_RELAY_PUBLIC_KEY_PREV");
        let token = std::env::var("VIRTUES_RELAY_TOKEN")
            .ok()
            .filter(|s| !s.is_empty());

        // Fail closed. With neither atlas's public key nor an explicit shared
        // bearer, the relay would otherwise authenticate every box against a
        // source-known default token — letting anyone register any SNI and
        // intercept that box's inbound TLS. Refuse to boot in that state unless
        // the operator explicitly opts into the insecure dev fallback.
        let token = match (&public_key, token) {
            // Signed-token verification governs auth; the shared bearer is unused.
            (Some(_), _) => String::new(),
            (None, Some(t)) => {
                tracing::warn!(
                    "VIRTUES_RELAY_PUBLIC_KEY unset — using a flat shared bearer; set the atlas \
                     public key for signed per-SNI auth in production"
                );
                t
            }
            (None, None) => {
                if std::env::var("VIRTUES_RELAY_ALLOW_INSECURE").is_ok() {
                    tracing::warn!(
                        "INSECURE: no VIRTUES_RELAY_PUBLIC_KEY/VIRTUES_RELAY_TOKEN set — accepting \
                         the well-known 'dev-token' for ANY SNI. Dev only."
                    );
                    "dev-token".to_string()
                } else {
                    eprintln!(
                        "FATAL: relay has no auth configured. Set VIRTUES_RELAY_PUBLIC_KEY \
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
            public_key,
            public_key_prev,
            token,
        }
    }
}
