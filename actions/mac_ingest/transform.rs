//! Mac activity → ontology transforms.
//!
//! The Mac client app posts batches of three record kinds in one webhook:
//!   - `app_events` → aggregated into app sessions in `data_activity_app_usage`
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
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use virtues_helpers::dedup::{build_batch_insert_query, BATCH_SIZE};

const PROVIDER: &str = "mac";

// ─────────────────────────────────────────────────────────────────────────────
// App usage — aggregate raw activate/deactivate events into sessions
// ─────────────────────────────────────────────────────────────────────────────

/// Aggregate raw app focus events into sessions and write to data_activity_app_usage.
/// Each consecutive run of events with the same bundle_id becomes one session
/// (start = first event ts, end = last event ts).
pub async fn write_app_events(db: &PgPool, events: &[Value]) -> Result<usize> {
    if events.is_empty() {
        return Ok(0);
    }

    // Sort by timestamp.
    let mut sorted: Vec<&Value> = events.iter().collect();
    sorted.sort_by_key(|e| {
        e.get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now)
    });

    #[derive(Default)]
    struct Session {
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
        bundle_id: String,
        app_name: String,
        window_title: Option<String>,
    }

    let mut sessions: Vec<Session> = Vec::new();
    let mut cur = Session::default();

    for ev in sorted {
        let ts = ev
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<DateTime<Utc>>().ok());
        let bundle = ev
            .get("bundle_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let name = ev
            .get("app_name")
            .and_then(|v| v.as_str())
            .unwrap_or(&bundle)
            .to_string();
        let title = ev
            .get("window_title")
            .and_then(|v| v.as_str())
            .map(String::from);

        match ts {
            Some(t) if !bundle.is_empty() => {
                if cur.bundle_id != bundle {
                    if cur.start.is_some() {
                        sessions.push(std::mem::take(&mut cur));
                    }
                    cur.start = Some(t);
                    cur.bundle_id = bundle;
                    cur.app_name = name;
                    cur.window_title = title.clone();
                }
                cur.end = Some(t);
                if title.is_some() && cur.window_title.is_none() {
                    cur.window_title = title;
                }
            }
            _ => continue,
        }
    }
    if cur.start.is_some() {
        sessions.push(cur);
    }

    // Filter sessions shorter than 1 second (noise).
    let rows: Vec<_> = sessions
        .into_iter()
        .filter_map(|s| {
            let start = s.start?;
            let end = s.end?;
            if end - start < Duration::seconds(1) {
                return None;
            }
            let stream_id = format!("{}:{}", s.bundle_id, start.timestamp());
            let id = Uuid::new_v5(
                &Uuid::NAMESPACE_OID,
                format!("mac:app_session:{stream_id}").as_bytes(),
            )
            .to_string();
            Some((
                id,
                s.app_name.clone(),
                s.bundle_id.clone(),
                start,
                end,
                s.window_title.clone(),
                stream_id,
                serde_json::json!({"bundle_id": s.bundle_id}),
            ))
        })
        .collect();

    let mut written = 0;
    for chunk in rows.chunks(BATCH_SIZE) {
        if chunk.is_empty() {
            continue;
        }
        let sql = build_batch_insert_query(
            "data_activity_app_usage",
            &[
                "id",
                "app_name",
                "app_bundle_id",
                "start_time",
                "end_time",
                "window_title",
                "source_stream_id",
                "source_table",
                "source_provider",
                "metadata",
            ],
            "source_stream_id",
            chunk.len(),
        );
        let mut q = sqlx::query(&sql);
        for r in chunk {
            q = q
                .bind(&r.0)
                .bind(&r.1)
                .bind(&r.2)
                .bind(r.3)
                .bind(r.4)
                .bind(&r.5)
                .bind(&r.6)
                .bind("mac_apps")
                .bind(PROVIDER)
                .bind(&r.7);
        }
        written += q.execute(db).await?.rows_affected() as usize;
    }
    Ok(written)
}

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
        let ts = visit
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now);

        let stream_id = format!("{}:{}", url, ts.timestamp_millis());
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
        let ts = m
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now);

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
