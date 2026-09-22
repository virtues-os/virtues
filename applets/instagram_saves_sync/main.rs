//! Instagram saves sync.
//!
//! Cron-driven, per-credential. Replays the same authenticated call the
//! Instagram web app makes for its Saved feed — there is no read API for your
//! own saves a personal appliance can use, and this is not that API: it is the
//! web client's own request, made from the box.
//!
//! A spike (agents/plan/bookmarks-plan.md §5) established that the box can
//! replay `GET /api/v1/feed/saved/posts/` from its own IP and get a 200. The
//! credential is the browser's cookie jar: the login `sessionid`, the
//! `csrftoken` (which Instagram wants echoed in the `x-csrftoken` header), and
//! the device cookies the server checks. One header is unresolved —
//! `x-ig-www-claim`, an HMAC — so the applet sends it when the credential
//! carries one (the collector harvest can supply it) and, if a run 401/302s
//! without it, fails loudly saying so rather than syncing nothing.
//!
//! The half that makes an image findable is not the page — the permalink is
//! login-walled and will not fetch — it is the caption (written here as the
//! description) and the picture. Each saved image's CDN URL is signed and
//! expires within hours, long before a delayed enrichment sweep would reach it,
//! so this sync downloads the image now and keeps it in Drive; the image pass
//! (`bookmark_enrichment::enrich_image`) reads it from there. Caption
//! findability survives even when the image cannot be fetched: the row is
//! written either way.
//!
//! Saves are an EVENT source, not a snapshot: a post's absence from a page
//! means nothing, so there is no tombstoning. Un-saves are a known gap, as with
//! X un-bookmarks and GitHub un-stars.

mod transform;

use anyhow::{Context, Result};
use serde_json::Value;
use virtues::storage::lake;
use virtues_helpers::{connect_from_env, output, read_input};

const ACTION: &str = "instagram_saves_sync";
const SAVED_URL: &str = "https://www.instagram.com/api/v1/feed/saved/posts/";

/// The web client's app id, a public constant it sends on every request.
const IG_APP_ID: &str = "936619743392459";

/// Saves fetched per page — the web client's own page size.
const PAGE_SIZE: u32 = 25;

/// Runaway guard and first-backfill ceiling. At 25 a page this reaches ~1,500
/// saves; stated in the summary when hit, per the no-silent-caps rule.
const MAX_PAGES: u32 = 60;

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-instagram_saves_sync").await?;

    let cookies = virtues_applets::secret(&input, "cookies")?.trim().to_string();
    let csrftoken = csrftoken_from(&cookies)
        .context("the saved cookies carry no csrftoken — re-copy the whole cookie jar")?;
    // Optional: only present if the collector harvest captured it. Read without
    // erroring, because a manual paste has no way to supply it.
    let www_claim = input
        .credentials
        .as_ref()
        .and_then(|c| c.get("secrets"))
        .and_then(|s| s.get("www_claim"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let last_seen = virtues_applets::config_str(&input, "newest_id").map(String::from);

    let storage = lake::storage_from_env()?;
    let client = virtues_applets::http_client();

    let mut total_written = 0usize;
    let mut images_stored = 0usize;
    let mut new_high: Option<String> = None;
    let mut max_id: Option<String> = None;
    let mut reached_last_seen = false;
    let mut hit_page_cap = true;

    for page in 0..MAX_PAGES {
        // Pace multi-page runs — Instagram flags rapid automated access, and a
        // flagged session costs the whole credential. Only a first backfill
        // pages at all; a steady-state run is one page and never sleeps.
        if page > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        let mut req = client
            .get(SAVED_URL)
            .header("accept", "*/*")
            .header("x-ig-app-id", IG_APP_ID)
            .header("x-csrftoken", &csrftoken)
            .header("x-requested-with", "XMLHttpRequest")
            .header("cookie", &cookies)
            .header("referer", "https://www.instagram.com/")
            .header(
                "user-agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
                 AppleWebKit/537.36 (KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36",
            );
        if let Some(claim) = &www_claim {
            req = req.header("x-ig-www-claim", claim);
        }
        if let Some(cursor) = &max_id {
            req = req.query(&[("max_id", cursor.as_str())]);
        }

        let resp = req
            .send()
            .await
            .context("instagram saved-feed request failed")?;
        // A 302 to login or a 401 is the session gone, or the missing
        // www-claim. Say which, loudly, rather than treating an empty redirect
        // body as "no saves".
        let status = resp.status();
        if status.is_redirection() || status == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!(
                "instagram returned {status} — the session is invalid, or the request needs an \
                 x-ig-www-claim header this credential does not carry. Reconnect Instagram."
            );
        }
        let resp = resp
            .error_for_status()
            .context("instagram returned an error status")?;
        let body: Value = resp.json().await.context("instagram response was not JSON")?;
        lake::archive_cloud(&pool, &storage, "instagram", ACTION, "saves", &[body.clone()]).await?;

        let page_data = transform::parse_page(&body)?;
        if new_high.is_none() {
            new_high = page_data.items.first().map(|i| i.media_id.clone());
        }

        for save in page_data.items {
            if last_seen.as_deref() == Some(save.media_id.as_str()) {
                reached_last_seen = true;
                break;
            }
            // Best-effort: keep the picture so the image pass can read it. A
            // failure (expired URL, CDN hiccup) is logged and the row is still
            // written — the caption alone makes it findable.
            let stored = match &save.image_url {
                Some(url) => match virtues::bookmark_media::store_saved_image(&pool, url).await {
                    Ok(s) => {
                        images_stored += 1;
                        Some(s)
                    }
                    Err(e) => {
                        tracing::warn!(media_id = %save.media_id, error = %e,
                            "could not store a saved image; writing the caption alone");
                        None
                    }
                },
                None => None,
            };
            total_written += transform::write_save(&pool, &save, stored.as_ref()).await?;
        }

        if reached_last_seen {
            hit_page_cap = false;
            break;
        }
        match page_data.next_max_id {
            Some(next) if page_data.more_available && Some(&next) != max_id.as_ref() => {
                max_id = Some(next);
            }
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
        "no new Instagram saves".to_string()
    } else {
        format!("synced {total_written} Instagram saves ({images_stored} images kept)")
    };
    if hit_page_cap {
        summary.push_str(&format!(
            " — stopped at the {MAX_PAGES}-page cap; saves past the oldest fetched are not synced yet"
        ));
    }
    output(&summary, &input.config)
}

/// Pull the `csrftoken` value out of a `Cookie:` header string. Instagram wants
/// it echoed in the `x-csrftoken` header, and it is already inside the jar the
/// person pasted, so ask for it once rather than as a second field.
fn csrftoken_from(cookies: &str) -> Option<String> {
    cookies.split(';').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k.trim() == "csrftoken")
            .then(|| v.trim().to_string())
            .filter(|s| !s.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csrftoken_is_read_out_of_the_jar() {
        let jar = "datr=abc; csrftoken=THE_TOKEN; sessionid=1%3Axyz; mid=zzz";
        assert_eq!(csrftoken_from(jar).as_deref(), Some("THE_TOKEN"));
    }

    #[test]
    fn a_jar_without_a_csrftoken_is_none() {
        assert_eq!(csrftoken_from("sessionid=1%3Axyz; mid=zzz"), None);
        assert_eq!(csrftoken_from("csrftoken=  ; sessionid=x"), None);
    }
}
