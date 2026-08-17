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
    #[serde(default)]
    pub appliance: bool,
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
/// False when there is no manifest, which is the answer that keeps us a polite
/// guest on a machine we do not own.
pub fn appliance() -> bool {
    get().as_ref().map(|m| m.appliance).unwrap_or(false)
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
        assert!(m.appliance);
        assert_eq!(m.sidecars, vec!["virtues-qnnd"]);
        assert!(m.units.contains(&"virtues-display".to_string()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_missing_manifest_is_not_an_appliance() {
        assert!(load_from(Path::new("/nonexistent/install.json")).is_none());
    }

    #[test]
    fn an_old_manifest_without_the_appliance_key_reads_as_diy() {
        // Boxes installed before this field existed. `#[serde(default)]` is
        // what keeps them parsing at all; the assertion is that the missing
        // key means DIY rather than a parse failure that would ALSO mean DIY
        // but for the wrong reason — and would take `sidecars` down with it.
        let d = tmp("legacy");
        let p = write(
            &d,
            r#"{"profile":"bundled","sidecars":["virtues-embed","virtues-rerank"]}"#,
        );
        let m = load_from(&p).expect("parses");
        assert!(!m.appliance);
        assert_eq!(m.sidecars.len(), 2, "sidecars must survive the missing key");
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
