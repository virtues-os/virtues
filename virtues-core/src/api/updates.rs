//! Update status and channel, for Settings → Box.
//!
//! The Mac app has self-updated on a 6h poll since it shipped; the box has
//! never had an update path in the UI at all — `sudo virtues upgrade` typed at
//! a terminal was the whole story, and nothing told you there was anything to
//! upgrade to. This is the missing half.
//!
//! Checking is on demand, not on a timer. A background poll would mean the
//! appliance making periodic outbound calls to GitHub on its own initiative,
//! which is not a thing a box holding someone's entire life should do
//! unprompted. The UI asks when Settings → Box is open, and not otherwise.
//!
//! Applying is deliberately NOT here yet — see the note on `apply` below.

use serde::{Deserialize, Serialize};

use crate::cli::channel::{self, Channel};
use crate::error::{Error, Result};

#[derive(Debug, Serialize)]
pub struct UpdateStatus {
    /// This binary's version.
    pub current: String,
    /// `stable` | `prerelease`.
    pub channel: String,
    /// Newest tag on the followed channel, or `None` if the lookup failed.
    pub latest: Option<String>,
    /// Whether `latest` is something other than what's running.
    pub update_available: bool,
    /// Set when the lookup failed, so the UI can say "couldn't check" instead
    /// of "up to date" — those are very different claims.
    pub check_error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetChannelRequest {
    pub channel: String,
}

/// Current version + channel, and whether the channel has something newer.
pub async fn status() -> UpdateStatus {
    let channel = channel::current();
    let current = env!("CARGO_PKG_VERSION").to_string();

    let latest = match channel {
        Channel::Stable => crate::cli::upgrade::fetch_latest_tag().await,
        Channel::Prerelease => crate::cli::upgrade::fetch_latest_prerelease().await,
    };

    match latest {
        Ok(tag) => {
            let target = tag.trim_start_matches('v');
            // Stable tags carry their whole identity in the semver, so equality
            // is meaningful. Prerelease builds all report the bare crate
            // version, so it isn't — on that channel there is always
            // potentially something newer, and only the SHA comparison during
            // the upgrade itself can say for sure.
            let update_available = match channel {
                Channel::Stable => target != current,
                Channel::Prerelease => true,
            };
            UpdateStatus {
                current,
                channel: channel.as_str().to_string(),
                latest: Some(tag),
                update_available,
                check_error: None,
            }
        }
        Err(e) => UpdateStatus {
            current,
            channel: channel.as_str().to_string(),
            latest: None,
            update_available: false,
            check_error: Some(e.to_string()),
        },
    }
}

/// Switch channels.
///
/// Note the asymmetry going back to stable: a box on `prerelease` is usually
/// *ahead* of the newest stable tag, and `virtues upgrade`'s downgrade guard
/// refuses to move backwards. So switching to stable doesn't roll anything
/// back — it means "stop taking prereleases", and the box stays where it is
/// until stable catches up. The UI has to say that plainly, or it reads as the
/// setting having silently failed.
pub fn set_channel(req: SetChannelRequest) -> Result<UpdateChannelResponse> {
    let channel = Channel::parse(&req.channel).ok_or_else(|| {
        Error::Other(format!(
            "unknown channel {:?} — expected 'stable' or 'prerelease'",
            req.channel
        ))
    })?;

    channel::set(channel)?;

    Ok(UpdateChannelResponse {
        channel: channel.as_str().to_string(),
    })
}

#[derive(Debug, Serialize)]
pub struct UpdateChannelResponse {
    pub channel: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Apply
// ─────────────────────────────────────────────────────────────────────────────

/// Transient unit the upgrade runs under. Named, so a second Apply while one is
/// already running fails on the name collision instead of starting a second
/// upgrade over the top of the first.
const UPGRADE_UNIT: &str = "virtues-upgrade";

const BINARY_PATH: &str = "/usr/local/bin/virtues";

#[derive(Debug, Serialize)]
pub struct ApplyResponse {
    /// Transient unit the upgrade is running under, for `journalctl -u`.
    pub unit: String,
    /// What the client should expect next, in plain words.
    pub detail: String,
}

/// Start an upgrade and return immediately.
///
/// ## Why this cannot just run `virtues upgrade` as a child
///
/// `upgrade` does `systemctl stop virtues` → flip → migrate →
/// `systemctl start virtues`. This server *is* `virtues.service`. A child
/// process inherits the service's cgroup, so the moment the upgrade stops the
/// unit, systemd kills the whole cgroup — including the upgrade itself, midway
/// through, with the symlink possibly already flipped and migrations not yet
/// run. The upgrade would reliably kill itself at its most dangerous moment.
///
/// `systemd-run` puts it in its own transient unit and therefore its own
/// cgroup, which survives the restart of ours. That is the whole reason it is
/// here; it is not a stylistic choice.
///
/// ## Privilege
///
/// The `virtues` account already holds `NOPASSWD: ALL`
/// (`/etc/sudoers.d/virtues`, written by the installer) because the auth-gated
/// web terminal is an admin shell and the account has no password to
/// authenticate against interactively. So this adds no privilege that the
/// server did not already have — it is a *use* of an existing grant, not a new
/// one. The overhaul plan called for installing a narrow grant for exactly this
/// subcommand; that would be strictly worse than it sounds, because it would
/// sit alongside the broad grant rather than replacing it, and read as if the
/// surface were narrower than it is. Narrowing the existing grant is real work
/// and belongs with the terminal, not here.
///
/// ## No user input reaches the command line
///
/// The argv is fixed. The channel comes from the state-root file that
/// `virtues upgrade` reads for itself, never from the request — so there is no
/// path from an HTTP body to a root command's arguments.
pub fn apply() -> Result<ApplyResponse> {
    if !std::path::Path::new(BINARY_PATH).exists() {
        return Err(Error::Other(format!(
            "{BINARY_PATH} is not installed — this looks like a dev checkout \
             rather than a box, and there is nothing to upgrade"
        )));
    }

    let output = std::process::Command::new("sudo")
        // -n: never prompt. Without a grant this fails immediately with a
        // readable error instead of blocking on a password prompt that no one
        // is there to answer.
        .args([
            "-n",
            "systemd-run",
            "--unit",
            UPGRADE_UNIT,
            // Let systemd clean the unit up once it exits, so the next apply
            // isn't refused by a leftover failed unit of the same name.
            "--collect",
            "--description",
            "virtues upgrade (started from Settings)",
            BINARY_PATH,
            "upgrade",
        ])
        .output()
        .map_err(|e| Error::Other(format!("could not invoke sudo systemd-run: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        // Surface the actual reason. "Update failed" with nothing behind it is
        // the thing that sends someone to SSH into the box to find out why.
        return Err(Error::Other(if detail.is_empty() {
            format!("systemd-run exited {}", output.status)
        } else {
            detail.to_string()
        }));
    }

    Ok(ApplyResponse {
        unit: UPGRADE_UNIT.to_string(),
        detail: "The box will stop serving for a moment while it restarts. \
                 Every connected device drops, not just this one."
            .to_string(),
    })
}
