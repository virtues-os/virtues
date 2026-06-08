//! Behavioral abuse blocklist (WS-6b).
//!
//! Keyed on the anonymous `bearer_hash` — never a customer (this service has
//! no customer link, by construction). The in-memory map is the hot path; the
//! `blocklist` table is a restart snapshot, reloaded on boot. Blocks are
//! TTL'd: a block is a cooldown, not a permanent ban, because usage here is
//! anonymous and there is no appeal channel — a permanent false-positive would
//! be unrecoverable for a paying user.
//!
//! Two ways a bearer lands here:
//!   - **manual** (`POST /internal/block`): ops flags a hash seen abusing the
//!     service in virtues-api's own logs. Atlas is never involved.
//!   - **rate** (auto): a single bearer exceeding a generous per-minute request
//!     ceiling. **Enforcement is OFF by default** (`BLOCKLIST_RATE_AUTOBLOCK`)
//!     because a bearer is *per-home-server* — it aggregates background jobs,
//!     chat, parallel tool calls, and per-keystroke autocomplete onto one
//!     counter, and a false positive would lock out a whole paying household
//!     for the cooldown with no appeal channel. Cost is already capped by the
//!     daily budget, so the rate ceiling only guards request-rate DoS. Until
//!     we've watched real peak rates (via the flagged watchlist + the
//!     `/internal/blocklist` introspection endpoint), exceeding the ceiling
//!     only *logs and flags* — it does not block.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use dashmap::DashMap;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Reason codes stored in `blocklist.reason_code`.
pub const REASON_MANUAL: i16 = 1;
pub const REASON_RATE_ABUSE: i16 = 2;

const RATE_WINDOW: Duration = Duration::from_secs(60);
/// Keep a flagged (over-ceiling) bearer in the watchlist this long after its
/// last trip, so the introspection endpoint shows recent signal without
/// growing unbounded.
const FLAG_RETENTION: ChronoDuration = ChronoDuration::hours(6);

struct RateWindow {
    started: Instant,
    count: u32,
}

/// A bearer that has exceeded the rate ceiling at least once — the
/// "would-block" signal, recorded whether or not enforcement is on.
struct Flag {
    /// How many windows this bearer has tripped the ceiling.
    trips: u32,
    /// Highest count seen in a single window.
    peak: u32,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
}

struct Inner {
    /// bearer_hash → block expiry. Presence (with a future expiry) = blocked.
    blocked: DashMap<Vec<u8>, DateTime<Utc>>,
    /// bearer_hash → fixed-window request counter (auto rate-block input).
    hits: DashMap<Vec<u8>, RateWindow>,
    /// bearer_hash → over-ceiling watchlist (observability, even when
    /// enforcement is off).
    flagged: DashMap<Vec<u8>, Flag>,
    /// Max requests per `RATE_WINDOW` before a bearer is flagged (and, if
    /// enforcement is on, blocked).
    rate_limit: u32,
    /// How long an auto rate-block lasts.
    block_ttl: ChronoDuration,
    /// Whether exceeding the ceiling actually blocks. Off by default —
    /// observe-only until real peak rates are known.
    autoblock: bool,
}

#[derive(Clone)]
pub struct Blocklist {
    inner: Arc<Inner>,
}

impl Blocklist {
    pub fn from_env() -> Self {
        // Generous default: 600/min (~10/s sustained). Real clients burst far
        // below this; only a runaway loop or a DoS trips it.
        let rate_limit = std::env::var("BLOCKLIST_RATE_LIMIT_PER_MIN")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(600);
        let block_ttl_secs: i64 = std::env::var("BLOCKLIST_BLOCK_TTL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(900); // 15 min cooldown

        // Enforcement is opt-in. Default OFF: observe + flag only.
        let autoblock = matches!(
            std::env::var("BLOCKLIST_RATE_AUTOBLOCK").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE")
        );
        if autoblock {
            tracing::warn!(rate_limit, "blocklist rate auto-block ENABLED");
        } else {
            tracing::info!(rate_limit, "blocklist rate auto-block disabled (observe-only)");
        }

        Self {
            inner: Arc::new(Inner {
                blocked: DashMap::new(),
                hits: DashMap::new(),
                flagged: DashMap::new(),
                rate_limit,
                block_ttl: ChronoDuration::seconds(block_ttl_secs),
                autoblock,
            }),
        }
    }

    /// Whether exceeding the rate ceiling actually blocks (vs observe-only).
    pub fn autoblock_enabled(&self) -> bool {
        self.inner.autoblock
    }

    /// Load the non-expired block snapshot from the table on startup.
    pub async fn load_snapshot(&self, pool: &PgPool) {
        let rows: Vec<(Vec<u8>, DateTime<Utc>)> = match sqlx::query_as(
            "SELECT bearer_hash, expires_at FROM blocklist WHERE expires_at > now()",
        )
        .fetch_all(pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("blocklist snapshot load failed: {e:#}");
                return;
            }
        };
        let n = rows.len();
        for (hash, exp) in rows {
            self.inner.blocked.insert(hash, exp);
        }
        if n > 0 {
            tracing::info!(loaded = n, "blocklist snapshot loaded");
        }
    }

    /// Is this bearer currently blocked? Lazily evicts expired entries.
    pub fn is_blocked(&self, hash: &[u8]) -> bool {
        if let Some(entry) = self.inner.blocked.get(hash) {
            if *entry.value() > Utc::now() {
                return true;
            }
        }
        // Expired (or absent) — drop the stale entry if present.
        self.inner.blocked.remove(hash);
        false
    }

    /// Record a request against the per-bearer rate window. Returns true if the
    /// bearer is over the ceiling this window. Over-ceiling events are flagged
    /// (and logged) here regardless of whether enforcement is on — the caller
    /// decides whether to actually block via [`autoblock_enabled`].
    pub fn note_request(&self, hash: &[u8]) -> bool {
        let now = Instant::now();
        let count = {
            let mut entry = self
                .inner
                .hits
                .entry(hash.to_vec())
                .or_insert(RateWindow { started: now, count: 0 });
            if now.duration_since(entry.started) > RATE_WINDOW {
                entry.started = now;
                entry.count = 0;
            }
            entry.count += 1;
            entry.count
        };

        if count <= self.inner.rate_limit {
            return false;
        }

        // Over the ceiling — record on the watchlist (observability).
        let utc = Utc::now();
        self.inner
            .flagged
            .entry(hash.to_vec())
            .and_modify(|f| {
                f.trips += 1;
                f.peak = f.peak.max(count);
                f.last_seen = utc;
            })
            .or_insert(Flag {
                trips: 1,
                peak: count,
                first_seen: utc,
                last_seen: utc,
            });
        tracing::warn!(
            count,
            limit = self.inner.rate_limit,
            autoblock = self.inner.autoblock,
            "bearer over rate ceiling"
        );
        true
    }

    /// Introspection snapshot for `GET /internal/blocklist`. Hashes are
    /// hex-encoded. Lets us watch the would-block signal (flagged) and the
    /// active blocks without enforcing.
    pub fn snapshot(&self) -> Value {
        let now = Utc::now();
        let blocked: Vec<Value> = self
            .inner
            .blocked
            .iter()
            .filter(|e| *e.value() > now)
            .map(|e| {
                json!({ "bearer_hash": hex(e.key()), "expires_at": e.value() })
            })
            .collect();
        let flagged: Vec<Value> = self
            .inner
            .flagged
            .iter()
            .map(|e| {
                let f = e.value();
                json!({
                    "bearer_hash": hex(e.key()),
                    "trips": f.trips,
                    "peak": f.peak,
                    "first_seen": f.first_seen,
                    "last_seen": f.last_seen,
                })
            })
            .collect();
        json!({
            "autoblock_enabled": self.inner.autoblock,
            "rate_limit_per_min": self.inner.rate_limit,
            "blocked": blocked,
            "flagged": flagged,
        })
    }

    /// Block a bearer: persist to the table (restart snapshot) and add to the
    /// in-memory hot map. `ttl` overrides the default rate-block cooldown when
    /// provided (manual blocks can set their own).
    pub async fn block(
        &self,
        pool: &PgPool,
        hash: &[u8],
        reason_code: i16,
        ttl: Option<ChronoDuration>,
    ) {
        let expires_at = Utc::now() + ttl.unwrap_or(self.inner.block_ttl);
        if let Err(e) = sqlx::query(
            "INSERT INTO blocklist (bearer_hash, reason_code, expires_at) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (bearer_hash) DO UPDATE \
             SET reason_code = $2, blocked_at = now(), expires_at = $3",
        )
        .bind(hash)
        .bind(reason_code)
        .bind(expires_at)
        .execute(pool)
        .await
        {
            tracing::warn!("blocklist persist failed: {e:#}");
        }
        self.inner.blocked.insert(hash.to_vec(), expires_at);
    }

    /// Lift a block (manual unblock). Removes from table + memory.
    pub async fn unblock(&self, pool: &PgPool, hash: &[u8]) {
        if let Err(e) = sqlx::query("DELETE FROM blocklist WHERE bearer_hash = $1")
            .bind(hash)
            .execute(pool)
            .await
        {
            tracing::warn!("blocklist unblock failed: {e:#}");
        }
        self.inner.blocked.remove(hash);
    }

    /// Spawn a periodic in-memory pruner: drops expired blocks and stale rate
    /// windows so the maps don't grow with churned bearers. (DB rows are swept
    /// by `sweeper.rs`.)
    pub fn spawn_pruner(&self) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(300));
            loop {
                tick.tick().await;
                let now = Utc::now();
                inner.blocked.retain(|_, exp| *exp > now);
                inner.flagged.retain(|_, f| now - f.last_seen <= FLAG_RETENTION);
                let cutoff = Instant::now();
                inner
                    .hits
                    .retain(|_, w| cutoff.duration_since(w.started) <= RATE_WINDOW);
            }
        });
    }
}

/// Lowercase hex (no `hex` crate dependency in this service).
fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}
