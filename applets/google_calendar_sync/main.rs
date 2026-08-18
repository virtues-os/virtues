//! Google Calendar sync.
//!
//! Cron-driven, per-credential. Lists the user's calendars, then for each
//! calendar pulls events using a `syncToken` cursor (incremental sync per
//! Google's calendar API). On first run there's no token; we query the
//! last 90 days + future and store the returned `nextSyncToken`.
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

    let calendars = list_calendars(&client, &access_token).await?;
    let mut total_written = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for cal in &calendars {
        let cal_id = cal
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("primary");

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

    input.config["sync_tokens"] = serde_json::json!(sync_tokens);

    let synced = calendars.len().saturating_sub(failures.len());
    let summary = format!(
        "synced {} events across {}/{} calendars",
        total_written,
        synced,
        calendars.len()
    );

    if !failures.is_empty() {
        if synced == 0 {
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
