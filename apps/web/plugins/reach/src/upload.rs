//! Upload coordinator.
//!
//! Hand-off from the native collectors is a **shared SQLite file**, not a
//! Swift→Rust call: the location plugin writes fixes to
//! `<AppSupport>/location_probe.sqlite`; this reads the unsent rows, batches them
//! into the box's ingest webhook over the warm iroh client, and marks them sent
//! only after the box durably acks. The box dedups on its own id, so a crash
//! between ack and mark re-sends harmlessly (at-least-once).

use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use rusqlite::Connection;
use serde_json::json;
use virtues_reach_client::{PairedBox, VirtuesIrohClient};

/// Max rows per batch — keeps a single request well under the ~512 KB ceiling.
const BATCH: usize = 500;

/// `<AppSupport>/location_probe.sqlite` — must match the Swift collector's path
/// (`FileManager.applicationSupportDirectory`).
fn location_db_path() -> PathBuf {
  let base = dirs::data_dir()
    .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support")))
    .unwrap_or_else(|| PathBuf::from("."));
  base.join("location_probe.sqlite")
}

struct Fix {
  id: i64,
  ts: String,
  lat: f64,
  lon: f64,
}

/// Drain unsent location fixes to the box. Returns how many were acked+marked.
pub async fn drain_location(client: &VirtuesIrohClient, rec: &PairedBox) -> Result<usize> {
  let path = location_db_path();
  if !path.exists() {
    return Ok(0);
  }

  let fixes = read_unsent(&path)?;
  if fixes.is_empty() {
    return Ok(0);
  }

  let action_id = rec
    .action_ids
    .get("ios_ingest")
    .or_else(|| rec.action_ids.values().next())
    .ok_or_else(|| anyhow!("no ingest action id in pairing — re-pair to fix"))?;
  let device_id = rec.device_id.clone().unwrap_or_default();

  let records: Vec<_> = fixes
    .iter()
    .map(|f| json!({ "timestamp": f.ts, "latitude": f.lat, "longitude": f.lon }))
    .collect();
  let body = json!({
    "source": "ios",
    "stream": "location",
    "device_id": device_id,
    "records": records,
  })
  .to_string();

  let raw = format!(
    "POST /webhook/{action_id} HTTP/1.1\r\nHost: box\r\nContent-Type: application/json\r\n\
     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
    body.len()
  );
  let resp = client.request(raw.as_bytes()).await?;
  let text = String::from_utf8_lossy(&resp);
  if !body_acks(&text) {
    bail!("box did not ack ingest: {}", text.lines().next().unwrap_or(""));
  }

  let ids: Vec<i64> = fixes.iter().map(|f| f.id).collect();
  mark_sent(&path, &ids)?;
  Ok(ids.len())
}

/// The box returns `{"status":"success"}` on a durable ingest. Anything else
/// (skipped/running/error) is retryable — leave the rows unsent.
fn body_acks(resp: &str) -> bool {
  let body = resp.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or(resp);
  body.contains("\"status\":\"success\"") || body.contains("\"status\": \"success\"")
}

fn open(path: &PathBuf) -> Result<Connection> {
  let conn = Connection::open(path)?;
  // WAL + a busy timeout so we coexist with the Swift writer.
  conn.pragma_update(None, "journal_mode", "WAL").ok();
  conn.pragma_update(None, "busy_timeout", 5000).ok();
  Ok(conn)
}

fn read_unsent(path: &PathBuf) -> Result<Vec<Fix>> {
  let conn = open(path)?;
  // The Swift collector owns the schema; add our `sent` flag if absent.
  let _ = conn.execute("ALTER TABLE rows ADD COLUMN sent INTEGER DEFAULT 0", []);
  let mut stmt = match conn.prepare(
    "SELECT id, ts, lat, lon FROM rows \
     WHERE sent = 0 AND source = 'update' ORDER BY id LIMIT ?1",
  ) {
    Ok(s) => s,
    // Table not created yet (collector never ran) — nothing to send.
    Err(_) => return Ok(Vec::new()),
  };
  let rows = stmt
    .query_map([BATCH as i64], |r| {
      Ok(Fix {
        id: r.get(0)?,
        ts: r.get(1)?,
        lat: r.get(2)?,
        lon: r.get(3)?,
      })
    })?
    .collect::<std::result::Result<Vec<_>, _>>()?;
  Ok(rows)
}

fn mark_sent(path: &PathBuf, ids: &[i64]) -> Result<()> {
  if ids.is_empty() {
    return Ok(());
  }
  let conn = open(path)?;
  let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
  let sql = format!("UPDATE rows SET sent = 1 WHERE id IN ({placeholders})");
  let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|i| i as &dyn rusqlite::ToSql).collect();
  conn.execute(&sql, params.as_slice())?;
  Ok(())
}
