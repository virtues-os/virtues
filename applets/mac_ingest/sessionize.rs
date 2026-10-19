//! Stateful sessionization: attended app sessions.
//!
//! The old aggregator grouped events into sessions WITHIN a single upload batch.
//! That cannot work, and it wasn't a tuning problem. The collector emits events
//! only when something changes — focus(Cursor) at 12:00, unfocus(Cursor) at 12:40 —
//! so a real 40-minute session put its focus in one 5-minute batch (start == end →
//! dropped as noise) and its unfocus in a batch 40 minutes later (also dropped).
//! **A deep-work session recorded nothing.** Meanwhile backlog batches delivered
//! hours of events at once and the consecutive-run merge fabricated enormous spans:
//! 326 of 429 recorded hours came from sessions longer than the upload interval,
//! which steady-state collection cannot produce.
//!
//! So state lives in Postgres, not in a 5-minute window. A session is opened by
//! focus, kept alive by heartbeats, and closed by the device event that ended it —
//! a switch, a lock, going idle, the lid closing. `closed_by` records WHICH, and
//! that is why there is no second "presence" table: the reason you stopped lives on
//! the session, so the gap that follows already explains itself. (`stale` means the
//! collector died mid-session, which is what distinguishes "we weren't watching"
//! from "you walked away".) The raw device events are archived in the lake, so a
//! full attention timeline can be DERIVED later if anyone ever wants one — no need
//! to model a question nobody has asked.
//!
//! `suspend` means the MACHINE slept. It says nothing about whether you did: a Mac
//! can observe its lid closing, not your sleeping. Human sleep is data_health_sleep,
//! from a watch that can actually measure it.
//!
//! A session left open at the end of a batch STAYS open. The next batch — minutes
//! or hours later — closes it. That is the whole fix.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

const PROVIDER: &str = "mac";
const SESSION_TABLE: &str = "mac_apps";

/// A flick through the app switcher isn't a session.
const MIN_SESSION: Duration = Duration::seconds(1);

/// An interval open longer than this lost its close event — a power cut, a panic,
/// a killed process. Clamp it to the last moment we knew it was alive rather than
/// letting it run forever and re-inventing the 665-minute session.
const MAX_OPEN: Duration = Duration::hours(8);

/// `loginwindow` is the lock screen; `ScreenSaverEngine` is the screensaver. They
/// are not apps you used — they are the machine saying nobody is there. Recording
/// them as app usage is how the lock screen became the box's most-used application.
fn is_absence_proxy(bundle: &str) -> bool {
    matches!(
        bundle,
        "com.apple.loginwindow" | "com.apple.ScreenSaver.Engine" | "com.apple.SecurityAgent"
    )
}

struct Ev {
    at: DateTime<Utc>,
    kind: String,
    app: String,
    bundle: String,
    title: Option<String>,
}

/// Ingest one batch of interleaved focus + device events.
///
/// Returns the number of sessions opened.
pub async fn ingest(db: &PgPool, device_id: &str, events: &[Value]) -> Result<usize> {
    // True time order across BOTH kinds of event. Sessionizing means interleaving
    // "you switched to Cursor" with "you walked away" — order IS the semantics. An
    // unparseable timestamp is skipped rather than defaulting to the wall clock,
    // which would make a record's identity depend on when it was ingested.
    let mut evs: Vec<Ev> = events
        .iter()
        .filter_map(|e| {
            Some(Ev {
                at: e
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<DateTime<Utc>>().ok())?,
                kind: e.get("event_type").and_then(|v| v.as_str())?.to_string(),
                app: e
                    .get("app_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string(),
                bundle: e
                    .get("bundle_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                title: e
                    .get("window_title")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from),
            })
        })
        .collect();
    evs.sort_by_key(|e| e.at);

    let mut sessions = 0usize;

    for ev in &evs {
        match ev.kind.as_str() {
            "focus_gained" | "launch" => {
                // The lock screen is not an app. Focusing it means you left.
                if is_absence_proxy(&ev.bundle) {
                    close_session(db, device_id, ev.at, "lock").await?;
                    continue;
                }
                sessions += open_session(db, device_id, ev).await?;
            }
            // "Still focused." Advances the provisional end, so a session
            // interrupted by a crash or an update is clamped to within a heartbeat
            // of the truth instead of to nothing.
            "heartbeat" => {
                if !is_absence_proxy(&ev.bundle) {
                    touch_session(db, device_id, ev).await?;
                }
            }
            "focus_lost" | "quit" => {
                close_session(db, device_id, ev.at, "switch").await?;
            }
            "idle_start" => {
                close_session(db, device_id, ev.at, "idle").await?;
            }
            // A video IS attention — it just isn't typing. The session stays OPEN.
            "watch_start" => mark_attention(db, device_id, "watching").await?,
            "watch_end" => mark_attention(db, device_id, "active").await?,
            "lock" => close_session(db, device_id, ev.at, "lock").await?,
            // The MACHINE slept. Says nothing about whether you did.
            "suspend" => close_session(db, device_id, ev.at, "suspend").await?,
            _ => {}
        }
    }

    reap_stale(db, device_id).await?;
    Ok(sessions)
}

// ── app sessions ────────────────────────────────────────────────────────────

async fn open_session(db: &PgPool, device_id: &str, ev: &Ev) -> Result<usize> {
    if ev.bundle.is_empty() {
        return Ok(0);
    }

    // Already focused? Then this is a duplicate focus — the 1s poll and the
    // workspace notification both fire — not a new session.
    if let Some(bundle) = open_session_bundle(db, device_id).await? {
        if bundle == ev.bundle {
            touch_session(db, device_id, ev).await?;
            return Ok(0);
        }
        close_session(db, device_id, ev.at, "switch").await?;
    }

    let stream_id = format!("{}:{}:{}", device_id, ev.bundle, ev.at.timestamp());
    let id = Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("mac:app_session:{stream_id}").as_bytes(),
    )
    .to_string();

    sqlx::query(
        "INSERT INTO data_activity_app_session (
             id, device_id, app_name, app_bundle_id, started_at, ended_at, window_title,
             attention, is_open, source_stream_id, source_table, source_provider, metadata
         ) VALUES ($1, $2, $3, $4, $5, $5, $6, 'active', true, $7, $8, $9, $10)
         ON CONFLICT (source_stream_id) DO NOTHING",
    )
    .bind(&id)
    .bind(device_id)
    .bind(&ev.app)
    .bind(&ev.bundle)
    .bind(ev.at)
    .bind(&ev.title)
    .bind(&stream_id)
    .bind(SESSION_TABLE)
    .bind(PROVIDER)
    .bind(json!({ "titles": titles_of(ev) }))
    .execute(db)
    .await?;

    Ok(1)
}

/// Advance the open session's provisional end, and accumulate title changes.
///
/// Within one 40-minute Cursor session you touch six files. Splitting on each title
/// change would recreate the fragmentation this rewrite exists to fix, so titles
/// accumulate as a timeline on ONE session.
async fn touch_session(db: &PgPool, device_id: &str, ev: &Ev) -> Result<()> {
    let Some(title) = &ev.title else {
        sqlx::query(
            "UPDATE data_activity_app_session
                SET ended_at = greatest(ended_at, $1), updated_at = now()
              WHERE device_id = $2 AND app_bundle_id = $3 AND is_open",
        )
        .bind(ev.at)
        .bind(device_id)
        .bind(&ev.bundle)
        .execute(db)
        .await?;
        return Ok(());
    };

    sqlx::query(
        "UPDATE data_activity_app_session
            SET ended_at = greatest(ended_at, $1),
                window_title = coalesce(window_title, $4),
                metadata = jsonb_set(
                    metadata, '{titles}',
                    CASE WHEN metadata->'titles' @> $5::jsonb THEN metadata->'titles'
                         ELSE coalesce(metadata->'titles', '[]'::jsonb) || $5::jsonb END),
                updated_at = now()
          WHERE device_id = $2 AND app_bundle_id = $3 AND is_open",
    )
    .bind(ev.at)
    .bind(device_id)
    .bind(&ev.bundle)
    .bind(title)
    .bind(json!([{ "t": title }]))
    .execute(db)
    .await?;

    Ok(())
}

/// The open session's attention: `watching` while the focused app holds the display
/// awake, `active` otherwise. Both count as usage.
async fn mark_attention(db: &PgPool, device_id: &str, attention: &str) -> Result<()> {
    sqlx::query(
        "UPDATE data_activity_app_session
            SET attention = $1, updated_at = now()
          WHERE device_id = $2 AND is_open",
    )
    .bind(attention)
    .bind(device_id)
    .execute(db)
    .await?;
    Ok(())
}

async fn open_session_bundle(db: &PgPool, device_id: &str) -> Result<Option<String>> {
    let row = sqlx::query(
        "SELECT app_bundle_id FROM data_activity_app_session
          WHERE device_id = $1 AND is_open
          ORDER BY started_at DESC LIMIT 1",
    )
    .bind(device_id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| r.get::<String, _>("app_bundle_id")))
}

async fn close_session(
    db: &PgPool,
    device_id: &str,
    at: DateTime<Utc>,
    reason: &str,
) -> Result<()> {
    // `greatest(ended_at, $1)` so a close can never move the end BACKWARDS: idle is
    // back-dated to when input actually stopped, which may precede the last
    // heartbeat, and a negative-duration session is worse than a wrong one.
    sqlx::query(
        "UPDATE data_activity_app_session
            SET ended_at = greatest(ended_at, $1), is_open = false, closed_by = $2,
                updated_at = now()
          WHERE device_id = $3 AND is_open",
    )
    .bind(at)
    .bind(reason)
    .bind(device_id)
    .execute(db)
    .await?;

    sqlx::query(
        "DELETE FROM data_activity_app_session
          WHERE device_id = $1 AND NOT is_open AND ended_at - started_at < $2",
    )
    .bind(device_id)
    .bind(MIN_SESSION)
    .execute(db)
    .await?;

    Ok(())
}

/// A session whose close never arrived — a power cut, a panic, a killed process, an
/// update swapping the binary. Clamp it where we last knew it was alive (its final
/// heartbeat) and mark it `stale`, so a gap caused by the COLLECTOR dying can be
/// told apart from a gap caused by you walking away.
async fn reap_stale(db: &PgPool, device_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE data_activity_app_session
            SET is_open = false, closed_by = 'stale', updated_at = now()
          WHERE device_id = $1 AND is_open AND now() - started_at > $2",
    )
    .bind(device_id)
    .bind(MAX_OPEN)
    .execute(db)
    .await?;
    Ok(())
}

fn titles_of(ev: &Ev) -> Value {
    match &ev.title {
        Some(t) => json!([{ "t": t }]),
        None => json!([]),
    }
}
