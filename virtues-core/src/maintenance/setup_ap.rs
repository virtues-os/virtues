//! The setup access point.
//!
//! An appliance arrives with no network — that is the whole premise of
//! onboarding — and its only interface is a display the owner cannot type on
//! (the digitizer doesn't work through the cover glass). So the box raises its
//! own wifi network, the display shows a QR that joins it, and the owner's
//! phone does the typing.
//!
//! **The rule is: the AP is up while the box is unclaimed AND has no network of
//! its own.** It comes down when a device pairs, or when the box gets a network
//! — whichever happens first — and it is *raised* again if the box loses that
//! network before anyone pairs.
//!
//! This rule replaced "up until a device pairs, full stop" on 2026-08-10, which
//! could not work on this radio and quietly broke the flow it was written to
//! protect. Pairing happens *after* provisioning (`/api/provision/*` 404s the
//! moment a device pairs, so it cannot happen the other way round), which means
//! the instant after a successful join the box is online, unclaimed, and its AP
//! is down. The old rule saw "unclaimed, no AP" and raised one — onto the single
//! radio now holding the association it had just formed. The box fell off the
//! owner's wifi ~20s after joining it, every time, and the owner never got to
//! the pairing step.
//!
//! The original worry behind that rule was tearing the AP down while the phone
//! is still sitting on it mid-provision. Two things cover it. [`PROVISIONING_LOCK`]
//! holds this reconciler off for the whole join window; and on hardware that
//! cannot do AP+STA, "box is online" and "phone is on our AP" are mutually
//! exclusive states — the situation the old rule guarded against cannot arise.
//!
//! It also stops the box raising a pointless setup network on the ethernet path,
//! where it has been online since boot and no AP was ever wanted.
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
/// In the state root, NOT `/run` — learned the hard way. `/run` is root-owned
/// tmpfs and the server runs as `User=virtues`, so every write there failed
/// with EACCES. The lock's write was `let _ =`-swallowed, so **the lock never
/// existed on any real box** — and the scan cache hit the same wall loudly on
/// 2026-08-10 ("could not write scan cache: Permission denied"), which is how
/// both were found. `/run`'s never-survives-reboot property was the appeal;
/// the TTL provides the same guarantee here (a stale lock dies in 120s).
pub const PROVISIONING_LOCK: &str = "/var/lib/virtues/provisioning.lock";

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
    let claimed = crate::api::pair::paired_device_count(pool).await > 0;
    let up = ap_is_up().await;
    // Only asked when it can change the answer — it shells out to nmcli, and
    // for a claimed box the AP is coming down regardless.
    let online = !claimed && has_own_network().await;

    match (claimed, online, up) {
        // Setup is over. Delete, don't just down: the profile is per-setup and
        // leaving it behind invites a later reconciler to resurrect it.
        (true, _, true) => {
            tracing::info!("setup_ap: box is claimed, dropping the setup AP");
            let _ = nmcli(&["connection", "down", AP_CON_NAME]).await;
            let _ = nmcli(&["connection", "delete", AP_CON_NAME]).await;
        }
        // Unclaimed, no network of its own, no AP: this is what the AP is for.
        (false, false, false) => {
            let ssid = ap_ssid();
            tracing::info!("setup_ap: box is unclaimed and offline, raising {ssid}");
            raise(&ssid).await?;
        }
        // Unclaimed, online, AP still up. Should be unreachable on a radio that
        // cannot do AP+STA — but if this hardware ever can, the AP is now
        // costing the owner's association nothing but risk, and the box no
        // longer needs it. Down, not delete: still unclaimed, so losing the
        // network must bring it back.
        (false, true, true) => {
            tracing::info!("setup_ap: box has its own network now, dropping the setup AP");
            let _ = nmcli(&["connection", "down", AP_CON_NAME]).await;
        }
        // (false, true, false) — online, unclaimed, no AP. THE PAIRING WINDOW.
        // Deliberately nothing: the owner has just provisioned wifi and is
        // about to type the code. Raising an AP here is precisely the bug this
        // match was rewritten to remove.
        _ => {}
    }
    Ok(())
}

/// Does the box have a network of its own — anything that is not our setup AP?
///
/// Asked of NetworkManager rather than derived from the route table. The
/// obvious alternative, `cli::link::primary_ip()`, answers "is there a route
/// out", which is a different question: it is also false on a network whose
/// uplink is down, and the box's own AP subnet is exactly the kind of thing
/// that makes route-based reasoning ambiguous. "NM holds an active wifi-station
/// or ethernet profile that isn't ours" is the fact we actually want.
///
/// Ethernet counts. A box provisioned by cable is online from boot and must
/// never raise a setup network.
async fn has_own_network() -> bool {
    let Some(out) = nmcli(&["-t", "-f", "NAME,TYPE", "connection", "show", "--active"]).await
    else {
        // Cannot tell. Say no: the cost of a wrong "no" is an unnecessary AP on
        // a box the owner can still reach, and of a wrong "yes" is a box with
        // no network and no AP — unreachable by any means, needing a trip to
        // wherever it is mounted.
        return false;
    };
    holds_own_network(&String::from_utf8_lossy(&out.stdout))
}

/// The parsing half of [`has_own_network`], split out so every branch is
/// testable without a radio.
fn holds_own_network(nmcli_active: &str) -> bool {
    nmcli_active
        .lines()
        .map(crate::api::provision::split_terse)
        .filter(|f| f.len() >= 2)
        .any(|f| {
            // `lo`/`loopback` and NM's `p2p-dev-*` shadow profiles are not a
            // network anyone can reach the box on.
            f[0] != AP_CON_NAME && (f[1] == "802-11-wireless" || f[1] == "802-3-ethernet")
        })
}

/// Where the pre-AP wifi scan is cached for `api::provision` to fall back on.
///
/// State root, not `/run`, for the same EACCES reason as [`PROVISIONING_LOCK`].
/// Surviving a reboot is fine here: the cache refreshes on every AP raise, and
/// a reboot's first raise overwrites it before anyone can read it.
pub const SCAN_CACHE: &str = "/var/lib/virtues/wifi-scan.json";

/// Bring the AP up on the wifi device.
///
/// WPA2, never open. The owner's home wifi password crosses this link during
/// provisioning; on an open AP that is cleartext to anyone in range. It costs
/// them no typing, because the passphrase rides in the QR the display shows.
///
/// **Scans first, then raises — the order is the fix for a live failure.** The
/// portal's "networks the box can see" list came back empty on hardware
/// (2026-08-10) with a phone joined to the AP, though the same scan works from
/// a root shell with no client attached. An AP serving an associated station
/// has to hold its channel, so off-channel scanning is exactly the thing it
/// cannot reliably do. The one moment a clean scan is guaranteed — and the
/// freshest the list can ever be — is right before the AP goes up, while the
/// radio is still free. So every raise refreshes the cache, which also covers
/// the failed-join path: the re-raise after a failure re-scans on the way.
async fn raise(ssid: &str) -> Result<(), crate::Error> {
    cache_scan().await;
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

/// Scan while the radio is free and write the result where `api::provision`
/// can find it. Best-effort on purpose: a failed cache write must never stop
/// the AP from rising — a box with a stale network list is still provisionable,
/// and a box with no AP is not reachable at all.
async fn cache_scan() -> () {
    match crate::api::provision::scan_networks().await {
        Ok(nets) if !nets.is_empty() => {
            match serde_json::to_vec(&nets) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(SCAN_CACHE, json) {
                        tracing::warn!("setup_ap: could not write scan cache: {e}");
                    } else {
                        tracing::info!(networks = nets.len(), "setup_ap: cached pre-AP wifi scan");
                    }
                }
                Err(e) => tracing::warn!("setup_ap: could not serialize scan: {e}"),
            }
        }
        Ok(_) => tracing::warn!("setup_ap: pre-AP scan saw no networks; leaving prior cache"),
        Err(e) => tracing::warn!("setup_ap: pre-AP scan failed: {e}; leaving prior cache"),
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

    #[test]
    fn our_own_ap_is_not_a_network_of_our_own() {
        // THE REGRESSION THIS FILE EXISTS TO PREVENT. A box hosting only its
        // setup AP is offline, so the reconciler must keep the AP up. If this
        // ever returns true, the AP is never raised and an appliance with no
        // network has no way to be reached at all.
        assert!(!holds_own_network("virtues-setup-ap:802-11-wireless\nlo:loopback"));
    }

    #[test]
    fn a_joined_wifi_network_counts() {
        // And THIS is the other half: after a successful join the box is
        // online and still unclaimed. Returning false here re-raises the AP on
        // top of the association and drops the box off the owner's wifi before
        // they can pair.
        assert!(holds_own_network("weworkwifi:802-11-wireless\nlo:loopback"));
    }

    #[test]
    fn ethernet_counts() {
        // The "plug in ethernet and this finishes itself" path: online from
        // boot, unclaimed for a while, and no setup network was ever wanted.
        assert!(holds_own_network("Wired connection 1:802-3-ethernet"));
    }

    #[test]
    fn an_ssid_containing_a_colon_still_counts() {
        // nmcli escapes it as `\:`; naive splitting truncates the name to
        // "my", which is not the AP's name either — so this happens to pass
        // for the wrong reason unless the escaping is honoured. Pinned because
        // the failure mode is silent.
        assert!(holds_own_network(r"my\:net:802-11-wireless"));
    }

    #[test]
    fn loopback_alone_is_not_a_network() {
        assert!(!holds_own_network("lo:loopback"));
        assert!(!holds_own_network(""));
    }
}
