//! Is the server's data disk actually there?
//!
//! Servers Virtues shipped as hardware keep the state root on its own disk
//! (an NVMe), with the Postgres cluster moved onto it and the OS on the boot
//! card. `fstab` carries `nofail`, so a missing disk never blocks boot. The
//! risk is the other way: the server comes up, Postgres can start a fresh
//! empty cluster on the card, and everything looks healthy while the
//! owner's record sits on a disk nobody mounted. `virtues doctor` reports
//! that state (`cli/doctor.rs`).
//!
//! Only a server whose cluster was moved into its state root
//! ([`relocated_cluster_dir`]) expects a separate disk. An ordinary install
//! keeps everything on one disk, which is correct.

use std::path::{Path, PathBuf};

/// What we can say about the data disk right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataDisk {
    /// The state root is a mount point of its own.
    Mounted,
    /// The state root is a directory on the root filesystem. Correct and
    /// unremarkable on an ordinary install, where there is one disk.
    OnRoot,
    /// This box is configured to have a separate data disk and does not have
    /// one right now. The state a human needs told about.
    Missing,
}

/// The state root this box is configured with.
///
/// **From the install manifest, because nothing sets an env var for it.** This
/// read `VIRTUES_DATA_DIR` first, which no part of the system writes — the
/// installer takes `DATA_DIR` at install time and bakes the *derived* absolute
/// paths (`STORAGE_PATH`, `VIRTUES_MODELS_DIR`, …) into the env file, never the
/// root itself. So on a box installed with a custom `DATA_DIR` this fell back to
/// `/var/lib/virtues`, found it was not a mount point, and reported an appliance
/// as having lost its disk — a false "Storage disconnected" on the panel, which
/// is the single most alarming thing this module can say.
///
/// The env var is still honored first as an override for tests and odd layouts;
/// it just is not the source of truth any more.
pub(crate) fn data_dir() -> PathBuf {
    if let Some(v) = std::env::var_os("VIRTUES_DATA_DIR") {
        return PathBuf::from(v);
    }
    if let Some(d) = crate::install_manifest::get()
        .as_ref()
        .and_then(|m| m.data_dir.clone())
    {
        return d;
    }
    PathBuf::from("/var/lib/virtues")
}

/// `/var/lib/postgresql`, which on a server with a moved cluster is a symlink
/// into the data dir.
pub const PG_LINK: &str = "/var/lib/postgresql";

/// The moved cluster directory, if this server has one.
///
/// Only a link that points into the state root counts: that is the layout
/// the hardware installer made (`<data dir>/postgresql`). `None` on an
/// ordinary install, where `/var/lib/postgresql` is the distro's own
/// directory, and `None` when someone linked it to a disk of their own,
/// which is theirs to look after and not a data disk this server expects.
pub fn relocated_cluster_dir() -> Option<PathBuf> {
    let md = std::fs::symlink_metadata(PG_LINK).ok()?;
    if !md.file_type().is_symlink() {
        return None;
    }
    let target = std::fs::read_link(PG_LINK).ok()?;
    moved_into(&target, &data_dir()).then_some(target)
}

/// Whether a `/var/lib/postgresql` link to `target` is the cluster moved
/// into the state root `data_dir`. A relative target is not: the installer
/// always wrote an absolute one.
fn moved_into(target: &Path, data_dir: &Path) -> bool {
    target.is_absolute() && target.starts_with(data_dir)
}

/// Assess the data disk.
///
/// Only a server whose Postgres cluster was moved to a data disk can be
/// missing one. On an ordinary install the state root is *supposed* to be a
/// directory on the root filesystem, and reporting that as a fault would
/// condemn every correct install.
pub fn status() -> DataDisk {
    status_of(&data_dir(), relocated_cluster_dir().is_some())
}

/// The decision, with its inputs passed in so it is testable without a mount.
pub fn status_of(dir: &Path, expects_disk: bool) -> DataDisk {
    if is_mount_point(dir) {
        return DataDisk::Mounted;
    }
    if expects_disk {
        DataDisk::Missing
    } else {
        DataDisk::OnRoot
    }
}

/// Is `dir` the root of a mounted filesystem?
///
/// Compares the device id of the directory with that of its parent — the
/// classic test, and the reason it beats parsing `/proc/mounts`: it needs no
/// agreement about how the path is spelled, and it is right for bind mounts
/// and symlinked roots too.
///
/// A directory that does not exist is not a mount point, which lands a server
/// expecting a disk in `Missing`: the correct answer, and one that says
/// something useful rather than panicking on an unwrap.
fn is_mount_point(dir: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let Ok(here) = std::fs::metadata(dir) else {
            return false;
        };
        let Some(parent) = dir.parent() else {
            // `/` — a mount point by definition, and not a path we are ever
            // handed as a state root.
            return true;
        };
        let Ok(up) = std::fs::metadata(parent) else {
            return false;
        };
        here.dev() != up.dev()
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_diy_box_on_one_disk_is_not_a_fault() {
        // The check that keeps this from condemning every correct self-hosted
        // install: a DIY state root is a plain directory, by design.
        let s = status_of(Path::new("/nonexistent/virtues"), false);
        assert_eq!(s, DataDisk::OnRoot);
    }

    #[test]
    fn a_server_expecting_a_disk_without_one_is_a_fault() {
        let s = status_of(Path::new("/nonexistent/virtues"), true);
        assert_eq!(s, DataDisk::Missing);
    }

    #[test]
    fn only_a_cluster_moved_into_the_state_root_expects_a_disk() {
        let data = Path::new("/var/lib/virtues");
        assert!(moved_into(Path::new("/var/lib/virtues/postgresql"), data));
        // Someone's own disk, or a relative link: not a disk we expect.
        assert!(!moved_into(Path::new("/mnt/bigdisk/postgresql"), data));
        assert!(!moved_into(Path::new("/var/lib/virtues-other/postgresql"), data));
        assert!(!moved_into(Path::new("virtues/postgresql"), data));
    }

    #[test]
    fn the_filesystem_root_reads_as_mounted() {
        // Not a state root anyone configures, but the parent-less branch has to
        // be right or the device-id compare would panic on unwrap.
        assert_eq!(status_of(Path::new("/"), true), DataDisk::Mounted);
    }

    #[test]
    fn a_plain_directory_is_not_a_mount_point() {
        let d = std::env::temp_dir().join(format!("dd-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert!(!is_mount_point(&d));
        let _ = std::fs::remove_dir_all(&d);
    }
}
