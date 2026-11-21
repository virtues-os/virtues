//! Fathom meetings sync.
//!
//! Cron-driven, per-credential. Reads the person's meetings from Fathom with
//! the key they pasted and stores each as one transcript row
//! (`virtues_applets::meeting`).
//!
//! Each run asks for meetings created since the saved watermark less three
//! days, so a meeting whose transcript or summary finished after the last run
//! is read again and updated. The first run reads 90 days back. A meeting
//! with neither a transcript nor a summary yet is skipped without failing.
//!
//! A Fathom key also returns meetings shared with the person's team, so only
//! meetings the person recorded or was invited to are kept, matched on the
//! address in `secrets.email`. Only those are archived to the lake.

mod transform;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use virtues::storage::lake;
use virtues_applets::meeting::{self, Written};
use virtues_helpers::{connect_from_env, output_with_records, read_input};

const ACTION: &str = "fathom_meetings_sync";
const MEETINGS_URL: &str = "https://api.fathom.ai/external/v1/meetings";
/// How far before the watermark each run starts reading.
const OVERLAP_DAYS: i64 = 3;
/// Runaway guard. A run that reaches it does not move the watermark, so the
/// next run reads the same window again and the run summary says so.
const MAX_PAGES: usize = 200;

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-fathom_meetings_sync").await?;
    let api_key = virtues_applets::secret(&input, "api_key")?.trim().to_string();
    let email = virtues_applets::secret(&input, "email")?.trim().to_string();
    if email.is_empty() {
        bail!("the Fathom connection has no email; reconnect Fathom with the address you sign in with");
    }
    let storage = lake::storage_from_env()?;
    let client = virtues_applets::http_client();

    let now = Utc::now();
    let from = meeting::read_from(&input, chrono::Duration::days(OVERLAP_DAYS), now)
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();

    let mut written = Written::default();
    let mut not_ready = 0usize;
    let mut newest: Option<DateTime<Utc>> = None;
    let mut cursor: Option<String> = None;
    let mut finished = false;

    for _ in 0..MAX_PAGES {
        let page = meeting::send_ok(
            || {
                let mut req = client
                    .get(MEETINGS_URL)
                    .header("X-Api-Key", &api_key)
                    .query(&[
                        ("include_transcript", "true"),
                        ("include_summary", "true"),
                        ("include_action_items", "true"),
                        ("created_after", from.as_str()),
                    ]);
                if let Some(c) = &cursor {
                    req = req.query(&[("cursor", c.as_str())]);
                }
                req
            },
            "Fathom",
        )
        .await?;

        let items = page.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let mut theirs: Vec<Value> = Vec::new();
        let mut meetings = Vec::new();
        for item in &items {
            if let Some(at) = transform::created_at(item) {
                newest = newest.max(Some(at));
            }
            match transform::shape(item, &email) {
                transform::Shaped::Meeting(m) => {
                    meetings.push(*m);
                    theirs.push(item.clone());
                }
                transform::Shaped::NotReady => not_ready += 1,
                transform::Shaped::NotTheirs => {}
                transform::Shaped::Invalid => tracing::warn!("Fathom sent a meeting without an id or a start time"),
            }
        }
        if !theirs.is_empty() {
            lake::archive_cloud(&pool, &storage, transform::PROVIDER, ACTION, "meetings", &theirs).await?;
        }
        let w = meeting::write_meetings(&pool, &meetings).await?;
        written.added += w.added;
        written.changed += w.changed;

        match page.get("next_cursor").and_then(Value::as_str).filter(|c| !c.is_empty()) {
            Some(next) => cursor = Some(next.to_string()),
            None => {
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
        (0, 0) => "no new Fathom meetings".to_string(),
        (a, c) => format!("synced {a} new Fathom meetings, {c} updated"),
    };
    if not_ready > 0 {
        summary.push_str(&format!("; {not_ready} not processed by Fathom yet"));
    }
    if !finished {
        summary.push_str(&format!("; stopped at the {MAX_PAGES}-page limit, the next run reads the same window"));
    }
    output_with_records(&summary, &input.config, (written.added + written.changed) as i64)
}
