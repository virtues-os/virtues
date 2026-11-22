//! Meetings from a meeting tool (Fathom, Fireflies.ai, Granola) as rows of
//! `data_communication_transcription`.
//!
//! Each vendor applet turns its API's answer into a [`Meeting`], and
//! [`write_meetings`] stores one row per meeting. The row is marked
//! `source_table = 'meeting'` ([`virtues::transcript::MEETING_TABLE`]), so
//! readers that measure this server's own recorded audio leave it out.
//!
//! The module also holds what the three applets share around the writer: the
//! watermark they read from, and an HTTP call that waits out "too many
//! requests" and fails clearly when the vendor refuses the pasted key.

use std::collections::HashSet;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use virtues::transcript::MEETING_TABLE;
use virtues_helpers::dedup::{build_batch_upsert_changed_query, dedup_refs_keep_last, BATCH_SIZE};
use virtues_helpers::AppletInput;

/// One line of a meeting transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub speaker: String,
    pub text: String,
}

/// Someone invited to or present at a meeting.
#[derive(Debug, Clone, PartialEq)]
pub struct Attendee {
    pub name: Option<String>,
    pub email: Option<String>,
}

/// A meeting as a meeting tool reports it, before it becomes a row.
#[derive(Debug, Clone, PartialEq)]
pub struct Meeting {
    /// `fathom`, `fireflies` or `granola`; stored as `source_provider`.
    pub provider: &'static str,
    /// The vendor's own id for the meeting.
    pub vendor_id: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub transcript: Vec<Line>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub attendees: Vec<Attendee>,
    pub action_items: Vec<String>,
    /// Where the person can open the meeting in the vendor's app.
    pub url: Option<String>,
}

/// One `data_communication_transcription` row.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub source_stream_id: String,
    pub provider: &'static str,
    pub text: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<f64>,
    pub speaker_count: Option<i32>,
    pub metadata: Value,
}

/// What one call to [`write_meetings`] did.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Written {
    pub added: usize,
    pub changed: usize,
    /// Meetings with neither a transcript nor a summary, which get no row.
    pub empty: usize,
}

/// The stored text of a transcript: `[Speaker name]: text`, one line per
/// change of speaker. Consecutive lines by the same speaker are joined with a
/// space. Lines with no text are dropped, and a blank speaker is written as
/// `Speaker`.
pub fn transcript_text(lines: &[Line]) -> String {
    let mut turns: Vec<(String, String)> = Vec::new();
    for line in lines {
        let text = line.text.trim();
        if text.is_empty() {
            continue;
        }
        let speaker = match line.speaker.trim() {
            "" => "Speaker",
            s => s,
        };
        match turns.last_mut() {
            Some((last, said)) if last == speaker => {
                said.push(' ');
                said.push_str(text);
            }
            _ => turns.push((speaker.to_string(), text.to_string())),
        }
    }
    turns.iter().map(|(s, t)| format!("[{s}]: {t}")).collect::<Vec<_>>().join("\n")
}

/// The row for a meeting, or `None` when it has neither a transcript nor a
/// summary to store.
pub fn shape(m: &Meeting) -> Option<Row> {
    let spoken = transcript_text(&m.transcript);
    let summary = m.summary.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from);
    let text = if !spoken.is_empty() { spoken.clone() } else { summary.clone()? };

    let speaker_count = (!spoken.is_empty()).then(|| {
        let speakers: HashSet<&str> = m
            .transcript
            .iter()
            .filter(|l| !l.text.trim().is_empty())
            .map(|l| match l.speaker.trim() {
                "" => "Speaker",
                s => s,
            })
            .collect();
        speakers.len() as i32
    });
    let duration_seconds = m
        .ended_at
        .filter(|end| *end > m.started_at)
        .map(|end| (end - m.started_at).num_milliseconds() as f64 / 1000.0);
    let attendees: Vec<Value> = m.attendees.iter().map(|a| json!({ "name": a.name, "email": a.email })).collect();

    Some(Row {
        source_stream_id: format!("{}:{}", m.provider, m.vendor_id),
        provider: m.provider,
        text,
        title: m.title.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from),
        summary,
        started_at: m.started_at,
        ended_at: m.ended_at,
        duration_seconds,
        speaker_count,
        metadata: json!({ "url": m.url, "attendees": attendees, "action_items": m.action_items }),
    })
}

/// Store meetings, one row each. A meeting read again with nothing changed is
/// left alone; one whose transcript or summary arrived since is updated.
pub async fn write_meetings(db: &PgPool, meetings: &[Meeting]) -> Result<Written> {
    let mut written = Written::default();
    let mut rows: Vec<Row> = Vec::with_capacity(meetings.len());
    for m in meetings {
        match shape(m) {
            Some(row) => rows.push(row),
            None => written.empty += 1,
        }
    }
    // One statement cannot update the same row twice.
    let rows = dedup_refs_keep_last(&rows, |r| r.source_stream_id.clone());
    for chunk in rows.chunks(BATCH_SIZE) {
        let (added, changed) = flush(db, chunk).await?;
        written.added += added;
        written.changed += changed;
    }
    Ok(written)
}

/// Upsert rows, returning how many were inserted and how many changed.
async fn flush(db: &PgPool, rows: &[&Row]) -> Result<(usize, usize)> {
    if rows.is_empty() {
        return Ok((0, 0));
    }
    let sql = build_batch_upsert_changed_query(
        "data_communication_transcription",
        &[
            "id",
            "text",
            "title",
            "summary",
            "started_at",
            "ended_at",
            "duration_seconds",
            "speaker_count",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        &["text", "title", "summary", "started_at", "ended_at", "duration_seconds", "speaker_count", "metadata"],
        rows.len(),
    );
    let mut q = sqlx::query_scalar::<_, bool>(&sql);
    for r in rows {
        // A new id only matters on insert; an existing row keeps its own.
        q = q
            .bind(Uuid::new_v4().to_string())
            .bind(&r.text)
            .bind(&r.title)
            .bind(&r.summary)
            .bind(r.started_at)
            .bind(r.ended_at)
            .bind(r.duration_seconds)
            .bind(r.speaker_count)
            .bind(&r.source_stream_id)
            .bind(MEETING_TABLE)
            .bind(r.provider)
            .bind(&r.metadata);
    }
    let inserted = q.fetch_all(db).await.context("could not store meetings")?;
    let added = inserted.iter().filter(|i| **i).count();
    Ok((added, inserted.len() - added))
}

/// Where the watermark lives in the applet's config.
pub const WATERMARK_KEY: &str = "watermark";

/// How far back the first run reads.
pub const FIRST_RUN_LOOKBACK_DAYS: i64 = 90;

/// The time to read meetings from: the saved watermark less `overlap`, so a
/// meeting whose processing finished after the last run is read again, or
/// [`FIRST_RUN_LOOKBACK_DAYS`] back on a first run.
pub fn read_from(input: &AppletInput, overlap: chrono::Duration, now: DateTime<Utc>) -> DateTime<Utc> {
    match watermark(input) {
        Some(w) => w - overlap,
        None => now - chrono::Duration::days(FIRST_RUN_LOOKBACK_DAYS),
    }
}

fn watermark(input: &AppletInput) -> Option<DateTime<Utc>> {
    input.config.get(WATERMARK_KEY).and_then(Value::as_str).and_then(parse_time)
}

/// Move the saved watermark forward to `newest`, the latest time this run
/// read. It never moves back, and never past `now`, so a vendor clock that
/// runs ahead cannot make later runs skip meetings.
pub fn advance(input: &mut AppletInput, newest: Option<DateTime<Utc>>, now: DateTime<Utc>) {
    let Some(newest) = newest.map(|n| n.min(now)) else { return };
    if watermark(input).is_none_or(|w| newest > w) {
        input.config[WATERMARK_KEY] = Value::String(newest.to_rfc3339());
    }
}

/// An RFC 3339 time as a UTC instant.
pub fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s.trim()).ok().map(|d| d.with_timezone(&Utc))
}

/// A string field, trimmed, when it has text.
pub fn text_at<'a>(v: &'a Value, pointer: &str) -> Option<&'a str> {
    v.pointer(pointer).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

/// What a vendor answered when it did not refuse the key.
#[derive(Debug)]
pub enum Answer {
    Json(Value),
    /// Any other non-2xx status, with the body for the error message.
    Status(u16, String),
}

/// How many times one request waits out "too many requests". Fathom can drop
/// to five calls a minute, so this covers about two minutes of waiting at the
/// longest single wait.
const RATE_LIMIT_TRIES: u32 = 8;

/// Send a request built by `build`, waiting out 429s as `Retry-After` asks
/// (within [`crate::MAX_RATE_LIMIT_WAIT_SECS`] each). A 401 or 403 fails with
/// a sentence telling the person to paste a new key for `vendor`; any other
/// non-2xx comes back as [`Answer::Status`].
pub async fn send(build: impl Fn() -> reqwest::RequestBuilder, vendor: &str) -> Result<Answer> {
    let mut tries = 0;
    loop {
        let resp = build().send().await.with_context(|| format!("{vendor} request failed"))?;
        let status = resp.status();
        if status.is_success() {
            let body = resp.json().await.with_context(|| format!("{vendor} answer was not JSON"))?;
            return Ok(Answer::Json(body));
        }
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            anyhow::bail!("the {vendor} key was refused ({status}); paste a new one");
        }
        tries += 1;
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS && tries < RATE_LIMIT_TRIES {
            tokio::time::sleep(crate::rate_limit_wait(resp.headers().get(reqwest::header::RETRY_AFTER))).await;
            continue;
        }
        // The body only explains the failure; one that cannot be read leaves
        // the status to say what happened.
        let body = resp.text().await.unwrap_or_default();
        return Ok(Answer::Status(status.as_u16(), body.chars().take(300).collect()));
    }
}

/// [`send`] that treats every non-2xx as an error.
pub async fn send_ok(build: impl Fn() -> reqwest::RequestBuilder, vendor: &str) -> Result<Value> {
    match send(build, vendor).await? {
        Answer::Json(v) => Ok(v),
        Answer::Status(status, body) => anyhow::bail!("{vendor} answered {status}: {body}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(speaker: &str, text: &str) -> Line {
        Line { speaker: speaker.into(), text: text.into() }
    }

    fn meeting() -> Meeting {
        Meeting {
            provider: "fathom",
            vendor_id: "123".into(),
            title: Some("Launch review".into()),
            summary: Some("We agreed to ship on Friday.".into()),
            transcript: vec![
                line("Nick", "Morning."),
                line("Nick", "Shall we start?"),
                line("David Okafor", "Yes."),
                line("Nick", "Good."),
            ],
            started_at: "2026-11-18T15:00:00Z".parse().unwrap(),
            ended_at: Some("2026-11-18T15:30:00Z".parse().unwrap()),
            attendees: vec![
                Attendee { name: Some("Nick".into()), email: Some("nick@example.com".into()) },
                Attendee { name: None, email: Some("david@example.com".into()) },
            ],
            action_items: vec!["Nick: send the deck".into()],
            url: Some("https://meetings.example.com/123".into()),
        }
    }

    #[test]
    fn a_transcript_is_one_tagged_line_per_change_of_speaker() {
        let lines = [
            line("Nick", "Morning."),
            line("Nick", " Shall we start? "),
            line("David Okafor", ""),
            line("David Okafor", "Yes."),
            line("", "Who is that?"),
            line("Nick", "Good."),
        ];
        assert_eq!(
            transcript_text(&lines),
            "[Nick]: Morning. Shall we start?\n[David Okafor]: Yes.\n[Speaker]: Who is that?\n[Nick]: Good."
        );
        assert_eq!(transcript_text(&[]), "");
    }

    #[test]
    fn a_meeting_becomes_a_row() {
        let r = shape(&meeting()).unwrap();
        assert_eq!(r.source_stream_id, "fathom:123");
        assert_eq!(r.provider, "fathom");
        assert_eq!(r.text, "[Nick]: Morning. Shall we start?\n[David Okafor]: Yes.\n[Nick]: Good.");
        assert_eq!(r.title.as_deref(), Some("Launch review"));
        assert_eq!(r.summary.as_deref(), Some("We agreed to ship on Friday."));
        assert_eq!(r.duration_seconds, Some(1800.0));
        assert_eq!(r.speaker_count, Some(2));
        assert_eq!(
            r.metadata,
            json!({
                "url": "https://meetings.example.com/123",
                "attendees": [
                    { "name": "Nick", "email": "nick@example.com" },
                    { "name": null, "email": "david@example.com" }
                ],
                "action_items": ["Nick: send the deck"]
            })
        );
    }

    #[test]
    fn without_a_transcript_the_summary_is_the_text_and_without_either_there_is_no_row() {
        let mut m = meeting();
        m.transcript = vec![line("Nick", "  ")];
        let r = shape(&m).unwrap();
        assert_eq!(r.text, "We agreed to ship on Friday.");
        assert_eq!(r.speaker_count, None);

        m.summary = Some(" ".into());
        assert_eq!(shape(&m), None);
        m.summary = None;
        assert_eq!(shape(&m), None);
    }

    #[test]
    fn an_end_before_the_start_gives_no_duration() {
        let mut m = meeting();
        m.ended_at = Some("2026-11-18T14:00:00Z".parse().unwrap());
        assert_eq!(shape(&m).unwrap().duration_seconds, None);
        m.ended_at = None;
        assert_eq!(shape(&m).unwrap().duration_seconds, None);
    }

    fn input(config: Value) -> AppletInput {
        serde_json::from_value(json!({ "config": config })).unwrap()
    }

    #[test]
    fn the_watermark_is_read_with_an_overlap_and_only_moves_forward() {
        let now: DateTime<Utc> = "2026-11-21T12:00:00Z".parse().unwrap();
        let mut first = input(json!({}));
        assert_eq!(read_from(&first, chrono::Duration::days(3), now), "2026-08-23T12:00:00Z".parse::<DateTime<Utc>>().unwrap());

        advance(&mut first, Some("2026-11-20T09:00:00Z".parse().unwrap()), now);
        assert_eq!(read_from(&first, chrono::Duration::days(3), now), "2026-11-17T09:00:00Z".parse::<DateTime<Utc>>().unwrap());

        // Older, absent and future times.
        advance(&mut first, Some("2026-11-01T00:00:00Z".parse().unwrap()), now);
        advance(&mut first, None, now);
        assert_eq!(first.config[WATERMARK_KEY], "2026-11-20T09:00:00+00:00");
        advance(&mut first, Some("2027-01-01T00:00:00Z".parse().unwrap()), now);
        assert_eq!(first.config[WATERMARK_KEY], "2026-11-21T12:00:00+00:00");
    }

    #[tokio::test]
    async fn a_refused_key_fails_and_too_many_requests_is_waited_out() {
        let (base, seen) = crate::tests::canned(vec![
            ("429 Too Many Requests", "Retry-After: 1\r\n", "{}"),
            ("200 OK", "", r#"{"items":[]}"#),
            ("401 Unauthorized", "", r#"{"error":"bad key"}"#),
            ("404 Not Found", "", r#"{"error":"NOTE_NOT_FOUND"}"#),
        ])
        .await;
        let client = reqwest::Client::new();
        let url = format!("{base}/meetings");
        let got = send_ok(|| client.get(&url).header("X-Api-Key", "k"), "Fathom").await.unwrap();
        assert_eq!(got, json!({ "items": [] }));
        assert_eq!(seen.lock().unwrap().len(), 2);

        let err = send_ok(|| client.get(&url), "Fathom").await.unwrap_err();
        assert!(err.to_string().contains("the Fathom key was refused"), "{err}");

        match send(|| client.get(&url), "Granola").await.unwrap() {
            Answer::Status(404, body) => assert!(body.contains("NOTE_NOT_FOUND")),
            other => panic!("expected a 404, got {other:?}"),
        }
    }

    /// Inserted once, left alone when read again unchanged, and updated when
    /// the transcript arrives after the summary.
    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn a_meeting_is_written_once_and_updated_only_when_it_changes(pool: PgPool) {
        let mut m = meeting();
        m.transcript.clear();
        let empty = Meeting { vendor_id: "456".into(), summary: None, ..m.clone() };

        let first = write_meetings(&pool, &[m.clone(), m.clone(), empty]).await.unwrap();
        assert_eq!(first, Written { added: 1, changed: 0, empty: 1 });

        let again = write_meetings(&pool, &[m.clone()]).await.unwrap();
        assert_eq!(again, Written::default(), "an unchanged meeting is not rewritten");

        let (id, text): (String, String) = sqlx::query_as(
            "SELECT id, text FROM data_communication_transcription WHERE source_stream_id = 'fathom:123'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(text, "We agreed to ship on Friday.");

        let full = meeting();
        let changed = write_meetings(&pool, &[full]).await.unwrap();
        assert_eq!(changed, Written { added: 0, changed: 1, empty: 0 });

        let row: (String, String, String, String, Option<i32>, Option<f64>, Value) = sqlx::query_as(
            "SELECT id, text, source_table, source_provider, speaker_count, duration_seconds, metadata \
             FROM data_communication_transcription WHERE source_stream_id = 'fathom:123'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, id, "the row keeps its id");
        assert!(row.1.starts_with("[Nick]: Morning."));
        assert_eq!((row.2.as_str(), row.3.as_str()), (MEETING_TABLE, "fathom"));
        assert_eq!((row.4, row.5), (Some(2), Some(1800.0)));
        assert_eq!(row.6["action_items"], json!(["Nick: send the deck"]));
    }
}
