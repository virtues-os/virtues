//! Radio-hygiene counters — the on-device A/B harness for the battery work.
//!
//! Every number here answers "is the radio actually sleeping?": drains that
//! dialed, cold endpoint builds, bytes shipped, parks. Persisted as a tiny JSON
//! file in `virtues_dir()` so relaunches accumulate; the device screen reads a
//! snapshot via the `radio_stats` command. Best-effort by design — a failed
//! read/write loses a counter bump, never a record.

use std::collections::BTreeMap;
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
  /// Unix seconds the box last acked a batch, per stream. The widget's "up to
  /// date at" time; a stream that never delivered is absent.
  #[serde(default)]
  pub delivered: BTreeMap<String, u64>,
  /// Unix seconds the box last answered anything, an ack or a "skipped"
  /// (busy with an earlier batch; the rows retry). Answering is reachable.
  #[serde(default)]
  pub last_contact_at: Option<u64>,
  /// Set at the first failed connection (a client that would not build, a
  /// request that got no answer) and cleared by the next answer. A box that
  /// answers "skipped" is busy, not unreachable, so it never sets this.
  #[serde(default)]
  pub unreachable_since: Option<u64>,
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

/// The box answered: reachable, whatever it said.
pub(crate) fn note_contact() {
  bump(|s| {
    s.last_contact_at = Some(now_secs());
    s.unreachable_since = None;
  });
}

/// No answer at all. Keeps the first failure's time, so the widget can say
/// how long the box has been out of reach.
pub(crate) fn note_unreachable() {
  bump(|s| {
    if s.unreachable_since.is_none() {
      s.unreachable_since = Some(now_secs());
    }
  });
}

pub(crate) fn now_secs() -> u64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(0)
}
