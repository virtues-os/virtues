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
    /// The first named senders in the message record, in the order the record
    /// met them. CHRONOLOGY, NEVER SIGNIFICANCE: ranking the people of a life
    /// by frequency would be both wrong and insulting (the interview's own
    /// doctrine), but "the earliest names in it" is the same honest motif as
    /// the oldest date — a fact about the record, not a judgment about the
    /// people. Empty when there are no messages or no presentable names.
    pub earliest_names: Vec<String>,
    /// The date of the first day the box actually narrated, so the client can
    /// point at a real page ("your first day is written up") rather than guess.
    /// None until SEGMENT→NARRATE has finished its first day.
    pub first_day: Option<chrono::NaiveDate>,
}

/// What gets counted, in the order the reveal should read it.
///
/// Ordered by how much a person cares, not by row count: messages before app
/// sessions, always.
///
/// THE LINE IS CONTINUOUS VS DISCRETE, not health vs everything else (refined
/// 2026-08-26). A count is only meaningful when the thing counted is a thing
/// a person could have counted themselves: a night of sleep, a workout, a
/// place they went. Continuous samplings — heart rates, steps, HRV — stay
/// out: "8,814 heart rates" is a machine's idea of a biography, and a
/// sampled stream's row count measures the sampler, not the life.
const SOURCES: &[(&str, &str, &str, &str)] = &[
    // (id, label, table, time column)
    //
    // Time columns follow the 2026-08-17 rename: a moment is `occurred_at`, a
    // span is `started_at`/`ended_at`. The old `timestamp`/`start_time` names
    // are gone, and this list silently counted zero for every source until it
    // caught up — see `count_of` for why that was invisible.
    ("messages", "messages", "data_communication_message", "occurred_at"),
    ("emails", "emails", "data_communication_email", "occurred_at"),
    ("conversations", "conversations", "data_content_conversation", "occurred_at"),
    ("events", "calendar events", "data_calendar_event", "started_at"),
    // Visits, not location points: a person went 462 places, a phone logged two
    // million coordinates. The letter's first promise is "where you went", and
    // this is the line that keeps it.
    ("visits", "places you went", "data_location_visit", "started_at"),
    ("browsing", "pages read", "data_activity_web_browsing", "occurred_at"),
    ("bookmarks", "things saved", "data_content_bookmark", "occurred_at"),
    ("recordings", "recordings", "data_audio_recording", "started_at"),
    ("transactions", "transactions", "data_financial_transaction", "occurred_at"),
    ("sleep", "nights of sleep", "data_health_sleep", "started_at"),
    ("workouts", "workouts", "data_health_workout", "started_at"),
    ("sessions", "app sessions", "data_activity_app_session", "started_at"),
];

/// Counted from the graph rather than the record, so they come last: these are
/// what the box has WORKED OUT, not what it was handed.
const DERIVED: &[(&str, &str, &str)] = &[
    ("people", "people", "wiki_people"),
    ("places", "places", "wiki_places"),
    // Narrated days only. A bare `wiki_days` count lies: `get_or_create_day`
    // inserts a stub row from merely VIEWING a day page, so "days written up"
    // once counted days nobody wrote up. `narrated_at` is the marker the
    // narration queue itself trusts (day_summary.rs).
    ("days", "days written up", "wiki_days WHERE narrated_at IS NOT NULL"),
];

/// Count one table, tolerating its absence but never hiding a failure.
///
/// A missing table is genuinely fine: sources arrive by migration and this list
/// will drift ahead of, or behind, any given box, and a census that 500s over
/// one renamed table is worse than one missing a line.
///
/// But the first version swallowed EVERY error into `0`, and within a day that
/// bit exactly as it deserved to: a schema-wide rename of `timestamp` to
/// `occurred_at` left this list pointing at columns that no longer existed, and
/// the census reported a box with a full record as empty — including to the
/// reveal, which would then show someone the "you have connected nothing"
/// screen. Silence in the face of a real error is how a count becomes a lie.
///
/// So: absence is quiet, everything else is loud.
async fn count_of(pool: &PgPool, table: &str) -> i64 {
    match sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(pool)
        .await
    {
        Ok(n) => n,
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("42P01") => {
            // undefined_table — this box does not have this source. Expected.
            0
        }
        Err(e) => {
            tracing::warn!(table, error = %e, "census: count failed; reporting 0 for this source");
            0
        }
    }
}

/// Oldest and newest value of `col`, ignoring tables that are not there.
async fn span_of(pool: &PgPool, table: &str, col: &str) -> Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> {
    sqlx::query_as::<_, (Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>)>(
        &format!("SELECT min({col}), max({col}) FROM {table}"),
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        if !matches!(&e, sqlx::Error::Database(d) if d.code().as_deref() == Some("42P01")) {
            tracing::warn!(table, col, error = %e, "census: span query failed");
        }
    })
    .ok()
    .and_then(|(lo, hi)| Some((lo?, hi?)))
}

/// The first few human-presentable sender names, ordered by when the record
/// first heard from them. The filter drops raw identifiers (phone numbers,
/// emails, empty strings) rather than showing "+15125550100" as a person.
/// Table absence and any other failure both come back empty — this line is
/// garnish, and garnish must never cost the census.
async fn earliest_names(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT from_name FROM data_communication_message \
         WHERE from_name IS NOT NULL AND from_name <> '' \
           AND from_name ~ '[A-Za-z]' \
           AND from_name !~ '^[+0-9 ().-]+$' \
           AND position('@' in from_name) = 0 \
         GROUP BY from_name \
         ORDER BY min(occurred_at) ASC \
         LIMIT 3",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_else(|e| {
        // Absence is quiet, everything else is loud — same rule as count_of.
        if !matches!(&e, sqlx::Error::Database(d) if d.code().as_deref() == Some("42P01")) {
            tracing::warn!(error = %e, "census: earliest-names query failed; omitting names");
        }
        Vec::new()
    })
}

/// The first narrated day's date. Absence quiet, failure loud — same rule as
/// `count_of`, and same garnish status as `earliest_names`.
async fn first_narrated_day(pool: &PgPool) -> Option<chrono::NaiveDate> {
    sqlx::query_scalar::<_, Option<chrono::NaiveDate>>(
        "SELECT min(date) FROM wiki_days WHERE narrated_at IS NOT NULL",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        if !matches!(&e, sqlx::Error::Database(d) if d.code().as_deref() == Some("42P01")) {
            tracing::warn!(error = %e, "census: first-narrated-day query failed; omitting");
        }
    })
    .ok()
    .flatten()
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
        earliest_names: earliest_names(pool).await,
        first_day: first_narrated_day(pool).await,
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
