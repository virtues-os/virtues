//! Mac activity → ontology transforms.
//!
//! The Mac client app posts batches of three record kinds in one webhook:
//!   - `app_events` → aggregated into app sessions in `data_activity_app_session`
//!   - `browser_history` → `data_activity_web_browsing`
//!   - `imessages` → `data_communication_message`
//!
//! The expected payload shape (matches the deleted `core/src/sources/mac/transform.rs`
//! aggregation behaviour):
//! ```json
//! {
//!   "app_events": [{"timestamp": "...", "bundle_id": "...", "app_name": "...", "window_title": "..."}, ...],
//!   "browser_history": [{"url": "...", "title": "...", "timestamp": "..."}, ...],
//!   "imessages": [{"guid": "...", "text": "...", "timestamp": "...", "from_handle": "...", "is_from_me": true}, ...]
//! }
//! ```

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use virtues_helpers::dedup::{build_batch_insert_query, BATCH_SIZE};

const PROVIDER: &str = "mac";

/// A record's timestamp, or `None` if it has none we can parse.
///
/// Deliberately NOT `unwrap_or_else(Utc::now)`. Every one of these transforms folds
/// the timestamp into either its dedup key or the row's place in the timeline, so a
/// wall-clock fallback doesn't "recover" a bad record — it silently mints a new
/// identity on every retry (duplicating the row) or files the record at ingest time
/// (wrong forever, since ON CONFLICT DO NOTHING never corrects it). Callers skip
/// records this returns `None` for; the raw record is still in the lake either way.
fn event_time(record: &Value) -> Option<DateTime<Utc>> {
    record
        .get("timestamp")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
}

// NOTE: app-event aggregation used to live here. It grouped events into sessions
// WITHIN a single upload batch, which is structurally incapable of recording a
// session longer than the upload interval — a 40-minute focus produced no row at
// all, while backlog batches fabricated enormous ones. It now lives in
// `sessionize.rs`, which holds sessions open across batches against the DB.

// ─────────────────────────────────────────────────────────────────────────────
// Browser history → data_activity_web_browsing
// ─────────────────────────────────────────────────────────────────────────────

pub async fn write_browser_history(db: &PgPool, visits: &[Value]) -> Result<usize> {
    let mut pending: Vec<(String, String, String, Option<String>, DateTime<Utc>, String, Value)> =
        Vec::new();
    let mut written = 0;

    for visit in visits {
        let url = visit.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if url.is_empty() {
            continue;
        }
        let domain = extract_domain(url).unwrap_or_else(|| "unknown".to_string());
        let title = visit
            .get("title")
            .and_then(|v| v.as_str())
            .map(String::from);
        // NO Utc::now() fallback. The timestamp goes straight into the dedup key
        // below, so defaulting to the wall clock means a retry of the SAME visit
        // computes a DIFFERENT source_stream_id and inserts a duplicate — every
        // 5 minutes, for as long as the device keeps retrying. A visit we cannot
        // place in time is not a visit we can dedup; skip it.
        let Some(ts) = event_time(visit) else {
            tracing::warn!(url, "browser visit has no parseable timestamp — skipping");
            continue;
        };

        // HASH the dedup key rather than embedding the URL in it.
        //
        // `{url}:{ts}` looks fine until someone visits a URL with a page of tracking
        // parameters (or a `data:` URI). source_stream_id is UNIQUE, and a btree
        // index row cannot exceed ~2704 bytes — so one long URL doesn't just drop
        // that visit, it fails the INSERT, which 500s the webhook, which poisons the
        // whole batch: app sessions and iMessages die with it, and the device retries
        // the same doomed payload every 5 minutes. A UUIDv5 is 36 bytes no matter how
        // deranged the URL, and it is just as deterministic.
        let stream_id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("mac:browse:{}:{}", url, ts.timestamp_millis()).as_bytes(),
        )
        .to_string();
        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("mac:browse:{stream_id}").as_bytes(),
        )
        .to_string();

        pending.push((
            id,
            url.to_string(),
            domain,
            title,
            ts,
            stream_id,
            serde_json::json!({"browser": visit.get("browser")}),
        ));

        if pending.len() >= BATCH_SIZE {
            written += flush_browser(db, &pending).await?;
            pending.clear();
        }
    }
    if !pending.is_empty() {
        written += flush_browser(db, &pending).await?;
    }
    Ok(written)
}

fn extract_domain(url: &str) -> Option<String> {
    let after_scheme = url.split("://").nth(1)?;
    let host = after_scheme.split('/').next()?;
    Some(host.trim_start_matches("www.").to_string())
}

async fn flush_browser(
    db: &PgPool,
    rows: &[(String, String, String, Option<String>, DateTime<Utc>, String, Value)],
) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let sql = build_batch_insert_query(
        "data_activity_web_browsing",
        &[
            "id",
            "url",
            "domain",
            "page_title",
            "timestamp",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        rows.len(),
    );
    let mut q = sqlx::query(&sql);
    for r in rows {
        q = q
            .bind(&r.0)
            .bind(&r.1)
            .bind(&r.2)
            .bind(&r.3)
            .bind(r.4)
            .bind(&r.5)
            .bind("mac_browser")
            .bind(PROVIDER)
            .bind(&r.6);
    }
    Ok(q.execute(db).await?.rows_affected() as usize)
}

// ─────────────────────────────────────────────────────────────────────────────
// iMessage → data_communication_message
// ─────────────────────────────────────────────────────────────────────────────

pub async fn write_imessages(db: &PgPool, messages: &[Value]) -> Result<usize> {
    let mut pending: Vec<(
        String,
        String,
        String,
        Option<String>,
        DateTime<Utc>,
        String,
        Value,
    )> = Vec::new();
    let mut written = 0;

    for m in messages {
        let guid = m.get("guid").and_then(|v| v.as_str()).unwrap_or("");
        if guid.is_empty() {
            continue;
        }
        let text = m
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let is_from_me = m
            .get("is_from_me")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // from_identifier is NOT NULL. In chat.db the handle identifies the *other*
        // party even on messages we sent, so is_from_me has to win — otherwise our
        // own messages get attributed to the recipient.
        let from_handle = if is_from_me {
            "me".to_string()
        } else {
            m.get("from_handle")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        // NOTE: `direction` ('sent'/'received') is a column on
        // data_communication_*EMAIL*, NOT on data_communication_message — inserting
        // it here failed every iMessage batch with "column \"direction\" ... does not
        // exist" (and, because app_events ride the same webhook batch, took those
        // down too). Sent-vs-received is preserved in `metadata.is_from_me` below.
        let chat_guid = m
            .get("chat_guid")
            .and_then(|v| v.as_str())
            .map(String::from);
        // The GUID keys the dedup, so an unparseable timestamp wouldn't duplicate the
        // row — it would silently file the message at INGEST time instead, landing a
        // months-old message in today's timeline and then never correcting it
        // (ON CONFLICT DO NOTHING). A message we can't place in time is worse than
        // no message.
        let Some(ts) = event_time(m) else {
            tracing::warn!(guid, "iMessage has no parseable timestamp — skipping");
            continue;
        };

        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("mac:imessage:{guid}").as_bytes(),
        )
        .to_string();

        pending.push((
            id,
            text,
            from_handle,
            chat_guid,
            ts,
            guid.to_string(),
            serde_json::json!({"is_from_me": is_from_me}),
        ));

        if pending.len() >= BATCH_SIZE {
            written += flush_imessage(db, &pending).await?;
            pending.clear();
        }
    }
    if !pending.is_empty() {
        written += flush_imessage(db, &pending).await?;
    }
    Ok(written)
}

async fn flush_imessage(
    db: &PgPool,
    rows: &[(
        String,
        String,
        String,
        Option<String>,
        DateTime<Utc>,
        String,
        Value,
    )],
) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let sql = build_batch_insert_query(
        "data_communication_message",
        &[
            "id",
            // The provider's native message id (the iMessage GUID). NOT NULL — omitting
            // it is what broke ingest after the `direction` fix.
            "message_id",
            "body",
            "from_identifier",
            // `channel` is what the registry reads for a message's source_type
            // ("message:" || channel), so name it rather than leaving it "unknown".
            "channel",
            "thread_id",
            "timestamp",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        rows.len(),
    );
    let mut q = sqlx::query(&sql);
    for r in rows {
        // r.5 is the GUID: it serves as both the native message_id and the dedup key.
        q = q
            .bind(&r.0)
            .bind(&r.5)
            .bind(&r.1)
            .bind(&r.2)
            .bind("imessage")
            .bind(&r.3)
            .bind(r.4)
            .bind(&r.5)
            .bind("mac_imessage")
            .bind(PROVIDER)
            .bind(&r.6);
    }
    Ok(q.execute(db).await?.rows_affected() as usize)
}
