//! Durable local outbox for collector records.
//!
//! One SQLite table, **one row per record**, keyed on a deterministic
//! `source_stream_id`. This is the write-once delivery spine shared by every
//! collector on every platform: a native shim hands a record to [`enqueue`]
//! (synchronously — safe from a background OS callback), and an async drain
//! ([`claim_batch`] → POST → [`ack`]) ships it to the box.
//!
//! Durability by construction:
//! - **Idempotent enqueue** — the PK is deterministic, so `INSERT OR IGNORE`
//!   collapses a record enqueued twice into one row (no local dupes).
//! - **Delete-only-on-ack** — a row is removed only after the box durably acks;
//!   a crash between POST and delete re-sends the *same* id → the box dedups on
//!   `source_stream_id` → no dupes, no loss (at-least-once + idempotent).
//! - **Crash recovery** — in-flight rows carry `claimed_at`; [`reset_stale`]
//!   clears it on launch so an interrupted drain is retried.
//!
//! The whole old-app status machine (pending/uploading/failed/completed,
//! multi-tier cleanup) collapses to: a synced row is *deleted*; a failing row
//! is just `attempts>0` with a future `next_attempt_at`.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use serde_json::Value;
use uuid::Uuid;

/// Namespace for on-device stream-id derivation. Records without a natural id
/// (e.g. location fixes) get `UUIDv5(NS, device + stream + canonical-record)` —
/// distinct per sample (the record carries a sub-second timestamp) yet stable
/// across retries. We stamp it into the record's `id` so the box uses the same
/// value verbatim (its own hash fallback never has to run).
const NS: Uuid = Uuid::from_u128(0x1e5f9c72_3a41_4b8d_9f26_7c0a5e3d81b4);

static DB_PATH: OnceLock<PathBuf> = OnceLock::new();
/// Namespacing for derived ids + the platform ingest key. Re-settable (a
/// pre-pair init can be refreshed once the device id is known).
static DEVICE_ID: Mutex<String> = Mutex::new(String::new());
static INGEST_KEY: Mutex<String> = Mutex::new(String::new());

/// Backoff schedule (seconds) indexed by attempt count, capped at 5 min.
const BACKOFF_SECS: [i64; 6] = [0, 30, 60, 120, 240, 300];

/// A batch claimed for delivery: the ids to ack/nack, and the box-shaped records.
pub struct Claimed {
    pub ids: Vec<String>,
    pub records: Vec<Value>,
}

/// Per-stream queue health for the device screen.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxStats {
    pub queued: i64,
    pub failing: i64,
    /// Unix seconds of the oldest queued row, or 0 if empty.
    pub oldest: i64,
}

/// Open + migrate the outbox at `db_path`. Call once at launch, before any
/// enqueue. `device_id` namespaces derived ids; `ingest_key` is the platform's
/// ingest action name (`ios_ingest`).
pub fn init(db_path: impl AsRef<Path>, device_id: &str, ingest_key: &str) -> Result<()> {
    let path = db_path.as_ref().to_path_buf();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let conn = open(&path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS outbox (
            source_stream_id TEXT PRIMARY KEY,
            stream           TEXT NOT NULL,
            action_key       TEXT NOT NULL,
            payload          TEXT NOT NULL,
            created_at       INTEGER NOT NULL,
            attempts         INTEGER NOT NULL DEFAULT 0,
            next_attempt_at  INTEGER NOT NULL DEFAULT 0,
            claimed_at       INTEGER
         );
         CREATE INDEX IF NOT EXISTS idx_outbox_due ON outbox(stream, next_attempt_at);
         CREATE INDEX IF NOT EXISTS idx_outbox_created ON outbox(stream, created_at);",
    )?;
    let _ = DB_PATH.set(path);
    *DEVICE_ID.lock().unwrap() = device_id.to_string();
    *INGEST_KEY.lock().unwrap() = ingest_key.to_string();
    Ok(())
}

/// Enqueue one record for `stream`. Idempotent: the same record enqueued twice
/// is one row. Synchronous and cheap — safe to call from a background OS
/// callback. `record` is the box-shaped record JSON; its `id` (if present) is
/// used as the dedup key, else one is derived and stamped in.
pub fn enqueue(stream: &str, mut record: Value) -> Result<()> {
    let device = DEVICE_ID.lock().unwrap().clone();
    let action_key = {
        let k = INGEST_KEY.lock().unwrap().clone();
        if k.is_empty() {
            "ios_ingest".to_string()
        } else {
            k
        }
    };

    let id = match record.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        Some(id) => id.to_string(),
        None => {
            let canon = serde_json::to_string(&record).unwrap_or_default();
            let name = format!("{device}\u{1f}{stream}\u{1f}{canon}");
            let id = Uuid::new_v5(&NS, name.as_bytes()).to_string();
            record["id"] = Value::String(id.clone());
            id
        }
    };
    let payload = serde_json::to_string(&record)?;

    let conn = conn()?;
    conn.execute(
        "INSERT OR IGNORE INTO outbox
           (source_stream_id, stream, action_key, payload, created_at, attempts, next_attempt_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, 0)",
        params![id, stream, action_key, payload, now()],
    )?;
    Ok(())
}

/// Streams that have rows due for delivery right now.
pub fn due_streams() -> Result<Vec<String>> {
    let conn = conn()?;
    let now = now();
    let mut stmt = conn.prepare(
        "SELECT DISTINCT stream FROM outbox WHERE claimed_at IS NULL AND next_attempt_at <= ?1",
    )?;
    let rows = stmt.query_map(params![now], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// The `action_key` (ingest binary name) for a stream's queued rows.
pub fn action_key_for(stream: &str) -> Result<Option<String>> {
    let conn = conn()?;
    let key: Option<String> = conn
        .query_row(
            "SELECT action_key FROM outbox WHERE stream = ?1 LIMIT 1",
            params![stream],
            |r| r.get(0),
        )
        .ok();
    Ok(key)
}

/// Claim up to `max_rows` / `max_bytes` due rows for `stream`, marking them
/// in-flight. Returns their ids + records. Always yields at least one row if any
/// is due (even if a single row exceeds `max_bytes`).
pub fn claim_batch(stream: &str, max_bytes: usize, max_rows: usize) -> Result<Claimed> {
    let conn = conn()?;
    let now = now();
    let mut stmt = conn.prepare(
        "SELECT source_stream_id, payload FROM outbox
         WHERE stream = ?1 AND claimed_at IS NULL AND next_attempt_at <= ?2
         ORDER BY created_at LIMIT ?3",
    )?;
    let candidates = stmt
        .query_map(params![stream, now, max_rows as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut ids = Vec::new();
    let mut records = Vec::new();
    let mut bytes = 0usize;
    for (id, payload) in candidates {
        if !ids.is_empty() && bytes + payload.len() > max_bytes {
            break;
        }
        match serde_json::from_str::<Value>(&payload) {
            Ok(v) => {
                bytes += payload.len();
                records.push(v);
                ids.push(id);
            }
            // A row that no longer parses can never be sent — drop it so it
            // doesn't wedge the queue forever.
            Err(_) => {
                conn.execute("DELETE FROM outbox WHERE source_stream_id = ?1", params![id])?;
            }
        }
    }

    if !ids.is_empty() {
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("UPDATE outbox SET claimed_at = ? WHERE source_stream_id IN ({placeholders})");
        let mut p: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];
        for id in &ids {
            p.push(Box::new(id.clone()));
        }
        let refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
        conn.execute(&sql, refs.as_slice())?;
    }
    Ok(Claimed { ids, records })
}

/// Durably delivered — remove the rows.
pub fn ack(ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let conn = conn()?;
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("DELETE FROM outbox WHERE source_stream_id IN ({placeholders})");
    let refs: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|i| i as &dyn rusqlite::ToSql).collect();
    conn.execute(&sql, refs.as_slice())?;
    Ok(())
}

/// Delivery failed — bump attempts, set the backoff gate, release the claim.
pub fn nack(ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let conn = conn()?;
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    // attempts+1, next_attempt_at = now + backoff(min(attempts,5)) + jitter.
    let sql = format!(
        "UPDATE outbox
         SET attempts = attempts + 1,
             claimed_at = NULL,
             next_attempt_at = ?1 + CASE
                WHEN attempts >= 5 THEN {b5}
                WHEN attempts = 4 THEN {b4}
                WHEN attempts = 3 THEN {b3}
                WHEN attempts = 2 THEN {b2}
                WHEN attempts = 1 THEN {b1}
                ELSE {b0} END
         WHERE source_stream_id IN ({placeholders})",
        b0 = BACKOFF_SECS[0], b1 = BACKOFF_SECS[1], b2 = BACKOFF_SECS[2],
        b3 = BACKOFF_SECS[3], b4 = BACKOFF_SECS[4], b5 = BACKOFF_SECS[5],
    );
    // Small jitter so many rows don't all wake together.
    let base = now() + (rand::random::<f64>() * 10.0) as i64;
    let mut p: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(base)];
    for id in ids {
        p.push(Box::new(id.clone()));
    }
    let refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql, refs.as_slice())?;
    Ok(())
}

/// Release any in-flight claims — call once at launch so a drain interrupted by
/// a crash/termination is retried rather than stranded.
pub fn reset_stale() -> Result<()> {
    let conn = conn()?;
    conn.execute("UPDATE outbox SET claimed_at = NULL WHERE claimed_at IS NOT NULL", [])?;
    Ok(())
}

/// Queue health for a stream (device screen).
pub fn stats(stream: &str) -> Result<OutboxStats> {
    let conn = conn()?;
    let (queued, failing, oldest): (i64, i64, i64) = conn.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN attempts > 0 THEN 1 ELSE 0 END), 0),
                COALESCE(MIN(created_at), 0)
         FROM outbox WHERE stream = ?1",
        params![stream],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    Ok(OutboxStats { queued, failing, oldest })
}

/// The most recent `n` records for a stream (newest first) — device-screen log.
pub fn recent(stream: &str, n: usize) -> Result<Vec<Value>> {
    let conn = conn()?;
    let mut stmt = conn.prepare(
        "SELECT payload FROM outbox WHERE stream = ?1 ORDER BY created_at DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![stream, n as i64], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        if let Ok(v) = serde_json::from_str::<Value>(&row?) {
            out.push(v);
        }
    }
    Ok(out)
}

// ─── internals ───────────────────────────────────────────────────────────────

fn conn() -> Result<Connection> {
    let path = DB_PATH
        .get()
        .ok_or_else(|| anyhow!("outbox not initialized — call outbox::init() at launch"))?;
    open(path)
}

fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    // WAL + busy timeout so the sync enqueue and the async drain coexist.
    conn.pragma_update(None, "journal_mode", "WAL").ok();
    conn.pragma_update(None, "busy_timeout", 5000).ok();
    Ok(conn)
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
