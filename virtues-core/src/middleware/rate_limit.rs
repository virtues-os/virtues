//! Sliding-window rate limiting, keyed by whatever the caller counts by.
//!
//! Two users, two very different reasons:
//!
//! - **`pair_limiter`**, keyed by source IP. The pair token is a 6-char code
//!   from a 24-char alphabet (≈191M combos). That's unbrutable within the
//!   30-minute window ONLY if we cap attempts per IP. Without this, a LAN
//!   attacker can enumerate freely. This one is a security control.
//! - **`event_limiter`**, keyed by device id. A client reporting its own
//!   errors (`POST /api/events`) can get stuck in a render loop and report the
//!   same failure thousands of times a minute. journald rate-limits per unit,
//!   so an unbounded client would push the box into dropping OTHER lines —
//!   the feature would cause blindness instead of curing it. This one is a
//!   budget, not a defense; the device is already authenticated.
//!
//! Implementation: an in-memory sliding-window counter per key. No external
//! crate; a single `Mutex<HashMap>` is sufficient at these volumes. Entries
//! expire automatically — we purge stale windows on each check.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Pair: sliding window size — matches the pair-token TTL so a token can only
/// be guessed within the window it is valid.
const WINDOW: Duration = Duration::from_secs(30 * 60);

/// Pair: maximum attempts from a single IP within the window. 10 allows a
/// reasonable number of typos/retries without opening meaningful enumeration.
const MAX_ATTEMPTS: usize = 10;

/// Events: one minute, because that is the timescale a runaway client loops
/// on and the timescale journald's own rate limiting works over.
const EVENT_WINDOW: Duration = Duration::from_secs(60);

/// Events: per device, per minute. A real client flushing warnings sends a
/// handful; 120 is far above any honest burst (a page that fails to load might
/// report a dozen) and far below a volume that could crowd the journal.
const EVENT_MAX: usize = 120;

pub struct SlidingWindowLimiter(Mutex<HashMap<String, Vec<Instant>>>, Duration, usize);

impl SlidingWindowLimiter {
    fn new(window: Duration, max: usize) -> Self {
        Self(Mutex::new(HashMap::new()), window, max)
    }

    /// Record an attempt against `key` and return `true` (allow) or `false`
    /// (deny — limit exceeded). The key is whatever the caller counts by: a
    /// source IP for pairing, a device id for client reports.
    ///
    /// Before checking, we sweep the WHOLE map: slide each IP's window and drop
    /// any IP whose window is now empty. This bounds the map by the number of
    /// IPs *actively* pairing within the last WINDOW — not by every IP ever
    /// seen — so distinct source IPs coming and going can't grow it without
    /// limit. The sweep is O(n) but n is tiny (a handful of real pairs).
    pub fn check_and_record(&self, key: &str) -> bool {
        let now = Instant::now();
        let cutoff = now - self.1;

        let mut map = self.0.lock().unwrap();

        // Slide every window and evict fully-expired keys.
        map.retain(|_, attempts| {
            attempts.retain(|t| *t > cutoff);
            !attempts.is_empty()
        });

        // A newly-inserted entry has 0 attempts, so it's never denied here (it
        // gets a timestamp pushed below); the deny path always has a non-empty
        // Vec. So `or_default` can't strand an empty entry.
        let attempts = map.entry(key.to_string()).or_default();
        if attempts.len() >= self.2 {
            return false;
        }
        attempts.push(now);
        true
    }
}

static PAIR_LIMITER: OnceLock<SlidingWindowLimiter> = OnceLock::new();

/// The pair-consume limiter, keyed by source IP. See the module header.
pub fn pair_limiter() -> &'static SlidingWindowLimiter {
    PAIR_LIMITER.get_or_init(|| SlidingWindowLimiter::new(WINDOW, MAX_ATTEMPTS))
}

static EVENT_LIMITER: OnceLock<SlidingWindowLimiter> = OnceLock::new();

/// The client-report limiter, keyed by device id. See the module header.
pub fn event_limiter() -> &'static SlidingWindowLimiter {
    EVENT_LIMITER.get_or_init(|| SlidingWindowLimiter::new(EVENT_WINDOW, EVENT_MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_max_attempts() {
        let lim = SlidingWindowLimiter::new(WINDOW, MAX_ATTEMPTS);
        for _ in 0..MAX_ATTEMPTS {
            assert!(lim.check_and_record("1.2.3.4"));
        }
        assert!(!lim.check_and_record("1.2.3.4"));
    }

    #[test]
    fn different_ips_dont_share_budget() {
        let lim = SlidingWindowLimiter::new(WINDOW, MAX_ATTEMPTS);
        for _ in 0..MAX_ATTEMPTS {
            lim.check_and_record("10.0.0.1");
        }
        // Separate IP still has a full budget.
        assert!(lim.check_and_record("10.0.0.2"));
    }

    #[test]
    fn map_does_not_retain_empty_entries() {
        let lim = SlidingWindowLimiter::new(WINDOW, MAX_ATTEMPTS);
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
