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
    /// (deny — limit exceeded).
    ///
    /// Before checking, we sweep the WHOLE map: slide each IP's window and drop
    /// any IP whose window is now empty. This bounds the map by the number of
    /// IPs *actively* pairing within the last WINDOW — not by every IP ever
    /// seen — so distinct source IPs coming and going can't grow it without
    /// limit. The sweep is O(n) but n is tiny (a handful of real pairs).
    pub fn check_and_record(&self, ip_key: &str) -> bool {
        let now = Instant::now();
        let cutoff = now - WINDOW;

        let mut map = self.0.lock().unwrap();

        // Slide every window and evict fully-expired IPs.
        map.retain(|_, attempts| {
            attempts.retain(|t| *t > cutoff);
            !attempts.is_empty()
        });

        // A newly-inserted entry has 0 attempts, so it's never denied here (it
        // gets a timestamp pushed below); the deny path always has a non-empty
        // Vec. So `or_default` can't strand an empty entry.
        let attempts = map.entry(ip_key.to_string()).or_default();
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

    #[test]
    fn map_does_not_retain_empty_entries() {
        let lim = PairRateLimiter::new();
        // One attempt creates one live entry.
        lim.check_and_record("10.0.0.1");
        assert_eq!(lim.0.lock().unwrap().len(), 1);
        // Force its timestamps to look expired by rewriting them past the window.
        {
            let mut map = lim.0.lock().unwrap();
            let v = map.get_mut("10.0.0.1").unwrap();
            *v = vec![Instant::now() - WINDOW - Duration::from_secs(1)];
        }
        // A check for a different IP sweeps the expired entry out.
        lim.check_and_record("10.0.0.2");
        let map = lim.0.lock().unwrap();
        assert!(!map.contains_key("10.0.0.1"), "expired IP should be evicted");
        assert_eq!(map.len(), 1, "only the active IP remains");
    }
}
