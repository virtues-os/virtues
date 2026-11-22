//! Granola meetings sync.
//!
//! Cron-driven, per-credential. Reads the person's notes from Granola's
//! public API with the key they pasted and stores each as one transcript row
//! (`virtues_applets::meeting`).
//!
//! `GET /v1/notes` filters on `updated_after`, so each run asks for notes
//! changed since the saved watermark less a few minutes; the first run reads
//! 90 days back. Granola lists only notes that have a summary, so every
//! listed note is ready. The list rows are thin, so each note is then read
//! with its transcript. A transcript too large to come inline (413) is read
//! in pages from `/v1/notes/{id}/transcript`.
//!
//! Private notes are removed before a note is archived or shaped. A note
//! deleted in Granola is skipped.

mod transform;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use virtues::storage::lake;
use virtues_applets::meeting::{self, Answer, Written};
use virtues_helpers::{connect_from_env, output_with_records, read_input};

const ACTION: &str = "granola_meetings_sync";
const API: &str = "https://public-api.granola.ai/v1";
/// Granola's largest list page.
const PAGE_SIZE: &str = "30";
/// Granola's largest transcript page.
const TRANSCRIPT_PAGE_SIZE: &str = "100";
/// How far before the watermark each run starts reading. Granola's filter is
/// exact, so this only covers clock differences between the two servers.
const OVERLAP_MINUTES: i64 = 10;
/// Runaway guards. A run that reaches the page limit does not move the
/// watermark, so the next run reads the same window again and the run
/// summary says so.
const MAX_PAGES: usize = 100;
const MAX_TRANSCRIPT_PAGES: usize = 200;

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-granola_meetings_sync").await?;
    let api_key = virtues_applets::secret(&input, "api_key")?.trim().to_string();
    let storage = lake::storage_from_env()?;
    let client = virtues_applets::http_client();
    let granola = Granola { client: &client, api_key: &api_key };

    let now = Utc::now();
    let from = meeting::read_from(&input, chrono::Duration::minutes(OVERLAP_MINUTES), now)
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();

    let mut written = Written::default();
    let mut newest: Option<DateTime<Utc>> = None;
    let mut cursor: Option<String> = None;
    let mut finished = false;

    for _ in 0..MAX_PAGES {
        let page = meeting::send_ok(
            || {
                let mut req = client
                    .get(format!("{API}/notes"))
                    .bearer_auth(&api_key)
                    .query(&[("updated_after", from.as_str()), ("page_size", PAGE_SIZE)]);
                if let Some(c) = &cursor {
                    req = req.query(&[("cursor", c.as_str())]);
                }
                req
            },
            "Granola",
        )
        .await?;

        let listed = page.get("notes").and_then(Value::as_array).cloned().unwrap_or_default();
        let mut notes: Vec<Value> = Vec::new();
        for row in &listed {
            if let Some(at) = transform::updated_at(row) {
                newest = newest.max(Some(at));
            }
            if row.get("deleted_at").is_some_and(|d| !d.is_null()) {
                continue;
            }
            let Some(id) = meeting::text_at(row, "/id") else { continue };
            if let Some(note) = granola.note(id).await? {
                notes.push(transform::without_private_notes(note));
            }
        }
        if !notes.is_empty() {
            lake::archive_cloud(&pool, &storage, transform::PROVIDER, ACTION, "notes", &notes).await?;
        }

        let mut meetings = Vec::new();
        for note in &notes {
            match transform::shape(note) {
                transform::Shaped::Meeting(m) => meetings.push(*m),
                transform::Shaped::Invalid => tracing::warn!("Granola sent a note without an id or a time"),
            }
        }
        let w = meeting::write_meetings(&pool, &meetings).await?;
        written.added += w.added;
        written.changed += w.changed;

        let more = page.get("hasMore").and_then(Value::as_bool).unwrap_or(false);
        match page.get("cursor").and_then(Value::as_str).filter(|c| !c.is_empty()) {
            Some(next) if more => cursor = Some(next.to_string()),
            _ => {
                finished = true;
                break;
            }
        }
    }

    if finished {
        meeting::advance(&mut input, newest, now);
    } else {
        tracing::warn!(pages = MAX_PAGES, "page limit reached; the watermark stays where it was");
    }

    let mut summary = match (written.added, written.changed) {
        (0, 0) => "no new Granola meetings".to_string(),
        (a, c) => format!("synced {a} new Granola meetings, {c} updated"),
    };
    if !finished {
        summary.push_str(&format!("; stopped at the {MAX_PAGES}-page limit, the next run reads the same window"));
    }
    output_with_records(&summary, &input.config, (written.added + written.changed) as i64)
}

struct Granola<'a> {
    client: &'a reqwest::Client,
    api_key: &'a str,
}

impl Granola<'_> {
    /// One note with its transcript, or `None` when Granola no longer lets
    /// this key read it (404: deleted, unshared, or its summary was removed).
    async fn note(&self, id: &str) -> Result<Option<Value>> {
        let url = format!("{API}/notes/{}", urlencoding::encode(id));
        let with_transcript =
            meeting::send(|| self.client.get(&url).bearer_auth(self.api_key).query(&[("include", "transcript")]), "Granola")
                .await?;
        match with_transcript {
            Answer::Json(note) => Ok(Some(note)),
            Answer::Status(404, _) => Ok(None),
            // TRANSCRIPT_TOO_LARGE: read the note alone, then its transcript
            // in pages.
            Answer::Status(413, _) => {
                let mut note = match meeting::send(|| self.client.get(&url).bearer_auth(self.api_key), "Granola").await? {
                    Answer::Json(note) => note,
                    Answer::Status(404, _) => return Ok(None),
                    Answer::Status(status, body) => bail!("Granola answered {status} for note {id}: {body}"),
                };
                note["transcript"] = Value::Array(self.transcript(id).await?);
                Ok(Some(note))
            }
            Answer::Status(status, body) => bail!("Granola answered {status} for note {id}: {body}"),
        }
    }

    /// A note's whole transcript, page by page.
    async fn transcript(&self, id: &str) -> Result<Vec<Value>> {
        let url = format!("{API}/notes/{}/transcript", urlencoding::encode(id));
        let mut lines = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_TRANSCRIPT_PAGES {
            let page = meeting::send_ok(
                || {
                    let mut req =
                        self.client.get(&url).bearer_auth(self.api_key).query(&[("page_size", TRANSCRIPT_PAGE_SIZE)]);
                    if let Some(c) = &cursor {
                        req = req.query(&[("cursor", c.as_str())]);
                    }
                    req
                },
                "Granola",
            )
            .await?;
            if let Some(items) = page.get("transcript").and_then(Value::as_array) {
                lines.extend(items.iter().cloned());
            }
            let more = page.get("hasMore").and_then(Value::as_bool).unwrap_or(false);
            match page.get("cursor").and_then(Value::as_str).filter(|c| !c.is_empty()) {
                Some(next) if more => cursor = Some(next.to_string()),
                _ => return Ok(lines),
            }
        }
        bail!("note {id} has a transcript longer than {MAX_TRANSCRIPT_PAGES} pages")
    }
}
