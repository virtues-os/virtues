//! Notion pages sync.
//!
//! Cron-driven, per-credential. Calls Notion's `/v1/search` paginated using
//! `next_cursor`. Cursor is reset on each full sync so re-runs always pick
//! up newly-edited pages too. For incremental: filter by `last_edited_time`
//! against `app_actions.config.last_sync_iso`.

mod transform;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use virtues_helpers::{connect_from_env, output, read_input};

const NOTION_SEARCH: &str = "https://api.notion.com/v1/search";
const NOTION_VERSION: &str = "2022-06-28";
const PAGE_SIZE: u32 = 100;
const MAX_PAGES: u32 = 50; // safety cap (5000 results)

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let mut input = read_input()?;
    let pool = connect_from_env().await?;

    let access_token = input
        .credentials
        .as_ref()
        .and_then(|c| c.get("secrets"))
        .and_then(|s| s.get("access_token"))
        .and_then(|v| v.as_str())
        .context("notion credentials missing secrets.access_token")?
        .to_string();

    let client = reqwest::Client::new();
    let mut total_written = 0usize;
    let mut cursor: Option<String> = None;

    for _page in 0..MAX_PAGES {
        let mut body = json!({
            "page_size": PAGE_SIZE,
            "sort": { "direction": "descending", "timestamp": "last_edited_time" }
        });
        if let Some(c) = &cursor {
            body["start_cursor"] = json!(c);
        }

        let resp: Value = client
            .post(NOTION_SEARCH)
            .bearer_auth(&access_token)
            .header("Notion-Version", NOTION_VERSION)
            .json(&body)
            .send()
            .await
            .context("notion request failed")?
            .error_for_status()
            .context("notion returned non-2xx")?
            .json()
            .await
            .context("notion response was not JSON")?;

        let results = resp.get("results").and_then(|v| v.as_array());
        let Some(pages) = results else {
            break;
        };

        if pages.is_empty() {
            break;
        }

        let written = transform::write_pages(&pool, pages).await?;
        total_written += written;

        let has_more = resp.get("has_more").and_then(|v| v.as_bool()).unwrap_or(false);
        if !has_more {
            break;
        }
        cursor = resp
            .get("next_cursor")
            .and_then(|v| v.as_str())
            .map(String::from);
        if cursor.is_none() {
            break;
        }
    }

    input.config["last_sync_iso"] = serde_json::Value::String(chrono::Utc::now().to_rfc3339());

    let summary = if total_written == 0 {
        "no new Notion pages".to_string()
    } else {
        format!("synced {total_written} Notion pages")
    };
    output(&summary, &input.config)
}
