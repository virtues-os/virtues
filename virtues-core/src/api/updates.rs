//! Update status and channel, for Settings → Box.
//!
//! The Mac app has self-updated on a 6h poll since it shipped; the box has
//! never had an update path in the UI at all — `sudo virtues upgrade` typed at
//! a terminal was the whole story, and nothing told you there was anything to
//! upgrade to. This is the missing half.
//!
//! ## On checking in the background
//!
//! This module used to argue that checking must be on demand, because a
//! background poll would mean the appliance calling GitHub on its own
//! initiative. That reasoning doesn't survive contact with the numbers. The box
//! already calls GitHub the moment anyone opens this screen, and a check is two
//! small requests that reveal nothing but that a box exists. The real cost was
//! never the check — it was conflating the check with the ~120MB download that
//! used to follow it inside a single function.
//!
//! Now they are separate operations, so they get separate policies:
//!
//! · **Checking** is cheap and happens on a timer, on both channels.
//! · **Downloading** happens on a timer only on `stable`, where a release lands
//!   every few weeks. On `prerelease` a build lands most days and gets
//!   installed rarely, so a box on that channel checks and waits to be asked.
//! · **Activating** — the part that restarts the box and runs migrations — is
//!   never automatic. See below.
//!
//! ## On applying in the background
//!
//! Nothing here ever activates a release on its own, and that is a considered
//! position rather than an unfinished one. A vendor who auto-ships to a fleet
//! can do it safely because they watch that fleet and halt a bad rollout;
//! virtues deliberately has no such telemetry, so it has no way to notice a bad
//! release and no way to stop one. The human pressing the button IS the halt
//! mechanism — every box that hasn't pressed it is a box a bad build never
//! reached. Preparation is what makes pressing it cheap.

use serde::{Deserialize, Serialize};

use crate::cli::channel::{self, Channel};
use crate::error::{Error, Result};

#[derive(Debug, Serialize)]
pub struct UpdateStatus {
    /// This binary's version. The BUILD COUNTER (`CARGO_PKG_VERSION`), which is
    /// not what the box is running — see `running_version`. Kept because the
    /// number is still the migration/compat coordinate everything else speaks.
    pub current: String,
    /// The stored update PREFERENCE: `stable` | `prerelease`. What the box will
    /// be offered next, not what it is on now.
    pub channel: String,
    /// RELEASE IDENTITY — what this box is actually running, from the baked
    /// build tag (`git describe`). `edge`, `staging.4`, `v0.3.0`, `dev`.
    ///
    /// Separate from `current` on purpose, and the distinction this endpoint
    /// used to lose: every prerelease build reports the same bare crate version,
    /// so a screen showing `current` told an owner on `edge` they were running
    /// "0.3.0" — the number of a release their build is AHEAD of.
    pub running_version: String,
    /// The channel of the RUNNING BUILD, derived from that same tag:
    /// `stable` | `staging` | `edge` | `dev`. Compare with `channel` to see a
    /// box whose build came from a different track than its preference — the
    /// state that made this screen offer a downgrade as an upgrade.
    pub running_channel: String,
    /// The running build came off a different, later track than the channel
    /// being followed — so the newest tag on that channel is probably BEHIND
    /// this box, and offering it would be a downgrade.
    pub running_ahead: bool,
    /// Newest tag on the followed channel, or `None` if the lookup failed.
    pub latest: Option<String>,
    /// Whether `latest` is something other than what's running.
    pub update_available: bool,
    /// A release already downloaded, verified, and preflighted — activating it
    /// is a flip, migrations, and a restart, with no transfer left to do.
    /// `None` means an update (if any) still has to be fetched first.
    pub staged: Option<crate::cli::upgrade::StagedRelease>,
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
    let staged = crate::cli::upgrade::staged_release();

    let latest = match channel {
        Channel::Stable => crate::cli::upgrade::fetch_latest_tag().await,
        Channel::Prerelease => crate::cli::upgrade::fetch_latest_prerelease().await,
    };

    match latest {
        Ok(tag) => {
            let target = tag.trim_start_matches('v');

            // Ask what commit the tag points at. This is the only comparison
            // that works on BOTH channels: stable tags carry their identity in
            // the semver, but every prerelease build reports the bare crate
            // version, so version equality there compares two identical strings
            // and always says "yes, newer". That was this endpoint's answer on
            // `prerelease` until now — permanently, regardless of the truth.
            //
            // Two small requests, and only when this screen is open or the
            // scheduled check runs. The tarball that also carries this answer is
            // ~120MB, which is the entire argument for asking this way.
            let commit = crate::cli::upgrade::fetch_tag_sha(&tag).await.ok();
            let by_commit = match (&commit, crate::cli::upgrade::running_commit()) {
                (Some(sha), Some(_)) => Some(!crate::cli::upgrade::is_running_commit(sha)),
                // Either the lookup failed or this binary can't say what commit
                // it is (a dev build). Fall through rather than guess.
                _ => None,
            };

            // OFF-CHANNEL BUILD. A box running `edge`/`staging` while its
            // preference says stable is normally AHEAD of the newest stable
            // tag, so "v0.3.0 is available" offered a downgrade as an upgrade —
            // seen live on a box running an edge build newer than the tag it
            // was being offered (2026-08-13).
            //
            // We cannot prove ancestry without asking about every commit
            // between, so this does not claim the box is newer; it declines to
            // claim it is OLDER. The UI says which build is running and what
            // the channel holds, and lets the owner decide — the same
            // human-is-the-halt-mechanism argument the rest of this module
            // rests on.
            let running_ahead = channel == Channel::Stable
                && matches!(crate::codename::channel(), "edge" | "staging" | "dev");

            // A staged release IS an available update, whatever the comparison
            // concluded — we already downloaded and preflighted it, which is a
            // far stronger statement than any version string.
            let update_available = staged.is_some()
                || (!running_ahead
                    && by_commit.unwrap_or(match channel {
                        Channel::Stable => target != current,
                        Channel::Prerelease => true,
                    }));

            UpdateStatus {
                current,
                channel: channel.as_str().to_string(),
                running_version: crate::codename::version().to_string(),
                running_channel: crate::codename::channel().to_string(),
                running_ahead,
                latest: Some(tag),
                update_available,
                staged,
                check_error: None,
            }
        }
        Err(e) => UpdateStatus {
            current,
            channel: channel.as_str().to_string(),
            running_version: crate::codename::version().to_string(),
            running_channel: crate::codename::channel().to_string(),
            running_ahead: false,
            latest: None,
            // A release we already hold is still installable with the network
            // down — that is rather the point of having fetched it early.
            update_available: staged.is_some(),
            staged,
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

/// Transient unit a background prepare runs under. Distinct from
/// [`UPGRADE_UNIT`] so the two can't be confused in `journalctl`, and so a
/// running prepare doesn't make an operator's apply look like it collided with
/// itself. They still serialise against each other on the upgrade lock.
const PREPARE_UNIT: &str = "virtues-prepare";

const BINARY_PATH: &str = "/usr/local/bin/virtues";

#[derive(Debug, Serialize)]
pub struct ApplyResponse {
    /// Transient unit the upgrade is running under, for `journalctl -u`.
    pub unit: String,
    /// Whether this is activating an already-staged release (seconds) or
    /// fetching one first (minutes). The client's waiting copy depends on it —
    /// telling someone to expect a couple of minutes for something that takes
    /// fifteen seconds trains them to distrust the next estimate.
    pub staged: bool,
    /// What the client should expect next, in plain words.
    pub detail: String,
}

/// How often the box looks for a release worth fetching.
///
/// Matches the Mac app's existing cadence, which has run since it shipped. On
/// the overwhelmingly common pass there is nothing to do and this costs two
/// small API calls; a release only lands every few weeks.
pub const PREPARE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(6 * 3600);

/// How long after boot the first check waits.
///
/// A box coming up has migrations to finish, sidecars to load, and collectors
/// reconnecting. Fetching ~120MB into the middle of that would make an update
/// the reason a restart felt slow — which is precisely the impression this
/// whole mechanism exists to avoid.
pub const PREPARE_FIRST_DELAY: std::time::Duration = std::time::Duration::from_secs(300);

/// Kick off a background prepare, if this box is one that should do that.
///
/// Returns `Ok(false)` when the box deliberately skips — not a real install, or
/// on a channel that doesn't auto-fetch — so the caller can stay quiet about
/// the normal case instead of logging a failure that isn't one.
///
/// ## Why `systemd-run` and not a direct call
///
/// Two reasons, either sufficient. `prepare` writes into
/// `/usr/local/share/virtues` and needs root; this server runs as `virtues`.
/// And a ~120MB transfer has no business living inside the request-serving
/// process — in its own transient unit it can be inspected with `journalctl -u
/// virtues-prepare`, killed on its own, and cannot take the server down with it.
///
/// Unlike [`apply`], nothing here restarts the box, so there is no cgroup
/// hazard to design around; this is about privilege and isolation only.
///
/// ## Why stable only
///
/// A stable release lands every few weeks and is nearly always wanted, so
/// fetching it early is a good trade. A prerelease lands most days and is
/// installed rarely — auto-fetching that channel would mean ~120MB a day, on
/// the boxes (Q6A, Jetson) least able to spare the bandwidth or the disk, for a
/// release that usually gets skipped. Boxes on `prerelease` still get an honest
/// answer from [`status`]; they just wait to be asked before downloading.
pub fn spawn_prepare() -> Result<bool> {
    if !std::path::Path::new(BINARY_PATH).exists() {
        return Ok(false);
    }
    if channel::current() != Channel::Stable {
        return Ok(false);
    }

    let output = std::process::Command::new("sudo")
        .args([
            "-n",
            "systemd-run",
            "--unit",
            PREPARE_UNIT,
            "--collect",
            "--description",
            "virtues prepare (scheduled)",
            BINARY_PATH,
            "prepare",
        ])
        .output()
        .map_err(|e| Error::Other(format!("could not invoke sudo systemd-run: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        return Err(Error::Other(if detail.is_empty() {
            format!("systemd-run exited {}", output.status)
        } else {
            detail.to_string()
        }));
    }
    Ok(true)
}

/// Run [`spawn_prepare`] on a timer for the life of the server.
///
/// Deliberately fire-and-forget. Nothing downstream waits on the result,
/// because nothing should: a box that couldn't reach GitHub, or is on a channel
/// that doesn't auto-fetch, or isn't a real install at all, is a box that
/// carries on exactly as before. The only visible consequence of this loop
/// working is that an update someone chooses to install is already on disk.
pub fn spawn() {
    tokio::spawn(async move {
        tokio::time::sleep(PREPARE_FIRST_DELAY).await;
        loop {
            // `systemd-run` returns as soon as the unit is queued, but it is
            // still a fork+exec, and a request-serving runtime thread is not
            // the place to do one.
            match tokio::task::spawn_blocking(spawn_prepare).await {
                Ok(Ok(true)) => tracing::info!("update: scheduled prepare started"),
                // Skipped on purpose — a dev checkout, or the prerelease
                // channel. Not worth a line every six hours.
                Ok(Ok(false)) => {}
                // Includes the ordinary case of a prepare already running, which
                // collides on the unit name. Warn rather than error: the next
                // pass will do it, and nothing is broken.
                Ok(Err(e)) => tracing::warn!("update: scheduled prepare did not start: {e}"),
                Err(e) => tracing::warn!("update: prepare task panicked: {e}"),
            }
            tokio::time::sleep(PREPARE_INTERVAL).await;
        }
    });
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

    // Use what's already on disk. A prepared release has been downloaded,
    // checksummed, and preflighted, so `upgrade` would spend minutes fetching a
    // byte-identical copy of a slot it is standing next to.
    //
    // Falls back to the full path when nothing is staged, which keeps this
    // endpoint working exactly as before on a box that has never prepared —
    // including one that just switched channels and hasn't checked yet.
    let staged = crate::cli::upgrade::staged_release().is_some();
    let subcommand = if staged { "activate" } else { "upgrade" };

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
            subcommand,
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
        staged,
        detail: if staged {
            "The release is already downloaded, so this is a restart and a \
             migration — well under a minute. Every connected device drops, \
             not just this one."
                .to_string()
        } else {
            "The box will stop serving for a moment while it restarts. \
             Every connected device drops, not just this one."
                .to_string()
        },
    })
}
