//! Google Calendar sync.
//!
//! Cron-driven, per-credential. Lists the user's calendars, then for each
//! calendar pulls events using a `syncToken` cursor (incremental sync per
//! Google's calendar API). On first run there's no token; we query the
//! last 90 days + future and store the returned `nextSyncToken`.

mod transform;

use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use virtues::storage::lake;
use virtues_helpers::{connect_from_env, output, read_input};

const ACTION: &str = "google_calendar_sync";
const CAL_LIST: &str = "https://www.googleapis.com/calendar/v3/users/me/calendarList";
const EVENTS_BASE: &str = "https://www.googleapis.com/calendar/v3/calendars";
const PAGE_SIZE: u32 = 250;
const MAX_PAGES: u32 = 20;

#[tokio::main]
async fn main() -> Result<()> {
    virtues_actions::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-google_calendar_sync").await?;

    let access_token = virtues_actions::secret(&input, "access_token")?
        .to_string();

    let storage = lake::storage_from_env()?;
    let client = reqwest::Client::new();

    // Per-calendar sync tokens live under config.sync_tokens (map cal_id → token).
    let mut sync_tokens: HashMap<String, String> = input
        .config
        .get("sync_tokens")
        .and_then(|v| v.as_object())
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let calendars = list_calendars(&client, &access_token).await?;
    let mut total_written = 0usize;

    for cal in &calendars {
        let cal_id = cal
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("primary");

        let prior_token = sync_tokens.get(cal_id).cloned();
        let (written, new_token) =
            sync_calendar(&client, &access_token, &pool, &storage, cal_id, prior_token.as_deref()).await?;
        total_written += written;

        if let Some(t) = new_token {
            sync_tokens.insert(cal_id.to_string(), t);
        }
    }

    input.config["sync_tokens"] = serde_json::json!(sync_tokens);

    let summary = format!(
        "synced {} events across {} calendars",
        total_written,
        calendars.len()
    );
    output(&summary, &input.config)
}

async fn list_calendars(client: &reqwest::Client, token: &str) -> Result<Vec<Value>> {
    let resp: Value = client
        .get(CAL_LIST)
        .bearer_auth(token)
        .send()
        .await
        .context("calendarList request failed")?
        .error_for_status()
        .context("calendarList non-2xx")?
        .json()
        .await
        .context("calendarList response was not JSON")?;

    Ok(resp
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default())
}

async fn sync_calendar(
    client: &reqwest::Client,
    token: &str,
    db: &sqlx::PgPool,
    storage: &virtues::storage::Storage,
    calendar_id: &str,
    prior_sync_token: Option<&str>,
) -> Result<(usize, Option<String>)> {
    let mut written = 0usize;
    let mut page_token: Option<String> = None;
    let mut next_sync_token: Option<String> = None;
    let url = format!(
        "{EVENTS_BASE}/{}/events",
        urlencoding::encode(calendar_id)
    );

    for _ in 0..MAX_PAGES {
        let mut req = client.get(&url).bearer_auth(token).query(&[
            ("maxResults", PAGE_SIZE.to_string()),
            ("singleEvents", "true".to_string()),
        ]);
        if let Some(t) = prior_sync_token {
            req = req.query(&[("syncToken", t)]);
        } else {
            // First sync: bound the window. Last 90 days + future events.
            let since = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
            req = req.query(&[("timeMin", since)]);
        }
        if let Some(p) = &page_token {
            req = req.query(&[("pageToken", p.clone())]);
        }

        let resp: Value = req
            .send()
            .await
            .context("events request failed")?
            .error_for_status()
            .context("events non-2xx (note: 410 means syncToken expired — clear it)")?
            .json()
            .await
            .context("events response was not JSON")?;

        // Archive the whole page. A syncToken response is a DELTA — it carries
        // cancellations and edits, and the token is single-use, so Google will never
        // describe this change again. Whatever the transform doesn't read here is gone.
        lake::archive_cloud(db, storage, "google", ACTION, "calendar", &[resp.clone()]).await?;

        if let Some(items) = resp.get("items").and_then(|v| v.as_array()) {
            written += transform::write_events(db, calendar_id, items).await?;
        }

        if let Some(np) = resp.get("nextPageToken").and_then(|v| v.as_str()) {
            page_token = Some(np.to_string());
            continue;
        }
        if let Some(nst) = resp.get("nextSyncToken").and_then(|v| v.as_str()) {
            next_sync_token = Some(nst.to_string());
        }
        break;
    }

    Ok((written, next_sync_token))
}
