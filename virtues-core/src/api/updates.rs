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

// Applying an update from the UI needs root: `virtues upgrade` writes
// /usr/local/bin and drives systemctl, while the server runs as `virtues`. The
// agreed route is a narrow sudoers grant for exactly that one binary and
// subcommand, installed by the installer and removed on uninstall. That grant
// is standing root-adjacent surface on an appliance, so it ships as its own
// change with the installer work rather than riding along here — and until it
// does, the UI shows the command instead of running it.
