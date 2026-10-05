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
    /// `update` | `disk` | `backup` | `source` | `stream`, most urgent first.
    pub kind: &'static str,
    pub title: String,
    pub body: String,
    /// The in-app page that shows or fixes it.
    pub route: &'static str,
}

pub async fn attention(db: &Database) -> Result<Vec<AttentionItem>> {
    let mut items = Vec::new();

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

    Ok(items)
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
            route: "/virtues/devices/server",
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
        route: "/virtues/system",
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
        route: "/virtues/system",
    });
}

/// `(credential id, name, status)` for every connection that needs the owner.
async fn broken_sources(db: &Database) -> Result<Vec<(String, String, String)>> {
    Ok(sqlx::query_as(
        "SELECT id, name, status FROM credentials \
         WHERE status IN ('reauth_required', 'error') \
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
            route: "/sources",
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
            route: "/sources",
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
            route: "/sources",
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
            route: "/sources",
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
