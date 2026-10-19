//! Notion pages sync.
//!
//! Cron-driven, per-credential. Calls Notion's `/v1/search` paginated using
//! `next_cursor`. Cursor is reset on each full sync so re-runs always pick
//! up newly-edited pages too. For incremental: filter by `last_edited_time`
//! against `app_applets.config.last_sync_iso`.

mod transform;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use virtues::storage::lake;
use virtues_helpers::{connect_from_env, output, read_input};

const ACTION: &str = "notion_pages_sync";
const NOTION_SEARCH: &str = "https://api.notion.com/v1/search";
const NOTION_VERSION: &str = "2022-06-28";
const PAGE_SIZE: u32 = 100;
const MAX_PAGES: u32 = 50; // safety cap (5000 results)

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-notion_pages_sync").await?;

    let access_token = virtues_applets::secret(&input, "access_token")?
        .to_string();

    let storage = lake::storage_from_env()?;
    // Must be the shared client, not `reqwest::Client::new()`: reqwest here is
    // built `rustls-tls-no-provider`, so a bare client panics "No provider set"
    // on the first HTTPS call. `http_client()` installs the ring provider (and
    // adds a timeout a bare client lacks entirely).
    let client = virtues_applets::http_client();
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

        // The whole response, not just `results` — `has_more`/`next_cursor` are part of
        // the evidence of what Notion actually said.
        lake::archive_cloud(&pool, &storage, "notion", ACTION, "pages", &[resp.clone()]).await?;

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
