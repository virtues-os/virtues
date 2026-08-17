//! The census — what the box actually holds, counted.
//!
//! WHY THIS EXISTS. Onboarding ends on a screen called "Meet yourself", and
//! until now that screen showed a paragraph of machine-written prose about a
//! person who had just spent an hour writing prose about themselves. Everything
//! on it was something they had said. The one thing a reveal has to do is show
//! someone what the box FOUND — the half they did not supply — and the cheapest,
//! truest form of that is a count.
//!
//! "Your box holds 41,000 messages, and the oldest is from March 2015" is
//! verifiable, impossible to fake, and is precisely the thing they paid for. The
//! oldest date does the most work: most people have no idea their Mac has been
//! keeping messages for a decade, and a specific date is the moment an appliance
//! stops being an abstraction.
//!
//! ONLY WHAT IS THERE. Lines with a zero count are dropped rather than shown as
//! "0 emails" — a census of absences is a list of reproaches, and someone who
//! deliberately skipped a source does not need it listed back at them. A box
//! with nothing connected returns an empty `lines`, which the reveal is required
//! to handle as its own screen rather than as a degenerate case of this one.
//!
//! RUNTIME-CHECKED QUERIES, deliberately: `sqlx::query_scalar` rather than the
//! `query!` macro. The macros need a live database or an up-to-date `.sqlx`
//! cache, and `make dev` builds with `SQLX_OFFLINE=true` — a macro here would
//! wedge every other agent's build the moment this landed.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use sqlx::PgPool;

use crate::error::Result;
use crate::server::AppState;

/// One line of the census: a thing the box holds, and how many of it.
#[derive(Debug, Serialize)]
pub struct CensusLine {
    /// Stable key, for the client to order or ignore. Never shown.
    pub id: String,
    /// Plural, lowercase, in the words someone would use. Not a table name.
    pub label: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct Census {
    pub lines: Vec<CensusLine>,
    /// Everything in `lines`, added up.
    pub total: i64,
    /// The oldest trace on the box, and the newest.
    pub earliest: Option<chrono::DateTime<chrono::Utc>>,
    pub latest: Option<chrono::DateTime<chrono::Utc>>,
    /// Whole days between the two. The span the record covers, not the number
    /// of days that have anything in them — a gap is still inside the span.
    pub span_days: i64,
}

/// What gets counted, in the order the reveal should read it.
///
/// Ordered by how much a person cares, not by row count: messages before app
/// sessions, always. Health samples are deliberately ABSENT — a heart-rate
/// table with 8,814 rows in it would dominate every other number on the screen
/// while meaning the least, and "8,814 heart rates" is a machine's idea of a
/// biography.
const SOURCES: &[(&str, &str, &str, &str)] = &[
    // (id, label, table, time column)
    ("messages", "messages", "data_communication_message", "timestamp"),
    ("emails", "emails", "data_communication_email", "timestamp"),
    ("conversations", "conversations", "data_content_conversation", "timestamp"),
    ("events", "calendar events", "data_calendar_event", "start_time"),
    ("browsing", "pages read", "data_activity_web_browsing", "timestamp"),
    ("bookmarks", "things saved", "data_content_bookmark", "timestamp"),
    ("recordings", "recordings", "data_audio_recording", "started_at"),
    ("transactions", "transactions", "data_financial_transaction", "timestamp"),
    ("sessions", "app sessions", "data_activity_app_session", "start_time"),
];

/// Counted from the graph rather than the record, so they come last: these are
/// what the box has WORKED OUT, not what it was handed.
const DERIVED: &[(&str, &str, &str)] = &[
    ("people", "people", "wiki_people"),
    ("places", "places", "wiki_places"),
    ("days", "days written up", "wiki_days"),
];

/// Count one table, tolerating its absence.
///
/// A missing table is not an error here. Sources arrive by migration and this
/// list will drift ahead of, or behind, any given box — and a census that 500s
/// because one table was renamed is worse than a census missing a line.
async fn count_of(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

/// Oldest and newest value of `col`, ignoring tables that are not there.
async fn span_of(pool: &PgPool, table: &str, col: &str) -> Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> {
    sqlx::query_as::<_, (Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>)>(
        &format!("SELECT min({col}), max({col}) FROM {table}"),
    )
    .fetch_one(pool)
    .await
    .ok()
    .and_then(|(lo, hi)| Some((lo?, hi?)))
}

pub async fn census(pool: &PgPool) -> Result<Census> {
    let mut lines: Vec<CensusLine> = Vec::new();
    let mut earliest: Option<chrono::DateTime<chrono::Utc>> = None;
    let mut latest: Option<chrono::DateTime<chrono::Utc>> = None;

    for (id, label, table, col) in SOURCES {
        let count = count_of(pool, table).await;
        if count == 0 {
            continue;
        }
        if let Some((lo, hi)) = span_of(pool, table, col).await {
            earliest = Some(earliest.map_or(lo, |cur| cur.min(lo)));
            latest = Some(latest.map_or(hi, |cur| cur.max(hi)));
        }
        lines.push(CensusLine {
            id: (*id).to_string(),
            label: (*label).to_string(),
            count,
        });
    }

    for (id, label, table) in DERIVED {
        let count = count_of(pool, table).await;
        if count == 0 {
            continue;
        }
        lines.push(CensusLine {
            id: (*id).to_string(),
            label: (*label).to_string(),
            count,
        });
    }

    let total = lines.iter().map(|l| l.count).sum();
    let span_days = match (earliest, latest) {
        (Some(lo), Some(hi)) => (hi - lo).num_days().max(0),
        _ => 0,
    };

    Ok(Census {
        lines,
        total,
        earliest,
        latest,
        span_days,
    })
}

/// Authenticated, like everything else that reads the record. The counts are
/// small numbers, but they describe a person's life in aggregate and there is
/// no reason for them to be readable before a session exists.
pub async fn census_handler(
    State(state): State<AppState>,
    _user: crate::middleware::auth::AuthUser,
) -> impl IntoResponse {
    match census(state.db.pool()).await {
        Ok(c) => (StatusCode::OK, Json(c)).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "census failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}
