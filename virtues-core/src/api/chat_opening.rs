//! The opening turn: what the box says first on an empty new chat.
//!
//! The new chat screen is where a person lands, so the box opens the
//! conversation with one or two sentences about the most notable thing in
//! their day, read from the same computed present a chat turn gets
//! (`circumstances`) plus today's events ranked by how unlike the owner's
//! usual they were. When nothing stands out the model answers NONE and the
//! screen stays empty.
//!
//! Openings are held in memory per part of the day, so opening new chats
//! costs nothing after the first. One is written again when today's events
//! change, at most once an hour, so a day that had nothing to say at noon can
//! still get a sentence after the afternoon's events land. Replying to one stores
//! it as the chat's first message (`store_user_turn`). The client sends only
//! the opening's id, and the text is read from this cache, so a client cannot
//! put words in the box's mouth.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, NaiveDate, Timelike, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use sqlx::PgPool;

use crate::error::Result;

/// How many openings stay resolvable by id. A chat replies to the opening it
/// was shown; a few windows of history covers a tab left open across them.
const KEEP: usize = 8;

/// At most this many events go to the writer, the most unusual first.
const MAX_EVENTS: usize = 12;

/// The least time between two openings in one part of the day.
const REWRITE_AFTER: std::time::Duration = std::time::Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Serialize)]
pub struct Opening {
    /// Null when the day has nothing worth opening with.
    pub id: Option<String>,
    pub text: Option<String>,
    /// The event the sentence is about, so the screen can mark it.
    pub event_id: Option<String>,
}

#[derive(Clone)]
struct Held {
    window: String,
    /// Today's events when this was written (`events_fingerprint`).
    fingerprint: String,
    written: std::time::Instant,
    id: String,
    text: Option<String>,
    event_id: Option<String>,
}

fn held() -> &'static Mutex<VecDeque<Held>> {
    static HELD: OnceLock<Mutex<VecDeque<Held>>> = OnceLock::new();
    HELD.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// One writer at a time, so two tabs opening at once make one model call.
fn writing() -> &'static tokio::sync::Mutex<()> {
    static W: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    W.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// The part of the day an opening belongs to: the date plus morning,
/// afternoon or evening, where the owner is.
fn window_key(date: NaiveDate, hour: u32) -> String {
    let part = match hour {
        0..=11 => "morning",
        12..=16 => "afternoon",
        _ => "evening",
    };
    format!("{date}-{part}")
}

/// The opening's text by id, if this box wrote it and still holds it.
pub fn text_for(id: &str) -> Option<String> {
    let held = held().lock().expect("opening lock poisoned");
    held.iter().find(|h| h.id == id).and_then(|h| h.text.clone())
}

/// The opening for now, writing it if this part of the day has none yet.
pub async fn current(pool: &PgPool) -> Result<Opening> {
    let timezone = crate::api::profile::get_timezone(pool).await?;
    let tz: Option<Tz> = timezone.as_deref().and_then(|t| t.parse().ok());
    let now = Utc::now();
    let (today, hour) = match tz {
        Some(tz) => {
            let local = now.with_timezone(&tz);
            (local.date_naive(), local.hour())
        }
        None => (now.date_naive(), now.hour()),
    };
    let window = window_key(today, hour);
    let fingerprint = events_fingerprint(pool, today).await?;

    if let Some(found) = lookup(&window, &fingerprint) {
        return Ok(found);
    }
    let _guard = writing().lock().await;
    if let Some(found) = lookup(&window, &fingerprint) {
        return Ok(found);
    }

    let (text, event_id) = write(pool, timezone.as_deref(), tz, today, now).await?;
    let id = format!("opening_{}", uuid::Uuid::new_v4().simple());
    let entry = Held { window, fingerprint, written: std::time::Instant::now(), id, text, event_id };
    {
        let mut held = held().lock().expect("opening lock poisoned");
        held.push_back(entry.clone());
        while held.len() > KEEP {
            held.pop_front();
        }
    }
    Ok(as_opening(&entry))
}

/// The opening held for this part of the day, unless today's events have
/// changed since and it is old enough to write again.
fn lookup(window: &str, fingerprint: &str) -> Option<Opening> {
    let held = held().lock().expect("opening lock poisoned");
    held.iter()
        .rev()
        .find(|h| h.window == window)
        .filter(|h| h.fingerprint == fingerprint || h.written.elapsed() < REWRITE_AFTER)
        .map(as_opening)
}

/// How many events today has and when the latest changed. Cheap enough to
/// read on every visit.
async fn events_fingerprint(pool: &PgPool, today: NaiveDate) -> Result<String> {
    let (count, latest): (i64, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT count(e.id), max(e.updated_at) FROM wiki_days d \
         LEFT JOIN wiki_events e ON e.day_id = d.id WHERE d.date = $1",
    )
    .bind(today)
    .fetch_one(pool)
    .await?;
    // absent-ok: no latest change means today has no events yet.
    Ok(format!("{count}:{}", latest.map(|t| t.timestamp_micros()).unwrap_or(0)))
}

fn as_opening(h: &Held) -> Opening {
    match &h.text {
        Some(text) => Opening { id: Some(h.id.clone()), text: Some(text.clone()), event_id: h.event_id.clone() },
        None => Opening { id: None, text: None, event_id: None },
    }
}

struct EventLine {
    id: String,
    line: String,
}

/// Today's events, most unlike the owner's usual first, as numbered lines.
async fn todays_events(pool: &PgPool, today: NaiveDate, tz: Option<Tz>) -> Result<Vec<EventLine>> {
    // Read the day without creating it: an empty new chat must not write.
    let day_id: Option<String> = sqlx::query_scalar("SELECT id FROM wiki_days WHERE date = $1")
        .bind(today)
        .fetch_optional(pool)
        .await?;
    let Some(day_id) = day_id else { return Ok(Vec::new()) };
    let mut events = crate::api::wiki_events::get_day_events(pool, day_id).await?;
    events.retain(|e| {
        // absent-ok: these are nullable columns on a row already read; a null
        // flag means the event was never marked hidden, unknown or sleep.
        !e.user_hidden.unwrap_or(false) && !e.is_unknown.unwrap_or(false) && !e.is_sleep.unwrap_or(false)
    });
    let score = |e: &crate::api::wiki_events::TemporalEvent| e.local_novelty_z.or(e.novelty_z);
    events.sort_by(|a, b| {
        score(b)
            .unwrap_or(f64::MIN)
            .partial_cmp(&score(a).unwrap_or(f64::MIN))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let clock = |t: DateTime<Utc>| match tz {
        Some(tz) => t.with_timezone(&tz).format("%-I:%M %p").to_string(),
        None => t.format("%H:%M UTC").to_string(),
    };
    Ok(events
        .into_iter()
        .take(MAX_EVENTS)
        .map(|e| {
            let label = e.user_label.clone().or(e.auto_label.clone()).unwrap_or_else(|| "Untitled".into());
            let place = e.user_location.clone().or(e.auto_location.clone());
            let unusual = match score(&e) {
                Some(z) if z >= 1.0 => "unlike their usual",
                Some(_) => "like their usual",
                None => "not scored",
            };
            let mut line = format!("{} to {}: {}", clock(e.start_time), clock(e.end_time), label);
            if let Some(place) = place {
                line.push_str(&format!(" at {place}"));
            }
            line.push_str(&format!(" ({unusual})"));
            if let Some(summary) = e.event_summary.as_deref().filter(|s| !s.trim().is_empty()) {
                let summary: String = summary.chars().take(240).collect();
                line.push_str(&format!(". {summary}"));
            }
            EventLine { id: e.id, line }
        })
        .collect())
}

const SYSTEM: &str = "You are the person's own server. You have read their record, and they have just opened a new conversation with you. Write the first thing you say.

Write one or two plain sentences to them, in the second person, about the single most notable thing in their day: something today that was unlike their usual, or something just ahead on their calendar that matters. Name the specific thing so they recognize it at once. Say only what the record below shows.

Do not greet them, ask how they are, give advice, praise them, or mention counts, totals, distances or durations. Do not use em dashes. Stay under 40 words.

Begin your reply with the number of the event you are writing about in square brackets, or [0] if it is about the calendar or nothing in the list. If nothing in their day is worth opening with, reply with exactly NONE.";

async fn write(
    pool: &PgPool,
    timezone: Option<&str>,
    tz: Option<Tz>,
    today: NaiveDate,
    now: DateTime<Utc>,
) -> Result<(Option<String>, Option<String>)> {
    let events = todays_events(pool, today, tz).await?;
    // The same quarter-hour floor a chat turn uses, so the block reads alike.
    let quantized = now.with_minute(now.minute() / 15 * 15).and_then(|t| t.with_second(0)).unwrap_or(now);
    let circumstances = crate::api::circumstances::build_circumstances(pool, timezone, quantized).await;

    if events.is_empty() && circumstances.is_none() {
        return Ok((None, None));
    }

    let mut prompt = String::new();
    if let Some(block) = circumstances {
        prompt.push_str(block.trim());
        prompt.push_str("\n\n");
    }
    prompt.push_str("<today>\nToday's events so far, the most unlike their usual first:\n");
    if events.is_empty() {
        prompt.push_str("(none yet)\n");
    }
    for (i, e) in events.iter().enumerate() {
        prompt.push_str(&format!("[{}] {}\n", i + 1, e.line));
    }
    prompt.push_str("</today>");

    let reply = crate::virtues_api::completion::system_completion(
        pool,
        virtues_registry::models::ModelSlot::Standard,
        "chat_opening",
        SYSTEM,
        &prompt,
        crate::virtues_api::request::Thinking::Off,
        0.7,
    )
    .await?;
    Ok(parse(&reply, &events))
}

/// Split the model's reply into the sentence and the event it names.
fn parse(reply: &str, events: &[EventLine]) -> (Option<String>, Option<String>) {
    let reply = reply.trim().trim_matches('"').trim();
    if reply.is_empty() || reply.eq_ignore_ascii_case("none") {
        return (None, None);
    }
    let (event_id, rest) = match reply.strip_prefix('[').and_then(|r| r.split_once(']')) {
        Some((n, rest)) => {
            let id = n.trim().parse::<usize>().ok().filter(|n| *n > 0).and_then(|n| events.get(n - 1)).map(|e| e.id.clone());
            (id, rest.trim())
        }
        None => (None, reply),
    };
    let text = rest.trim().trim_matches('"').trim().replace('—', ", ");
    if text.is_empty() || text.eq_ignore_ascii_case("none") {
        return (None, None);
    }
    (Some(text), event_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events() -> Vec<EventLine> {
        vec![
            EventLine { id: "evt_a".into(), line: "first".into() },
            EventLine { id: "evt_b".into(), line: "second".into() },
        ]
    }

    #[test]
    fn the_bracketed_number_names_the_event_and_is_not_shown() {
        let (text, id) = parse("[2] You spent the morning somewhere new.", &events());
        assert_eq!(text.as_deref(), Some("You spent the morning somewhere new."));
        assert_eq!(id.as_deref(), Some("evt_b"));
    }

    #[test]
    fn zero_or_a_number_past_the_list_names_no_event() {
        assert_eq!(parse("[0] Your flight leaves at noon.", &events()).1, None);
        assert_eq!(parse("[9] Something.", &events()).1, None);
    }

    #[test]
    fn none_means_the_screen_stays_empty() {
        assert_eq!(parse("NONE", &events()), (None, None));
        assert_eq!(parse("[0] NONE", &events()), (None, None));
    }

    #[test]
    fn a_window_is_the_local_date_and_part_of_day() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 4).unwrap();
        assert_eq!(window_key(d, 9), "2026-03-04-morning");
        assert_eq!(window_key(d, 12), "2026-03-04-afternoon");
        assert_eq!(window_key(d, 22), "2026-03-04-evening");
    }
}
