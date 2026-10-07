//! The upload half of the iOS widget's snapshot.
//!
//! The Virtues widget (gen/apple/VirtuesControls) runs in its own process and
//! cannot open this outbox or reach the box, so after every drain pass, and at
//! most every 30 s on enqueue, this writes `uploads.json` into the App Group
//! container: per stream, how much is queued, how much has failed a send, the
//! oldest queued row, and when the box last acked that stream; and since when
//! the box has given no answer at all, if it has not. The widget reads it on
//! its own refresh schedule; nothing here wakes it.
//!
//! Best-effort like `stats`: a failed write loses one snapshot, never a record.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::json;
use virtues_reach_client::outbox;

static SHARED_DIR: OnceLock<PathBuf> = OnceLock::new();
static LAST_WRITE: Mutex<Option<Instant>> = Mutex::new(None);

/// Enqueues come in bursts (a location fix every few seconds in precise
/// mode); one snapshot per this long is plenty for a widget.
const MIN_GAP: Duration = Duration::from_secs(30);

pub(crate) fn set_shared_dir(dir: PathBuf) {
  let _ = SHARED_DIR.set(dir);
}

/// Write the snapshot. `force` skips the throttle (after a drain pass or a
/// failed dial, when the numbers just changed for a reason).
pub(crate) fn publish(force: bool) {
  let Some(dir) = SHARED_DIR.get() else { return };
  if !force && LAST_WRITE.lock().is_ok_and(|l| l.is_some_and(|t| t.elapsed() < MIN_GAP)) {
    return;
  }
  // Fails until the outbox is open (Swift hands over the folder first, at
  // plugin registration); the throttle is only spent by a write that lands.
  let Ok(queued) = outbox::stats_all() else { return };
  let stats = crate::stats::snapshot();

  // Every stream that has rows or has ever delivered. A collector that was
  // never turned on has neither, so it never shows up as "up to date".
  let mut streams: BTreeMap<String, serde_json::Value> = BTreeMap::new();
  for (name, at) in &stats.delivered {
    streams.insert(
      name.clone(),
      json!({ "stream": name, "queued": 0, "failing": 0, "oldest": 0, "delivered": at }),
    );
  }
  for (name, q) in queued {
    let entry = streams
      .entry(name.clone())
      .or_insert_with(|| json!({ "stream": name, "delivered": null }));
    entry["queued"] = json!(q.queued);
    entry["failing"] = json!(q.failing);
    entry["oldest"] = json!(q.oldest);
  }
  let doc = json!({
    "at": crate::stats::now_secs(),
    "unreachableSince": stats.unreachable_since,
    "streams": streams.into_values().collect::<Vec<_>>(),
  });

  // Write then rename, so the widget never reads half a file.
  let tmp = dir.join("uploads.json.tmp");
  if std::fs::write(&tmp, doc.to_string()).is_ok()
    && std::fs::rename(&tmp, dir.join("uploads.json")).is_ok()
  {
    if let Ok(mut last) = LAST_WRITE.lock() {
      *last = Some(Instant::now());
    }
  }
}
