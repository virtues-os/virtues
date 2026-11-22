//! Outlook calendar sync, from Microsoft Graph through virtues-api.
//!
//! Cron-driven, per-credential. Lists the owner's calendars, then reads each
//! one's `calendarView` over a fixed window, 90 days back (the Google
//! applet's full-sync reach) to 180 days ahead, and upserts every event in it.
//! `calendarView` expands recurring series into their instances, as the
//! Google applet's `singleEvents=true` does.
//!
//! Every run reads the whole window, so absence carries information: an event
//! stored from this calendar inside the window that Graph no longer returns
//! was deleted, and is tombstoned. That happens only after the calendar's
//! window was read to the last page. A calendar that hits the page limit
//! fails instead, because tombstoning after a partial read would mark events
//! that were simply never asked for.
//!
//! Calendars are independent: one failing is reported on stderr and the run
//! still exits clean. Only when every calendar fails, or the sign-in ended,
//! does the run fail.
//!
//! The applet only reads. Its scopes are read-only, and virtues-api forwards
//! only the GET calls listed in `SKILL.md`.

mod transform;

use anyhow::{anyhow, bail, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use virtues::storage::lake;
use virtues_applets::{graph_error, graph_get, graph_link};
use virtues_helpers::{connect_from_env, output_with_records, read_input};

const ACTION: &str = "outlook_calendar_sync";
const LOOKBACK_DAYS: i64 = 90;
const LOOKAHEAD_DAYS: i64 = 180;
const PAGE_SIZE: u32 = 100;
/// Pages per calendar per run: 2,000 events in the window.
const MAX_PAGES: usize = 20;
/// Pages of the calendar list.
const MAX_CALENDAR_PAGES: usize = 5;
/// Event times in UTC. Graph may ignore it, so the transform still reads
/// other zones.
const PREFER: &str = r#"outlook.timezone="UTC""#;
const SELECT: &str = "id,subject,bodyPreview,location,start,end,isAllDay,isCancelled,webLink,responseStatus,\
     attendees,organizer,isOrganizer,showAs,type,seriesMasterId,iCalUId,onlineMeeting,lastModifiedDateTime,sensitivity";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-outlook_calendar_sync").await?;
    let token = virtues_applets::secret(&input, "access_token")?.to_string();
    let graph = virtues_applets::outlook_graph(&input)?;
    let storage = lake::storage_from_env()?;
    let client = virtues_applets::http_client();

    // The owner's address tells their own calendars from shared ones.
    let user_email = match virtues_applets::config_str(&input, "user_email").filter(|e| !e.is_empty()) {
        Some(e) => e.to_string(),
        None => {
            let me = graph_get(&client, &token, &format!("{graph}/me"), None).await?;
            let email = ["mail", "userPrincipalName"]
                .iter()
                .find_map(|k| me.get(*k).and_then(Value::as_str).filter(|s| !s.is_empty()))
                .ok_or_else(|| anyhow!("Graph /me has no mail or userPrincipalName"))?
                .to_string();
            input.config["user_email"] = Value::from(email.clone());
            email
        }
    };

    let calendars: Vec<transform::Calendar> = list_calendars(&client, &token, &graph)
        .await?
        .iter()
        .filter_map(|c| transform::Calendar::from_graph(c, &user_email))
        .collect();

    let now = Utc::now();
    let window = Window {
        start: now - chrono::Duration::days(LOOKBACK_DAYS),
        end: now + chrono::Duration::days(LOOKAHEAD_DAYS),
    };
    let sync = Sync { client: &client, token: &token, graph: &graph, pool: &pool, storage: &storage, window };

    let (mut seen, mut changed, mut unknown_zone, mut tombstoned) = (0usize, 0usize, 0usize, 0u64);
    let mut failures: Vec<String> = Vec::new();
    for cal in &calendars {
        match sync.calendar(cal).await {
            Ok(run) => {
                seen += run.seen;
                changed += run.changed;
                unknown_zone += run.unknown_zone;
                tombstoned += run.tombstoned;
            }
            Err(e) if graph_error(&e).is_some_and(|g| g.is_reauth()) => return Err(e),
            Err(e) => {
                tracing::warn!(calendar = %cal.id, error = %e, "calendar sync failed");
                failures.push(format!("{}: {e:#}", cal.name.as_deref().unwrap_or(&cal.id)));
            }
        }
    }

    if !failures.is_empty() {
        if failures.len() == calendars.len() {
            bail!("all {} Outlook calendars failed: {}", calendars.len(), failures.join("; "));
        }
        eprintln!("{} of {} Outlook calendars failed: {}", failures.len(), calendars.len(), failures.join("; "));
    }

    let synced = calendars.len() - failures.len();
    let mut summary = format!(
        "read {seen} Outlook events across {synced}/{} calendars; {changed} new or changed, {tombstoned} removed",
        calendars.len()
    );
    if unknown_zone > 0 {
        summary.push_str(&format!("; {unknown_zone} skipped for a time zone the applet cannot read"));
    }
    output_with_records(&summary, &input.config, changed as i64 + tombstoned as i64)
}

#[derive(Clone, Copy)]
struct Window {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

struct Sync<'a> {
    client: &'a reqwest::Client,
    token: &'a str,
    /// `{api_base}/v1.0`.
    graph: &'a str,
    pool: &'a sqlx::PgPool,
    storage: &'a virtues::storage::Storage,
    window: Window,
}

struct CalendarRun {
    seen: usize,
    changed: usize,
    unknown_zone: usize,
    tombstoned: u64,
}

impl Sync<'_> {
    /// Read one calendar's whole window, then tombstone what is gone.
    async fn calendar(&self, cal: &transform::Calendar) -> Result<CalendarRun> {
        let mut url = view_url(self.graph, &cal.id, self.window);
        let mut seen: Vec<String> = Vec::new();
        let (mut changed, mut unknown_zone) = (0usize, 0usize);
        let mut finished = false;

        for _ in 0..MAX_PAGES {
            let page = graph_get(self.client, self.token, &url, Some(PREFER)).await?;
            // Identical pages are stored once, so re-reading an unchanged
            // window every run adds nothing to the lake.
            lake::archive_cloud(self.pool, self.storage, "outlook", ACTION, "outlook_calendar", std::slice::from_ref(&page))
                .await?;
            let items = page.get("value").and_then(Value::as_array).cloned().unwrap_or_default();
            let w = transform::write_events(self.pool, cal, &items).await?;
            seen.extend(w.seen);
            changed += w.changed;
            unknown_zone += w.unknown_zone;

            match page.get("@odata.nextLink").and_then(Value::as_str) {
                Some(next) => url = graph_link(self.graph, next)?,
                None => {
                    finished = true;
                    break;
                }
            }
        }
        if !finished {
            bail!("page limit ({MAX_PAGES}) reached before the end of the window; the calendar is too large for one run");
        }

        let tombstoned = transform::tombstone_absent(self.pool, &cal.id, self.window.start, self.window.end, &seen).await?;
        if tombstoned > 0 {
            tracing::info!(calendar = %cal.id, count = tombstoned, "tombstoned events Graph no longer lists");
        }
        Ok(CalendarRun { seen: seen.len(), changed, unknown_zone, tombstoned })
    }
}

/// Every calendar the owner can read, following Graph's paging.
async fn list_calendars(client: &reqwest::Client, token: &str, graph: &str) -> Result<Vec<Value>> {
    let mut url = format!("{graph}/me/calendars");
    let mut out = Vec::new();
    for _ in 0..MAX_CALENDAR_PAGES {
        let page = graph_get(client, token, &url, None).await?;
        out.extend(page.get("value").and_then(Value::as_array).cloned().unwrap_or_default());
        match page.get("@odata.nextLink").and_then(Value::as_str) {
            Some(next) => url = graph_link(graph, next)?,
            None => return Ok(out),
        }
    }
    bail!("the calendar list ran past {MAX_CALENDAR_PAGES} pages")
}

/// The first page of a calendar's events in the window.
fn view_url(graph: &str, calendar_id: &str, window: Window) -> String {
    let fmt = "%Y-%m-%dT%H:%M:%SZ";
    format!(
        "{graph}/me/calendars/{}/calendarView?startDateTime={}&endDateTime={}&$top={PAGE_SIZE}&$select={}",
        urlencoding::encode(calendar_id),
        urlencoding::encode(&window.start.format(fmt).to_string()),
        urlencoding::encode(&window.end.format(fmt).to_string()),
        urlencoding::encode(SELECT)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_asks_for_the_window_in_one_path_segment_per_calendar() {
        let window = Window {
            start: "2026-08-23T08:00:00Z".parse().unwrap(),
            end: "2027-05-20T08:00:00Z".parse().unwrap(),
        };
        let url = view_url("https://api.example.com/v1/outlook/c/conn_1/v1.0", "AAMk/x+y=", window);
        assert!(url.starts_with(
            "https://api.example.com/v1/outlook/c/conn_1/v1.0/me/calendars/AAMk%2Fx%2By%3D/calendarView?\
             startDateTime=2026-08-23T08%3A00%3A00Z&endDateTime=2027-05-20T08%3A00%3A00Z&$top=100&$select=id%2Csubject"
        ));
    }
}
