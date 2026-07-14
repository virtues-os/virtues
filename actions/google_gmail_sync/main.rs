//! Gmail sync.
//!
//! Cron-driven, per-credential. Lists message ids since the cursor (Unix
//! seconds stored in `config.last_sync_unix`), batch-fetches full messages
//! (1 request per id is cheap because Gmail batches naturally with HTTP/2
//! keepalive). Writes rows to `data_communication_email`.
//!
//! Per-page list size: 100. We stop after MAX_PAGES = 5 → 500 messages per
//! sync tick. Subsequent ticks pick up newer messages.

mod transform;

use anyhow::{Context, Result};
use serde_json::Value;
use virtues::storage::lake;
use virtues_helpers::{connect_from_env, output, read_input};

const ACTION: &str = "google_gmail_sync";
const PROFILE: &str = "https://gmail.googleapis.com/gmail/v1/users/me/profile";
const LIST: &str = "https://gmail.googleapis.com/gmail/v1/users/me/messages";
const GET_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me/messages";
const PAGE_SIZE: u32 = 100;
const MAX_PAGES: u32 = 5;

#[tokio::main]
async fn main() -> Result<()> {
    virtues_actions::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-google_gmail_sync").await?;

    let access_token = virtues_actions::secret(&input, "access_token")?
        .to_string();

    let storage = lake::storage_from_env()?;
    let client = reqwest::Client::new();

    let user_email = fetch_user_email(&client, &access_token).await?;

    // Cursor: Unix-seconds. First run: pull last 30 days.
    let after = input
        .config
        .get("last_sync_unix")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp() - 30 * 86400);

    let mut page_token: Option<String> = None;
    let mut total_written = 0usize;
    let mut latest_ts = after;
    let query = format!("after:{after}");

    for _ in 0..MAX_PAGES {
        let mut req = client.get(LIST).bearer_auth(&access_token).query(&[
            ("q", query.as_str()),
            ("maxResults", &PAGE_SIZE.to_string()),
        ]);
        if let Some(p) = &page_token {
            req = req.query(&[("pageToken", p.as_str())]);
        }

        let list_resp: Value = req
            .send()
            .await
            .context("gmail list failed")?
            .error_for_status()
            .context("gmail list non-2xx")?
            .json()
            .await
            .context("gmail list non-JSON")?;

        let ids: Vec<String> = list_resp
            .get("messages")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        if ids.is_empty() {
            break;
        }

        // Fetch each message in full. Keep simple — Gmail's batch endpoint is
        // fiddly (multipart/mixed); plain HTTP/2 keepalive is fast enough for
        // 100 messages per page.
        let mut messages = Vec::with_capacity(ids.len());
        for id in ids {
            let m: Value = client
                .get(format!("{GET_BASE}/{id}"))
                .bearer_auth(&access_token)
                .query(&[("format", "full")])
                .send()
                .await
                .context("gmail get failed")?
                .error_for_status()
                .context("gmail get non-2xx")?
                .json()
                .await
                .context("gmail get non-JSON")?;

            if let Some(internal) = m
                .get("internalDate")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok())
            {
                let ts_secs = internal / 1000;
                if ts_secs > latest_ts {
                    latest_ts = ts_secs;
                }
            }
            messages.push(m);
        }

        // Raw Gmail message objects, exactly as the API returned them — headers,
        // labels and MIME parts included. The transform reads a handful of those
        // fields; the rest is the part that would be gone forever.
        lake::archive_cloud(&pool, &storage, "google", ACTION, "gmail", &messages).await?;

        let written = transform::write_messages(&pool, &user_email, &messages).await?;
        total_written += written;

        page_token = list_resp
            .get("nextPageToken")
            .and_then(|v| v.as_str())
            .map(String::from);
        if page_token.is_none() {
            break;
        }
    }

    input.config["last_sync_unix"] = Value::from(latest_ts);

    let summary = if total_written == 0 {
        "no new Gmail messages".to_string()
    } else {
        format!("synced {total_written} Gmail messages")
    };
    output(&summary, &input.config)
}

async fn fetch_user_email(client: &reqwest::Client, token: &str) -> Result<String> {
    let resp: Value = client
        .get(PROFILE)
        .bearer_auth(token)
        .send()
        .await
        .context("gmail profile failed")?
        .error_for_status()
        .context("gmail profile non-2xx")?
        .json()
        .await
        .context("gmail profile non-JSON")?;
    Ok(resp
        .get("emailAddress")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string())
}
