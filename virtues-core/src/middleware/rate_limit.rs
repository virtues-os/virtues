//! Per-IP rate limiter for the `/api/pair/consume` endpoint.
//!
//! The pair token is a 6-char code from a 24-char alphabet (≈191M combos).
//! That's unbrutable within the 30-minute window ONLY if we cap attempts per
//! IP. Without this, a LAN attacker can enumerate freely.
//!
//! Implementation: an in-memory sliding-window counter per source IP. No
//! external crate; a single `Mutex<HashMap>` is sufficient for an endpoint
//! that sees at most a handful of real pairs per day. Entries expire
//! automatically — we purge stale windows on each check.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Sliding window size — matches the pair-token TTL so a token can only be
/// guessed within the window it is valid.
const WINDOW: Duration = Duration::from_secs(30 * 60);

/// Maximum attempts from a single IP within the window. 10 allows a
/// reasonable number of typos/retries without opening meaningful enumeration.
const MAX_ATTEMPTS: usize = 10;

pub struct PairRateLimiter(Mutex<HashMap<String, Vec<Instant>>>);

impl PairRateLimiter {
    fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    /// Record an attempt from `ip_key` and return `true` (allow) or `false`
    /// (deny — limit exceeded). Purges timestamps older than WINDOW on every
    /// call so the map doesn't grow unboundedly.
    pub fn check_and_record(&self, ip_key: &str) -> bool {
        let now = Instant::now();
        let cutoff = now - WINDOW;

        let mut map = self.0.lock().unwrap();
        let attempts = map.entry(ip_key.to_string()).or_default();

        // Slide the window: discard timestamps that have expired.
        attempts.retain(|t| *t > cutoff);

        if attempts.len() >= MAX_ATTEMPTS {
            return false;
        }
        attempts.push(now);
        true
    }
}

static PAIR_LIMITER: OnceLock<PairRateLimiter> = OnceLock::new();

/// Returns the global `PairRateLimiter` instance, initialising it on first call.
pub fn pair_limiter() -> &'static PairRateLimiter {
    PAIR_LIMITER.get_or_init(PairRateLimiter::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_max_attempts() {
        let lim = PairRateLimiter::new();
        for _ in 0..MAX_ATTEMPTS {
            assert!(lim.check_and_record("1.2.3.4"));
        }
        assert!(!lim.check_and_record("1.2.3.4"));
    }

    #[test]
    fn different_ips_dont_share_budget() {
        let lim = PairRateLimiter::new();
        for _ in 0..MAX_ATTEMPTS {
            lim.check_and_record("10.0.0.1");
        }
        // Separate IP still has a full budget.
        assert!(lim.check_and_record("10.0.0.2"));
    }
}
