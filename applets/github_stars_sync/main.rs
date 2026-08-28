//! GitHub stars sync.
//!
//! Cron-driven, per-credential. Pages `GET /user/starred` newest-first with
//! `Accept: application/vnd.github.star+json` — the ONLY variant that includes
//! `starred_at`, which is both the bookmark's timestamp and the incremental
//! cursor (stop as soon as a page reaches stars we've already seen).
//!
//! Stars are an event source, not a snapshot: absence from a page means
//! nothing, so there is no tombstoning here. Unstars are a known gap — catching
//! them needs a periodic full re-walk, which isn't worth 30-minutely API spend
//! for v1 (agents/plan/bookmarks-plan.md).

mod transform;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use virtues::storage::lake;
use virtues_helpers::{connect_from_env, output, read_input};

const ACTION: &str = "github_stars_sync";
const STARRED_URL: &str = "https://api.github.com/user/starred";
const PAGE_SIZE: u32 = 100;
/// Runaway guard, and — because GitHub paginates by page number with no time
/// filter on /user/starred — a hard COVERAGE limit for the initial backfill:
/// stars older than the newest 10,000 are simply not synced. Stated in the
/// run summary when hit, per the no-silent-caps rule.
const MAX_PAGES: u32 = 100;

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-github_stars_sync").await?;

    let token = virtues_applets::secret(&input, "token")?.to_string();
    let since: Option<DateTime<Utc>> = virtues_applets::config_str(&input, "last_sync_iso")
        .and_then(|s| s.parse::<DateTime<Utc>>().ok());

    let storage = lake::storage_from_env()?;
    let client = virtues_applets::http_client();

    let mut total_written = 0usize;
    let mut newest: Option<DateTime<Utc>> = None;
    let mut reached_cursor = false;
    let mut hit_page_cap = true;

    for page in 1..=MAX_PAGES {
        let resp = client
            .get(STARRED_URL)
            .query(&[("per_page", PAGE_SIZE), ("page", page)])
            .bearer_auth(&token)
            // GitHub rejects requests without a User-Agent outright (403).
            .header("User-Agent", "virtues-box")
            .header("Accept", "application/vnd.github.star+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .context("github request failed")?
            .error_for_status()
            .context("github returned non-2xx")?;

        let stars: Vec<Value> = resp.json().await.context("github response was not JSON")?;
        if stars.is_empty() {
            hit_page_cap = false;
            break;
        }

        lake::archive_cloud(&pool, &storage, "github", ACTION, "stars", &stars).await?;

        // Newest-first, so the cursor check is "have we scrolled back past
        // what we already had". Strictly-older-than-cursor stops the scan;
        // items AT the cursor second are deliberately re-ingested — starred_at
        // is second-granular, so a second star landing in the cursor's second
        // would otherwise be filtered forever, while a re-upsert of the one we
        // already have is a no-op.
        let fresh: Vec<Value> = stars
            .iter()
            .filter(|s| {
                let Some(at) = starred_at(s) else { return true };
                if newest.is_none_or(|n| at > n) {
                    newest = Some(at);
                }
                match since {
                    Some(cursor) if at < cursor => {
                        reached_cursor = true;
                        false
                    }
                    _ => true,
                }
            })
            .cloned()
            .collect();

        total_written += transform::write_stars(&pool, &fresh).await?;

        if reached_cursor {
            hit_page_cap = false;
            break;
        }
    }

    if let Some(n) = newest {
        input.config["last_sync_iso"] = Value::String(n.to_rfc3339());
    }

    let mut summary = if total_written == 0 {
        "no new GitHub stars".to_string()
    } else {
        format!("synced {total_written} GitHub stars")
    };
    if hit_page_cap {
        summary.push_str(&format!(
            " — stopped at the {MAX_PAGES}-page cap; stars older than the oldest fetched are not synced"
        ));
    }
    output(&summary, &input.config)
}

fn starred_at(star: &Value) -> Option<DateTime<Utc>> {
    star.get("starred_at")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
}
