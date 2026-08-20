//! Strava activities sync.
//!
//! Cron-driven, per-credential. Pulls `GET /api/v3/athlete/activities` since
//! the cursor stored in `app_applets.config.after_unix`, writes them to
//! `data_health_workout`, advances the cursor.
//!
//! Page size: 100 (Strava's max). Pagination loop until response < 100 or
//! we've fetched 1000 records (safety cap, ~10 pages).

mod transform;

use anyhow::{Context, Result};
use serde_json::Value;
use virtues::storage::lake;
use virtues_helpers::{connect_from_env, output, read_input};

const ACTION: &str = "strava_activities_sync";
const STRAVA_API: &str = "https://www.strava.com/api/v3/athlete/activities";
const PAGE_SIZE: u32 = 100;
const MAX_PAGES: u32 = 10;

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-strava_activities_sync").await?;

    let access_token = virtues_applets::secret(&input, "access_token")?
        .to_string();

    // Cursor: Unix-seconds. First run: pull last 90 days. Subsequent: only
    // activities started after the last sync.
    let after = input
        .config
        .get("after_unix")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp() - 90 * 86400);

    let storage = lake::storage_from_env()?;
    // Must be the shared client, not `reqwest::Client::new()`: reqwest here is
    // built `rustls-tls-no-provider`, so a bare client panics "No provider set"
    // on the first HTTPS call. `http_client()` installs the ring provider (and
    // adds a timeout a bare client lacks entirely).
    let client = virtues_applets::http_client();
    let mut total_written = 0usize;
    let mut latest_start: i64 = after;

    for page in 1..=MAX_PAGES {
        let resp: Vec<Value> = client
            .get(STRAVA_API)
            .bearer_auth(&access_token)
            .query(&[
                ("after", after.to_string()),
                ("page", page.to_string()),
                ("per_page", PAGE_SIZE.to_string()),
            ])
            .send()
            .await
            .context("strava request failed")?
            .error_for_status()
            .context("strava returned non-2xx")?
            .json()
            .await
            .context("strava response was not JSON array")?;

        if resp.is_empty() {
            break;
        }

        for activity in &resp {
            if let Some(start) = activity
                .get("start_date")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
                .map(|dt| dt.timestamp())
            {
                if start > latest_start {
                    latest_start = start;
                }
            }
        }

        lake::archive_cloud(&pool, &storage, "strava", ACTION, "activities", &resp).await?;

        let written = transform::write_activities(&pool, &resp).await?;
        total_written += written;

        if (resp.len() as u32) < PAGE_SIZE {
            break;
        }
    }

    // Advance cursor to the latest start_date we saw.
    input.config["after_unix"] = serde_json::Value::from(latest_start);

    let summary = if total_written == 0 {
        "no new activities".to_string()
    } else {
        format!("synced {total_written} Strava activities")
    };
    output(&summary, &input.config)
}
