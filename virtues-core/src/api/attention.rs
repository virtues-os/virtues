//! What needs the owner, in one list.
//!
//! The box already knows when something is wrong: a stream stopped arriving,
//! a sign-in expired, the backup drive went missing, the disk is filling, an
//! update failed and rolled back. Each of those was shown on its own page
//! (Sources, System, Software), and a person who doesn't open those pages
//! never learned any of it. This gathers them so a client can say so without
//! being asked: the Mac app turns a new item into a notification.
//!
//! Every item's `key` stays the same for as long as its condition holds, so a
//! client notifies once per problem, not once per poll.
//!
//! Each source is read independently, and one that fails to read is left out
//! rather than failing the list: a broken backup query must not hide an
//! expired sign-in.

use serde::Serialize;

use crate::database::Database;
use crate::Result;

/// Below this much free space where the archive lives, the box says so. The
/// lake itself stops archiving at 2 GiB (`storage::lake`); this leaves time to
/// act before that.
const LOW_DISK_BYTES: u64 = 5 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AttentionItem {
    /// Stable while the condition holds; a new key is a new problem.
    pub key: String,
    /// `key` | `update` | `disk` | `backup` | `source` | `connection` |
    /// `stream` | `applet` | `applet_new`, most urgent first.
    pub kind: &'static str,
    pub title: String,
    pub body: String,
    /// The in-app page that shows or fixes it.
    pub route: String,
}

pub async fn attention(db: &Database) -> Result<Vec<AttentionItem>> {
    let mut items = Vec::new();

    key_items(&mut items, crate::secrets::key_at_start());
    update_items(&mut items);
    disk_items(&mut items, crate::storage::lake::free_bytes());

    match crate::api::backup_status::get_backup_status(db.pool()).await {
        Ok(b) => backup_items(&mut items, &b.state, b.age_seconds),
        Err(e) => tracing::warn!("attention: backup status unreadable: {e}"),
    }

    match broken_sources(db).await {
        Ok(rows) => source_items(&mut items, rows),
        Err(e) => tracing::warn!("attention: credentials unreadable: {e}"),
    }

    match signed_out_connections(db).await {
        Ok(rows) => connection_items(&mut items, rows),
        Err(e) => tracing::warn!("attention: connections unreadable: {e}"),
    }

    match expiring_sources(db).await {
        Ok(rows) => expiring_items(&mut items, rows, chrono::Utc::now()),
        Err(e) => tracing::warn!("attention: credential expiry unreadable: {e}"),
    }

    match failed_applets(db).await {
        Ok(rows) => failed_applet_items(&mut items, rows),
        Err(e) => tracing::warn!("attention: applet runs unreadable: {e}"),
    }

    match crate::api::stream_health::stream_health(db).await {
        Ok(streams) => {
            stream_items(&mut items, &streams);
            match crate::api::stream_health::stream_days(db, RHYTHM_WINDOW_DAYS).await {
                Ok(days) => quiet_stream_items(&mut items, &streams, &days),
                Err(e) => tracing::warn!("attention: stream arrivals unreadable: {e}"),
            }
        }
        Err(e) => tracing::warn!("attention: stream health unreadable: {e}"),
    }

    match new_from_applets(db).await {
        Ok(rows) => new_applet_items(&mut items, rows),
        Err(e) => tracing::warn!("attention: applet results unreadable: {e}"),
    }

    Ok(items)
}

/// `(applet id, name, run id, error)` for each applet that is on and whose
/// latest finished run failed. It stays until a run succeeds, or the owner
/// turns the applet off.
async fn failed_applets(db: &Database) -> Result<Vec<(String, String, String, Option<String>)>> {
    Ok(sqlx::query_as(
        "SELECT a.id, a.name, r.id, r.error FROM app_applets a \
         JOIN LATERAL (SELECT id, status, error FROM app_applet_runs \
                        WHERE applet_id = a.id AND status IN ('success', 'error', 'budget_exceeded') \
                        ORDER BY started_at DESC LIMIT 1) r ON true \
         WHERE a.enabled AND a.archived_at IS NULL AND r.status = 'error' \
         ORDER BY a.name",
    )
    .fetch_all(db.pool())
    .await?)
}

fn failed_applet_items(items: &mut Vec<AttentionItem>, rows: Vec<(String, String, String, Option<String>)>) {
    for (id, name, run_id, error) in rows {
        let why = error
            .as_deref()
            .map(|e| {
                let line = e.lines().next().unwrap_or(e);
                line.split(". ").next().unwrap_or(line).trim_end_matches('.').trim().chars().take(160).collect::<String>()
            })
            .filter(|e| !e.is_empty());
        items.push(AttentionItem {
            key: format!("applet:{run_id}"),
            kind: "applet",
            title: format!("{name} didn't finish its last run"),
            body: match why {
                Some(why) => format!("{why}. Open it to see what happened."),
                None => "Open it to see what happened.".into(),
            },
            route: format!("/applet/{id}"),
        });
    }
}

/// `(applet id, name, run id, page title, what the run said)` for each applet
/// with an unattended run since the owner last opened it that has something
/// to read: a page it wrote, or what a prompt applet said. A sync's or a
/// shipped applet's own report is left out, so a run that only keeps things
/// current never lights the Inbox. One row per applet, its latest such run.
async fn new_from_applets(db: &Database) -> Result<Vec<(String, String, String, Option<String>, Option<String>)>> {
    Ok(sqlx::query_as(
        "SELECT DISTINCT ON (a.id) a.id, a.name, r.id, p.title, r.result_summary \
           FROM app_applets a \
           JOIN app_applet_runs r ON r.applet_id = a.id \
           LEFT JOIN app_applet_run_outputs o ON o.run_id = r.id AND o.kind = 'page' \
           LEFT JOIN app_pages p ON p.id = o.ref_id AND p.deleted_at IS NULL \
          WHERE a.enabled AND a.archived_at IS NULL \
            AND r.status = 'success' AND r.trigger IN ('cron', 'webhook') \
            AND r.started_at > coalesce(a.seen_at, '-infinity'::timestamptz) \
            AND (p.id IS NOT NULL \
                 OR (a.owner <> 'system' AND a.agent IS NOT NULL AND btrim(coalesce(r.result_summary, '')) <> '')) \
          ORDER BY a.id, r.started_at DESC",
    )
    .fetch_all(db.pool())
    .await?)
}

fn new_applet_items(items: &mut Vec<AttentionItem>, rows: Vec<(String, String, String, Option<String>, Option<String>)>) {
    for (id, name, run_id, page, said) in rows {
        let (title, body) = match page {
            Some(page) => (format!("{name} wrote {page}"), "Open it to read the page.".to_string()),
            None => {
                let said = said.unwrap_or_default();
                let line = said.lines().next().unwrap_or_default().trim();
                let body = if line.chars().count() > 160 {
                    format!("{}…", line.chars().take(159).collect::<String>())
                } else {
                    line.to_string()
                };
                (format!("{name} has something for you"), body)
            }
        };
        items.push(AttentionItem {
            key: format!("applet_new:{run_id}"),
            kind: "applet_new",
            title,
            body,
            route: format!("/applet/{id}"),
        });
    }
}

/// The server's key opens none of its saved secrets (`secrets::check_key_at_start`).
/// First in the list, because every sign-in, password and key is unreadable
/// until it is fixed, and anything saved meanwhile is sealed with the wrong
/// key. The check runs once per start, so the item stays until a restart
/// with the right key. Putting the key back also changes what the restricted
/// database logins have to be, and only root can reset those, so the fix runs
/// `sudo virtues migrate` before the restart.
fn key_items(items: &mut Vec<AttentionItem>, check: Option<crate::secrets::KeyCheck>) {
    if !check.is_some_and(|c| c.opens_nothing()) {
        return;
    }
    items.push(AttentionItem {
        key: "key:wrong".into(),
        kind: "key",
        title: "Your server can't open its saved sign-ins and keys".into(),
        body: "The encryption key it started with isn't the one they were saved with. \
               Put back the virtues.env from your backup, run sudo virtues migrate, then restart your server."
            .into(),
        route: "/virtues/system".into(),
    });
}

fn update_items(items: &mut Vec<AttentionItem>) {
    let last = crate::cli::auto_update::last();
    if let Some(install) = last.install.filter(|i| !i.ok) {
        // Only while the box is still on the release it fell back to: once a
        // later install succeeds, `install` is that one, and this is gone.
        items.push(AttentionItem {
            key: format!("update:{}", install.to),
            kind: "update",
            title: "An update didn't install".into(),
            body: "Your server went back to the version it had, so nothing changed. Open Software to see what happened."
                .into(),
            route: "/virtues/devices/server".into(),
        });
    }
}

fn disk_items(items: &mut Vec<AttentionItem>, free: Option<u64>) {
    let Some(free) = free else { return };
    if free >= LOW_DISK_BYTES {
        return;
    }
    let gb = free as f64 / (1024.0 * 1024.0 * 1024.0);
    items.push(AttentionItem {
        key: "disk:low".into(),
        kind: "disk",
        title: "Your server is almost out of space".into(),
        body: format!(
            "{gb:.1} GB left. When it runs out, new data stops arriving. Open System to see what's using it."
        ),
        route: "/virtues/system".into(),
    });
}

fn backup_items(items: &mut Vec<AttentionItem>, state: &str, age_seconds: Option<i64>) {
    let (title, body) = match state {
        "none" => (
            "Your server has no backup".to_string(),
            "Everything is on one disk. Open System to add a backup drive.".to_string(),
        ),
        "never" => (
            "Your server hasn't backed up to the drive yet".to_string(),
            "Your server hasn't finished a backup to it. Open System to check the drive.".to_string(),
        ),
        "failing" => (
            "Backups are failing".to_string(),
            "Your server couldn't finish its last backup. Open System to see why.".to_string(),
        ),
        "stale" => {
            let days = age_seconds.unwrap_or(0) / 86_400;
            (
                format!("Your last backup is {days} days old"),
                "Plug in your backup drive, then open System to see the last backup."
                    .to_string(),
            )
        }
        _ => return,
    };
    items.push(AttentionItem {
        key: format!("backup:{state}"),
        kind: "backup",
        title,
        body,
        route: "/virtues/system".into(),
    });
}

/// `(credential id, name, status)` for every connection that needs the owner.
async fn broken_sources(db: &Database) -> Result<Vec<(String, String, String)>> {
    Ok(sqlx::query_as(
        "SELECT id, name, status FROM credentials \
         WHERE kind = 'source' AND status IN ('reauth_required', 'error') \
         ORDER BY name",
    )
    .fetch_all(db.pool())
    .await?)
}

fn source_items(items: &mut Vec<AttentionItem>, rows: Vec<(String, String, String)>) {
    for (id, name, status) in rows {
        let (title, body) = if status == "reauth_required" {
            (
                format!("{name} needs you to sign in again"),
                "Your server can't sync it until you reconnect. Open Sources to reconnect.".to_string(),
            )
        } else {
            (
                format!("{name} isn't connecting"),
                "Your server can't reach it. Open Sources to see why.".to_string(),
            )
        };
        items.push(AttentionItem {
            key: format!("source:{id}:{status}"),
            kind: "source",
            title,
            body,
            route: "/sources".into(),
        });
    }
}

/// `(short name, name)` for every connected service that stopped accepting
/// the owner's sign-in (`tools/connection.rs`).
async fn signed_out_connections(db: &Database) -> Result<Vec<(String, String)>> {
    Ok(sqlx::query_as(
        "SELECT key, name FROM credentials \
         WHERE kind = 'connection' AND status = 'reauth_required' \
         ORDER BY name",
    )
    .fetch_all(db.pool())
    .await?)
}

fn connection_items(items: &mut Vec<AttentionItem>, rows: Vec<(String, String)>) {
    for (key, name) in rows {
        items.push(AttentionItem {
            key: format!("connection:{key}:reauth_required"),
            kind: "connection",
            title: format!("{name} needs you to sign in again"),
            body: "The assistant can't use it until you do. Open Sources to sign in.".to_string(),
            route: "/sources/catalog".into(),
        });
    }
}

/// How far ahead the owner hears that a sign-in is ending. Long enough to
/// see it on a weekday, and the session still works until then.
const EXPIRY_WARNING_DAYS: i64 = 3;

/// `(credential id, name, expires_at)` for working sign-ins that end soon.
/// Only sessions: an OAuth credential has `next_refresh_at` and renews itself.
async fn expiring_sources(db: &Database) -> Result<Vec<(String, String, chrono::DateTime<chrono::Utc>)>> {
    Ok(sqlx::query_as(
        "SELECT id, name, expires_at FROM credentials \
         WHERE kind = 'source' AND status = 'active' AND next_refresh_at IS NULL \
           AND expires_at IS NOT NULL \
           AND expires_at < now() + make_interval(days => $1) \
         ORDER BY expires_at",
    )
    .bind(EXPIRY_WARNING_DAYS as i32)
    .fetch_all(db.pool())
    .await?)
}

fn expiring_items(
    items: &mut Vec<AttentionItem>,
    rows: Vec<(String, String, chrono::DateTime<chrono::Utc>)>,
    now: chrono::DateTime<chrono::Utc>,
) {
    for (id, name, expires_at) in rows {
        let left = expires_at - now;
        let when = match left.num_hours() {
            h if h <= 0 => "has ended".to_string(),
            h if h < 24 => "ends within a day".to_string(),
            h if h < 48 => "ends tomorrow".to_string(),
            h => format!("ends in {} days", (h + 23) / 24),
        };
        items.push(AttentionItem {
            key: format!("source:{id}:expiring:{}", expires_at.timestamp()),
            kind: "source",
            title: format!("{name} needs you to sign in again soon"),
            body: format!(
                "Your server's sign-in to it {when}. Open Sources and reconnect to keep it syncing."
            ),
            route: "/sources".into(),
        });
    }
}

/// How long a whole source must be silent before it is an alarm. One quiet
/// day is a weekend; two in a row from every stream it writes is a pipe.
const SOURCE_QUIET_DAYS: i64 = 2;

fn stream_items(items: &mut Vec<AttentionItem>, streams: &[crate::api::stream_health::StreamHealth]) {
    // A stream whose writer keeps failing is a known fault, reported as one.
    for s in streams.iter().filter(|s| s.connected && s.status == "blocked") {
        items.push(AttentionItem {
            key: format!("stream:{}:blocked", s.name),
            kind: "stream",
            title: format!("{} can't sync", s.display_name),
            body: "Your server's sync for it keeps failing. Open Sources to see why.".into(),
            route: "/sources".into(),
        });
    }

    // Silence is judged per SOURCE, not per stream. Many streams are sparse
    // by nature (workouts, calendar events, bookmarks), and "Workouts stopped
    // arriving" after two days without a workout would be a false alarm. A
    // source whose every stream has gone quiet, though, is the failure that
    // actually happens: the Mac collector stopped, the phone app was killed,
    // a sync lost its permission.
    let mut by_source: std::collections::BTreeMap<&str, Vec<&crate::api::stream_health::StreamHealth>> =
        Default::default();
    for s in streams.iter().filter(|s| s.connected) {
        if let Some(source) = s.provided_by.first() {
            by_source.entry(source.as_str()).or_default().push(s);
        }
    }
    let now = chrono::Utc::now();
    for (source, streams) in by_source {
        if streams.iter().any(|s| s.status == "live" || s.status == "blocked") {
            continue;
        }
        // Only a source that was flowing recently: `stalled` means it had
        // data in the last 30 days. Older silence is a source nobody uses.
        if !streams.iter().any(|s| s.status == "stalled") {
            continue;
        }
        let Some(newest) = streams.iter().filter_map(|s| s.last_ingest).max() else {
            continue;
        };
        let days = (now - newest).num_days();
        if days < SOURCE_QUIET_DAYS {
            continue;
        }
        items.push(AttentionItem {
            key: format!("source-quiet:{source}"),
            kind: "stream",
            title: format!("{source} stopped sending data"),
            body: format!(
                "Your server hasn't received anything from it for {days} days. Open Sources to see why."
            ),
            route: "/sources".into(),
        });
    }
}

/// Days of arrivals read to judge a stream's rhythm: the 30 before it went
/// quiet, plus up to `QUIET_MAX_DAYS` of quiet.
const RHYTHM_WINDOW_DAYS: i64 = 75;
/// The days before a stream went quiet that set its rhythm.
const RHYTHM_DAYS: usize = 30;
/// A stream must have arrived on this many of those days to have a rhythm at
/// all; sparser ones (workouts, bookmarks) are never flagged on their own.
const RHYTHM_MIN_ARRIVALS: usize = 6;
/// Never flag a gap shorter than this, however steady the stream.
const QUIET_MIN_DAYS: i64 = 3;
/// Past this the silence is a stream nobody uses, as for a whole source.
const QUIET_MAX_DAYS: i64 = 30;

/// One stream gone quiet while its source still sends others.
///
/// `stream_items` judges silence per source, so a phone that keeps sending
/// steps hides the sleep and heart rate it stopped sending: a watch set aside
/// took both, and nothing said so for six weeks. A stream is judged against its
/// own rhythm instead of a fixed threshold, because streams arrive at different
/// paces (heart rate daily, sleep in batches every few days): it is quiet when
/// it has gone twice its longest gap of the month before, and at least
/// `QUIET_MIN_DAYS`. A stream whose whole source is quiet is left to that
/// source's item, so one stopped device is one item.
fn quiet_stream_items(
    items: &mut Vec<AttentionItem>,
    streams: &[crate::api::stream_health::StreamHealth],
    arrivals: &crate::api::stream_health::StreamDaysResponse,
) {
    let quiet_sources: std::collections::HashSet<String> = items
        .iter()
        .filter_map(|i| i.key.strip_prefix("source-quiet:").map(str::to_string))
        .collect();
    let today = arrivals.start + chrono::Duration::days(arrivals.days - 1);
    let width = arrivals.days as usize;

    for s in streams.iter().filter(|s| s.connected && !s.derived && s.status != "blocked") {
        let Some(source) = s.provided_by.first() else { continue };
        if quiet_sources.contains(source) {
            continue;
        }
        // Every provider's rows, one count per day.
        let mut days = vec![0i64; width];
        for row in arrivals.streams.iter().filter(|r| r.name == s.name) {
            for (d, n) in days.iter_mut().zip(&row.days) {
                *d += n;
            }
        }
        let Some(last) = days.iter().rposition(|&n| n > 0) else { continue };
        let quiet = (width - 1 - last) as i64;
        if !(QUIET_MIN_DAYS..=QUIET_MAX_DAYS).contains(&quiet) {
            continue;
        }
        let from = (last + 1).saturating_sub(RHYTHM_DAYS);
        let arrived: Vec<usize> = (from..=last).filter(|&i| days[i] > 0).collect();
        if arrived.len() < RHYTHM_MIN_ARRIVALS {
            continue;
        }
        let longest_gap = arrived.windows(2).map(|w| (w[1] - w[0]) as i64).max().unwrap_or(1);
        if quiet < (2 * longest_gap).max(QUIET_MIN_DAYS) {
            continue;
        }
        let since = (today - chrono::Duration::days(quiet)).format("%B %-d");
        items.push(AttentionItem {
            key: format!("stream-quiet:{}", s.name),
            kind: "stream",
            title: format!("{} stopped arriving", s.display_name),
            body: format!(
                "Nothing since {since}, though {source} still sends other data. \
                 If you set aside the device that records it, there's nothing to fix. \
                 Otherwise, open Sources to check."
            ),
            route: "/sources".into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrong_key_is_one_item_and_a_right_or_unchecked_one_says_nothing() {
        use crate::secrets::KeyCheck;
        let mut items = vec![];
        key_items(&mut items, None);
        key_items(&mut items, Some(KeyCheck { tried: 0, opened: 0 }));
        key_items(&mut items, Some(KeyCheck { tried: 5, opened: 4 }));
        assert!(items.is_empty(), "{items:?}");

        key_items(&mut items, Some(KeyCheck { tried: 5, opened: 0 }));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].key, "key:wrong");
        assert_eq!(items[0].kind, "key");
        // A restart alone leaves the database logins on the old key.
        assert!(items[0].body.contains("sudo virtues migrate"), "{}", items[0].body);
    }

    #[test]
    fn plenty_of_disk_says_nothing() {
        let mut items = vec![];
        disk_items(&mut items, Some(LOW_DISK_BYTES));
        disk_items(&mut items, None);
        assert!(items.is_empty());
    }

    #[test]
    fn low_disk_says_how_much_is_left() {
        let mut items = vec![];
        disk_items(&mut items, Some(3 * 1024 * 1024 * 1024));
        assert_eq!(items.len(), 1);
        assert!(items[0].body.starts_with("3.0 GB left"), "{}", items[0].body);
    }

    #[test]
    fn a_healthy_backup_says_nothing_and_each_problem_has_its_own_key() {
        let mut items = vec![];
        backup_items(&mut items, "ok", Some(3600));
        assert!(items.is_empty());
        for state in ["none", "never", "failing", "stale"] {
            backup_items(&mut items, state, Some(4 * 86_400));
        }
        let keys: Vec<_> = items.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, ["backup:none", "backup:never", "backup:failing", "backup:stale"]);
        assert_eq!(items[3].title, "Your last backup is 4 days old");
    }

    #[test]
    fn a_source_needing_sign_in_is_named() {
        let mut items = vec![];
        source_items(
            &mut items,
            vec![("cred_1".into(), "Google Calendar".into(), "reauth_required".into())],
        );
        assert_eq!(items[0].title, "Google Calendar needs you to sign in again");
        assert_eq!(items[0].key, "source:cred_1:reauth_required");
    }

    #[test]
    fn a_connection_needing_sign_in_is_named() {
        let mut items = vec![];
        connection_items(&mut items, vec![("linear".into(), "Linear".into())]);
        assert_eq!(items[0].title, "Linear needs you to sign in again");
        assert_eq!((items[0].key.as_str(), items[0].route.as_str()), ("connection:linear:reauth_required", "/sources/catalog"));
    }

    fn stream(name: &str, source: &str, status: &str, days_ago: i64) -> crate::api::stream_health::StreamHealth {
        crate::api::stream_health::StreamHealth {
            name: name.into(),
            display_name: name.into(),
            status: status.into(),
            total: 10,
            count_24h: 0,
            count_7d: 0,
            last_event: None,
            last_ingest: Some(chrono::Utc::now() - chrono::Duration::days(days_ago)),
            provided_by: vec![source.into()],
            connected: true,
            derived: false,
            blocked_reason: None,
            coverage: None,
        }
    }

    /// Arrivals for one stream over `RHYTHM_WINDOW_DAYS`, oldest first,
    /// arriving on the given days-ago and nothing else.
    fn arrivals(name: &str, days_ago: &[i64]) -> crate::api::stream_health::StreamDays {
        let width = RHYTHM_WINDOW_DAYS as usize;
        let mut days = vec![0; width];
        for &ago in days_ago {
            days[width - 1 - ago as usize] = 1;
        }
        crate::api::stream_health::StreamDays {
            name: name.into(),
            display_name: name.into(),
            provider: "ios".into(),
            days,
        }
    }

    fn window(rows: Vec<crate::api::stream_health::StreamDays>) -> crate::api::stream_health::StreamDaysResponse {
        crate::api::stream_health::StreamDaysResponse {
            start: chrono::NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
            days: RHYTHM_WINDOW_DAYS,
            streams: rows,
        }
    }

    #[test]
    fn a_steady_stream_that_stops_is_flagged_while_its_source_still_sends() {
        let mut items = vec![];
        // Heart rate arrived daily until 5 days ago; steps still arrive.
        let heart: Vec<i64> = (5..40).collect();
        let steps: Vec<i64> = (0..40).collect();
        stream_items(
            &mut items,
            &[stream("Heart rate", "iPhone", "idle", 5), stream("Steps", "iPhone", "live", 0)],
        );
        quiet_stream_items(
            &mut items,
            &[stream("Heart rate", "iPhone", "idle", 5), stream("Steps", "iPhone", "live", 0)],
            &window(vec![arrivals("Heart rate", &heart), arrivals("Steps", &steps)]),
        );
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].title, "Heart rate stopped arriving");
        assert_eq!(items[0].key, "stream-quiet:Heart rate");
    }

    #[test]
    fn a_stream_is_judged_against_its_own_rhythm() {
        // Sleep arrives in batches every few days, up to six apart.
        let sleep: Vec<i64> = vec![10, 14, 20, 23, 27, 30, 36, 40];
        let mut items = vec![];
        let s = [stream("Sleep", "iPhone", "idle", 10)];
        quiet_stream_items(&mut items, &s, &window(vec![arrivals("Sleep", &sleep)]));
        assert!(items.is_empty(), "10 days is under twice its 6-day gap: {items:?}");

        let later: Vec<i64> = sleep.iter().map(|d| d + 3).collect();
        let s = [stream("Sleep", "iPhone", "idle", 13)];
        quiet_stream_items(&mut items, &s, &window(vec![arrivals("Sleep", &later)]));
        assert_eq!(items.len(), 1, "13 days is past twice its 6-day gap");
    }

    #[test]
    fn a_sparse_stream_or_an_old_silence_says_nothing() {
        let mut items = vec![];
        // Workouts: four in a month, then a quiet week.
        let s = [stream("Workouts", "iPhone", "idle", 7)];
        quiet_stream_items(&mut items, &s, &window(vec![arrivals("Workouts", &[7, 15, 22, 30])]));
        // A daily stream quiet for six weeks is a stream nobody uses now.
        let old: Vec<i64> = (42..72).collect();
        let s = [stream("Heart rate", "iPhone", "idle", 42)];
        quiet_stream_items(&mut items, &s, &window(vec![arrivals("Heart rate", &old)]));
        assert!(items.is_empty(), "{items:?}");
    }

    #[test]
    fn one_quiet_stream_is_not_an_alarm_but_a_quiet_source_is() {
        let mut items = vec![];
        // Workouts are quiet; the phone's other stream is live.
        stream_items(
            &mut items,
            &[stream("workouts", "iPhone", "stalled", 5), stream("location", "iPhone", "live", 0)],
        );
        assert!(items.is_empty(), "{items:?}");

        // Every stream the Mac writes has been silent for three days.
        stream_items(
            &mut items,
            &[stream("messages", "Mac", "stalled", 3), stream("apps", "Mac", "stalled", 4)],
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Mac stopped sending data");
        assert!(items[0].body.contains("for 3 days"), "{}", items[0].body);
    }

    #[test]
    fn an_ending_sign_in_is_named_with_the_time_left() {
        let now = chrono::Utc::now();
        let mut items = Vec::new();
        expiring_items(
            &mut items,
            vec![
                ("cred_a".into(), "Grok".into(), now + chrono::Duration::hours(60)),
                ("cred_b".into(), "X".into(), now + chrono::Duration::hours(30)),
                ("cred_c".into(), "Instagram".into(), now + chrono::Duration::hours(5)),
                ("cred_d".into(), "Claude".into(), now - chrono::Duration::hours(1)),
            ],
            now,
        );
        let bodies: Vec<&str> = items.iter().map(|i| i.body.as_str()).collect();
        assert!(bodies[0].contains("ends in 3 days"), "{}", bodies[0]);
        assert!(bodies[1].contains("ends tomorrow"), "{}", bodies[1]);
        assert!(bodies[2].contains("ends within a day"), "{}", bodies[2]);
        assert!(bodies[3].contains("has ended"), "{}", bodies[3]);
        assert_eq!(items[0].title, "Grok needs you to sign in again soon");
        assert!(items[0].key.starts_with("source:cred_a:expiring:"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn only_sessions_ending_soon_are_listed(pool: sqlx::PgPool) {
        sqlx::query(
            "INSERT INTO credentials (id, key, name, status, secrets_ciphertext, expires_at, next_refresh_at) VALUES \
             ('cred_soon', 'grok', 'Grok', 'active', 'x', now() + interval '2 days', NULL), \
             ('cred_later', 'x', 'X', 'active', 'x', now() + interval '20 days', NULL), \
             ('cred_oauth', 'google', 'Google', 'active', 'x', now() + interval '1 hour', now()), \
             ('cred_broken', 'instagram', 'Instagram', 'reauth_required', 'x', now() + interval '1 day', NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let db = Database::from_pool(pool);
        let ids: Vec<String> = expiring_sources(&db).await.unwrap().into_iter().map(|r| r.0).collect();
        assert_eq!(ids, ["cred_soon"]);
    }

    #[test]
    fn an_applet_item_opens_its_applet_and_says_what_happened() {
        let mut items = vec![];
        failed_applet_items(&mut items, vec![("applet_user__tides".into(), "Tides".into(), "run_1".into(), Some("its SQL failed: relation does not exist\nmore".into()))]);
        new_applet_items(&mut items, vec![
            ("applet_morning_examen".into(), "Morning examen".into(), "run_2".into(), Some("Tuesday, 3 March".into()), None),
            ("applet_user__garden".into(), "Garden".into(), "run_3".into(), None, Some("The tomatoes are ready.\nAnd more.".into())),
        ]);
        assert_eq!(items[0].route, "/applet/applet_user__tides");
        assert_eq!(items[0].body, "its SQL failed: relation does not exist. Open it to see what happened.");
        assert_eq!(items[1].title, "Morning examen wrote Tuesday, 3 March");
        assert_eq!((items[2].title.as_str(), items[2].body.as_str()), ("Garden has something for you", "The tomatoes are ready."));
        assert_ne!(items[1].key, items[2].key);
    }

    /// A scheduled run with something to read is in the Inbox until the
    /// owner opens the applet; a failed sync is there until a run succeeds;
    /// and the rows a run wrote add up on the run.
    #[sqlx::test]
    async fn applet_results_and_failures_reach_the_inbox(pool: sqlx::PgPool) {
        let db = Database::from_pool(pool.clone());
        sqlx::query(
            "INSERT INTO app_applets (id, name, owner, agent, enabled) VALUES \
             ('applet_user__review', 'Week in review', 'ai', 'Write the review.', TRUE), \
             ('applet_sync', 'Calendar sync', 'system', NULL, TRUE), \
             ('applet_quiet', 'Quiet', 'system', 'Think.', TRUE)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let run = |id: &'static str, applet: &'static str, status: &'static str, trigger: &'static str, said: Option<&'static str>, secs: f64| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO app_applet_runs (id, applet_id, status, trigger, result_summary, error, started_at) \
                     VALUES ($1, $2, $3, $4, $5, CASE WHEN $3 = 'error' THEN 'token expired. sign in again' END, now() - make_interval(secs => $6))",
                )
                .bind(id).bind(applet).bind(status).bind(trigger).bind(said).bind(secs)
                .execute(&pool).await.unwrap();
            }
        };
        run("run_r1", "applet_user__review", "success", "cron", Some("Your week was mostly meetings."), 60.0).await;
        run("run_r2", "applet_user__review", "success", "manual", Some("Started by hand."), 30.0).await;
        run("run_q1", "applet_quiet", "success", "cron", Some("A shipped applet's own report."), 30.0).await;
        run("run_s1", "applet_sync", "error", "cron", None, 30.0).await;

        let new = new_from_applets(&db).await.unwrap();
        assert_eq!(new.len(), 1, "one item, for the unattended run: {new:?}");
        assert_eq!(new[0].2, "run_r1");
        let failed = failed_applets(&db).await.unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].0, "applet_sync");

        crate::scheduler::applets::mark_seen(&pool, "applet_user__review").await.unwrap();
        assert!(new_from_applets(&db).await.unwrap().is_empty(), "opening the applet clears it");
        run("run_s2", "applet_sync", "success", "cron", Some("Synced 4."), 10.0).await;
        assert!(failed_applets(&db).await.unwrap().is_empty(), "a later success clears it");

        crate::scheduler::applets::record_rows_output(&pool, "run_r1", "applet_water.drinks", 2).await.unwrap();
        crate::scheduler::applets::record_rows_output(&pool, "run_r1", "applet_water.drinks", 3).await.unwrap();
        let made = crate::scheduler::applets::made_by_runs(&pool, &["run_r1".to_string()]).await.unwrap();
        assert_eq!(made["run_r1"][0].count, 5);
        assert!(crate::scheduler::applets::run_made_rows(&pool, "run_r1").await.unwrap());
    }
}
