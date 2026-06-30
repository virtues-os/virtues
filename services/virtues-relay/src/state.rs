//! Shared relay state, cloned into every accept task.

use dashmap::DashMap;
use std::sync::Arc;
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

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub registry: Registry,
    pub pending: Pending,
    pub meter: Meter,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            registry: Registry::new(),
            pending: Arc::new(DashMap::new()),
            meter: Meter::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Meter;

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
