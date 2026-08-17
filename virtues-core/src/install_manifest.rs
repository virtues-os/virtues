//! `install.json` — what the installer declared this box to be.
//!
//! The installer is the only writer (`tools/virtues-installer`, `install.rs`
//! `write_install_manifest`); everything here reads. That direction is the
//! whole point.
//!
//! ## Why this module exists
//!
//! Three places used to each re-derive the shape of an install from whatever
//! evidence was nearest to hand, and all three were wrong in different ways:
//!
//! * `maintenance::setup_ap::is_appliance()` tested whether
//!   `/etc/systemd/system/virtues-display.service` existed. That file gates
//!   Improv/BLE provisioning, the setup AP, and whether an account is
//!   required — so a headless appliance, or one installed before the display
//!   unit was written, silently reported itself as somebody's DIY server and
//!   turned off its own onboarding transport.
//! * `cli::uninstall` carried a hardcoded unit list that still named
//!   `virtues-wireguard` (which `cli::upgrade` actively deletes as legacy) and
//!   had never heard of the display, first-boot or captive-redirect units — so
//!   uninstalling an appliance left a kiosk pointed at a dead server.
//! * `cli::upgrade` restarted a third subset.
//!
//! A decision that was made once, at install time, should be recorded once and
//! read — not reconstructed from side effects three different ways.
//!
//! ## Absence is meaningful, and it means DIY
//!
//! A box with no manifest is either a dev checkout or an install from before
//! this file existed. Both should behave as they did: no self-administered
//! radio, no forced account. So every accessor degrades toward the
//! conservative answer rather than failing, and `appliance()` in particular is
//! false when we cannot tell.

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Where the installer writes it. Inside the shipped tree, not the state root:
/// it describes the *installation*, and an upgrade replacing that tree is
/// exactly when it should be rewritten.
pub const MANIFEST_PATH: &str = "/usr/local/share/virtues/install.json";

#[derive(Debug, Clone, Default, Deserialize)]
pub struct InstallManifest {
    /// `dragon` | `bundled` | `manual` — which inference topology.
    #[serde(default)]
    pub profile: String,
    /// Our hardware or `--appliance`: a guided product rather than someone's
    /// own server. Gates self-administration of the network and the account
    /// requirement.
    ///
    /// `Option`, and the difference between `None` and `Some(false)` is
    /// load-bearing — see [`appliance()`]. A manifest written before this field
    /// existed has no OPINION; it does not say "DIY".
    #[serde(default)]
    pub appliance: Option<bool>,
    /// Inference sidecar units, without the `.service` suffix.
    #[serde(default)]
    pub sidecars: Vec<String>,
    /// EVERY unit the installer wrote, in stop order.
    #[serde(default)]
    pub units: Vec<String>,
    /// Files outside `/etc/systemd/system` that exist only because we put them
    /// there — the kiosk shim, the first-boot script, the polkit rule.
    #[serde(default)]
    pub extra_files: Vec<String>,
    /// The state root this install was configured with.
    #[serde(default)]
    pub data_dir: Option<PathBuf>,
}

/// Read and cache the manifest.
///
/// Cached for the life of the process: it changes only when the installer or
/// an upgrade runs, and both of those restart the service. `is_appliance()` is
/// consulted on hot paths (the BLE reconcile loop, the setup-AP reconciler),
/// and those must not stat a file every few seconds.
pub fn get() -> &'static Option<InstallManifest> {
    static CACHE: OnceLock<Option<InstallManifest>> = OnceLock::new();
    CACHE.get_or_init(|| load_from(Path::new(MANIFEST_PATH)))
}

/// The read itself, taking a path so it is testable without `/usr/local`.
///
/// A malformed manifest reads as absent rather than propagating an error: the
/// box must still boot and serve, and every caller here already has a defined
/// behavior for "no manifest".
pub fn load_from(path: &Path) -> Option<InstallManifest> {
    let bytes = std::fs::read(path).ok()?;
    match serde_json::from_slice::<InstallManifest>(&bytes) {
        Ok(m) => Some(m),
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "install manifest is unreadable — treating this box as DIY"
            );
            None
        }
    }
}

/// Is this a guided appliance rather than somebody's own Linux server?
///
/// ## Three cases, and the middle one is why this is not a one-liner
///
/// * The manifest says so → believe it. The answer we want, and the only one
///   that is right for a HEADLESS appliance.
/// * The manifest exists but has no opinion → **fall back to the old signal**,
///   the presence of `virtues-display.service`. Every appliance already in the
///   field is in exactly this state, and `virtues upgrade` does NOT re-run the
///   installer — it swaps the binary inside a release slot and leaves
///   `install.json` untouched. Without this fallback, every one of those boxes
///   silently stops being an appliance the moment it takes an update: no
///   Improv, no setup AP, no account requirement, and no way for the owner to
///   tell why Bluetooth setup stopped working. Verified against the live box,
///   whose manifest carries `profile: dragon` and no `appliance` key at all.
/// * No manifest → false, which keeps us a polite guest on a machine we do not
///   own.
///
/// The fallback is deliberately the exact check this field replaced,
/// imperfections and all: it is wrong only for a headless appliance, and a
/// headless appliance predating this field does not exist.
pub fn appliance() -> bool {
    match get().as_ref() {
        None => false,
        Some(m) => match m.appliance {
            Some(v) => v,
            None => std::path::Path::new("/etc/systemd/system/virtues-display.service").exists(),
        },
    }
}

/// Inference sidecar units, for stop/start around an upgrade.
///
/// Falls back to probing the unit directory for boxes installed before the
/// manifest carried them — the same fallback `cli::upgrade` had inline.
pub fn sidecar_units() -> Vec<String> {
    if let Some(m) = get().as_ref() {
        if !m.sidecars.is_empty() {
            return m.sidecars.clone();
        }
    }
    ["virtues-embed", "virtues-rerank", "virtues-qnnd"]
        .into_iter()
        .filter(|u| Path::new(&format!("/etc/systemd/system/{u}.service")).exists())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, body: &str) -> PathBuf {
        let p = dir.join("install.json");
        std::fs::write(&p, body).unwrap();
        p
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("im-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn reads_an_appliance_manifest() {
        let d = tmp("appliance");
        let p = write(
            &d,
            r#"{"profile":"dragon","appliance":true,
                "sidecars":["virtues-qnnd"],
                "units":["virtues-display","virtues","virtues-qnnd"],
                "extra_files":["/usr/local/lib/virtues/display.py"]}"#,
        );
        let m = load_from(&p).expect("parses");
        assert_eq!(m.appliance, Some(true));
        assert_eq!(m.sidecars, vec!["virtues-qnnd"]);
        assert!(m.units.contains(&"virtues-display".to_string()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_missing_manifest_is_not_an_appliance() {
        assert!(load_from(Path::new("/nonexistent/install.json")).is_none());
    }

    #[test]
    fn an_old_manifest_has_no_opinion_rather_than_saying_diy() {
        // THE upgrade hazard, and the reason `appliance` is an Option. Every
        // appliance in the field was installed before this field existed, and
        // `virtues upgrade` swaps the binary WITHOUT re-running the installer —
        // so `install.json` keeps its old shape forever. If a missing key read
        // as `false`, every one of those boxes would stop serving Improv the
        // moment it updated, silently. `None` is what lets `appliance()` fall
        // back to the old signal instead. Shape copied from the live box.
        let d = tmp("legacy");
        let p = write(
            &d,
            r#"{"profile":"dragon","sidecars":["virtues-qnnd"],
                "models_dir":"/var/lib/virtues/models","written_by":"0.1.0"}"#,
        );
        let m = load_from(&p).expect("parses");
        assert_eq!(m.appliance, None, "a missing key must not read as an explicit no");
        assert_eq!(m.sidecars.len(), 1, "sidecars must survive the missing key");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_explicit_false_is_not_the_same_as_silence() {
        let d = tmp("explicit");
        let p = write(&d, r#"{"profile":"manual","appliance":false}"#);
        assert_eq!(load_from(&p).unwrap().appliance, Some(false));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_malformed_manifest_reads_as_absent() {
        let d = tmp("malformed");
        let p = write(&d, "{not json");
        assert!(load_from(&p).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
