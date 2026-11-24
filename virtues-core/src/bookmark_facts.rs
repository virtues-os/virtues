//! Facts a bookmark's card shows that need no model: a page's own site name,
//! icon, reading time and video length, and a preview of the page a post
//! links to. Plan: agents/plan/bookmarks-plan.md, section 1, slice F.
//!
//! The enrichment pass writes page facts as it reads a page
//! ([`write_page_facts`]). Pages read before that existed, and the pages
//! posts link to, are filled by the two passes here, which the enrichment
//! applet runs after its own work. Each is a fetch and no model call, so they
//! cost nothing but time, and each marks a row it has tried so it never
//! fetches the same page twice.
//!
//! Every value lands in `metadata`. Remote pictures named here (`icon_url`,
//! `link_preview.image_url`) are copied into Drive by
//! `bookmark_media::keep_remote_pictures`; the room never loads them from
//! their own servers.

use serde_json::{json, Value};
use sqlx::PgPool;

use crate::error::Result;
use crate::fetch::{self, article::MAX_TEXT_CHARS, FetchedPage};

/// Words read per minute, for a page's reading time.
const WORDS_PER_MINUTE: usize = 230;
/// Below this many words a page is not an article, and gets no reading time.
const MIN_ARTICLE_WORDS: usize = 400;

/// What one pass did.
#[derive(Debug, Default, Clone, Copy)]
pub struct FilledFacts {
    pub filled: usize,
    pub failed: usize,
}

/// The facts a fetched page offers, as a metadata patch.
///
/// Only facts that exist are written, so the card never holds an empty slot
/// for one. Each fact is kept only for the kind of page it describes, by the
/// extraction record's `medium`: a reading time for an article, and only when
/// its whole text was read, since a page cut off at the text cap would read
/// shorter than it is; a length for a video, since schema.org's `duration`
/// also describes events and recipes.
pub fn page_facts(page: &FetchedPage, medium: Option<&str>) -> Value {
    let a = &page.article;
    let mut facts = json!({ "facts_at": chrono::Utc::now().to_rfc3339() });
    if let Some(name) = &a.site_name {
        facts["site_name"] = json!(name);
    }
    if let Some(secs) = a.duration_secs.filter(|_| medium == Some("video")) {
        facts["duration_secs"] = json!(secs);
    }
    if let Some(icon) = a.icon_url.as_deref().and_then(|i| resolve(&page.final_url, i)) {
        facts["icon_url"] = json!(icon);
    }
    let words = a.text.split_whitespace().count();
    if medium == Some("article") && words >= MIN_ARTICLE_WORDS && a.text.chars().count() < MAX_TEXT_CHARS {
        facts["reading_minutes"] = json!(words.div_ceil(WORDS_PER_MINUTE));
    }
    facts
}

/// An icon's address made absolute against the page it came from, https only.
fn resolve(base: &str, href: &str) -> Option<String> {
    let url = url::Url::parse(base).ok()?.join(href.trim()).ok()?;
    (url.scheme() == "https").then(|| url.to_string())
}

/// Merge a page's facts into a bookmark's metadata.
pub async fn write_page_facts(db: &PgPool, id: &str, facts: &Value) -> Result<()> {
    sqlx::query(
        "UPDATE data_content_bookmark
            SET metadata = COALESCE(metadata, '{}'::jsonb) || $2, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(facts)
    .execute(db)
    .await?;
    Ok(())
}

/// Fill page facts for pages the enrichment pass read before it kept them.
///
/// Posts are left out (their card shows the author, not a site), and so are
/// asset-backed rows, which are pictures. A page that cannot be fetched is
/// marked `facts_failed` with the time, and tried again after
/// `bookmark_media::RETRY_AFTER_DAYS`.
pub async fn fill_page_facts(db: &PgPool, limit: i64) -> Result<FilledFacts> {
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(&format!(
        "SELECT id, url, extraction->>'medium' FROM data_content_bookmark
          WHERE deleted_at_source IS NULL
            AND enrichment_status = 'done'
            AND NOT {asset}
            AND COALESCE(source_platform, '') NOT IN ('x', 'instagram')
            AND metadata->>'facts_at' IS NULL
            AND {retry}
            AND url ~ '^https?://'
          ORDER BY created_at DESC
          LIMIT $1",
        asset = crate::bookmark_enrichment::ASSET_BACKED_SQL,
        retry = crate::bookmark_media::failure_expired_sql("'{facts_failed}'"),
    ))
    .bind(limit)
    .fetch_all(db)
    .await?;

    let mut out = FilledFacts::default();
    for (id, url, medium) in rows {
        let facts = match fetch::fetch_page(&url).await {
            Ok(page) => {
                out.filled += 1;
                let mut f = page_facts(&page, medium.as_deref());
                f["facts_failed"] = Value::Null;
                f
            }
            Err(e) => {
                out.failed += 1;
                tracing::debug!(bookmark = %id, error = %e, "could not read page facts");
                json!({ "facts_failed": chrono::Utc::now().to_rfc3339() })
            }
        };
        write_page_facts(db, &id, &facts).await?;
    }
    Ok(out)
}

/// Fill the preview of the page an X post links to.
///
/// A post's first link that leads off X becomes `metadata.link_preview`: its
/// address, title, site name and picture. A link to another post on X is not
/// previewed, since X answers a fetch with a sign-in wall. When the page
/// cannot be read, the preview keeps the address alone, so the card still
/// shows where the link leads.
pub async fn fill_link_previews(db: &PgPool, limit: i64) -> Result<FilledFacts> {
    let rows: Vec<(String, Value)> = sqlx::query_as(
        "SELECT id, metadata->'links' FROM data_content_bookmark
          WHERE deleted_at_source IS NULL
            AND source_platform = 'x'
            AND jsonb_typeof(metadata->'links') = 'array'
            AND jsonb_array_length(metadata->'links') > 0
            AND NOT metadata ? 'link_preview'
          ORDER BY created_at DESC
          LIMIT $1",
    )
    .bind(limit)
    .fetch_all(db)
    .await?;

    let mut out = FilledFacts::default();
    for (id, links) in rows {
        let Some(target) = first_outside_link(&links) else {
            // Every link leads back to X: record that, so the row is done.
            write_page_facts(db, &id, &json!({ "link_preview": Value::Null })).await?;
            continue;
        };
        let preview = match fetch::fetch_page(&target).await {
            Ok(page) => {
                out.filled += 1;
                let a = &page.article;
                let mut p = json!({ "url": page.final_url });
                if let Some(t) = &a.title {
                    p["title"] = json!(t);
                }
                if let Some(n) = &a.site_name {
                    p["site_name"] = json!(n);
                }
                if let Some(img) = a.image_url.as_deref().and_then(|i| resolve(&page.final_url, i)) {
                    p["image_url"] = json!(img);
                }
                p
            }
            Err(e) => {
                out.failed += 1;
                tracing::debug!(bookmark = %id, error = %e, "could not read a linked page");
                json!({ "url": target })
            }
        };
        write_page_facts(db, &id, &json!({ "link_preview": preview })).await?;
    }
    Ok(out)
}

/// The first expanded link in a post that does not lead back to X.
fn first_outside_link(links: &Value) -> Option<String> {
    links.as_array()?.iter().find_map(|l| {
        let expanded = l.get("expanded")?.as_str()?;
        let host = url::Url::parse(expanded).ok()?.host_str()?.trim_start_matches("www.").to_string();
        let on_x = matches!(host.as_str(), "x.com" | "twitter.com" | "t.co" | "mobile.twitter.com");
        (!on_x).then(|| expanded.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::article::Article;

    fn page(text: &str, icon: Option<&str>) -> FetchedPage {
        FetchedPage {
            final_url: "https://example.com/posts/one".into(),
            article: Article {
                site_name: Some("Half Step".into()),
                duration_secs: Some(1122),
                icon_url: icon.map(String::from),
                text: text.into(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn facts_hold_only_what_the_page_offers() {
        let long = "word ".repeat(920);
        let f = page_facts(&page(&long, Some("/favicon.ico")), Some("article"));
        assert_eq!(f["site_name"], "Half Step");
        assert!(f.get("duration_secs").is_none(), "an article has no length");
        assert_eq!(page_facts(&page("", None), Some("video"))["duration_secs"], 1122);
        assert_eq!(f["icon_url"], "https://example.com/favicon.ico", "a relative icon is resolved");
        assert_eq!(f["reading_minutes"], 4);

        let short = page_facts(&page("a few words", Some("http://example.com/i.png")), Some("article"));
        assert!(
            page_facts(&page(&long, None), Some("product")).get("reading_minutes").is_none(),
            "only an article gets a reading time"
        );
        assert!(short.get("reading_minutes").is_none(), "a short page is not an article");
        assert!(short.get("icon_url").is_none(), "an http icon is not kept");
    }

    #[test]
    fn a_link_back_to_x_is_not_previewed() {
        let links = json!([
            { "short": "https://t.co/a", "expanded": "https://x.com/nick/status/1", "display": "x.com/nick/status/1" },
            { "short": "https://t.co/b", "expanded": "https://www.example.com/essay", "display": "example.com/essay" }
        ]);
        assert_eq!(first_outside_link(&links).as_deref(), Some("https://www.example.com/essay"));
        assert_eq!(first_outside_link(&json!([{ "expanded": "https://twitter.com/a" }])), None);
    }
}
