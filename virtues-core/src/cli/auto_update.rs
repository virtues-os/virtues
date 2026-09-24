//! `virtues auto-update` — the nightly pass: prepare, then activate.
//!
//! The server fires this once a night (see `api::updates::spawn`) in its own
//! transient unit, `virtues-auto-update`, because activating restarts the very
//! service that would otherwise be running it. It is also safe to run by hand:
//! it does exactly what the night would.
//!
//! Every box follows its channel automatically, `stable` and `prerelease`
//! alike, unless the owner turns it off in Settings. The halt mechanisms that
//! remain are the ones `upgrade` already has — preflight refuses a release the
//! schema can't take, activation dumps the database before migrating and
//! flips back on any failure — plus one this module adds: a release that
//! failed to activate is not retried. The box waits for a newer build, or for
//! a person, instead of restarting into the same failure every night.
//!
//! State is two files in the state root, beside `channel`, for the same reason
//! the channel lives there: this runs as root with no database.

use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::upgrade::{self, Prepared};

/// `off` turns the nightly pass off. Absent, or anything else, means on — the
/// default for every box, including ones that predate the setting.
const ENABLED_PATH: &str = "/var/lib/virtues/auto-update";

/// What the last nightly pass did. Written by this command (root), read by the
/// server for Settings.
const LAST_PATH: &str = "/var/lib/virtues/auto-update.json";

pub fn enabled() -> bool {
    fs::read_to_string(ENABLED_PATH)
        .map(|s| s.trim() != "off")
        .unwrap_or(true)
}

/// Persist the setting. The server calls this as `virtues`, which owns the
/// state root.
pub fn set_enabled(on: bool) -> Result<(), crate::Error> {
    if let Some(dir) = Path::new(ENABLED_PATH).parent() {
        fs::create_dir_all(dir)
            .map_err(|e| crate::Error::Other(format!("create {}: {e}", dir.display())))?;
    }
    fs::write(ENABLED_PATH, if on { "on\n" } else { "off\n" })
        .map_err(|e| crate::Error::Other(format!("write {ENABLED_PATH}: {e}")))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LastRun {
    /// When the pass last ran at all, whatever it concluded.
    pub checked_at: Option<DateTime<Utc>>,
    /// The last time it tried to install something. Kept across nights on
    /// which there was nothing to do, so Settings can still say when the box
    /// last moved.
    pub install: Option<Install>,
    /// Why the last pass stopped short, when it did: the download failed, or
    /// the staged release is one that already failed to install.
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Install {
    pub at: DateTime<Utc>,
    pub from: String,
    /// The release slot, `<tag>-<sha7>`.
    pub to: String,
    pub ok: bool,
    pub error: Option<String>,
}

/// The record, or an empty one. A missing or corrupt file means "never ran",
/// which is the truth on every box until its first night.
pub fn last() -> LastRun {
    fs::read(LAST_PATH)
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

fn save(run: &LastRun) {
    let Ok(body) = serde_json::to_vec_pretty(run) else { return };
    let tmp = format!("{LAST_PATH}.tmp");
    // Best-effort: a record that fails to write must never fail an upgrade
    // that succeeded. The journal still has the whole story.
    if fs::write(&tmp, body).is_ok() {
        let _ = fs::rename(&tmp, LAST_PATH);
    }
}

pub async fn run() -> Result<(), crate::Error> {
    if !enabled() {
        super::ui::ok("automatic updates are off - nothing to do");
        return Ok(());
    }

    let mut record = last();
    record.checked_at = Some(Utc::now());
    record.problem = None;

    let slot_id = match upgrade::prepare(false).await {
        Ok(Prepared::UpToDate) => {
            save(&record);
            super::ui::ok("already on the newest build for this channel");
            return Ok(());
        }
        Ok(Prepared::Already { slot_id }) | Ok(Prepared::Staged { slot_id }) => slot_id,
        Err(e) => {
            record.problem = Some(format!("couldn't download the update: {e}"));
            save(&record);
            return Err(e);
        }
    };

    if let Some(prev) = &record.install {
        if !prev.ok && prev.to == slot_id {
            let why = format!(
                "{slot_id} failed to install on a previous night - waiting for a newer \
                 build, or for `sudo virtues activate`"
            );
            super::ui::warn(&why);
            record.problem = Some(why);
            save(&record);
            return Ok(());
        }
    }

    let from = crate::codename::version().to_string();
    let result = upgrade::activate_prepared().await;
    record.install = Some(Install {
        at: Utc::now(),
        from,
        to: slot_id,
        ok: result.is_ok(),
        error: result.as_ref().err().map(|e| e.to_string()),
    });
    save(&record);
    result
}
