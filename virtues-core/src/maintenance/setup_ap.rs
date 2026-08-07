//! The setup access point.
//!
//! An appliance arrives with no network — that is the whole premise of
//! onboarding — and its only interface is a display the owner cannot type on
//! (the digitizer doesn't work through the cover glass). So the box raises its
//! own wifi network, the display shows a QR that joins it, and the owner's
//! phone does the typing.
//!
//! **The rule is: the AP is up while the box is unclaimed, and comes down once
//! a device pairs.** Not "up until the box has wifi" — that would tear the AP
//! down at the exact moment the phone is still on it, mid-flow, having just
//! handed over the home wifi credentials. Keeping it up until a device is
//! actually paired means provisioning and pairing can both finish in the one
//! visit, while the phone is still on the box's own network, where proximity
//! already proves ownership.
//!
//! **AP+STA concurrency does NOT work on this radio — measured, not assumed.**
//! `iw list` advertises `#{managed} <= 1, #{AP} <= 1, total <= 4`, which reads
//! like the box could host the AP and join the owner's wifi at once. It cannot.
//! Tested on the Q6A 2026-08-07: a second virtual interface is created fine and
//! adopted by NetworkManager, and then the join fails with *"object is in an
//! unsuitable state"*. Do not re-derive optimism from the capability table.
//!
//! What the radio *can* do while hosting the AP is **scan** — 21 SSIDs, 133
//! BSSs — which is what makes `/api/provision/networks` viable at all.
//!
//! So the switchover is sequential: drop the AP, join, re-raise it if the join
//! failed. `api::provision` holds [`PROVISIONING_LOCK`] across that window so
//! this reconciler does not put the AP back on top of the association being
//! formed. Sequential is cheap in practice — the re-join succeeded on the first
//! attempt, within seconds, in every measured run.
//!
//! Three traps if anyone revisits the virtual-interface route: hand-created
//! interfaces get renamed by udev's predictable-naming (`ap0` becomes
//! `wlx<mac>`, so `nmcli ... ifname ap0` fails with "not a Wi-Fi device"), get
//! flipped to `managed` by wpa_supplicant on sight, and silently consume the
//! 4-interface budget until creation fails with a bare `-22`. All three cost an
//! afternoon.
//!
//! One tokio task spawned by `server::run`, mirroring `maintenance::sweeper`
//! and `maintenance::pair_rotator`.

use std::process::Stdio;
use std::time::Duration;

use sqlx::PgPool;
use tokio::process::Command;
use tokio::time::{interval, MissedTickBehavior};

/// How often to reconcile the AP against the box's claimed state.
const RECONCILE_SECS: u64 = 20;

/// Set by `api::provision` for the duration of a join attempt, so the
/// reconciler does not re-raise the AP on top of it.
///
/// Measured on the Q6A 2026-08-07: the radio **can** scan while hosting the AP
/// (21 SSIDs, 133 BSSs), but it **cannot** hold an AP and a client association
/// at once — a second virtual interface is created and adopted by NM, then the
/// join fails with "object is in an unsuitable state". So the switchover is
/// necessarily sequential: drop the AP, join, and re-raise it if the join
/// failed. Without this guard the reconciler would notice "unclaimed and no AP"
/// mid-join and raise the AP straight back onto the radio the join needs.
///
/// Lives in `/run` so it can never survive a reboot, and carries a deadline so
/// a process that dies mid-join cannot suppress the AP forever.
pub const PROVISIONING_LOCK: &str = "/run/virtues-provisioning";

/// How long a provisioning lock is honoured before it is treated as abandoned.
/// Generous relative to a join (seconds) and short relative to a person's
/// patience with a box that never shows its network again.
const LOCK_TTL_SECS: u64 = 120;

/// Is a join in flight right now?
pub fn provisioning_in_flight() -> bool {
    let Ok(meta) = std::fs::metadata(PROVISIONING_LOCK) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    match modified.elapsed() {
        Ok(age) => age.as_secs() < LOCK_TTL_SECS,
        // A clock that moved backwards makes the lock look like it is from the
        // future. Honour it — the failure mode of waiting is a delayed AP; the
        // failure mode of ignoring it is stomping a live join.
        Err(_) => true,
    }
}

/// NetworkManager connection NAME for the setup AP.
///
/// Distinct from the SSID it broadcasts (`Virtues-XXXX`, see [`ap_ssid`]), and
/// conflating the two is easy: `api::display` originally scanned the connection
/// list for a `Virtues-` prefix, which never matches this, so the display
/// reported "no setup network" while the AP was up and broadcasting.
pub const AP_CON_NAME: &str = "virtues-setup-ap";

/// Only appliances raise an AP. A DIY box is someone's general-purpose Linux
/// server, reached over a network they already run; hijacking its radio to
/// broadcast an open-ended setup network would be a rude surprise and a real
/// security change on a machine we are a guest on.
fn is_appliance() -> bool {
    std::path::Path::new("/etc/systemd/system/virtues-display.service").exists()
}

pub fn spawn(pool: PgPool) {
    if !is_appliance() {
        tracing::debug!("setup_ap: not an appliance, not managing a setup AP");
        return;
    }
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(RECONCILE_SECS));
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            if let Err(e) = reconcile(&pool).await {
                // Never fatal. A box that cannot raise its AP must still serve
                // the display, which is where the owner would learn about it.
                tracing::warn!("setup_ap: reconcile failed: {e:#}");
            }
        }
    });
}

async fn reconcile(pool: &PgPool) -> Result<(), crate::Error> {
    // A join is in flight. It needed the radio, so the AP is down on purpose —
    // do not helpfully put it back underneath the association being formed.
    if provisioning_in_flight() {
        tracing::debug!("setup_ap: join in flight, leaving the radio alone");
        return Ok(());
    }

    // Excludes the always-present `local-console` row; see
    // `api::pair::paired_device_count`. Counting it made a fresh box look
    // claimed from first boot, so the AP never rose at all.
    let claimed = crate::api::pair::paired_device_count(pool).await;
    let up = ap_is_up().await;

    match (claimed > 0, up) {
        // Claimed and the AP is still up: setup is over, take it down. This is
        // the only thing that ends the AP's life — see the module docs on why
        // it is not "the box got wifi".
        (true, true) => {
            tracing::info!("setup_ap: box is claimed, dropping the setup AP");
            let _ = nmcli(&["connection", "down", AP_CON_NAME]).await;
            let _ = nmcli(&["connection", "delete", AP_CON_NAME]).await;
        }
        // Unclaimed with no AP: raise it.
        (false, false) => {
            let ssid = ap_ssid();
            tracing::info!("setup_ap: box is unclaimed, raising {ssid}");
            raise(&ssid).await?;
        }
        _ => {}
    }
    Ok(())
}

/// Bring the AP up on the wifi device.
///
/// WPA2, never open. The owner's home wifi password crosses this link during
/// provisioning; on an open AP that is cleartext to anyone in range. It costs
/// them no typing, because the passphrase rides in the QR the display shows.
async fn raise(ssid: &str) -> Result<(), crate::Error> {
    let psk = ap_passphrase()?;
    let Some(dev) = wifi_device().await else {
        return Err(crate::Error::Other("no wifi device to host the AP".into()));
    };
    let out = nmcli(&[
        "device", "wifi", "hotspot", "ifname", &dev, "con-name", AP_CON_NAME, "ssid", ssid,
        "password", &psk,
    ])
    .await;
    match out {
        Some(o) if o.status.success() => Ok(()),
        Some(o) => Err(crate::Error::Other(format!(
            "nmcli hotspot failed: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ))),
        None => Err(crate::Error::Other("nmcli not available".into())),
    }
}

async fn ap_is_up() -> bool {
    let Some(out) = nmcli(&["-t", "-f", "NAME", "connection", "show", "--active"]).await else {
        return false;
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.trim() == AP_CON_NAME)
}

/// First managed wifi device. Skips the `p2p-dev-*` shadow devices NM reports
/// alongside every real radio, which are not hotspot-capable.
async fn wifi_device() -> Option<String> {
    let out = nmcli(&["-t", "-f", "DEVICE,TYPE", "device"]).await?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_once(':'))
        .find(|(dev, kind)| *kind == "wifi" && !dev.starts_with("p2p-"))
        .map(|(dev, _)| dev.to_string())
}

/// Per-box SSID: stable across reboots, distinct between two boxes in one
/// house, and meaningless to anyone who has not been told which box is theirs.
/// Derived from machine-id rather than stored, so it survives a reset and needs
/// no migration — and because machine-id is itself re-minted per unit at first
/// boot, two clones of one image never collide.
pub fn ap_ssid() -> String {
    let id = std::fs::read_to_string("/etc/machine-id").unwrap_or_default();
    let suffix: String = id.trim().chars().take(4).collect::<String>().to_uppercase();
    if suffix.len() == 4 {
        format!("Virtues-{suffix}")
    } else {
        "Virtues-Setup".to_string()
    }
}

/// The AP passphrase.
///
/// Read from the state root so the installer (and an operator) can set it, with
/// a machine-derived fallback so a box always has one. It is not a long-term
/// secret: it guards a network that exists only while the box is unclaimed, and
/// it is displayed as a QR to anyone standing in front of the screen. Its job is
/// to encrypt the link the home wifi password crosses, not to keep neighbours
/// out of a network that carries nothing else.
fn ap_passphrase() -> Result<String, crate::Error> {
    if let Ok(s) = std::fs::read_to_string("/var/lib/virtues/ap-passphrase") {
        let s = s.trim().to_string();
        if s.len() >= 8 {
            return Ok(s);
        }
    }
    let id = std::fs::read_to_string("/etc/machine-id").unwrap_or_default();
    let derived: String = id.trim().chars().take(12).collect();
    if derived.len() >= 8 {
        Ok(derived)
    } else {
        Err(crate::Error::Other(
            "no AP passphrase and machine-id is too short to derive one".into(),
        ))
    }
}

async fn nmcli(args: &[&str]) -> Option<std::process::Output> {
    Command::new("nmcli")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssid_is_prefixed_and_bounded() {
        let s = ap_ssid();
        assert!(s.starts_with("Virtues-"), "got {s}");
        // Must stay short enough to read off a screen and type into a phone.
        assert!(s.len() <= 16, "got {s}");
    }
}
