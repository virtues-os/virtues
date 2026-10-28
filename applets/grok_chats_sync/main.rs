//! Grok chats sync.
//!
//! Cron-driven, per-credential. Keeps a copy of the owner's grok.com chats by
//! replaying the requests the Grok web client makes for its own chat list,
//! with the owner's session cookies and a plain HTTP client. A spike
//! (agents/record/ai-chat-sync-spike-2026-10-23.md) found that grok.com, unlike
//! claude.ai and ChatGPT, answers such a client without a bot challenge.
//!
//! A run lists chats newest-updated first, fetches the ones the box does not
//! have, and writes their messages through the shared conversation writer, so
//! an export imported with `chat_import` and this sync never write a message
//! twice. Which chats to fetch, and when to stop, is `transform::Walk`.
//!
//! **Loud on drift.** An empty first page, a body without the expected array,
//! or a list that is not sorted newest first fails the run. Each of those is
//! what an expired session or a changed endpoint looks like, and reading them
//! as "no new chats" would hide it.

mod transform;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::time::Duration;
use virtues_helpers::{connect_from_env, conversations, output, read_input, AppletInput};

use transform::{ChatSummary, Stop, Synced, Walk};

const BASE: &str = "https://grok.com/rest/app-chat";

/// Chats per list page. The web client's own page size.
const PAGE_SIZE: u32 = 60;

/// The query parameter that carries `nextPageToken` back.
const PAGE_TOKEN_PARAM: &str = "pageToken";

/// Runaway guard on list pages: 50 pages of 60 is 3,000 chats. Stated in the
/// summary when hit.
const MAX_PAGES: u32 = 50;

/// Chats fetched per run. Each takes two requests plus a pause, so 100 stays
/// well inside the runner's 300 s subprocess limit. A larger first sync
/// continues on the next run.
const MAX_CHATS_PER_RUN: usize = 100;

/// Response ids sent per `load-responses` call.
const RESPONSES_PER_CALL: usize = 100;

/// Pause between requests, so a first sync does not hit grok.com in a burst.
const PAUSE: Duration = Duration::from_millis(300);

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
     AppleWebKit/537.36 (KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-grok_chats_sync").await?;
    let grok = Grok {
        client: virtues_applets::http_client(),
        cookies: session_cookies(&input)?,
    };

    let prior = read_synced(&input)?;
    let had_prior = prior.is_some();
    let mut walk = Walk::new(prior, MAX_CHATS_PER_RUN);

    let mut written = 0usize;
    let mut fetched = 0usize;
    let mut token: Option<String> = None;
    let mut first_id_of_last_page: Option<String> = None;
    let mut list_ended = false;
    let mut hit_page_cap = true;

    for page_no in 0..MAX_PAGES {
        let mut query = vec![("pageSize", PAGE_SIZE.to_string())];
        if let Some(t) = &token {
            query.push((PAGE_TOKEN_PARAM, t.clone()));
        }
        let body = match grok.get("/conversations", &query).await {
            Err(e) if e.is::<SessionEnded>() => {
                virtues_applets::needs_reconnect(&pool, &input, "Grok ended this session. Sign in again to reconnect.").await;
                return Err(e);
            }
            r => r?,
        };
        let page = transform::parse_list_page(&body)?;

        if page_no == 0 && page.chats.is_empty() {
            bail!(
                "grok returned an empty chat list{}. An expired session can look like this; \
                 if this account has no chats yet, start one on grok.com",
                if had_prior { " although earlier runs found chats" } else { "" }
            );
        }
        // A page token the server ignores returns the first page again, and
        // the walk would loop over it until the page cap.
        let first_id = page.chats.first().map(|c| c.id.clone());
        if page_no > 0 && first_id.is_some() && first_id == first_id_of_last_page {
            bail!("grok returned the same page twice; the `{PAGE_TOKEN_PARAM}` parameter is no longer honored");
        }
        first_id_of_last_page = first_id;

        for chat in walk.take(&page.chats)? {
            let rows = match grok.chat_messages(&chat).await {
                Err(e) if e.is::<SessionEnded>() => {
                    virtues_applets::needs_reconnect(&pool, &input, "Grok ended this session. Sign in again to reconnect.").await;
                    return Err(e);
                }
                r => r.with_context(|| format!("fetching grok chat {}", chat.id))?,
            };
            written += conversations::write_messages(&pool, &rows).await?;
            fetched += 1;
        }

        if walk.stopped().is_some() {
            hit_page_cap = false;
            break;
        }
        match page.next_page_token {
            Some(next) if Some(&next) != token.as_ref() => token = Some(next),
            _ => {
                list_ended = true;
                hit_page_cap = false;
                break;
            }
        }
    }

    let synced = walk.finish(list_ended);
    if let Some(s) = &synced {
        input.config["synced"] = json!({
            "newest": s.newest.to_rfc3339(),
            "oldest": s.oldest.to_rfc3339(),
            "complete": s.complete,
        });
    }

    let mut summary = match (fetched, written) {
        (0, _) => "no new Grok chats".to_string(),
        (f, w) => format!("synced {w} new messages from {f} Grok chats"),
    };
    if walk.stopped() == Some(Stop::Budget) || synced.as_ref().is_some_and(|s| !s.complete) {
        summary.push_str(&format!(
            " — older chats remain; the next run continues ({MAX_CHATS_PER_RUN} chats per run)"
        ));
    }
    if hit_page_cap {
        summary.push_str(&format!(
            " — stopped at the {MAX_PAGES}-page cap; chats past the oldest listed are not synced"
        ));
    }
    output(&summary, &input.config)
}

/// The `Cookie:` header for grok.com, from this run's credential.
///
/// Read from the stored session (`secrets.jar`), or from the `cookies` header
/// a sign-in older than the jar stored. Cloudflare's cookies are left out of
/// both: they belong to the browser that earned them, and the box makes its
/// own requests. The box already drops them from `jar`; the old header still
/// carries them.
fn session_cookies(input: &AppletInput) -> Result<String> {
    const CLOUDFLARE: [&str; 4] = ["cf_clearance", "__cf_bm", "__cflb", "_cfuvid"];
    let header = virtues_applets::cookie_header(input, "cookies")?
        .split(';')
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .filter(|c| !CLOUDFLARE.iter().any(|n| c.split('=').next() == Some(*n)))
        .collect::<Vec<_>>()
        .join("; ");
    if header.is_empty() {
        bail!("the Grok credential has no cookies; sign in to Grok again from Sources");
    }
    Ok(header)
}

/// The stored range, from `config.synced`. Absent before the first run.
fn read_synced(input: &AppletInput) -> Result<Option<Synced>> {
    let Some(s) = input.config.get("synced").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let time = |key: &str| {
        s.get(key)
            .and_then(Value::as_str)
            .and_then(transform::parse_time)
            .with_context(|| format!("config.synced.{key} is not a time; clear config.synced to resync"))
    };
    Ok(Some(Synced {
        newest: time("newest")?,
        oldest: time("oldest")?,
        complete: s.get("complete").and_then(Value::as_bool).unwrap_or(false),
    }))
}

/// grok.com answered 401 or 403: the session is gone, and only the owner
/// signing in again fixes it.
#[derive(Debug)]
struct SessionEnded(u16);

impl std::fmt::Display for SessionEnded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "grok returned {}; the session has ended and the owner must sign in again", self.0)
    }
}

impl std::error::Error for SessionEnded {}

struct Grok {
    client: reqwest::Client,
    cookies: String,
}

impl Grok {
    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        tokio::time::sleep(PAUSE).await;
        let req = self.client.get(format!("{BASE}{path}")).query(query);
        self.send(req).await
    }

    async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        tokio::time::sleep(PAUSE).await;
        let req = self
            .client
            .post(format!("{BASE}{path}"))
            .header("origin", "https://grok.com")
            .json(body);
        self.send(req).await
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<Value> {
        let resp = req
            .header("cookie", &self.cookies)
            .header("accept", "application/json")
            .header("referer", "https://grok.com/")
            .header("user-agent", USER_AGENT)
            .send()
            .await
            .context("grok request failed")?;
        let status = resp.status();
        // Cloudflare answers a challenge with 403 too. That says nothing about
        // the session, so it must not send the owner to sign in again.
        if resp.headers().contains_key("cf-mitigated") {
            bail!("grok.com answered with a Cloudflare challenge ({status}); trying again on the next run");
        }
        if matches!(status.as_u16(), 401 | 403) {
            return Err(SessionEnded(status.as_u16()).into());
        }
        let resp = resp.error_for_status().context("grok returned an error status")?;
        resp.json().await.context("grok response was not JSON")
    }

    /// Every kept message in one chat.
    async fn chat_messages(&self, chat: &ChatSummary) -> Result<Vec<conversations::ConversationMsg>> {
        let nodes = self
            .get(
                &format!("/conversations/{}/response-node", chat.id),
                &[("includeThreads", "true".to_string())],
            )
            .await?;
        let ids = transform::parse_response_ids(&nodes)?;
        let mut rows = Vec::new();
        for batch in ids.chunks(RESPONSES_PER_CALL) {
            let body = self
                .post(
                    &format!("/conversations/{}/load-responses", chat.id),
                    &json!({ "responseIds": batch }),
                )
                .await?;
            rows.extend(transform::parse_responses(chat, &body)?);
        }
        Ok(rows)
    }
}
