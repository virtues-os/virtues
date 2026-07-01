//! Shared relay state, cloned into every accept task.

use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::config::Config;
use crate::registry::Registry;

/// Inbound clients waiting for their box to dial a work connection. Keyed by the
/// `conn_id` the relay sent in `OpenConn`; the work-connection handler hands the
/// raw `TcpStream` to the waiting client task via the oneshot.
pub type Pending = Arc<DashMap<Uuid, oneshot::Sender<TcpStream>>>;

/// Per-box byte counter. Blindness-safe: it's an aggregate *volume* per SNI (a
/// number, not a log of who/what/when), used for quota + billing. Reported to
/// virtues-api in aggregate; never a per-connection record.
#[derive(Clone, Default)]
pub struct Meter {
    totals: Arc<DashMap<String, u64>>,
}

impl Meter {
    pub fn add(&self, sni: &str, bytes: u64) {
        if bytes == 0 {
            return;
        }
        self.totals
            .entry(sni.to_string())
            .and_modify(|v| *v += bytes)
            .or_insert(bytes);
    }

    pub fn get(&self, sni: &str) -> u64 {
        self.totals.get(sni).map(|v| *v).unwrap_or(0)
    }

    /// Aggregate `(sni, total_bytes)` snapshot for periodic reporting/metrics.
    pub fn snapshot(&self) -> Vec<(String, u64)> {
        self.totals
            .iter()
            .map(|e| (e.key().clone(), *e.value()))
            .collect()
    }
}

/// Per-SNI abuse floor: a concurrent-connection cap and a new-connection rate
/// limit, both keyed on the **SNI** (the box's tunnel identity) rather than the
/// source IP. That's deliberate — inbound clients arrive from the relay's own
/// browsers behind shared CGNAT egress, so a per-source-IP limit would throttle
/// unrelated users sharing an IP. Per-SNI bounds the blast radius to a single
/// box: flooding one box's name can't exhaust the relay or starve other boxes.
///
/// Only reached for SNIs with a *registered* box (routing rejects unknown SNIs
/// earlier), so the keyset is bounded by the connected fleet, not by attacker
/// input — no unbounded map growth from random SNI floods.
#[derive(Clone, Default)]
pub struct Limits {
    /// Live concurrent client connections per SNI. Self-pruning: the entry is
    /// removed when its count returns to zero.
    inflight: Arc<DashMap<String, u32>>,
    /// Token-bucket rate state per SNI (new-connection throttle).
    rate: Arc<DashMap<String, RateState>>,
}

struct RateState {
    tokens: f64,
    last: Instant,
}

/// RAII slot in the per-SNI concurrent-connection cap. Decrements (and prunes the
/// entry at zero) on drop, so a slot is released however the connection ends.
pub struct InflightGuard {
    inflight: Arc<DashMap<String, u32>>,
    sni: String,
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        if let Some(mut e) = self.inflight.get_mut(&self.sni) {
            *e = e.saturating_sub(1);
            if *e == 0 {
                drop(e);
                // Remove only if still zero — a concurrent acquire may have raced.
                self.inflight.remove_if(&self.sni, |_, v| *v == 0);
            }
        }
    }
}

impl Limits {
    /// Reserve a concurrent-connection slot for `sni`, or `None` if at `max`.
    pub fn try_acquire(&self, sni: &str, max: u32) -> Option<InflightGuard> {
        let mut count = self.inflight.entry(sni.to_string()).or_insert(0);
        if *count >= max {
            return None;
        }
        *count += 1;
        drop(count);
        Some(InflightGuard {
            inflight: self.inflight.clone(),
            sni: sni.to_string(),
        })
    }

    /// Token-bucket admission for a new connection to `sni`: `burst` capacity
    /// refilling at `refill_per_sec`. Returns `false` when the bucket is empty.
    pub fn allow_rate(&self, sni: &str, burst: f64, refill_per_sec: f64) -> bool {
        let now = Instant::now();
        let mut st = self
            .rate
            .entry(sni.to_string())
            .or_insert(RateState { tokens: burst, last: now });
        let elapsed = now.saturating_duration_since(st.last).as_secs_f64();
        st.tokens = (st.tokens + elapsed * refill_per_sec).min(burst);
        st.last = now;
        if st.tokens >= 1.0 {
            st.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub registry: Registry,
    pub pending: Pending,
    pub meter: Meter,
    pub limits: Limits,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            registry: Registry::new(),
            pending: Arc::new(DashMap::new()),
            meter: Meter::default(),
            limits: Limits::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Limits, Meter};

    #[test]
    fn inflight_cap_blocks_at_max_and_releases_on_drop() {
        let l = Limits::default();
        let g1 = l.try_acquire("a.virtues.ch", 2);
        let g2 = l.try_acquire("a.virtues.ch", 2);
        assert!(g1.is_some() && g2.is_some(), "first two slots acquire");
        assert!(l.try_acquire("a.virtues.ch", 2).is_none(), "third blocked at cap");
        // A different SNI has its own budget.
        assert!(l.try_acquire("b.virtues.ch", 2).is_some());
        // Releasing a slot frees capacity again.
        drop(g1);
        assert!(l.try_acquire("a.virtues.ch", 2).is_some(), "slot freed on drop");
    }

    #[test]
    fn rate_limit_exhausts_burst_then_denies() {
        let l = Limits::default();
        // With a burst of 3 and no time elapsing, exactly 3 are admitted.
        assert!(l.allow_rate("a.virtues.ch", 3.0, 1.0));
        assert!(l.allow_rate("a.virtues.ch", 3.0, 1.0));
        assert!(l.allow_rate("a.virtues.ch", 3.0, 1.0));
        assert!(!l.allow_rate("a.virtues.ch", 3.0, 1.0), "4th denied — bucket empty");
        // Independent bucket per SNI.
        assert!(l.allow_rate("b.virtues.ch", 3.0, 1.0));
    }

    #[test]
    fn meter_accumulates_per_sni() {
        let m = Meter::default();
        m.add("a.boxes.virtues.com", 100);
        m.add("a.boxes.virtues.com", 50);
        m.add("b.boxes.virtues.com", 10);
        m.add("a.boxes.virtues.com", 0); // ignored
        assert_eq!(m.get("a.boxes.virtues.com"), 150);
        assert_eq!(m.get("b.boxes.virtues.com"), 10);
        assert_eq!(m.get("missing"), 0);
        let mut snap = m.snapshot();
        snap.sort();
        assert_eq!(
            snap,
            vec![
                ("a.boxes.virtues.com".to_string(), 150),
                ("b.boxes.virtues.com".to_string(), 10),
            ]
        );
    }
}
