//! GitHub star events → `data_content_bookmark` rows.
//!
//! With `Accept: application/vnd.github.star+json` each item is
//! `{"starred_at": "...", "repo": { full GitHub repo object }}`.
//!
//! Container-why note: GitHub star *lists* have no public API, so the closest
//! user-authored containers we can harvest are the repo's topics (plus its
//! language) — they become `tags`.

use anyhow::Result;
use serde_json::Value;
use sqlx::PgPool;
use virtues_helpers::bookmarks::{self, BookmarkRow};

pub async fn write_stars(db: &PgPool, stars: &[Value]) -> Result<usize> {
    let mut rows: Vec<BookmarkRow> = Vec::with_capacity(stars.len());

    for star in stars {
        let Some(repo) = star.get("repo") else {
            continue;
        };
        let Some(url) = repo
            .get("html_url")
            .and_then(|v| v.as_str())
            .filter(|u| !u.is_empty())
        else {
            continue;
        };
        // node_id over numeric id: stable across renames AND transfers, and
        // it's what the GraphQL API speaks if we ever cross over.
        let Some(node_id) = repo
            .get("node_id")
            .and_then(|v| v.as_str())
            .filter(|n| !n.is_empty())
        else {
            tracing::warn!(url, "starred repo has no node_id — skipping");
            continue;
        };
        // Same doctrine as every transform: no wall-clock fallback. A star we
        // cannot place in time would also poison the incremental cursor.
        let Some(ts) = star
            .get("starred_at")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
        else {
            tracing::warn!(url, "star has no parseable starred_at — skipping");
            continue;
        };

        let mut tags: Vec<String> = repo
            .get("topics")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|t| t.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let language = repo.get("language").and_then(|v| v.as_str());
        if let Some(lang) = language {
            tags.push(lang.to_string());
        }

        rows.push(BookmarkRow {
            url: url.to_string(),
            title: repo
                .get("full_name")
                .and_then(|v| v.as_str())
                .map(String::from),
            description: repo
                .get("description")
                .and_then(|v| v.as_str())
                .filter(|d| !d.is_empty())
                .map(String::from),
            source_platform: Some("github".to_string()),
            bookmark_type: Some("star".to_string()),
            author: repo
                .get("owner")
                .and_then(|o| o.get("login"))
                .and_then(|v| v.as_str())
                .map(String::from),
            tags: (!tags.is_empty()).then(|| serde_json::json!(tags)),
            thumbnail_url: repo
                .get("owner")
                .and_then(|o| o.get("avatar_url"))
                .and_then(|v| v.as_str())
                .map(String::from),
            timestamp: ts,
            source_stream_id: format!("github:star:{node_id}"),
            source_table: "github_stars".to_string(),
            source_provider: "github".to_string(),
            metadata: serde_json::json!({
                "node_id": node_id,
                "full_name": repo.get("full_name"),
                "language": language,
                "stargazers_count": repo.get("stargazers_count"),
                "homepage": repo.get("homepage").and_then(|v| v.as_str()).filter(|h| !h.is_empty()),
            }),
        });
    }

    bookmarks::upsert_bookmarks(db, &rows).await
}
