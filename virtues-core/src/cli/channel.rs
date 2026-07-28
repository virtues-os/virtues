//! Which release channel this box follows.
//!
//! One line in the state root. Not the database, because `virtues upgrade` has
//! to work when the database is unhealthy — which is half the reason anyone
//! upgrades. Not `/etc/virtues/virtues.env` either: the server runs as
//! `virtues` and can't write that without root, and `sudo virtues upgrade`
//! doesn't inherit the systemd EnvironmentFile anyway (see the hand-rolled
//! loader in `upgrade.rs`).
//!
//! Resolution order for a target tag is `--version` > `--pre` > stored channel
//! > stable, so `--pre` stays a one-off override and nothing that worked before
//! changes behaviour.

use std::fmt;
use std::fs;
use std::path::Path;

const CHANNEL_PATH: &str = "/var/lib/virtues/channel";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Channel {
    /// Stable `vX.Y.Z` releases. What a box should be on unless asked otherwise.
    #[default]
    Stable,
    /// Newest prerelease — the staging/edge line. Explicit opt-in.
    Prerelease,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Prerelease => "prerelease",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "stable" => Some(Channel::Stable),
            // `edge` and `nightly` are what people actually type for this.
            "prerelease" | "pre" | "edge" | "nightly" => Some(Channel::Prerelease),
            _ => None,
        }
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The stored channel, or Stable. An unreadable or unrecognised file reads as
/// Stable rather than failing: a box with a corrupt channel file should still
/// upgrade, and the conservative direction is the released one.
pub fn current() -> Channel {
    fs::read_to_string(CHANNEL_PATH)
        .ok()
        .and_then(|s| Channel::parse(&s))
        .unwrap_or_default()
}

/// Persist the channel. Requires write access to the state root.
pub fn set(channel: Channel) -> Result<(), crate::Error> {
    if let Some(dir) = Path::new(CHANNEL_PATH).parent() {
        fs::create_dir_all(dir).map_err(|e| {
            crate::Error::Other(format!("create {}: {e}", dir.display()))
        })?;
    }
    fs::write(CHANNEL_PATH, format!("{channel}\n"))
        .map_err(|e| crate::Error::Other(format!("write {CHANNEL_PATH}: {e}")))
}
