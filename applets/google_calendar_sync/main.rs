//! Google Calendar sync.
//!
//! Cron-driven, per-credential. Lists the user's calendars, then for each
//! calendar pulls events using a `syncToken` cursor (incremental sync per
//! Google's calendar API). On first run there's no token; we query the
//! last 90 days + future and store the returned `nextSyncToken`.
//!
//! Not every calendar is fetched on every run. The calendar list is fetched
//! once a day. Calendars Google publishes (holidays and the like) are checked
//! once a day, and a calendar with no changes for a week is checked hourly
//! until it changes again. Everything else is checked every run. The
//! schedule is `is_due`; the times it reads live in `checked_at` and
//! `changed_at` in the applet's state.
//!
//! When the box has handed over a push token (`push` in state, written by
//! `virtues-core/src/scheduler/google_push.rs`), each of the owner's and
//! shared calendars also gets a Google watch. Google then calls virtues-api
//! when the calendar changes, the box pulls that signal, and runs this applet
//! early with the watch's channel id in `push_channels`. A watched calendar
//! is still checked on the schedule above, in case a signal was lost. Watches
//! last up to 30 days and are renewed a day before they end.
//!
//! Two failure modes are load-bearing here, because together they froze a real
//! box's calendar for three days:
//!
//!   * A `syncToken` does not live forever. Google answers an expired one with
//!     **410 Gone**, and the documented recovery is to forget the token and
//!     resync in full. This used to be a bare `?`, so an expired token on ONE
//!     subscribed calendar (US Holidays, as it happens) failed the whole run.
//!   * Calendars are independent, so one failing must never starve the rest.
//!     The old loop propagated the first error, which meant the calendars
//!     listed *after* the broken one simply stopped syncing — silently, and
//!     the ordering of `calendarList` decided who lost.
//!
//! A partial failure is reported on **stderr with a clean exit**, not by
//! returning `Err`. That is deliberate: the runner discards stdout entirely on
//! a non-zero exit, so returning `Err` would also throw away the sync tokens of
//! every calendar that *did* succeed — and those calendars would then re-fetch
//! the same delta on every run, forever. The runner folds stderr into the run
//! summary, so the failure still surfaces in Telemetry. Only a total failure —
//! nothing synced at all — returns `Err`.

mod transform;

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use virtues::storage::lake;
use virtues_helpers::{connect_from_env, output_with_records, read_input};

const ACTION: &str = "google_calendar_sync";
const CAL_LIST: &str = "https://www.googleapis.com/calendar/v3/users/me/calendarList";
const EVENTS_BASE: &str = "https://www.googleapis.com/calendar/v3/calendars";
const PAGE_SIZE: u32 = 250;
const MAX_PAGES: u32 = 20;
/// How far back a full sync reaches. Only applies when there is no usable
/// cursor — normal runs are deltas, which are unbounded in age.
const FULL_SYNC_LOOKBACK_DAYS: i64 = 90;
/// People rarely add or remove a calendar, so the list is fetched once a day.
const CALENDAR_LIST_EVERY_SECS: i64 = 86_400;
/// Calendars Google publishes (holidays, sports, moon phases) change a few
/// times a year. Checked once a day.
const PUBLISHED_EVERY_SECS: i64 = 86_400;
/// A calendar with no changes for a week is checked hourly until it changes.
const IDLE_AFTER_SECS: i64 = 7 * 86_400;
const IDLE_EVERY_SECS: i64 = 3_600;
/// Runs land a few seconds after the quarter hour, not exactly on it, so a
/// check is due slightly before its interval is up.
const DUE_SLACK_SECS: i64 = 300;
/// A watched calendar is still checked this often, in case a signal was lost.
/// It matches the 15-minute schedule, so a watch never makes a calendar
/// slower than it was without one: on the first box to run watches, edits
/// went unsignalled for 13 hours before signals started arriving. Raise it
/// once the API's log of Google's calls shows signals can be relied on.
const WATCHED_EVERY_SECS: i64 = 900;
/// The longest watch Google gives a calendar.
const WATCH_TTL_SECS: i64 = 30 * 86_400;
/// Renew a watch when less than this is left on it.
const WATCH_RENEW_SECS: i64 = 86_400;
/// After Google refuses a watch, wait this long before asking again.
const WATCH_RETRY_SECS: i64 = 86_400;
const CHANNELS_STOP: &str = "https://www.googleapis.com/calendar/v3/channels/stop";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-google_calendar_sync").await?;

    let access_token = virtues_applets::secret(&input, "access_token")?
        .to_string();

    let storage = lake::storage_from_env()?;
    // Must be the shared client, not `reqwest::Client::new()`: reqwest here is
    // built `rustls-tls-no-provider`, so a bare client panics "No provider set"
    // on the first HTTPS call. `http_client()` installs the ring provider (and
    // adds a timeout a bare client lacks entirely).
    let client = virtues_applets::http_client();

    // Per-calendar sync tokens live under config.sync_tokens (map cal_id → token).
    let mut sync_tokens: HashMap<String, String> = input
        .config
        .get("sync_tokens")
        .and_then(|v| v.as_object())
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let now = chrono::Utc::now().timestamp();
    let listed_at = input.config.get("calendars_listed_at").and_then(|v| v.as_i64()).unwrap_or(0);
    let cached: Option<Vec<Value>> = input.config.get("calendars").and_then(|v| v.as_array()).cloned();
    let calendars = match cached {
        Some(c) if now - listed_at < CALENDAR_LIST_EVERY_SECS - DUE_SLACK_SECS => c,
        _ => {
            // Keep only what the loop reads, so state stays small.
            let fresh: Vec<Value> = list_calendars(&client, &access_token)
                .await?
                .iter()
                .map(|c| serde_json::json!({ "id": c.get("id"), "accessRole": c.get("accessRole") }))
                .collect();
            input.config["calendars"] = Value::from(fresh.clone());
            input.config["calendars_listed_at"] = Value::from(now);
            fresh
        }
    };

    let mut checked_at = int_map(&input.config, "checked_at");
    let mut changed_at = int_map(&input.config, "changed_at");
    let push = Push::from_config(&input.config);
    let mut watches: HashMap<String, Watch> = input
        .config
        .get("watches")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let mut watch_failed_at = int_map(&input.config, "watch_failed_at");
    // Channels Google said changed, from the run's payload.
    let woken: std::collections::HashSet<String> = input
        .payload
        .as_ref()
        .and_then(|p| p.get("push_channels"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|c| c.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let mut total_written = 0usize;
    let mut skipped = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for cal in &calendars {
        let cal_id = cal
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("primary");

        // A calendar seen for the first time starts out active.
        let changed = *changed_at.entry(cal_id.to_string()).or_insert(now);
        let watch = watches.get(cal_id).filter(|w| push.is_some() && w.expires_at > now);
        let was_woken = watch.is_some_and(|w| woken.contains(&w.id));
        if !was_woken && !is_due(cal_id, now, checked_at.get(cal_id).copied(), changed, watch.is_some()) {
            skipped += 1;
            continue;
        }

        // `accessRole` is the whole reason we walk the calendarList rather than
        // hitting `primary` directly: it is what distinguishes the owner's own
        // calendar from one they merely SUBSCRIBE to. A `reader` calendar's
        // events are other people's plans, and nothing downstream can work that
        // out from the event resource alone — Google puts it here, once, per
        // calendar. See migration 0066.
        let access_role = cal.get("accessRole").and_then(|v| v.as_str());

        let prior_token = sync_tokens.get(cal_id).cloned();
        match sync_calendar(
            &client,
            &access_token,
            &pool,
            &storage,
            cal_id,
            access_role,
            prior_token.as_deref(),
        )
        .await
        {
            Ok((written, new_token)) => {
                total_written += written;
                checked_at.insert(cal_id.to_string(), now);
                if written > 0 {
                    changed_at.insert(cal_id.to_string(), now);
                }
                match new_token {
                    Some(t) => {
                        sync_tokens.insert(cal_id.to_string(), t);
                    }
                    // No cursor came back. Drop any stale one rather than keep
                    // a token that no longer describes where we stopped.
                    None => {
                        sync_tokens.remove(cal_id);
                    }
                }
            }
            Err(e) => {
                // One calendar's problem is that calendar's problem. Leave its
                // last-known cursor untouched and carry on with the others.
                tracing::warn!(calendar = cal_id, error = %e, "calendar sync failed");
                failures.push(format!("{cal_id}: {e}"));
            }
        }
    }

    // Start or renew watches, so the next change is signalled.
    if let Some(push) = &push {
        for cal in &calendars {
            let Some(cal_id) = cal.get("id").and_then(|v| v.as_str()) else { continue };
            let role = cal.get("accessRole").and_then(|v| v.as_str());
            if !wants_watch(cal_id, role) {
                continue;
            }
            if watches.get(cal_id).is_some_and(|w| w.expires_at - now > WATCH_RENEW_SECS) {
                continue;
            }
            if watch_failed_at.get(cal_id).is_some_and(|t| now - t < WATCH_RETRY_SECS) {
                continue;
            }
            match start_watch(&client, &access_token, cal_id, push).await {
                Ok(w) => {
                    if let Some(old) = watches.insert(cal_id.to_string(), w) {
                        stop_watch(&client, &access_token, &old).await;
                    }
                    watch_failed_at.remove(cal_id);
                }
                Err(e) => {
                    tracing::warn!(calendar = cal_id, error = %e, "could not watch calendar; it stays on the schedule");
                    watch_failed_at.insert(cal_id.to_string(), now);
                }
            }
        }
    }

    // Drop entries for calendars that are no longer listed.
    let listed: std::collections::HashSet<&str> =
        calendars.iter().filter_map(|c| c.get("id").and_then(|v| v.as_str())).collect();
    for m in [&mut checked_at, &mut changed_at, &mut watch_failed_at] {
        m.retain(|k, _| listed.contains(k.as_str()));
    }
    sync_tokens.retain(|k, _| listed.contains(k.as_str()));
    let unlisted: Vec<String> = watches.keys().filter(|k| !listed.contains(k.as_str())).cloned().collect();
    for k in unlisted {
        if let Some(w) = watches.remove(&k) {
            stop_watch(&client, &access_token, &w).await;
        }
    }
    input.config["watches"] = serde_json::json!(watches);
    input.config["watch_failed_at"] = serde_json::json!(watch_failed_at);

    input.config["sync_tokens"] = serde_json::json!(sync_tokens);
    input.config["checked_at"] = serde_json::json!(checked_at);
    input.config["changed_at"] = serde_json::json!(changed_at);

    let attempted = calendars.len() - skipped;
    let synced = attempted.saturating_sub(failures.len());
    let summary = format!(
        "synced {} events across {}/{} calendars ({} not due)",
        total_written, synced, attempted, skipped
    );

    if !failures.is_empty() {
        if synced == 0 && attempted > 0 {
            // Nothing worked, so there are no fresh cursors to protect by
            // exiting clean. Fail the run outright.
            return Err(anyhow!(
                "all {} calendars failed: {}",
                calendars.len(),
                failures.join("; ")
            ));
        }
        eprintln!(
            "{} of {} calendars failed: {}",
            failures.len(),
            calendars.len(),
            failures.join("; ")
        );
    }

    output_with_records(&summary, &input.config, total_written as i64)
}

/// Whether a calendar should be fetched on this run, when no signal asked
/// for it.
fn is_due(cal_id: &str, now: i64, checked_at: Option<i64>, changed_at: i64, watched: bool) -> bool {
    let Some(checked) = checked_at else { return true };
    let every = if is_published(cal_id) {
        PUBLISHED_EVERY_SECS
    } else if watched {
        WATCHED_EVERY_SECS
    } else if now - changed_at >= IDLE_AFTER_SECS {
        IDLE_EVERY_SECS
    } else {
        return true;
    };
    now - checked >= every - DUE_SLACK_SECS
}

fn is_published(cal_id: &str) -> bool {
    cal_id.ends_with("@group.v.calendar.google.com")
}

/// Published calendars change too rarely to watch, and Google gives no
/// watch on a calendar that shows only free and busy times.
fn wants_watch(cal_id: &str, access_role: Option<&str>) -> bool {
    !is_published(cal_id) && matches!(access_role, Some("owner" | "writer" | "reader"))
}

/// Where Google should send change signals, from the box.
struct Push {
    token: String,
    address: String,
}

impl Push {
    fn from_config(config: &Value) -> Option<Push> {
        let p = config.get("push")?;
        let token = p.get("token")?.as_str()?.to_string();
        let address = p.get("address")?.as_str()?.to_string();
        (!token.is_empty() && address.starts_with("https://")).then_some(Push { token, address })
    }
}

/// A live Google watch on one calendar.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct Watch {
    id: String,
    resource_id: String,
    expires_at: i64,
}

async fn start_watch(client: &reqwest::Client, token: &str, cal_id: &str, push: &Push) -> Result<Watch> {
    let id = uuid::Uuid::new_v4().to_string();
    let url = format!("{EVENTS_BASE}/{}/events/watch", urlencoding::encode(cal_id));
    let resp: Value = client
        .post(&url)
        .bearer_auth(token)
        .json(&serde_json::json!({
            "id": id,
            "type": "web_hook",
            "address": push.address,
            "token": push.token,
            "params": { "ttl": WATCH_TTL_SECS.to_string() },
        }))
        .send()
        .await
        .context("watch request failed")?
        .error_for_status()
        .context("watch non-2xx")?
        .json()
        .await
        .context("watch response was not JSON")?;
    let resource_id = resp
        .get("resourceId")
        .and_then(|v| v.as_str())
        .context("watch response has no resourceId")?
        .to_string();
    // Google gives the end as milliseconds, in a string.
    let expires_at = resp
        .get("expiration")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .map(|ms| ms / 1000)
        .context("watch response has no expiration")?;
    Ok(Watch { id, resource_id, expires_at })
}

/// Stop a watch that was replaced. Best effort: an unstopped watch ends on
/// its own, and its signals name a channel this applet no longer knows.
async fn stop_watch(client: &reqwest::Client, token: &str, w: &Watch) {
    let res = client
        .post(CHANNELS_STOP)
        .bearer_auth(token)
        .json(&serde_json::json!({ "id": w.id, "resourceId": w.resource_id }))
        .send()
        .await
        .and_then(|r| r.error_for_status());
    if let Err(e) = res {
        tracing::debug!(error = %e, "could not stop an old calendar watch");
    }
}

fn int_map(config: &Value, key: &str) -> HashMap<String, i64> {
    config
        .get(key)
        .and_then(|v| v.as_object())
        .map(|o| o.iter().filter_map(|(k, v)| v.as_i64().map(|n| (k.clone(), n))).collect())
        .unwrap_or_default()
}

async fn list_calendars(client: &reqwest::Client, token: &str) -> Result<Vec<Value>> {
    let resp: Value = client
        .get(CAL_LIST)
        .bearer_auth(token)
        .send()
        .await
        .context("calendarList request failed")?
        .error_for_status()
        .context("calendarList non-2xx")?
        .json()
        .await
        .context("calendarList response was not JSON")?;

    Ok(resp
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default())
}

/// Sync one calendar, recovering from an expired cursor.
///
/// Google invalidates a `syncToken` after a while (and whenever the calendar
/// changes in ways a delta cannot express). It answers with **410 Gone**, and
/// the only correct response is to forget the token and resync in full — which
/// we do exactly once, so a permanently broken calendar cannot spin.
async fn sync_calendar(
    client: &reqwest::Client,
    token: &str,
    db: &sqlx::PgPool,
    storage: &virtues::storage::Storage,
    calendar_id: &str,
    access_role: Option<&str>,
    prior_sync_token: Option<&str>,
) -> Result<(usize, Option<String>)> {
    match sync_pages(
        client, token, db, storage, calendar_id, access_role, prior_sync_token,
    )
    .await
    {
        Err(e) if prior_sync_token.is_some() && is_token_expired(&e) => {
            tracing::warn!(
                calendar = calendar_id,
                "syncToken expired (410 Gone) — discarding it and resyncing in full"
            );
            sync_pages(client, token, db, storage, calendar_id, access_role, None).await
        }
        other => other,
    }
}

/// True when the error chain carries a 410 from the events endpoint.
fn is_token_expired(err: &anyhow::Error) -> bool {
    err.chain().any(|c| {
        c.downcast_ref::<reqwest::Error>()
            .and_then(|e| e.status())
            .is_some_and(|s| s == reqwest::StatusCode::GONE)
    })
}

async fn sync_pages(
    client: &reqwest::Client,
    token: &str,
    db: &sqlx::PgPool,
    storage: &virtues::storage::Storage,
    calendar_id: &str,
    access_role: Option<&str>,
    prior_sync_token: Option<&str>,
) -> Result<(usize, Option<String>)> {
    let full_sync = prior_sync_token.is_none();
    let since = (chrono::Utc::now() - chrono::Duration::days(FULL_SYNC_LOOKBACK_DAYS)).to_rfc3339();

    let mut written = 0usize;
    let mut page_token: Option<String> = None;
    let mut next_sync_token: Option<String> = None;
    // Only collected on a full sync, where absence carries information.
    let mut seen_keys: Vec<String> = Vec::new();
    let mut ran_out_of_pages = true;
    let url = format!(
        "{EVENTS_BASE}/{}/events",
        urlencoding::encode(calendar_id)
    );

    for _ in 0..MAX_PAGES {
        let mut req = client.get(&url).bearer_auth(token).query(&[
            ("maxResults", PAGE_SIZE.to_string()),
            ("singleEvents", "true".to_string()),
        ]);
        if let Some(t) = prior_sync_token {
            req = req.query(&[("syncToken", t)]);
        } else {
            // First sync, or post-410 recovery: bound the window.
            req = req.query(&[("timeMin", since.clone())]);
        }
        if let Some(p) = &page_token {
            req = req.query(&[("pageToken", p.clone())]);
        }

        let resp: Value = req
            .send()
            .await
            .context("events request failed")?
            .error_for_status()
            .context("events non-2xx")?
            .json()
            .await
            .context("events response was not JSON")?;

        // Archive the whole page. A syncToken response is a DELTA — it carries
        // cancellations and edits, and the token is single-use, so Google will never
        // describe this change again. Whatever the transform doesn't read here is gone.
        lake::archive_cloud(db, storage, "google", ACTION, "calendar", &[resp.clone()]).await?;

        if let Some(items) = resp.get("items").and_then(|v| v.as_array()) {
            let keys = transform::write_events(db, calendar_id, access_role, items).await?;
            written += keys.len();
            if full_sync {
                seen_keys.extend(keys);
            }
        }

        if let Some(np) = resp.get("nextPageToken").and_then(|v| v.as_str()) {
            page_token = Some(np.to_string());
            continue;
        }
        if let Some(nst) = resp.get("nextSyncToken").and_then(|v| v.as_str()) {
            next_sync_token = Some(nst.to_string());
        }
        ran_out_of_pages = false;
        break;
    }

    // Out of page budget with more to fetch. Google only hands back a cursor on
    // the last page, so there is nothing to resume from, and the window is
    // incomplete — which means a full-sync reconciliation here would tombstone
    // events we simply never asked for. Fail this calendar loudly instead of
    // writing a half-truth.
    if ran_out_of_pages {
        return Err(anyhow!(
            "page limit ({MAX_PAGES}) reached before the end of the event stream — \
             calendar too large for a single run"
        ));
    }

    // A full sync is the only time absence carries information. Google returns
    // only live events, so anything we already hold in that window that did NOT
    // come back was deleted while we weren't looking — most likely while a dead
    // syncToken had us locked out. Tombstone it; never delete, because the row
    // is history and `deleted_at_source` is how the rest of the system reads
    // "this didn't happen". Without this, a meeting cancelled during an outage
    // stays on the calendar forever and the day narrator reads it as somewhere
    // the owner actually was.
    if full_sync {
        let tombstoned = tombstone_absent(db, calendar_id, &since, &seen_keys).await?;
        if tombstoned > 0 {
            tracing::info!(
                calendar = calendar_id,
                count = tombstoned,
                "full resync: tombstoned events Google no longer lists"
            );
        }
    }

    Ok((written, next_sync_token))
}

/// Mark rows in the resynced window that Google no longer returns as deleted.
///
/// Scoped hard to this provider's rows for this calendar: iOS EventKit writes
/// into the same table, and its rows are not ours to tombstone.
///
/// This is safe to be wrong about. The upsert re-projects `deleted_at_source`
/// on every sync, and a live event carries `None` — so if an event is
/// tombstoned here and Google lists it again later, the next sync clears the
/// tombstone. That self-healing is what makes it acceptable to act on absence
/// at all; a hard DELETE here would not be recoverable.
async fn tombstone_absent(
    db: &sqlx::PgPool,
    calendar_id: &str,
    since: &str,
    seen_keys: &[String],
) -> Result<u64> {
    let res = sqlx::query(
        "UPDATE data_calendar_event \
         SET deleted_at_source = now() \
         WHERE source_provider = 'google' \
           AND source_table = 'google_calendar' \
           AND calendar_name = $1 \
           AND started_at >= $2::timestamptz \
           AND deleted_at_source IS NULL \
           AND source_stream_id <> ALL($3)",
    )
    .bind(calendar_id)
    .bind(since)
    .bind(seen_keys)
    .execute(db)
    .await
    .context("tombstoning absent calendar events failed")?;

    Ok(res.rows_affected())
}

#[cfg(test)]
mod due_tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn an_active_calendar_is_checked_every_run() {
        assert!(is_due("me@example.com", NOW, Some(NOW - 900), NOW - 3_600, false));
    }

    #[test]
    fn an_idle_calendar_is_checked_hourly() {
        let week_ago = NOW - IDLE_AFTER_SECS;
        assert!(!is_due("me@example.com", NOW, Some(NOW - 900), week_ago, false));
        assert!(is_due("me@example.com", NOW, Some(NOW - 3_600), week_ago, false));
    }

    #[test]
    fn a_watched_calendar_is_still_checked_every_run() {
        assert!(is_due("me@example.com", NOW, Some(NOW - 900), NOW, true));
        // Idle for a week and watched: the watch's interval applies, not the hourly one.
        assert!(is_due("me@example.com", NOW, Some(NOW - 900), NOW - IDLE_AFTER_SECS, true));
        assert!(!is_due("me@example.com", NOW, Some(NOW - 200), NOW, true));
    }

    #[test]
    fn only_calendars_with_events_to_read_are_watched() {
        assert!(wants_watch("me@example.com", Some("owner")));
        assert!(wants_watch("team@example.com", Some("reader")));
        assert!(!wants_watch("busy@example.com", Some("freeBusyReader")));
        assert!(!wants_watch("en.usa#holiday@group.v.calendar.google.com", Some("reader")));
    }

    #[test]
    fn push_needs_a_token_and_an_https_address() {
        let ok = serde_json::json!({ "push": { "token": "t", "address": "https://api.example.com/v1/push/google/calendar" } });
        assert!(Push::from_config(&ok).is_some());
        let plain = serde_json::json!({ "push": { "token": "t", "address": "http://api.example.com/x" } });
        assert!(Push::from_config(&plain).is_none());
        assert!(Push::from_config(&serde_json::json!({})).is_none());
    }

    #[test]
    fn a_published_calendar_is_checked_daily() {
        let id = "en.usa#holiday@group.v.calendar.google.com";
        assert!(!is_due(id, NOW, Some(NOW - 3_600), NOW, false));
        assert!(is_due(id, NOW, Some(NOW - 86_400 + 60), NOW, false));
        assert!(is_due(id, NOW, None, NOW, false));
    }
}
