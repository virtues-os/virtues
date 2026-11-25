//! `install.json` — what the installer declared this box to be.
//!
//! The installer is the only writer (`tools/virtues-installer`, `install.rs`
//! `write_install_manifest`); everything here reads. That direction is the
//! whole point.
//!
//! ## Why this module exists
//!
//! A decision made once, at install time (which inference topology, which
//! units and files the installer wrote, which state root), is recorded once
//! and read, rather than reconstructed from side effects in each place that
//! needs it. `cli::uninstall` and `cli::upgrade` read the unit list here.
//!
//! ## Absence is meaningful
//!
//! A server with no manifest is either a dev checkout or an install from
//! before this file existed. Every accessor degrades toward the conservative
//! answer rather than failing.

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Where the installer writes it, under the DEFAULT prefix. Inside the shipped
/// tree, not the state root: it describes the *installation*, and an upgrade
/// replacing that tree is exactly when it should be rewritten.
///
/// Use [`manifest_path`], not this — the prefix is not always `/usr/local`.
pub const DEFAULT_MANIFEST_PATH: &str = "/usr/local/share/virtues/install.json";

/// The manifest path for THIS installation, honouring `INSTALL_PREFIX`.
///
/// The installer has always supported a custom prefix
/// (`tools/virtues-installer/src/config.rs` reads `INSTALL_PREFIX` and writes
/// the manifest to `<prefix>/share/virtues/install.json`), but this side only
/// ever looked under `/usr/local`. So on a prefixed install the file was
/// written somewhere the box never read.
///
/// The consequence was worse than a missing file: no manifest is not an
/// error here, so a prefixed install would quietly fall back to defaults
/// (`data_dir` to the literal, the unit list to a probe) with nothing logged,
/// because every caller has a defined behavior for the absent case.
///
/// Env var rather than a build-time constant: the same binary is installed
/// under different prefixes, and it is the installer that knows which.
pub fn manifest_path() -> PathBuf {
    match std::env::var_os("INSTALL_PREFIX") {
        Some(p) if !p.is_empty() => PathBuf::from(p).join("share/virtues/install.json"),
        _ => PathBuf::from(DEFAULT_MANIFEST_PATH),
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct InstallManifest {
    /// `bundled` | `manual` — which inference topology. Older installs may
    /// say `dragon`, which nothing reads any more.
    #[serde(default)]
    pub profile: String,
    /// Inference sidecar units, without the `.service` suffix.
    #[serde(default)]
    pub sidecars: Vec<String>,
    /// EVERY unit the installer wrote, in stop order.
    #[serde(default)]
    pub units: Vec<String>,
    /// Files outside `/etc/systemd/system` that exist only because we put them
    /// there.
    #[serde(default)]
    pub extra_files: Vec<String>,
    /// The state root this install was configured with.
    #[serde(default)]
    pub data_dir: Option<PathBuf>,
    /// Firewall rules the installer opened, and only those (a rule already
    /// open was left alone): `firewalld:port:8000/tcp`,
    /// `firewalld:service:mdns`, `ufw:5353/udp`. Uninstall closes exactly
    /// these.
    #[serde(default)]
    pub firewall: Vec<String>,
}

/// Read and cache the manifest.
///
/// Cached for the life of the process: it changes only when the installer or
/// an upgrade runs, and both of those restart the service.
pub fn get() -> &'static Option<InstallManifest> {
    static CACHE: OnceLock<Option<InstallManifest>> = OnceLock::new();
    CACHE.get_or_init(|| load_from(&manifest_path()))
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
                "install manifest is unreadable — treating this server as having none"
            );
            None
        }
    }
}

/// Inference sidecar units installed right now, for stop/start around an
/// upgrade and for uninstall.
///
/// The manifest says what the installer set up, but upgrades change it since:
/// the reranker is retired on the recommended setup, and a search model change
/// runs `virtues-embed-next` until it finishes (`cli::model_set`). So the
/// list is the manifest's units plus every sidecar Virtues can install, kept to
/// those whose unit file exists.
pub fn sidecar_units() -> Vec<String> {
    let known = ["virtues-embed", "virtues-rerank", "virtues-embed-next"];
    let mut units: Vec<String> = get().as_ref().map(|m| m.sidecars.clone()).unwrap_or_default();
    for u in known {
        if !units.iter().any(|x| x == u) {
            units.push(u.to_string());
        }
    }
    units.retain(|u| Path::new(&format!("/etc/systemd/system/{u}.service")).exists());
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `INSTALL_PREFIX` moves the manifest path, and its absence keeps the
    /// default.
    ///
    /// The installer has always honoured a custom prefix; this side only ever
    /// looked under `/usr/local`, so on a prefixed box the manifest was written
    /// where nothing read it, and absence fell back to defaults silently.
    #[test]
    fn manifest_path_follows_the_install_prefix() {
        // Serialized against other env-mutating tests by construction: this is
        // the only test in the module and it restores what it finds.
        let saved = std::env::var_os("INSTALL_PREFIX");

        std::env::remove_var("INSTALL_PREFIX");
        assert_eq!(manifest_path(), std::path::Path::new(DEFAULT_MANIFEST_PATH));

        std::env::set_var("INSTALL_PREFIX", "/opt");
        assert_eq!(
            manifest_path(),
            std::path::Path::new("/opt/share/virtues/install.json"),
            "a prefixed install must be found where the installer wrote it"
        );

        // Empty is treated as unset rather than as the root.
        std::env::set_var("INSTALL_PREFIX", "");
        assert_eq!(manifest_path(), std::path::Path::new(DEFAULT_MANIFEST_PATH));

        match saved {
            Some(v) => std::env::set_var("INSTALL_PREFIX", v),
            None => std::env::remove_var("INSTALL_PREFIX"),
        }
    }

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
    fn reads_a_manifest() {
        let d = tmp("bundled");
        let p = write(
            &d,
            r#"{"profile":"bundled",
                "sidecars":["virtues-embed"],
                "units":["virtues-display","virtues","virtues-embed"],
                "extra_files":["/usr/local/lib/virtues/display.py"]}"#,
        );
        let m = load_from(&p).expect("parses");
        assert_eq!(m.sidecars, vec!["virtues-embed"]);
        assert!(m.units.contains(&"virtues-display".to_string()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_missing_manifest_reads_as_absent() {
        assert!(load_from(Path::new("/nonexistent/install.json")).is_none());
    }

    #[test]
    fn an_old_manifest_with_an_appliance_key_still_parses() {
        // Servers installed when the installer still wrote `appliance` keep
        // that key forever (`virtues upgrade` does not rewrite the manifest),
        // so an unknown key must not make the manifest unreadable.
        let d = tmp("legacy");
        let p = write(&d, r#"{"profile":"bundled","appliance":true,"sidecars":["virtues-embed"]}"#);
        let m = load_from(&p).expect("parses");
        assert_eq!(m.sidecars.len(), 1);
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
