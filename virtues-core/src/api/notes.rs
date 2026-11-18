//! Notes: what your server noticed about your tasks and threads while you
//! can still act on it. The Threads door's Insights view lists them, a
//! thread's or task's preview shows the ones about it, and home's notes line
//! shows them in turn.
//!
//! A note is worked out each time it is read, from the owner's tasks,
//! threads and calendar, so it leaves on its own once its condition clears
//! or its moment passes. Its key names the condition, so "Not now"
//! (`app_note_dismissals`) sets aside that condition only; a changed one is a
//! new note. Every rule here needs no model to find. The sentence is fixed
//! wording for now.
//!
//! The rules:
//!
//! - Two calendar events in the coming week, filed in different threads,
//!   overlap.
//! - An open task in a thread is due within three days, and no calendar time
//!   is set aside for that thread before the end of its day.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;
use sqlx::PgPool;

use crate::api::day_summary::day_bounds;
use crate::error::Result;

/// How far ahead a clash is looked for.
const CLASH_DAYS: i64 = 7;
/// How soon a task's day has to be for a missing block of time to matter.
const UNPLANNED_DAYS: i64 = 3;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Note {
    /// Stable while the condition holds.
    pub key: String,
    /// `clash` | `unplanned`.
    pub kind: &'static str,
    /// The note, in one sentence.
    pub text: String,
    /// What it is about, as routes (`/thread/…`, `/task/…`); the first is
    /// the one it is listed under.
    pub subjects: Vec<String>,
    /// The first subject's name, for the line under the sentence.
    pub about: String,
    /// When it matters, in words: "Saturday at 10:00 AM", "By Thursday".
    pub when: String,
    /// The one thing to do about it, and where that is.
    pub action: String,
    pub action_route: String,
    /// When its moment passes and it leaves on its own.
    pub until: DateTime<Utc>,
}

/// One note with what it rests on, for its page.
#[derive(Debug, Clone, Serialize)]
pub struct NoteDetail {
    #[serde(flatten)]
    pub note: Note,
    /// Set aside with "Not now". Its page still opens.
    pub is_dismissed: bool,
    /// The kind, in words: "A calendar clash".
    pub kind_label: &'static str,
    /// Why it was raised, in one sentence.
    pub why: String,
    /// How it was found.
    pub found_by: &'static str,
    /// The calendar entries it rests on.
    pub events: Vec<NoteEvent>,
    /// The task it is about, when it is about one.
    pub task: Option<NoteTask>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct NoteEvent {
    pub id: String,
    pub title: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub thread_id: String,
    pub thread_name: String,
    pub thread_icon: Option<String>,
    pub thread_color: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct NoteTask {
    pub id: String,
    pub title: String,
    pub due_on: NaiveDate,
    pub thread_id: String,
    pub thread_name: String,
}

pub async fn notes(pool: &PgPool) -> Result<Vec<Note>> {
    notes_at(pool, Utc::now()).await
}

pub async fn notes_at(pool: &PgPool, now: DateTime<Utc>) -> Result<Vec<Note>> {
    let (mut out, dismissed) = every_note(pool, now).await?;
    out.retain(|n| !dismissed.contains(&n.key));
    Ok(out)
}

/// Every note whose condition holds, and the keys set aside.
async fn every_note(pool: &PgPool, now: DateTime<Utc>) -> Result<(Vec<Note>, Vec<String>)> {
    let zone = crate::timezone::home_timezone_or_utc(pool, now.date_naive()).await;
    let tz = zone.parse::<chrono_tz::Tz>().ok();
    let today = tz.map(|tz| now.with_timezone(&tz).date_naive()).unwrap_or_else(|| now.date_naive());

    let mut out = clashes(pool, now, tz).await?;
    out.extend(unplanned(pool, now, today, &zone).await?);

    let dismissed: Vec<String> = sqlx::query_scalar("SELECT key FROM app_note_dismissals")
        .fetch_all(pool)
        .await?;
    Ok((out, dismissed))
}

/// One note and what it rests on. A note whose condition has cleared is
/// gone, and says so.
pub async fn note(pool: &PgPool, key: &str) -> Result<NoteDetail> {
    note_at(pool, key, Utc::now()).await
}

pub async fn note_at(pool: &PgPool, key: &str, now: DateTime<Utc>) -> Result<NoteDetail> {
    let (all, dismissed) = every_note(pool, now).await?;
    let note = all.into_iter().find(|n| n.key == key).ok_or_else(|| {
        crate::error::Error::NotFound(
            "This note has gone: what it was about has changed or its moment has passed.".into(),
        )
    })?;
    let is_dismissed = dismissed.contains(&note.key);
    let mut parts = note.key.split(':');
    let kind = parts.next().unwrap_or_default();
    if kind == "clash" {
        let ids: Vec<String> = parts.map(str::to_string).collect();
        let events: Vec<NoteEvent> = sqlx::query_as(
            "SELECT DISTINCT ON (e.id) e.id, e.title, e.started_at, e.ended_at,
                    t.id AS thread_id, t.name AS thread_name, t.icon AS thread_icon, t.accent_color AS thread_color
             FROM data_calendar_event e
             JOIN app_thread_items i ON i.url = '/record/calendar_event/' || e.id
             JOIN app_threads t ON t.id = i.thread_id AND t.deleted_at IS NULL
             WHERE e.id = ANY($1)
             ORDER BY e.id, i.added_at",
        )
        .bind(&ids)
        .fetch_all(pool)
        .await?;
        let mut events = events;
        events.sort_by_key(|e| e.started_at);
        return Ok(NoteDetail {
            why: "Two entries on your calendar overlap, and they belong to different threads.".into(),
            kind_label: "A calendar clash",
            found_by: "A rule that compares your calendar entries. The sentence is fixed wording.",
            note,
            is_dismissed,
            events,
            task: None,
        });
    }
    let task_id = parts.next().unwrap_or_default().to_string();
    let task: Option<NoteTask> = sqlx::query_as(
        "SELECT t.id, t.title, t.due_on, th.id AS thread_id, th.name AS thread_name
         FROM app_tasks t JOIN app_threads th ON th.id = t.thread_id
         WHERE t.id = $1 AND t.due_on IS NOT NULL",
    )
    .bind(&task_id)
    .fetch_optional(pool)
    .await?;
    let why = match &task {
        Some(t) => format!(
            "It is due soon, and nothing on your calendar is filed in {} between now and then.",
            t.thread_name
        ),
        None => "It is due soon, and nothing on your calendar is set aside for it.".into(),
    };
    Ok(NoteDetail {
        why,
        kind_label: "No time set aside",
        found_by: "A rule that compares a task's day with the calendar entries in its thread. The sentence is fixed wording.",
        note,
        is_dismissed,
        events: Vec::new(),
        task,
    })
}

/// Set a note aside. Its condition changing makes a new note, which shows.
pub async fn dismiss(pool: &PgPool, key: &str) -> Result<()> {
    sqlx::query("INSERT INTO app_note_dismissals (key) VALUES ($1) ON CONFLICT (key) DO NOTHING")
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}

/// Bring a set-aside note back.
pub async fn restore(pool: &PgPool, key: &str) -> Result<()> {
    sqlx::query("DELETE FROM app_note_dismissals WHERE key = $1").bind(key).execute(pool).await?;
    Ok(())
}

fn at_label(at: DateTime<Utc>, tz: Option<chrono_tz::Tz>) -> String {
    match tz {
        Some(tz) => at.with_timezone(&tz).format("%A at %-I:%M %p").to_string(),
        None => at.format("%A at %-I:%M %p UTC").to_string(),
    }
}

fn day_name(day: NaiveDate, today: NaiveDate) -> String {
    if day == today {
        "today".into()
    } else if day == today + Duration::days(1) {
        "tomorrow".into()
    } else {
        day.format("%A").to_string()
    }
}

type ClashRow = (String, String, String, String, DateTime<Utc>, String, String, String, String, DateTime<Utc>);

async fn clashes(pool: &PgPool, now: DateTime<Utc>, tz: Option<chrono_tz::Tz>) -> Result<Vec<Note>> {
    let rows: Vec<ClashRow> = sqlx::query_as(
        "WITH filed AS (
             SELECT i.thread_id, t.name AS thread_name, e.id, e.title, e.started_at, e.ended_at
             FROM app_thread_items i
             JOIN app_threads t ON t.id = i.thread_id AND t.deleted_at IS NULL AND t.archived_at IS NULL
             JOIN data_calendar_event e ON i.url = '/record/calendar_event/' || e.id
             WHERE e.ended_at > $1 AND e.started_at < $2 AND NOT e.is_all_day
         )
         SELECT a.thread_id, a.thread_name, a.id, a.title, a.started_at,
                b.thread_id, b.thread_name, b.id, b.title, b.started_at
         FROM filed a
         JOIN filed b ON a.id < b.id AND a.thread_id <> b.thread_id
                     AND a.started_at < b.ended_at AND b.started_at < a.ended_at
         ORDER BY LEAST(a.started_at, b.started_at)",
    )
    .bind(now)
    .bind(now + Duration::days(CLASH_DAYS))
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            // The event that starts first leads the sentence and lists the note.
            let (a_thread, a_name, a_id, a_title, a_start, b_thread, b_name, b_id, b_title, b_start) =
                if row.9 < row.4 { (row.5, row.6, row.7, row.8, row.9, row.0, row.1, row.2, row.3, row.4) } else { row };
            // The overlap begins when the later of the two starts.
            let start = a_start.max(b_start);
            let day = tz.map(|tz| start.with_timezone(&tz).date_naive()).unwrap_or_else(|| start.date_naive());
            Note {
                key: format!("clash:{}:{}", a_id.as_str().min(b_id.as_str()), a_id.as_str().max(b_id.as_str())),
                kind: "clash",
                text: format!("{a_title} for {a_name} and {b_title} for {b_name} overlap on {}.", at_label(start, tz)),
                subjects: vec![format!("/thread/{a_thread}"), format!("/thread/{b_thread}")],
                about: a_name,
                when: at_label(start, tz),
                action: "See the day".into(),
                action_route: format!("/day/day_{}", day.format("%Y-%m-%d")),
                until: start,
            }
        })
        .collect())
}

async fn unplanned(pool: &PgPool, now: DateTime<Utc>, today: NaiveDate, zone: &str) -> Result<Vec<Note>> {
    let rows: Vec<(String, String, NaiveDate, String, String)> = sqlx::query_as(
        "SELECT t.id, t.title, t.due_on, t.thread_id, th.name
         FROM app_tasks t
         JOIN app_threads th ON th.id = t.thread_id AND th.deleted_at IS NULL AND th.archived_at IS NULL
         WHERE t.assignee = 'owner' AND t.status IN ('open', 'active')
           AND t.due_on BETWEEN $1 AND $2
         ORDER BY t.due_on, t.sort_order",
    )
    .bind(today)
    .bind(today + Duration::days(UNPLANNED_DAYS))
    .fetch_all(pool)
    .await?;

    let mut out = Vec::new();
    for (id, title, due, thread_id, thread_name) in rows {
        let (_, end_of_day) = day_bounds(due, Some(zone));
        let set_aside: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM app_thread_items i
                 JOIN data_calendar_event e ON i.url = '/record/calendar_event/' || e.id
                 WHERE i.thread_id = $1 AND e.ended_at > $2 AND e.started_at < $3 AND NOT e.is_all_day
             )",
        )
        .bind(&thread_id)
        .bind(now)
        .bind(end_of_day)
        .fetch_one(pool)
        .await?;
        if set_aside {
            continue;
        }
        let day = day_name(due, today);
        out.push(Note {
            key: format!("unplanned:{id}:{due}"),
            kind: "unplanned",
            text: format!("{title} is due {day}, and no time is set aside for {thread_name} before then."),
            subjects: vec![format!("/task/{id}"), format!("/thread/{thread_id}")],
            about: title,
            when: format!("By {}", day),
            action: "Open the task".into(),
            action_route: format!("/task/{id}"),
            until: end_of_day,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().to_utc()
    }

    async fn event(pool: &PgPool, id: &str, title: &str, start: &str, end: &str, thread: &str) {
        sqlx::query(
            "INSERT INTO data_calendar_event (id, title, started_at, ended_at, source_stream_id, source_table, source_provider)
             VALUES ($1, $2, $3, $4, 'stream_' || $1, 'google_calendar', 'google')",
        )
        .bind(id)
        .bind(title)
        .bind(at(start))
        .bind(at(end))
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO app_thread_items (thread_id, url) VALUES ($1, '/record/calendar_event/' || $2)")
            .bind(thread)
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    /// Overlapping events in two threads make one note about both, a task
    /// due soon with nothing set aside makes another, and "Not now" sets a
    /// note aside until its condition changes.
    #[sqlx::test(migrations = "./migrations")]
    async fn notes_come_from_clashes_and_unplanned_tasks(pool: PgPool) {
        sqlx::query(
            "INSERT INTO app_threads (id, name) VALUES ('nb_hall', 'Repaint the hallway'), ('nb_trip', 'Plan the trip'), ('nb_tax', 'Tax paperwork')",
        )
        .execute(&pool)
        .await
        .unwrap();
        // Wednesday 2030-01-09 at noon, UTC.
        let now = at("2030-01-09T12:00:00Z");
        event(&pool, "ev_paint", "Second coat", "2030-01-12T10:00:00Z", "2030-01-12T13:00:00Z", "nb_hall").await;
        event(&pool, "ev_call", "Call with the agent", "2030-01-12T12:00:00Z", "2030-01-12T12:30:00Z", "nb_trip").await;
        event(&pool, "ev_later", "Pack", "2030-01-12T15:00:00Z", "2030-01-12T16:00:00Z", "nb_trip").await;

        let task = |title: &str, thread: &str, due: &str| crate::api::tasks::NewTask {
            title: title.into(),
            thread_id: Some(thread.into()),
            due_on: Some(due.parse().unwrap()),
            ..Default::default()
        };
        crate::api::tasks::create_tasks(
            &pool,
            vec![
                task("File the receipts", "nb_tax", "2030-01-10"),
                task("Buy tape", "nb_hall", "2030-01-12"),
                task("Book the ferry", "nb_tax", "2030-02-01"),
            ],
            crate::api::tasks::Origin::Owner,
            &crate::api::tasks::Writer::owner(),
        )
        .await
        .unwrap();

        let found = notes_at(&pool, now).await.unwrap();
        let clash = found.iter().find(|n| n.kind == "clash").expect("a clash");
        assert_eq!(clash.subjects, ["/thread/nb_hall", "/thread/nb_trip"]);
        assert_eq!(clash.action_route, "/day/day_2030-01-12");
        assert!(clash.text.contains("Second coat") && clash.text.contains("Call with the agent"), "{}", clash.text);

        // Receipts are due tomorrow with nothing set aside for Tax paperwork.
        // Buy tape has the second coat set aside before its day, and the
        // ferry is too far off.
        let unplanned: Vec<&Note> = found.iter().filter(|n| n.kind == "unplanned").collect();
        assert_eq!(unplanned.len(), 1);
        assert_eq!(unplanned[0].about, "File the receipts");
        assert_eq!(unplanned[0].when, "By tomorrow");

        dismiss(&pool, &clash.key).await.unwrap();
        assert!(notes_at(&pool, now).await.unwrap().iter().all(|n| n.kind != "clash"));
        restore(&pool, &clash.key).await.unwrap();
        assert!(notes_at(&pool, now).await.unwrap().iter().any(|n| n.kind == "clash"));

        // Its page has both events, in order, with their threads, and still
        // opens once it is set aside.
        dismiss(&pool, &clash.key).await.unwrap();
        let detail = note_at(&pool, &clash.key, now).await.unwrap();
        assert!(detail.is_dismissed);
        assert_eq!(detail.events.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), ["Second coat", "Call with the agent"]);
        assert_eq!(detail.events[1].thread_name, "Plan the trip");
        let task_note = note_at(&pool, &unplanned[0].key, now).await.unwrap();
        assert_eq!(task_note.task.as_ref().map(|t| t.thread_name.as_str()), Some("Tax paperwork"));
        restore(&pool, &clash.key).await.unwrap();

        // Once both events have passed, the clash has gone on its own, and
        // its page says so.
        assert!(notes_at(&pool, at("2030-01-13T00:00:00Z")).await.unwrap().iter().all(|n| n.kind != "clash"));
        assert!(note_at(&pool, &clash.key, at("2030-01-13T00:00:00Z")).await.is_err());
    }
}
