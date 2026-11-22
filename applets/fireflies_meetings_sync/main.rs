//! Fireflies.ai meetings sync.
//!
//! Cron-driven, per-credential. Reads the person's meetings from the
//! Fireflies GraphQL API with the key they pasted and stores each as one
//! transcript row (`virtues_applets::meeting`).
//!
//! The first call looks up the key owner's `user_id`, which is kept in config.
//! `transcripts(user_id:)` then returns the meetings that person organized or
//! joined. Each run asks for meetings since the saved watermark less three
//! days, so a meeting Fireflies finished processing after the last run is
//! read again and updated. The first run reads 90 days back. Live meetings
//! and ones still processing are skipped without failing.

mod transform;

use anyhow::{anyhow, bail, Result};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use virtues::storage::lake;
use virtues_applets::meeting::{self, Written};
use virtues_helpers::{connect_from_env, output_with_records, read_input};

const ACTION: &str = "fireflies_meetings_sync";
const GRAPHQL_URL: &str = "https://api.fireflies.ai/graphql";
/// Transcripts per page. Each carries every sentence of its meeting, so pages
/// stay small.
const PAGE_SIZE: usize = 10;
/// How far before the watermark each run starts reading.
const OVERLAP_DAYS: i64 = 3;
/// Runaway guard. A run that reaches it does not move the watermark, so the
/// next run reads the same window again and the run summary says so.
const MAX_PAGES: usize = 100;

const USER_QUERY: &str = "query { user { user_id email } }";
const TRANSCRIPTS_QUERY: &str = "query Transcripts($userId: String, $fromDate: DateTime, $limit: Int, $skip: Int) {
  transcripts(user_id: $userId, fromDate: $fromDate, limit: $limit, skip: $skip) {
    id title dateString duration transcript_url is_live
    meeting_attendees { displayName email name }
    summary { overview short_summary action_items }
    sentences { text speaker_name }
    meeting_info { summary_status }
  }
}";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-fireflies_meetings_sync").await?;
    let api_key = virtues_applets::secret(&input, "api_key")?.trim().to_string();
    let storage = lake::storage_from_env()?;
    let client = virtues_applets::http_client();
    let gql = Graphql { client: &client, api_key: &api_key };

    let user_id = match virtues_applets::config_str(&input, "user_id").filter(|u| !u.is_empty()) {
        Some(u) => u.to_string(),
        None => {
            let data = gql.query(USER_QUERY, json!({})).await?;
            let id = meeting::text_at(&data, "/user/user_id")
                .ok_or_else(|| anyhow!("Fireflies did not say whose key this is"))?
                .to_string();
            input.config["user_id"] = Value::from(id.clone());
            id
        }
    };

    let now = Utc::now();
    let from = meeting::read_from(&input, chrono::Duration::days(OVERLAP_DAYS), now)
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    let mut written = Written::default();
    let mut not_ready = 0usize;
    let mut newest: Option<DateTime<Utc>> = None;
    let mut finished = false;

    for page in 0..MAX_PAGES {
        let vars = json!({ "userId": user_id, "fromDate": from, "limit": PAGE_SIZE, "skip": page * PAGE_SIZE });
        let data = gql.query(TRANSCRIPTS_QUERY, vars).await?;
        let list = data.get("transcripts").and_then(Value::as_array).cloned().unwrap_or_default();
        if !list.is_empty() {
            lake::archive_cloud(&pool, &storage, transform::PROVIDER, ACTION, "transcripts", &list).await?;
        }

        let mut meetings = Vec::new();
        for t in &list {
            if let Some(at) = transform::date(t) {
                newest = newest.max(Some(at));
            }
            match transform::shape(t) {
                transform::Shaped::Meeting(m) => meetings.push(*m),
                transform::Shaped::NotReady => not_ready += 1,
                transform::Shaped::Invalid => tracing::warn!("Fireflies sent a transcript without an id or a date"),
            }
        }
        let w = meeting::write_meetings(&pool, &meetings).await?;
        written.added += w.added;
        written.changed += w.changed;

        if list.len() < PAGE_SIZE {
            finished = true;
            break;
        }
    }

    if finished {
        meeting::advance(&mut input, newest, now);
    } else {
        tracing::warn!(pages = MAX_PAGES, "page limit reached; the watermark stays where it was");
    }

    let mut summary = match (written.added, written.changed) {
        (0, 0) => "no new Fireflies meetings".to_string(),
        (a, c) => format!("synced {a} new Fireflies meetings, {c} updated"),
    };
    if not_ready > 0 {
        summary.push_str(&format!("; {not_ready} not processed by Fireflies yet"));
    }
    if !finished {
        summary.push_str(&format!("; stopped at the {MAX_PAGES}-page limit, the next run reads the same window"));
    }
    output_with_records(&summary, &input.config, (written.added + written.changed) as i64)
}

struct Graphql<'a> {
    client: &'a reqwest::Client,
    api_key: &'a str,
}

impl Graphql<'_> {
    /// Run a query and return its `data`. GraphQL errors fail the run; one
    /// that says the key is not accepted fails with the same sentence as an
    /// HTTP 401.
    async fn query(&self, query: &str, variables: Value) -> Result<Value> {
        let body = json!({ "query": query, "variables": variables });
        let answer = meeting::send_ok(
            || self.client.post(GRAPHQL_URL).bearer_auth(self.api_key).json(&body),
            "Fireflies",
        )
        .await?;
        if let Some(errors) = answer.get("errors").and_then(Value::as_array).filter(|e| !e.is_empty()) {
            if errors.iter().any(is_auth_error) {
                bail!("the Fireflies key was refused; paste a new one");
            }
            let messages: Vec<&str> = errors.iter().filter_map(|e| e.get("message").and_then(Value::as_str)).collect();
            bail!("Fireflies answered with errors: {}", messages.join("; "));
        }
        answer.get("data").cloned().ok_or_else(|| anyhow!("Fireflies answered without data"))
    }
}

/// A GraphQL error that means the key was not accepted. Fireflies puts its
/// error code in `extensions.code`, or in `code` on older answers.
fn is_auth_error(error: &Value) -> bool {
    let code = error
        .pointer("/extensions/code")
        .or_else(|| error.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(code.as_str(), "auth_failed" | "invalid_api_key" | "unauthenticated" | "unauthorized" | "forbidden")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_errors_are_told_apart_from_other_errors() {
        assert!(is_auth_error(&json!({ "message": "Invalid API key", "extensions": { "code": "auth_failed" } })));
        assert!(is_auth_error(&json!({ "message": "nope", "code": "UNAUTHENTICATED" })));
        assert!(!is_auth_error(&json!({ "message": "bad argument", "extensions": { "code": "invalid_arguments" } })));
        assert!(!is_auth_error(&json!({ "message": "something broke" })));
    }
}
