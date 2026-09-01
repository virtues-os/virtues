//! `GET /api/backup/status` — is anything backed up, and how stale is it.
//!
//! One number carries this surface: **the age of the newest successful backup.**
//! Not archive counts, not bytes written, not how many volumes are registered —
//! those describe the machinery. Age is the only figure that answers the question
//! a person is actually asking, which is "if the box died right now, how much
//! would I lose."
//!
//! A box with no destination reports that as its own state rather than as zero
//! volumes. Nothing configured is not a neutral reading; it means one copy of
//! everything exists.

use serde::Serialize;
use sqlx::{PgPool, Row};

#[derive(Debug, Serialize)]
pub struct BackupStatus {
    /// `none` · `never` · `ok` · `stale` · `failing`
    pub state: String,
    /// Seconds since the newest successful backup on any volume.
    pub age_seconds: Option<i64>,
    pub volumes: Vec<VolumeStatus>,
}

#[derive(Debug, Serialize)]
pub struct VolumeStatus {
    pub id: String,
    pub name: String,
    pub attached: bool,
    pub last_ok_at: Option<String>,
    pub age_seconds: Option<i64>,
    pub last_error: Option<String>,
}

/// Past this, a backup is stale enough to say so. The nightly applet runs at
/// 04:00, so two missed nights — not one — is the threshold, because a single
/// skipped run is the normal consequence of an unplugged drive and crying about
/// it teaches people to ignore the signal.
const STALE_AFTER_SECONDS: i64 = 60 * 60 * 49;

pub async fn get_backup_status(pool: &PgPool) -> Result<BackupStatus, crate::Error> {
    let rows = sqlx::query(
        "SELECT id, name, fs_uuid, last_ok_at, last_error, \
                EXTRACT(EPOCH FROM (NOW() - last_ok_at))::BIGINT AS age_secs \
         FROM storage_volume \
         ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("read backup status: {e}")))?;

    let mut volumes = Vec::with_capacity(rows.len());
    let mut newest: Option<i64> = None;
    let mut any_error = false;

    for row in &rows {
        let fs_uuid: String = row.get("fs_uuid");
        let age: Option<i64> = row.get("age_secs");
        let last_error: Option<String> = row.get("last_error");
        any_error |= last_error.is_some();
        if let Some(a) = age {
            newest = Some(newest.map_or(a, |n: i64| n.min(a)));
        }
        let last_ok: Option<chrono::DateTime<chrono::Utc>> = row.get("last_ok_at");
        volumes.push(VolumeStatus {
            id: row.get("id"),
            name: row.get("name"),
            // Resolved live rather than read from the cached mount_path: the
            // question is whether the drive is plugged in NOW.
            attached: crate::storage::volumes::resolve_mount(&fs_uuid).is_some(),
            last_ok_at: last_ok.map(|t| t.to_rfc3339()),
            age_seconds: age,
            last_error,
        });
    }

    Ok(BackupStatus {
        state: derive_state(volumes.len(), any_error, newest).to_string(),
        age_seconds: newest,
        volumes,
    })
}

/// Collapse the fleet of volumes into one word.
///
/// Precedence is the whole content of this function, and it is easy to get
/// backwards. A failure outranks staleness (a drive erroring is a live problem,
/// not an old one), and staleness outranks nothing-yet only because "never"
/// is the more specific reading of the same absence. `none` is not a neutral
/// state: it means one copy of everything exists.
fn derive_state(volume_count: usize, any_error: bool, newest_age: Option<i64>) -> &'static str {
    if volume_count == 0 {
        return "none";
    }
    if any_error {
        return "failing";
    }
    match newest_age {
        None => "never",
        Some(a) if a > STALE_AFTER_SECONDS => "stale",
        Some(_) => "ok",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3600;

    #[test]
    fn no_registered_volume_is_not_a_neutral_reading() {
        // The box has exactly one copy of everything. That is the loudest state
        // this surface has, not the quietest.
        assert_eq!(derive_state(0, false, None), "none");
        assert_eq!(derive_state(0, false, Some(HOUR)), "none");
    }

    #[test]
    fn a_live_failure_outranks_an_old_success() {
        // A drive erroring right now matters more than yesterday's good run,
        // however recent that run was.
        assert_eq!(derive_state(2, true, Some(HOUR)), "failing");
        assert_eq!(derive_state(1, true, None), "failing");
    }

    #[test]
    fn freshness_is_measured_against_two_missed_nights_not_one() {
        // The applet runs at 04:00. One skipped night is the ordinary
        // consequence of an unplugged drive; crying about it teaches people to
        // ignore the signal, so the threshold sits past the second.
        assert_eq!(derive_state(1, false, Some(HOUR)), "ok");
        assert_eq!(derive_state(1, false, Some(30 * HOUR)), "ok");
        assert_eq!(derive_state(1, false, Some(50 * HOUR)), "stale");
    }

    #[test]
    fn registered_but_never_run_is_distinct_from_stale() {
        // Different remedies: "stale" means look at the drive, "never" means
        // the schedule has not fired yet.
        assert_eq!(derive_state(1, false, None), "never");
    }
}
