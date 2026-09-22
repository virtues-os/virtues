//! X bookmarks timeline → `data_content_bookmark` rows.
//!
//! The payload is the `bookmark_timeline_v2` GraphQL response — the same one
//! the web client renders. One page is a list of timeline `instructions`; the
//! `TimelineAddEntries` instruction holds the entries, which are `tweet-*`
//! items interleaved with `cursor-*` markers.
//!
//! What X does NOT give us: a "bookmarked_at". The timeline is ordered by save
//! time (newest first) but exposes no per-item save timestamp — only the post's
//! own `created_at`. So `occurred_at` is the post's time, not the save's. That
//! is a real timestamp and the one a person recognizes ("the tweet from March"),
//! and it is the honest choice over inventing a save time; the module doc on the
//! sync notes the limitation. As everywhere, there is no wall-clock fallback: a
//! post we cannot place in time is skipped rather than stamped `now()`.

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use sqlx::PgPool;
use virtues_helpers::bookmarks::{self, BookmarkRow};

/// One parsed page: the rows to write, and the cursor for the next page.
pub struct ParsedPage {
    pub rows: Vec<XBookmark>,
    pub bottom_cursor: Option<String>,
}

/// A bookmark plus the post id the sync needs for its high-water cursor. The
/// id also becomes the row's stable identity (`x:bookmark:{id}`).
pub struct XBookmark {
    pub tweet_id: String,
    row: BookmarkRow,
}

/// Parse one bookmarks page.
///
/// Returns an error when the timeline is absent — a rotated query id or an
/// expired session can yield a 200 whose body carries no `bookmark_timeline_v2`,
/// and reading that as "zero bookmarks" would look like a drained queue. An
/// empty-but-present timeline is a legitimate empty page and parses to no rows.
pub fn parse_page(body: &Value) -> Result<ParsedPage> {
    let instructions = body
        .get("data")
        .and_then(|d| d.get("bookmark_timeline_v2"))
        .and_then(|t| t.get("timeline"))
        .and_then(|t| t.get("instructions"))
        .and_then(|i| i.as_array())
        .ok_or_else(|| {
            anyhow!("x bookmarks response has no bookmark_timeline_v2 (query id rotated, or session invalid)")
        })?;

    let entries: &[Value] = instructions
        .iter()
        .find_map(|ins| {
            (ins.get("type").and_then(Value::as_str) == Some("TimelineAddEntries"))
                .then(|| ins.get("entries").and_then(Value::as_array))
                .flatten()
        })
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let mut rows = Vec::new();
    let mut bottom_cursor = None;

    for entry in entries {
        let id = entry.get("entryId").and_then(Value::as_str).unwrap_or("");
        if id.starts_with("cursor-bottom") {
            bottom_cursor = entry
                .get("content")
                .and_then(|c| c.get("value"))
                .and_then(Value::as_str)
                .map(String::from);
        } else if id.starts_with("tweet-") {
            if let Some(b) = parse_tweet_entry(entry) {
                rows.push(b);
            }
        }
    }

    Ok(ParsedPage {
        rows,
        bottom_cursor,
    })
}

/// One `tweet-*` entry → a bookmark, or `None` when the shape is unexpected or
/// the post carries no parseable timestamp.
fn parse_tweet_entry(entry: &Value) -> Option<XBookmark> {
    let result = entry
        .get("content")?
        .get("itemContent")?
        .get("tweet_results")?
        .get("result")?;
    // A limited-visibility post nests the real tweet one level down.
    let result = match result.get("__typename").and_then(Value::as_str) {
        Some("TweetWithVisibilityResults") => result.get("tweet")?,
        _ => result,
    };
    let legacy = result.get("legacy")?;

    let tweet_id = legacy.get("id_str").and_then(Value::as_str)?.to_string();

    // X's classic timestamp format, e.g. "Wed Oct 10 20:19:24 +0000 2018".
    // No wall-clock fallback: an unparseable date drops the row.
    let created_at = legacy
        .get("created_at")
        .and_then(Value::as_str)
        .and_then(|s| chrono::DateTime::parse_from_str(s, "%a %b %d %H:%M:%S %z %Y").ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))?;

    let screen_name = result
        .get("core")
        .and_then(|c| c.get("user_results"))
        .and_then(|u| u.get("result"))
        .and_then(user_screen_name);

    // The canonical permalink. Falls back to the id-only form X itself accepts
    // when the author handle is missing, so `url` is never empty (the room, the
    // citation chip and canonicalization all assume it).
    let url = match &screen_name {
        Some(h) => format!("https://x.com/{h}/status/{tweet_id}"),
        None => format!("https://x.com/i/status/{tweet_id}"),
    };

    let text = legacy
        .get("full_text")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .map(String::from);

    let thumbnail_url = legacy
        .get("extended_entities")
        .or_else(|| legacy.get("entities"))
        .and_then(|e| e.get("media"))
        .and_then(Value::as_array)
        .and_then(|m| m.first())
        .and_then(|m| m.get("media_url_https"))
        .and_then(Value::as_str)
        .map(String::from);

    let row = BookmarkRow {
        url,
        // A tweet has no title; its text is the readable content, so it lands as
        // the description (which the room and the embed text both read).
        title: None,
        description: text,
        source_platform: Some("x".to_string()),
        bookmark_type: Some("bookmark".to_string()),
        author: screen_name.clone(),
        // Folders/collections are a separate GraphQL call, deferred — so no tags
        // at the item level yet.
        tags: None,
        thumbnail_url,
        timestamp: created_at,
        source_stream_id: format!("x:bookmark:{tweet_id}"),
        source_table: "x_bookmarks".to_string(),
        source_provider: "x".to_string(),
        metadata: json!({
            "tweet_id": tweet_id,
            "screen_name": screen_name,
            "conversation_id_str": legacy.get("conversation_id_str").and_then(Value::as_str),
            "lang": legacy.get("lang").and_then(Value::as_str),
            "favorite_count": legacy.get("favorite_count"),
            "retweet_count": legacy.get("retweet_count"),
            "reply_count": legacy.get("reply_count"),
            "bookmark_count": legacy.get("bookmark_count"),
        }),
    };

    Some(XBookmark { tweet_id, row })
}

/// The author's `@handle` from a `user_results.result`, tolerant of where X
/// currently keeps it — `legacy.screen_name` historically, `core.screen_name`
/// in newer payloads.
fn user_screen_name(user: &Value) -> Option<String> {
    user.get("legacy")
        .and_then(|l| l.get("screen_name"))
        .and_then(Value::as_str)
        .or_else(|| {
            user.get("core")
                .and_then(|c| c.get("screen_name"))
                .and_then(Value::as_str)
        })
        .map(String::from)
}

/// Upsert a run's parsed bookmarks. Idempotent on `x:bookmark:{id}`.
pub async fn write_bookmarks(db: &PgPool, items: &[XBookmark]) -> Result<usize> {
    if items.is_empty() {
        return Ok(0);
    }
    let rows: Vec<BookmarkRow> = items.iter().map(|b| b.row.clone()).collect();
    bookmarks::upsert_bookmarks(db, &rows).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal `bookmark_timeline_v2` page in X's real shape — two tweet
    /// entries (one of them the visibility-wrapped variant) and a bottom
    /// cursor. Hand-built, not captured: a real page is one person's private
    /// bookmarks and must never live in the repo.
    fn fixture() -> Value {
        json!({
            "data": { "bookmark_timeline_v2": { "timeline": { "instructions": [{
                "type": "TimelineAddEntries",
                "entries": [
                    {
                        "entryId": "tweet-1111",
                        "content": { "itemContent": { "tweet_results": { "result": {
                            "__typename": "Tweet",
                            "core": { "user_results": { "result": {
                                "legacy": { "screen_name": "davidokafor" }
                            }}},
                            "legacy": {
                                "id_str": "1111",
                                "full_text": "a thing worth keeping",
                                "created_at": "Wed Oct 10 20:19:24 +0000 2018",
                                "lang": "en"
                            }
                        }}}}
                    },
                    {
                        "entryId": "tweet-2222",
                        "content": { "itemContent": { "tweet_results": { "result": {
                            "__typename": "TweetWithVisibilityResults",
                            "tweet": {
                                "core": { "user_results": { "result": {
                                    "core": { "screen_name": "nick" }
                                }}},
                                "legacy": {
                                    "id_str": "2222",
                                    "full_text": "behind a visibility wrapper",
                                    "created_at": "Thu Mar 05 08:00:00 +0000 2026"
                                }
                            }
                        }}}}
                    },
                    { "entryId": "cursor-bottom-0", "content": { "value": "CURSOR_B" } }
                ]
            }]}}}
        })
    }

    #[test]
    fn parses_both_tweet_shapes_and_the_cursor() {
        let page = parse_page(&fixture()).expect("valid page parses");
        assert_eq!(page.bottom_cursor.as_deref(), Some("CURSOR_B"));
        assert_eq!(page.rows.len(), 2, "both tweet entries parsed");

        let first = &page.rows[0];
        assert_eq!(first.tweet_id, "1111");
        assert_eq!(first.row.url, "https://x.com/davidokafor/status/1111");
        assert_eq!(first.row.description.as_deref(), Some("a thing worth keeping"));
        assert_eq!(first.row.source_stream_id, "x:bookmark:1111");
        assert_eq!(first.row.bookmark_type.as_deref(), Some("bookmark"));

        // The visibility-wrapped tweet is unwrapped, and the newer author
        // location (core.screen_name) is read.
        let second = &page.rows[1];
        assert_eq!(second.tweet_id, "2222");
        assert_eq!(second.row.url, "https://x.com/nick/status/2222");
    }

    #[test]
    fn a_missing_timeline_is_an_error_not_an_empty_page() {
        // What a rotated query id or a dead session returns: a 200 whose body
        // has no timeline. It must fail loudly, never read as zero bookmarks.
        let err = parse_page(&json!({ "data": {} })).err().expect("must error");
        assert!(err.to_string().contains("bookmark_timeline_v2"));
    }

    #[test]
    fn an_empty_but_present_timeline_yields_no_rows() {
        let page = parse_page(&json!({
            "data": { "bookmark_timeline_v2": { "timeline": { "instructions": [
                { "type": "TimelineAddEntries", "entries": [
                    { "entryId": "cursor-bottom-0", "content": { "value": "C" } }
                ]}
            ]}}}
        }))
        .expect("empty is valid");
        assert!(page.rows.is_empty());
        assert_eq!(page.bottom_cursor.as_deref(), Some("C"));
    }

    #[test]
    fn an_unparseable_timestamp_drops_the_row_rather_than_stamping_now() {
        let mut f = fixture();
        f["data"]["bookmark_timeline_v2"]["timeline"]["instructions"][0]["entries"][0]
            ["content"]["itemContent"]["tweet_results"]["result"]["legacy"]["created_at"] =
            json!("not a date");
        let page = parse_page(&f).expect("still parses");
        // The bad-timestamp row is gone; the good one and the cursor remain.
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].tweet_id, "2222");
    }

    #[test]
    fn a_missing_author_falls_back_to_the_id_permalink() {
        let mut f = fixture();
        f["data"]["bookmark_timeline_v2"]["timeline"]["instructions"][0]["entries"][0]
            ["content"]["itemContent"]["tweet_results"]["result"]["core"] = json!(null);
        let page = parse_page(&f).expect("parses without author");
        assert_eq!(page.rows[0].row.url, "https://x.com/i/status/1111");
        assert!(page.rows[0].row.author.is_none());
    }
}
