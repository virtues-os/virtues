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
        Ok(streams) => stream_items(&mut items, &streams),
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
            "Your backup drive hasn't been used yet".to_string(),
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
                "Check that the backup drive is plugged in. Open System to see the last backup."
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

fn stream_items(items: &mut Vec<AttentionItem>, streams: &[crate::api::stream_health::StreamHealth]) {
    for s in streams {
        // A stream nothing is connected to is quiet by definition, not stuck.
        if !s.connected {
            continue;
        }
        let (title, body) = match s.status.as_str() {
            "stalled" => {
                let days = s
                    .last_ingest
                    .map(|t| (chrono::Utc::now() - t).num_days().max(1))
                    .unwrap_or(1);
                let span = if days == 1 { "a day".to_string() } else { format!("{days} days") };
                (
                    format!("{} stopped arriving", s.display_name),
                    format!("Your server hasn't received any for {span}. Open Sources to see why."),
                )
            }
            "blocked" => (
                format!("{} can't sync", s.display_name),
                "Your server's sync for it keeps failing. Open Sources to see why.".to_string(),
            ),
            _ => continue,
        };
        items.push(AttentionItem {
            key: format!("stream:{}:{}", s.name, s.status),
            kind: "stream",
            title,
            body,
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
}
