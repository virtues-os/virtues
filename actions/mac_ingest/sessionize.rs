//! Stateful sessionization: app usage + presence.
//!
//! The old aggregator grouped events into sessions WITHIN a single upload batch.
//! That cannot work, and it wasn't a tuning problem. The collector emits events
//! only when something changes — focus(Cursor) at 12:00, unfocus(Cursor) at 12:40 —
//! so a real 40-minute session puts its focus in one 5-minute batch (start == end →
//! dropped as noise) and its unfocus in a batch 40 minutes later (also dropped).
//! **A deep-work session recorded nothing.** Meanwhile backlog batches (a collector
//! restart, upload backoff, sleep/wake) delivered hours of events at once and the
//! consecutive-run merge fabricated enormous spans out of them: 326 of 429 recorded
//! hours came from sessions longer than the upload interval, which steady-state
//! collection is structurally incapable of producing. The box's most-used "app" was
//! the lock screen, at 211 hours.
//!
//! So state lives in Postgres, not in a 5-minute window:
//!
//!   focus/launch     → open a session (or keep the current one)
//!   heartbeat        → the app is STILL focused; advance its provisional end
//!   unfocus/quit     → close it
//!   idle/lock/sleep  → close it, and open a presence span
//!   watch            → NOT a close: a video is attention, just not typing
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
const PRESENCE_TABLE: &str = "mac_presence";

/// Sessions shorter than this are noise (a flick through the app switcher).
const MIN_SESSION: Duration = Duration::seconds(1);

/// A session open longer than this lost its close event — a hard power-off, a
/// kernel panic, a battery that died. Clamp it to the last moment we actually knew
/// it was alive (its heartbeat) rather than letting it run forever.
const MAX_OPEN: Duration = Duration::hours(8);

/// `loginwindow` is the lock screen and `ScreenSaverEngine` is the screensaver.
/// They are not apps you used; they are the machine telling you nobody is there.
/// Recording them as app usage is how "the lock screen" became the most-used
/// application on the box.
fn is_presence_proxy(bundle: &str) -> bool {
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

/// Ingest one batch of interleaved focus + presence events.
///
/// Returns `(sessions_touched, presence_spans_touched)`.
pub async fn ingest(db: &PgPool, device_id: &str, events: &[Value]) -> Result<(usize, usize)> {
    // True time order across BOTH kinds of event. Sessionizing means interleaving
    // "you switched to Cursor" with "you walked away" — order is the whole
    // semantics. An unparseable timestamp sorts last and is then skipped, rather
    // than defaulting to the wall clock (which would make a record's identity
    // depend on when it happened to be ingested).
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
    let mut spans = 0usize;

    for ev in &evs {
        match ev.kind.as_str() {
            "focus_gained" | "launch" => {
                // The lock screen is not an app. Focusing it means you left.
                if is_presence_proxy(&ev.bundle) {
                    close_open_session(db, device_id, ev.at, "lock").await?;
                    open_span(db, device_id, "locked", ev.at).await?;
                    spans += 1;
                    continue;
                }
                close_open_span(db, device_id, ev.at).await?;
                sessions += open_session(db, device_id, ev).await?;
            }
            // "Still focused." Advances the provisional end so an interrupted
            // session is clamped to within a heartbeat of the truth, not to nothing.
            "heartbeat" => {
                if is_presence_proxy(&ev.bundle) {
                    continue;
                }
                touch_open_session(db, device_id, ev).await?;
            }
            "focus_lost" | "quit" => {
                close_open_session(db, device_id, ev.at, "switch").await?;
            }
            "idle_start" => {
                close_open_session(db, device_id, ev.at, "idle").await?;
                open_span(db, device_id, "idle", ev.at).await?;
                spans += 1;
            }
            // Watching a video IS attention — it just isn't typing. The session
            // stays open; we only note the presence state.
            "watch_start" => {
                open_span(db, device_id, "watching", ev.at).await?;
                spans += 1;
            }
            "idle_end" | "watch_end" | "unlock" | "wake" => {
                close_open_span(db, device_id, ev.at).await?;
            }
            "lock" => {
                close_open_session(db, device_id, ev.at, "lock").await?;
                open_span(db, device_id, "locked", ev.at).await?;
                spans += 1;
            }
            "sleep" => {
                close_open_session(db, device_id, ev.at, "sleep").await?;
                open_span(db, device_id, "asleep", ev.at).await?;
                spans += 1;
            }
            _ => {}
        }
    }

    // Anything still open from a session whose close event never arrived.
    reap_stale(db, device_id).await?;

    Ok((sessions, spans))
}

// ── sessions ────────────────────────────────────────────────────────────────

async fn open_session(db: &PgPool, device_id: &str, ev: &Ev) -> Result<usize> {
    if ev.bundle.is_empty() {
        return Ok(0);
    }

    // Already focused? Then this is a duplicate focus (the 1s poll and the
    // workspace notification both fire), not a new session.
    if let Some((_, bundle)) = current_open(db, device_id).await? {
        if bundle == ev.bundle {
            touch_open_session(db, device_id, ev).await?;
            return Ok(0);
        }
        close_open_session(db, device_id, ev.at, "switch").await?;
    }

    let stream_id = format!("{}:{}:{}", device_id, ev.bundle, ev.at.timestamp());
    let id = Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("mac:app_session:{stream_id}").as_bytes(),
    )
    .to_string();

    // end_time is NOT NULL, so an open session's end is PROVISIONAL: it starts
    // equal to start_time and walks forward with each heartbeat.
    sqlx::query(
        "INSERT INTO data_activity_app_usage (
             id, device_id, app_name, app_bundle_id, start_time, end_time, window_title,
             is_open, source_stream_id, source_table, source_provider, metadata
         ) VALUES ($1, $2, $3, $4, $5, $5, $6, true, $7, $8, $9, $10)
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
    .bind(json!({ "titles": title_entry(ev) }))
    .execute(db)
    .await?;

    Ok(1)
}

/// Advance the open session's provisional end, and record a title change.
///
/// Within one 40-minute Cursor session you touch six files. Splitting on each
/// title change would just recreate the fragmentation this rewrite exists to fix,
/// so the titles accumulate as a timeline on ONE session.
async fn touch_open_session(db: &PgPool, device_id: &str, ev: &Ev) -> Result<()> {
    let Some(title) = &ev.title else {
        sqlx::query(
            "UPDATE data_activity_app_usage
                SET end_time = $1, updated_at = now()
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
        "UPDATE data_activity_app_usage
            SET end_time = $1,
                window_title = coalesce(window_title, $4),
                metadata = jsonb_set(
                    metadata, '{titles}',
                    CASE
                      WHEN metadata->'titles' @> $5::jsonb THEN metadata->'titles'
                      ELSE coalesce(metadata->'titles', '[]'::jsonb) || $5::jsonb
                    END),
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

async fn current_open(db: &PgPool, device_id: &str) -> Result<Option<(String, String)>> {
    let row = sqlx::query(
        "SELECT id, app_bundle_id FROM data_activity_app_usage
          WHERE device_id = $1 AND is_open
          ORDER BY start_time DESC LIMIT 1",
    )
    .bind(device_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|r| {
        (
            r.get::<String, _>("id"),
            r.get::<String, _>("app_bundle_id"),
        )
    }))
}

async fn close_open_session(
    db: &PgPool,
    device_id: &str,
    at: DateTime<Utc>,
    reason: &str,
) -> Result<()> {
    // `greatest(end_time, $1)` so a close can never move the end BACKWARDS: idle is
    // back-dated to when input actually stopped, which may precede the last
    // heartbeat, and a negative-duration session would be worse than a wrong one.
    sqlx::query(
        "UPDATE data_activity_app_usage
            SET end_time = greatest(end_time, $1), is_open = false, closed_by = $2,
                updated_at = now()
          WHERE device_id = $3 AND is_open",
    )
    .bind(at)
    .bind(reason)
    .bind(device_id)
    .execute(db)
    .await?;

    // Drop the noise: a flick through the app switcher isn't a session.
    sqlx::query(
        "DELETE FROM data_activity_app_usage
          WHERE device_id = $1 AND NOT is_open AND end_time - start_time < $2",
    )
    .bind(device_id)
    .bind(MIN_SESSION)
    .execute(db)
    .await?;

    Ok(())
}

/// A session whose close never arrived (power cut, panic, killed process).
///
/// Clamp it to its last heartbeat — the last moment we actually knew it was alive —
/// rather than letting it run forever and re-inventing the 665-minute session.
async fn reap_stale(db: &PgPool, device_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE data_activity_app_usage
            SET is_open = false, closed_by = 'stale', updated_at = now()
          WHERE device_id = $1 AND is_open AND now() - start_time > $2",
    )
    .bind(device_id)
    .bind(MAX_OPEN)
    .execute(db)
    .await?;

    sqlx::query(
        "UPDATE data_activity_presence
            SET is_open = false, updated_at = now()
          WHERE device_id = $1 AND is_open AND now() - started_at > $2",
    )
    .bind(device_id)
    .bind(MAX_OPEN)
    .execute(db)
    .await?;

    Ok(())
}

// ── presence ────────────────────────────────────────────────────────────────

async fn open_span(db: &PgPool, device_id: &str, state: &str, at: DateTime<Utc>) -> Result<()> {
    close_open_span(db, device_id, at).await?;

    let stream_id = format!("{device_id}:{state}:{}", at.timestamp());
    let id = Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("mac:presence:{stream_id}").as_bytes(),
    )
    .to_string();

    sqlx::query(
        "INSERT INTO data_activity_presence (
             id, device_id, state, started_at, ended_at, is_open,
             source_stream_id, source_table, source_provider
         ) VALUES ($1, $2, $3, $4, $4, true, $5, $6, $7)
         ON CONFLICT (source_stream_id) DO NOTHING",
    )
    .bind(&id)
    .bind(device_id)
    .bind(state)
    .bind(at)
    .bind(&stream_id)
    .bind(PRESENCE_TABLE)
    .bind(PROVIDER)
    .execute(db)
    .await?;

    Ok(())
}

async fn close_open_span(db: &PgPool, device_id: &str, at: DateTime<Utc>) -> Result<()> {
    sqlx::query(
        "UPDATE data_activity_presence
            SET ended_at = greatest(ended_at, $1), is_open = false, updated_at = now()
          WHERE device_id = $2 AND is_open",
    )
    .bind(at)
    .bind(device_id)
    .execute(db)
    .await?;

    Ok(())
}

fn title_entry(ev: &Ev) -> Value {
    match &ev.title {
        Some(t) => json!([{ "t": t }]),
        None => json!([]),
    }
}
