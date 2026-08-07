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
//! **Concurrency is real on this radio but deliberately unused.** The Q6A
//! reports `#{managed} <= 1, #{AP} <= 1, total <= 4`, so it can run the AP and
//! a client connection at once — which is what lets the box join the owner's
//! wifi without dropping the network their phone is sitting on. We rely on
//! NetworkManager to place the AP on its own virtual interface rather than
//! managing one ourselves: hand-created interfaces get renamed out from under
//! you by udev's predictable-naming (`ap0` becomes `wlx<mac>`), get flipped to
//! `managed` by wpa_supplicant on sight, and silently consume the 4-interface
//! budget until creation fails with a bare `-22`. All three cost an afternoon;
//! none of them are worth re-discovering.
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

/// NetworkManager connection name for the setup AP. Also how we recognise it
/// later — `api::display` looks for a `Virtues-` prefixed wireless connection.
const AP_CON_NAME: &str = "virtues-setup-ap";

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
    let claimed: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app_device WHERE revoked_at IS NULL")
            .fetch_one(pool)
            .await
            .unwrap_or(0);
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
