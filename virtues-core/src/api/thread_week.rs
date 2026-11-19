//! The week of threads: for each active thread and each day of a week, the
//! minutes spent and the minutes ahead, and each day's recorded time that is
//! in no thread. The Threads door draws it as dots, one per fifteen minutes.
//!
//! It counts only what needs no model to place:
//!
//! - A record with a start and an end (a calendar event, a workout, any
//!   `data_*` row with `started_at` and `ended_at`) that the owner added to the
//!   thread. The part before now is spent; the part after is ahead.
//! - The thread's own tasks. A task has no length, so it is one dot: an open
//!   task with a date is ahead on that day (or today, once its day has
//!   passed, since it is still to do), and a task finished that week is spent
//!   on the day it was finished. A task counts only in its home.
//! - A calendar event in no thread is recorded time in no thread, on the days
//!   before now.
//!
//! A span of twelve hours or more is left out, because an all-day or
//! multi-day event says when something was, not how long it took.

use std::collections::HashMap;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::api::day_summary::day_bounds;
use crate::error::{Error, Result};

/// Minutes in one dot, and what a task counts as.
pub const DOT_MINUTES: i64 = 15;

/// Longer than this, a span is not time spent.
const LONGEST_SPAN_HOURS: i64 = 12;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WeekDay {
    pub date: NaiveDate,
    pub done_minutes: i64,
    pub ahead_minutes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadWeek {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    pub days: Vec<WeekDay>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Week {
    pub start: NaiveDate,
    pub today: NaiveDate,
    pub threads: Vec<ThreadWeek>,
    /// Recorded minutes in no thread, one per day.
    pub unfiled_minutes: Vec<i64>,
}

/// The seven days from `start`, in the box's home timezone.
pub async fn week(pool: &PgPool, start: NaiveDate) -> Result<Week> {
    week_at(pool, start, Utc::now()).await
}

pub async fn week_at(pool: &PgPool, start: NaiveDate, now: DateTime<Utc>) -> Result<Week> {
    let zone = crate::timezone::home_timezone_or_utc(pool, start).await;
    let days: Vec<NaiveDate> = (0..7).map(|i| start + Duration::days(i)).collect();
    let bounds: Vec<(DateTime<Utc>, DateTime<Utc>)> = days.iter().map(|d| day_bounds(*d, Some(&zone))).collect();
    let today = zone
        .parse::<chrono_tz::Tz>()
        .map(|tz| now.with_timezone(&tz).date_naive())
        .unwrap_or_else(|_| now.date_naive());
    let (week_start, week_end) = (bounds[0].0, bounds[6].1);

    let threads: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id, name, icon, accent_color FROM app_threads
         WHERE deleted_at IS NULL AND archived_at IS NULL AND NOT is_backlog
         ORDER BY sort_order ASC, updated_at DESC",
    )
    .fetch_all(pool)
    .await?;
    let ids: Vec<String> = threads.iter().map(|t| t.0.clone()).collect();
    let mut tally: HashMap<String, Vec<WeekDay>> = ids
        .iter()
        .map(|id| (id.clone(), days.iter().map(|d| WeekDay { date: *d, ..Default::default() }).collect()))
        .collect();

    // Records with a span, added to a thread by hand.
    for (thread_id, started, ended) in filed_spans(pool, &ids, week_start, week_end).await? {
        if let Some(row) = tally.get_mut(&thread_id) {
            add_span(row, &bounds, started, ended, now);
        }
    }

    // Each thread's own tasks: one dot each.
    let tasks: Vec<(String, Option<NaiveDate>, Option<DateTime<Utc>>, String)> = sqlx::query_as(
        "SELECT thread_id, due_on, closed_at, status FROM app_tasks
         WHERE thread_id = ANY($1)
           AND ((status IN ('open', 'active', 'ready') AND due_on IS NOT NULL)
                OR (status = 'done' AND closed_at >= $2 AND closed_at < $3))",
    )
    .bind(&ids)
    .bind(week_start)
    .bind(week_end)
    .fetch_all(pool)
    .await?;
    for (thread_id, due_on, closed_at, status) in tasks {
        let Some(row) = tally.get_mut(&thread_id) else { continue };
        if status == "done" {
            if let Some(i) = closed_at.and_then(|at| bounds.iter().position(|(s, e)| at >= *s && at < *e)) {
                row[i].done_minutes += DOT_MINUTES;
            }
        } else if let Some(due) = due_on {
            let day = due.max(today);
            if let Some(i) = days.iter().position(|d| *d == day) {
                row[i].ahead_minutes += DOT_MINUTES;
            }
        }
    }

    // Calendar time in no thread, before now.
    let unfiled_spans: Vec<(DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT e.started_at, e.ended_at FROM data_calendar_event e
         WHERE e.started_at < $2 AND e.ended_at > $1 AND NOT e.is_all_day
           AND NOT EXISTS (
               SELECT 1 FROM app_thread_items i
               JOIN app_threads t ON t.id = i.thread_id AND t.deleted_at IS NULL
               WHERE i.url = '/record/calendar_event/' || e.id
           )",
    )
    .bind(week_start)
    .bind(week_end)
    .fetch_all(pool)
    .await?;
    let mut unfiled: Vec<WeekDay> = days.iter().map(|d| WeekDay { date: *d, ..Default::default() }).collect();
    for (started, ended) in unfiled_spans {
        add_span(&mut unfiled, &bounds, started, ended, now);
    }

    Ok(Week {
        start,
        today,
        threads: threads
            .into_iter()
            .map(|(id, name, icon, accent_color)| ThreadWeek {
                days: tally.remove(&id).unwrap_or_default(),
                id,
                name,
                icon,
                accent_color,
            })
            .collect(),
        unfiled_minutes: unfiled.into_iter().map(|d| d.done_minutes).collect(),
    })
}

/// Split a span over the days it touches: before `now` is spent, after is
/// ahead.
fn add_span(
    row: &mut [WeekDay],
    bounds: &[(DateTime<Utc>, DateTime<Utc>)],
    started: DateTime<Utc>,
    ended: DateTime<Utc>,
    now: DateTime<Utc>,
) {
    if ended <= started || ended - started >= Duration::hours(LONGEST_SPAN_HOURS) {
        return;
    }
    for (i, (day_start, day_end)) in bounds.iter().enumerate() {
        let (from, to) = (started.max(*day_start), ended.min(*day_end));
        if to <= from {
            continue;
        }
        let split = now.clamp(from, to);
        row[i].done_minutes += (split - from).num_minutes();
        row[i].ahead_minutes += (to - split).num_minutes();
    }
}

/// `(thread, started_at, ended_at)` for every record with a span that is in
/// one of `threads` and touches the window. Only tables with both columns
/// are read.
async fn filed_spans(
    pool: &PgPool,
    threads: &[String],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<(String, DateTime<Utc>, DateTime<Utc>)>> {
    let items: Vec<(String, String)> = sqlx::query_as(
        "SELECT thread_id, url FROM app_thread_items WHERE thread_id = ANY($1) AND url LIKE '/record/%'",
    )
    .bind(threads)
    .fetch_all(pool)
    .await?;
    let mut by_table: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (thread_id, url) in items {
        if let Some((table, id)) = crate::api::record_refs::record_table(&url) {
            by_table.entry(table).or_default().push((thread_id, id));
        }
    }

    let mut out = Vec::new();
    for (table, members) in by_table {
        let columns = crate::api::record_refs::columns_of(pool, &table).await?;
        if !columns.contains("started_at") || !columns.contains("ended_at") {
            continue;
        }
        let record_ids: Vec<String> = members.iter().map(|(_, id)| id.clone()).collect();
        // `table` is `data_` plus a checked name (`record_table`), and
        // information_schema found it above.
        let rows: Vec<(String, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(&format!(
            "SELECT id, started_at, ended_at FROM {table}
             WHERE id = ANY($1) AND started_at < $3 AND ended_at > $2"
        ))
        .bind(&record_ids)
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to read spans from {table}: {e}")))?;
        let span_of: HashMap<String, (DateTime<Utc>, DateTime<Utc>)> =
            rows.into_iter().map(|(id, s, e)| (id, (s, e))).collect();
        for (thread_id, id) in members {
            if let Some((s, e)) = span_of.get(&id) {
                out.push((thread_id, *s, *e));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tasks::{create_tasks, update_task, NewTask, Origin, Status, TaskPatch, Writer};

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().to_utc()
    }

    async fn event(pool: &PgPool, id: &str, start: &str, end: &str) {
        sqlx::query(
            "INSERT INTO data_calendar_event (id, title, started_at, ended_at, source_stream_id, source_table, source_provider)
             VALUES ($1, 'Event', $2, $3, 'stream_' || $1, 'google_calendar', 'google')",
        )
        .bind(id)
        .bind(at(start))
        .bind(at(end))
        .execute(pool)
        .await
        .unwrap();
    }

    /// A filed event splits at now into spent and ahead, a dated task is a
    /// dot ahead, a finished one a dot spent, a past open task waits on
    /// today, and an unfiled event is time in no thread. Backlog threads
    /// stay out, and so does an all-day span.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_week_counts_filed_time_and_tasks(pool: PgPool) {
        sqlx::query(
            "INSERT INTO app_threads (id, name, is_backlog)
             VALUES ('nb_hall', 'Repaint the hallway', false), ('nb_later', 'Learn to bake', true)",
        )
        .execute(&pool)
        .await
        .unwrap();
        // Monday 2030-01-07 to Sunday 2030-01-13, UTC. Now is Wednesday at noon.
        let start = NaiveDate::from_ymd_opt(2030, 1, 7).unwrap();
        let now = at("2030-01-09T12:00:00Z");

        event(&pool, "ev_paint", "2030-01-09T11:00:00Z", "2030-01-09T13:00:00Z").await;
        event(&pool, "ev_lunch", "2030-01-08T12:00:00Z", "2030-01-08T13:00:00Z").await;
        event(&pool, "ev_trip", "2030-01-10T00:00:00Z", "2030-01-11T00:00:00Z").await;
        for url in ["/record/calendar_event/ev_paint", "/record/calendar_event/ev_trip"] {
            sqlx::query("INSERT INTO app_thread_items (thread_id, url) VALUES ('nb_hall', $1)")
                .bind(url)
                .execute(&pool)
                .await
                .unwrap();
        }

        let task = |title: &str, due: Option<&str>| NewTask {
            title: title.into(),
            thread_id: Some("nb_hall".into()),
            due_on: due.map(|d| d.parse().unwrap()),
            ..Default::default()
        };
        let made = create_tasks(
            &pool,
            vec![task("Buy tape", Some("2030-01-11")), task("Pick a color", Some("2030-01-07")), task("Sand", None)],
            Origin::Owner,
            &Writer::owner(),
        )
        .await
        .unwrap();
        update_task(&pool, &made[2].id, TaskPatch { status: Some(Status::Done), ..Default::default() }, &Writer::owner())
            .await
            .unwrap();
        sqlx::query("UPDATE app_tasks SET closed_at = '2030-01-08T10:00:00Z' WHERE id = $1")
            .bind(&made[2].id)
            .execute(&pool)
            .await
            .unwrap();

        let week = week_at(&pool, start, now).await.unwrap();
        assert_eq!(week.threads.len(), 1, "a backlog thread is not in the week");
        let hall = &week.threads[0].days;
        // The filed event: an hour before noon spent, an hour after ahead.
        assert_eq!((hall[2].done_minutes, hall[2].ahead_minutes - DOT_MINUTES), (60, 60));
        // "Pick a color" was due Monday and is still open, so it waits on today.
        assert_eq!(hall[2].ahead_minutes, 60 + DOT_MINUTES);
        assert_eq!(hall[4].ahead_minutes, DOT_MINUTES, "Buy tape is a dot on Friday");
        assert_eq!(hall[3], WeekDay { date: start + Duration::days(3), ..Default::default() }, "an all-day span is left out");
        assert_eq!(hall[1].done_minutes, DOT_MINUTES, "Sand was finished on Tuesday");
        assert_eq!(hall.iter().map(|d| d.done_minutes).sum::<i64>(), 60 + DOT_MINUTES);
        assert_eq!(week.unfiled_minutes[1], 60, "lunch is time in no thread");
        assert_eq!(week.today, NaiveDate::from_ymd_opt(2030, 1, 9).unwrap());
    }

    #[test]
    fn a_span_splits_over_midnight() {
        let bounds = vec![
            (at("2030-01-07T00:00:00Z"), at("2030-01-08T00:00:00Z")),
            (at("2030-01-08T00:00:00Z"), at("2030-01-09T00:00:00Z")),
        ];
        let mut row = vec![WeekDay::default(), WeekDay::default()];
        add_span(&mut row, &bounds, at("2030-01-07T23:00:00Z"), at("2030-01-08T01:00:00Z"), at("2030-01-08T00:30:00Z"));
        assert_eq!((row[0].done_minutes, row[0].ahead_minutes), (60, 0));
        assert_eq!((row[1].done_minutes, row[1].ahead_minutes), (30, 30));
    }
}
