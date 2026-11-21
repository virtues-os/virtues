//! Spotify recently-played sync: adds what you play on Spotify between data
//! downloads to `data_activity_listening`.
//!
//! Cron-driven, per-credential. The credential comes from a sign-in to the
//! owner's own Spotify app, and the box refreshes `secrets.access_token`
//! before each run, so this only reads it.
//!
//! `GET /v1/me/player/recently-played` returns at most the last 50 tracks,
//! newest first, and never podcasts. The newest `played_at` this sync has
//! seen is kept in `config.after_ms` and sent as `after`, so each run asks
//! only for what came since. The list is short and every row dedups on its
//! key, so asking again for a play already here costs nothing.
//!
//! A data download covers the same plays with what this list leaves out
//! (how long each one ran, skips, shuffle), so `spotify_import` replaces the
//! rows written here. This sync never changes a row that is already here
//! (`virtues_applets::listening`).

mod transform;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use virtues::storage::lake;
use virtues_applets::listening::{self, Writer};
use virtues_helpers::{connect_from_env, output_with_records, read_input, AppletInput};

const ACTION: &str = "spotify_recent_sync";
const RECENT_URL: &str = "https://api.spotify.com/v1/me/player/recently-played";
/// A `next` link is followed only under this root, so the token is never
/// sent to another host.
const API_ROOT: &str = "https://api.spotify.com/";
const PAGE_SIZE: u32 = 50;
/// The list holds 50 tracks, so one page is normal. This stops a `next` link
/// that never ends.
const MAX_PAGES: usize = 5;
/// How many times one request waits out a "too many requests" answer.
const RATE_LIMIT_TRIES: u32 = 4;
/// Where the cursor is kept in the applet's config: Unix milliseconds.
const CURSOR_KEY: &str = "after_ms";

const SIGN_IN_REFUSED: &str = "Spotify refused the sign-in; connect your Spotify app again.";
const REQUEST_REFUSED: &str = "Spotify refused this request. Check that your Spotify account's email is added \
     under Users Management in your Spotify app, and that the app's owner has Premium.";
const QUOTA_USED: &str = "Your Spotify app has used up its requests for today. The sync will try again later.";
const STILL_BUSY: &str = "Spotify kept asking for fewer requests. The sync will try again on its next run.";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();
    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-spotify_recent_sync").await?;
    let token = virtues_applets::secret(&input, "access_token")?.to_string();
    let storage = lake::storage_from_env()?;
    let client = virtues_applets::http_client();

    let mut url = first_url(cursor(&input.config));
    let mut written: Vec<DateTime<Utc>> = Vec::new();
    let mut newest: Option<i64> = None;
    let mut pages = 0;
    loop {
        let page = get_page(&client, &token, &url, &pool, &input).await?;
        pages += 1;
        let items: Vec<Value> = page.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        if !items.is_empty() {
            lake::archive_cloud(&pool, &storage, "spotify", ACTION, "recently_played", &items).await?;
        }
        newest = newest.max(newest_ms(&items));
        let plays: Vec<_> = items.iter().filter_map(transform::shape).collect();
        written.extend(listening::write_plays(&pool, &plays, Writer::Recent).await?);

        match next_url(&page) {
            Some(next) if pages < MAX_PAGES => url = next,
            Some(_) => {
                tracing::warn!(pages = MAX_PAGES, "{ACTION}: stopped following Spotify's next links at the page limit");
                break;
            }
            None => break,
        }
    }
    advance(&mut input.config, newest);

    let tz = home_tz(&pool).await;
    output_with_records(&summary(&written, tz), &input.config, written.len() as i64)
}

/// Fetch one page, waiting out "too many requests" a few times. Every other
/// refusal ends the run with a sentence saying what to do.
async fn get_page(
    client: &reqwest::Client,
    token: &str,
    url: &str,
    pool: &sqlx::PgPool,
    input: &AppletInput,
) -> Result<Value> {
    let mut tries = 0;
    loop {
        let resp = client.get(url).bearer_auth(token).send().await.context("The request to Spotify failed")?;
        let status = resp.status();
        if status.is_success() {
            return resp.json().await.context("Spotify's answer was not JSON");
        }
        let retry_after = resp.headers().get(reqwest::header::RETRY_AFTER).cloned();
        // The body only explains the refusal; one that cannot be read leaves
        // the status to say what happened.
        let body = resp.text().await.unwrap_or_default();
        match refusal(status.as_u16(), &body) {
            Refusal::SignIn => {
                virtues_applets::needs_reconnect(pool, input, SIGN_IN_REFUSED).await;
                bail!("{SIGN_IN_REFUSED}");
            }
            Refusal::Request => bail!("{REQUEST_REFUSED}"),
            Refusal::Quota => bail!("{QUOTA_USED}"),
            Refusal::Busy => {
                tries += 1;
                if tries >= RATE_LIMIT_TRIES {
                    bail!("{STILL_BUSY}");
                }
                tokio::time::sleep(virtues_applets::rate_limit_wait(retry_after.as_ref())).await;
            }
            Refusal::Other => bail!("Spotify answered {status}: {}", body.chars().take(300).collect::<String>()),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Refusal {
    /// 401: the token is no good, and refreshing it did not help.
    SignIn,
    /// 403: the Spotify app does not let this account use it.
    Request,
    /// 429 with `QUOTA_EXCEEDED`: the app's daily allowance is gone, so
    /// waiting a few seconds cannot help.
    Quota,
    /// Any other 429: wait as `Retry-After` asks.
    Busy,
    Other,
}

fn refusal(status: u16, body: &str) -> Refusal {
    match status {
        401 => Refusal::SignIn,
        403 => Refusal::Request,
        429 => {
            let json: Value = serde_json::from_str(body).unwrap_or(Value::Null);
            let reason = json
                .get("reason")
                .or_else(|| json.get("error").and_then(|e| e.get("reason")))
                .and_then(Value::as_str);
            if reason == Some("QUOTA_EXCEEDED") {
                Refusal::Quota
            } else {
                Refusal::Busy
            }
        }
        _ => Refusal::Other,
    }
}

/// The cursor from the last run, if there was one.
fn cursor(config: &Value) -> Option<i64> {
    config.get(CURSOR_KEY).and_then(Value::as_i64)
}

/// The first request. A first run asks for the whole list.
fn first_url(after: Option<i64>) -> String {
    match after {
        Some(ms) => format!("{RECENT_URL}?limit={PAGE_SIZE}&after={ms}"),
        None => format!("{RECENT_URL}?limit={PAGE_SIZE}"),
    }
}

/// The newest `played_at` on a page, in Unix milliseconds.
fn newest_ms(items: &[Value]) -> Option<i64> {
    items
        .iter()
        .filter_map(|i| i.get("played_at")?.as_str()?.parse::<DateTime<Utc>>().ok())
        .map(|t| t.timestamp_millis())
        .max()
}

/// Move the cursor forward to `newest`. It never moves back, so a page of
/// older plays cannot make the next run ask again for what it already has.
fn advance(config: &mut Value, newest: Option<i64>) {
    let Some(n) = newest else { return };
    if cursor(config).is_none_or(|c| n > c) {
        config[CURSOR_KEY] = Value::from(n);
    }
}

/// The page's `next` link, when there is one under Spotify's API root.
fn next_url(page: &Value) -> Option<String> {
    let next = page.get("next").and_then(Value::as_str).filter(|n| !n.is_empty())?;
    if next.starts_with(API_ROOT) {
        Some(next.to_string())
    } else {
        tracing::warn!("{ACTION}: Spotify's next link is not under {API_ROOT}; not following it");
        None
    }
}

/// The box's home zone, for the summary's clock times.
async fn home_tz(pool: &sqlx::PgPool) -> chrono_tz::Tz {
    let zone = virtues::timezone::home_timezone_or_utc(pool, Utc::now().date_naive()).await;
    zone.parse().unwrap_or(chrono_tz::UTC)
}

/// One sentence naming the span the new plays cover, never how many there
/// were.
fn summary(written: &[DateTime<Utc>], tz: chrono_tz::Tz) -> String {
    let (Some(first), Some(last)) = (written.iter().min(), written.iter().max()) else {
        return "No new Spotify plays.".to_string();
    };
    let (a, b) = (first.with_timezone(&tz), last.with_timezone(&tz));
    let clock = "%-I:%M %p";
    let (from, to) = if a.date_naive() == b.date_naive() {
        (a.format(clock).to_string(), b.format(clock).to_string())
    } else {
        let dated = "%b %-d, %-I:%M %p";
        (a.format(dated).to_string(), b.format(dated).to_string())
    };
    if from == to {
        format!("Added what you played on Spotify at {from}.")
    } else {
        format!("Added your Spotify plays from {from} to {to}.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item() -> Value {
        json!({
            "played_at": "2026-03-05T22:15:30.874Z",
            "context": { "type": "playlist", "uri": "spotify:playlist:0000000000000000000009" },
            "track": {
                "id": "0000000000000000000001",
                "uri": "spotify:track:0000000000000000000001",
                "name": "Paper Lanterns",
                "duration_ms": 183_000,
                "artists": [{ "name": "The Example Band" }, { "name": "Example Guest" }],
                "album": { "name": "Night Market" }
            }
        })
    }

    #[test]
    fn a_recently_played_item_is_a_span_ending_at_played_at() {
        let p = transform::shape(&item()).unwrap();
        assert_eq!(p.kind, "track");
        assert_eq!(p.title.as_deref(), Some("Paper Lanterns"));
        assert_eq!(p.artist_name.as_deref(), Some("The Example Band, Example Guest"));
        assert_eq!(p.album_name.as_deref(), Some("Night Market"));
        assert_eq!(p.uri.as_deref(), Some("spotify:track:0000000000000000000001"));
        assert_eq!(p.ended_at.to_rfc3339(), "2026-03-05T22:15:30.874+00:00");
        assert_eq!(p.started_at.to_rfc3339(), "2026-03-05T22:12:27.874+00:00");
        assert_eq!(p.played_ms, 183_000);
        assert_eq!(p.source_stream_id, "spotify:2026-03-05T22:15:30Z:spotify:track:0000000000000000000001");
        assert_eq!((p.is_skipped, p.is_shuffle, p.reason_start.as_deref(), p.reason_end.as_deref()), (None, None, None, None));
        assert_eq!(p.metadata, json!({ "context_uri": "spotify:playlist:0000000000000000000009", "played_ms_estimated": true }));
    }

    #[test]
    fn an_item_without_a_time_or_uri_is_skipped_and_a_missing_context_is_null() {
        let mut no_time = item();
        no_time["played_at"] = Value::Null;
        assert!(transform::shape(&no_time).is_none());
        let mut no_uri = item();
        no_uri["track"]["uri"] = Value::Null;
        assert!(transform::shape(&no_uri).is_none());
        let mut no_context = item();
        no_context["context"] = Value::Null;
        no_context["track"]["artists"] = json!([]);
        let p = transform::shape(&no_context).unwrap();
        assert_eq!(p.metadata["context_uri"], Value::Null);
        assert_eq!(p.artist_name, None);
    }

    #[test]
    fn the_cursor_starts_empty_and_only_moves_forward() {
        let mut config = json!({});
        assert_eq!(cursor(&config), None);
        assert_eq!(first_url(None), "https://api.spotify.com/v1/me/player/recently-played?limit=50");

        let mut later = item();
        later["played_at"] = json!("2026-03-05T22:19:00Z");
        let newest = newest_ms(&[item(), later]);
        assert_eq!(newest, Some(1_772_749_140_000));
        advance(&mut config, newest);
        assert_eq!(cursor(&config), Some(1_772_749_140_000));
        assert_eq!(
            first_url(cursor(&config)),
            "https://api.spotify.com/v1/me/player/recently-played?limit=50&after=1772749140000"
        );

        advance(&mut config, newest_ms(&[item()]));
        assert_eq!(cursor(&config), Some(1_772_749_140_000), "an older play does not move it back");
        advance(&mut config, None);
        assert_eq!(cursor(&config), Some(1_772_749_140_000), "an empty page leaves it alone");
    }

    #[test]
    fn next_links_are_followed_only_on_spotifys_api() {
        let next = "https://api.spotify.com/v1/me/player/recently-played?before=1772749140000&limit=50";
        assert_eq!(next_url(&json!({ "next": next })).as_deref(), Some(next));
        assert_eq!(next_url(&json!({ "next": null })), None);
        assert_eq!(next_url(&json!({ "next": "https://api.spotify.com.example.com/x" })), None);
    }

    #[test]
    fn each_refusal_says_what_to_do() {
        assert_eq!(refusal(401, ""), Refusal::SignIn);
        assert_eq!(refusal(403, "{}"), Refusal::Request);
        assert_eq!(refusal(429, ""), Refusal::Busy);
        assert_eq!(refusal(429, r#"{"error":{"status":429,"message":"x","reason":"QUOTA_EXCEEDED"}}"#), Refusal::Quota);
        assert_eq!(refusal(429, r#"{"reason":"QUOTA_EXCEEDED"}"#), Refusal::Quota);
        assert_eq!(refusal(500, ""), Refusal::Other);
    }

    #[test]
    fn the_summary_names_a_span_and_never_a_count() {
        let tz: chrono_tz::Tz = "America/Chicago".parse().unwrap();
        let t = |s: &str| s.parse::<DateTime<Utc>>().unwrap();
        assert_eq!(summary(&[], tz), "No new Spotify plays.");
        assert_eq!(
            summary(&[t("2026-03-05T22:45:00Z"), t("2026-03-05T20:10:00Z"), t("2026-03-05T21:00:00Z")], tz),
            "Added your Spotify plays from 2:10 PM to 4:45 PM."
        );
        assert_eq!(summary(&[t("2026-03-05T20:10:00Z")], tz), "Added what you played on Spotify at 2:10 PM.");
        assert_eq!(
            summary(&[t("2026-03-05T05:50:00Z"), t("2026-03-05T20:10:00Z")], tz),
            "Added your Spotify plays from Mar 4, 11:50 PM to Mar 5, 2:10 PM."
        );
    }
}
