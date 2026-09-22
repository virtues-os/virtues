//! X (Twitter) bookmarks sync.
//!
//! Cron-driven, per-credential. Replays the same authenticated GraphQL call the
//! X web client makes for its own bookmarks timeline — there is no read API for
//! your own bookmarks a personal appliance can use (the v2 endpoint caps around
//! 800 and its pagination dies after a few pages), and this is not that API: it
//! is the client's own request, made from the box.
//!
//! A spike (agents/plan/bookmarks-plan.md §6) established the two things that
//! make this cheap and durable:
//!
//!   - The box can replay it from its own IP with a plain HTTP client. No
//!     `x-client-transaction-id` (the reverse-engineered, breakable header), no
//!     `cf_clearance`. The session cookies plus the public web bearer are enough.
//!   - The credential is two values: `auth_token` (the login) and `ct0` (the
//!     CSRF token, which X wants echoed back in the `x-csrf-token` header).
//!
//! Bookmarks are an EVENT source, not a snapshot: a post's absence from a page
//! means nothing, so there is no tombstoning here. Un-bookmarks are a known gap
//! — catching them needs a periodic full re-walk, not worth a 30-minutely spend
//! for v1, exactly as with GitHub un-stars.
//!
//! **Loud on drift.** The GraphQL query id in the path (`QUERY_ID`) rotates
//! every few weeks. A stale one does not error cleanly — X can answer 200 with a
//! body that has no `bookmark_timeline_v2` — so `transform::parse_page` treats a
//! missing timeline as an error rather than "zero bookmarks". A silent-empty
//! here would look like a drained queue; it is a broken query id, and it must
//! read as one.

mod transform;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use virtues::storage::lake;
use virtues_helpers::{connect_from_env, output, read_input};

const ACTION: &str = "x_bookmarks_sync";

/// The GraphQL operation id for the `Bookmarks` query, read out of the path of
/// the request the web client makes. It rotates when X ships a new client
/// bundle; refresh it from the Network tab (a `graphql/<id>/Bookmarks` request)
/// when a run starts failing with a missing-timeline error.
const QUERY_ID: &str = "-dgKZ58Dr9YSJYrcgEb5KA";

/// The public web bearer. Not a secret — X ships this same constant to every
/// unauthenticated visitor in its JS bundle; the session cookies are what
/// authenticate. Kept verbatim (the `%3D` is part of the token as X sends it).
const WEB_BEARER: &str = "Bearer AAAAAAAAAAAAAAAAAAAAANRILgAAAAAAnNwIzUejRCOuH5E6I8xnZz4puTs%3D1Zv7ttfk8LF81IUq16cHjhLTvJu4FA33AGWWjCpTnA";

/// Posts requested per page. The web client asks for 20; matched so our request
/// looks like the client's and so a page's cursor behaves as observed.
const PAGE_SIZE: u32 = 20;

/// Runaway guard AND the coverage ceiling for the first backfill: at 20 a page,
/// this reaches ~1,000 bookmarks, past X's own ~800 wall. Stated in the summary
/// when hit, per the no-silent-caps rule.
const MAX_PAGES: u32 = 60;

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-x_bookmarks_sync").await?;

    let auth_token = virtues_applets::secret(&input, "auth_token")?.to_string();
    let ct0 = virtues_applets::secret(&input, "ct0")?.to_string();

    // High-water mark: the newest-bookmarked post id from the last successful
    // run. Bookmarks are prepended newest-first, so paging can stop the moment
    // it reaches this id — everything above it is new. Absent on the first run,
    // which then backfills to the page cap.
    let last_seen = virtues_applets::config_str(&input, "newest_id").map(String::from);

    let storage = lake::storage_from_env()?;
    let client = virtues_applets::http_client();

    // The feature block is the same for every page; build it once.
    let features = features().to_string();

    let mut total_written = 0usize;
    let mut new_high: Option<String> = None;
    let mut cursor: Option<String> = None;
    let mut reached_last_seen = false;
    let mut hit_page_cap = true;

    for page in 0..MAX_PAGES {
        // Pace multi-page runs. X flags rapid automated access aggressively, and
        // a flagged session costs the whole credential — far more than the extra
        // seconds. Only the first backfill (or a large new batch) pages at all;
        // a steady-state run is one page and never sleeps. Kept well inside the
        // 300s subprocess ceiling: at ~1s each, 60 pages is a minute of pause.
        if page > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        let variables = match &cursor {
            Some(c) => json!({ "count": PAGE_SIZE, "cursor": c, "includePromotedContent": false }),
            None => json!({ "count": PAGE_SIZE, "includePromotedContent": false }),
        };

        let resp = client
            .get(format!("https://x.com/i/api/graphql/{QUERY_ID}/Bookmarks"))
            .query(&[
                ("variables", variables.to_string()),
                ("features", features.clone()),
            ])
            .header("authorization", WEB_BEARER)
            .header("content-type", "application/json")
            .header("cookie", format!("auth_token={auth_token}; ct0={ct0}"))
            .header("x-csrf-token", &ct0)
            .header("x-twitter-auth-type", "OAuth2Session")
            .header("x-twitter-active-user", "yes")
            .header("x-twitter-client-language", "en")
            // A real browser UA: X serves the API to the web client, and an
            // empty or scripted UA is the cheapest thing for it to gate on.
            .header(
                "user-agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
                 AppleWebKit/537.36 (KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36",
            )
            .send()
            .await
            .context("x bookmarks request failed")?
            .error_for_status()
            .context("x returned non-2xx (session expired, or the query id rotated)")?;

        let body: Value = resp.json().await.context("x response was not JSON")?;
        lake::archive_cloud(&pool, &storage, "x", ACTION, "bookmarks", &[body.clone()]).await?;

        // Shape assertion lives here: a missing timeline is an error, never an
        // empty page (see the module note on loud-on-drift).
        let page = transform::parse_page(&body)?;

        if new_high.is_none() {
            new_high = page.rows.first().map(|r| r.tweet_id.clone());
        }

        // Take rows down to the last-seen id; everything above it is new.
        let mut fresh = Vec::new();
        for row in page.rows {
            if last_seen.as_deref() == Some(row.tweet_id.as_str()) {
                reached_last_seen = true;
                break;
            }
            fresh.push(row);
        }

        total_written += transform::write_bookmarks(&pool, &fresh).await?;

        if reached_last_seen {
            hit_page_cap = false;
            break;
        }
        // Advance, unless there is no next page or X handed back the cursor we
        // just used — it returns a stable bottom cursor on the final page, and
        // following that again is an infinite loop.
        match page.bottom_cursor {
            Some(next) if Some(&next) != cursor.as_ref() => cursor = Some(next),
            _ => {
                hit_page_cap = false;
                break;
            }
        }
    }

    if let Some(id) = new_high {
        input.config["newest_id"] = Value::String(id);
    }

    let mut summary = if total_written == 0 {
        "no new X bookmarks".to_string()
    } else {
        format!("synced {total_written} X bookmarks")
    };
    if hit_page_cap {
        summary.push_str(&format!(
            " — stopped at the {MAX_PAGES}-page cap; bookmarks past the oldest fetched are not synced yet"
        ));
    }
    output(&summary, &input.config)
}

/// The `features` flag block the web client sends with the Bookmarks query.
///
/// Sent whole rather than trimmed: X answers a request that omits a flag it
/// expects with a 400 naming the missing flag, and the set it demands changes
/// with the client version. Carrying the client's own block is the stable
/// choice — when X adds a flag, a refreshed capture updates this in one place.
fn features() -> Value {
    json!({
        "graphql_timeline_v2_bookmark_timeline": true,
        "rweb_video_screen_enabled": false,
        "rweb_cashtags_enabled": true,
        "profile_label_improvements_pcf_label_in_post_enabled": true,
        "responsive_web_profile_redirect_enabled": true,
        "rweb_tipjar_consumption_enabled": false,
        "verified_phone_label_enabled": false,
        "creator_subscriptions_tweet_preview_api_enabled": true,
        "responsive_web_graphql_timeline_navigation_enabled": true,
        "premium_content_api_read_enabled": false,
        "communities_web_enable_tweet_community_results_fetch": true,
        "c9s_tweet_anatomy_moderator_badge_enabled": true,
        "responsive_web_grok_analyze_button_fetch_trends_enabled": false,
        "responsive_web_grok_analyze_post_followups_enabled": true,
        "rweb_cashtags_composer_attachment_enabled": true,
        "responsive_web_jetfuel_frame": true,
        "rweb_sports_post_context_enabled": false,
        "responsive_web_grok_share_attachment_enabled": true,
        "responsive_web_grok_annotations_enabled": true,
        "articles_preview_enabled": true,
        "responsive_web_edit_tweet_api_enabled": true,
        "rweb_conversational_replies_downvote_enabled": false,
        "graphql_is_translatable_rweb_tweet_is_translatable_enabled": true,
        "view_counts_everywhere_api_enabled": true,
        "longform_notetweets_consumption_enabled": true,
        "responsive_web_twitter_article_tweet_consumption_enabled": true,
        "content_disclosure_indicator_enabled": true,
        "content_disclosure_ai_generated_indicator_enabled": true,
        "responsive_web_grok_show_grok_translated_post": true,
        "responsive_web_grok_analysis_button_from_backend": true,
        "post_ctas_fetch_enabled": false,
        "freedom_of_speech_not_reach_fetch_enabled": true,
        "standardized_nudges_misinfo": true,
        "tweet_with_visibility_results_prefer_gql_limited_actions_policy_enabled": true,
        "longform_notetweets_rich_text_read_enabled": true,
        "longform_notetweets_inline_media_enabled": false,
        "responsive_web_nested_quote_preview_enabled": false,
        "responsive_web_grok_image_annotation_enabled": true,
        "responsive_web_grok_imagine_annotation_enabled": true,
        "responsive_web_grok_community_note_auto_translation_is_enabled": true,
        "responsive_web_enhance_cards_enabled": false
    })
}
