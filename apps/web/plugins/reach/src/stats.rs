//! Radio-hygiene counters — the on-device A/B harness for the battery work.
//!
//! Every number here answers "is the radio actually sleeping?": drains that
//! dialed, cold endpoint builds, bytes shipped, parks. Persisted as a tiny JSON
//! file in `virtues_dir()` so relaunches accumulate; the device screen reads a
//! snapshot via the `radio_stats` command. Best-effort by design — a failed
//! read/write loses a counter bump, never a record.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RadioStats {
  /// Drain passes that ran against a client (i.e. touched the radio).
  pub drains: u64,
  /// Cold endpoint builds — each is a full fresh dial (bind + relay + QUIC).
  pub dials: u64,
  /// Records delivered (acked by the box).
  pub records: u64,
  /// Request-body bytes shipped.
  pub bytes: u64,
  /// Times the endpoint was parked (torn down so the radio can idle).
  pub parks: u64,
  /// Unix seconds of the last completed drain pass.
  pub last_drain_at: Option<u64>,
}

static STATS: Mutex<Option<RadioStats>> = Mutex::new(None);

fn path() -> std::path::PathBuf {
  crate::virtues_dir().join("radio_stats.json")
}

fn load() -> RadioStats {
  std::fs::read_to_string(path())
    .ok()
    .and_then(|s| serde_json::from_str(&s).ok())
    .unwrap_or_default()
}

/// Apply a mutation and persist. Cheap (a <200-byte file at drain cadence).
pub(crate) fn bump(f: impl FnOnce(&mut RadioStats)) {
  let Ok(mut guard) = STATS.lock() else { return };
  let stats = guard.get_or_insert_with(load);
  f(stats);
  if let Ok(json) = serde_json::to_string(stats) {
    let _ = std::fs::write(path(), json);
  }
}

pub(crate) fn snapshot() -> RadioStats {
  let Ok(mut guard) = STATS.lock() else {
    return RadioStats::default();
  };
  guard.get_or_insert_with(load).clone()
}

pub(crate) fn now_secs() -> u64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(0)
}
