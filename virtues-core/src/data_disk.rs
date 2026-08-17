//! Is the box's data disk actually there?
//!
//! On an appliance the state root is its own filesystem — a blank NVMe claimed
//! at first boot (`virtues-firstboot.sh`) — while the OS lives on the eMMC.
//! That split is deliberate: the eMMC is soldered and has modest write
//! endurance, so the database, the lake and the journal (everything that
//! actually writes) belong on the replaceable part, and the part that cannot be
//! replaced carries only a rootfs that changes once per release.
//!
//! The split's other half is the reason this module exists. Keeping a bootable
//! OS on the eMMC means a box whose NVMe is dead, unseated, or was never
//! fitted still comes up — and a box that comes up can SAY SO. `fstab` carries
//! `nofail` for exactly that, so a missing disk must never block boot.
//!
//! But "must not block boot" and "may be ignored" are different claims, and
//! conflating them is how you get the worst outcome available here: Postgres
//! cheerfully `initdb`s a fresh empty cluster onto the eMMC, every service
//! reports healthy, and the owner's box looks perfectly fine while being
//! empty. Their record is not lost — it is sitting on a disk nobody mounted —
//! but nothing on any screen would tell them that, and the longer it runs the
//! more a "restore from backup" would destroy.
//!
//! So: boot anyway, refuse to serve, and put it on the glass.

use std::path::{Path, PathBuf};

/// What we can say about the data disk right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataDisk {
    /// The state root is a mount point of its own — the expected appliance
    /// shape, and the only one where writes are landing where they should.
    Mounted,
    /// The state root is a directory on the root filesystem. Correct and
    /// unremarkable on a DIY box, where there is one disk by definition.
    OnRoot,
    /// This box is configured to have a separate data disk and does not have
    /// one right now. The state a human needs told about.
    Missing,
}

impl DataDisk {
    /// Should the box refuse to serve?
    ///
    /// Only [`DataDisk::Missing`]. `OnRoot` is a perfectly good DIY install.
    pub fn is_fault(self) -> bool {
        matches!(self, DataDisk::Missing)
    }

    /// One line, written for someone standing in front of the box rather than
    /// reading a log. Names the part, says the record is intact, and gives the
    /// single next action.
    pub fn message(self) -> Option<&'static str> {
        match self {
            DataDisk::Missing => Some(
                "I can't find my storage disk. Your record is on it, not lost — \
                 the disk needs reseating or replacing.",
            ),
            _ => None,
        }
    }
}

/// The state root this box is configured with.
fn data_dir() -> PathBuf {
    // The same resolution order the rest of the box uses: the env the systemd
    // unit passes, then the compiled-in default.
    std::env::var_os("VIRTUES_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/virtues"))
}

/// Assess the data disk.
///
/// The appliance/DIY distinction is what makes `Missing` sayable at all: on a
/// DIY box the state root is *supposed* to be a directory on the root
/// filesystem, and reporting that as a fault would condemn every correct
/// self-hosted install. Only a box that declared itself an appliance — which
/// means `virtues-firstboot` was going to claim a disk for it — can be missing
/// one.
pub fn status() -> DataDisk {
    status_of(&data_dir(), crate::install_manifest::appliance())
}

/// The decision, with its inputs passed in so it is testable without a mount.
pub fn status_of(dir: &Path, appliance: bool) -> DataDisk {
    if is_mount_point(dir) {
        return DataDisk::Mounted;
    }
    if appliance {
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
/// A directory that does not exist is not a mount point, which lands a
/// misconfigured appliance in `Missing` — the correct answer, and one that
/// says something useful rather than panicking on an unwrap.
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
        assert!(!s.is_fault());
        assert!(s.message().is_none());
    }

    #[test]
    fn an_appliance_without_its_disk_is_a_fault() {
        let s = status_of(Path::new("/nonexistent/virtues"), true);
        assert_eq!(s, DataDisk::Missing);
        assert!(s.is_fault());
        assert!(s.message().unwrap().contains("not lost"));
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

    #[test]
    fn the_message_leads_with_the_record_being_intact() {
        // Copy rule, pinned because it is the whole point of the state: the
        // owner is looking at a box that says it cannot find its storage, and
        // the sentence they need first is that their life is still on it.
        let m = DataDisk::Missing.message().unwrap();
        assert!(m.contains("Your record is on it"));
        assert!(!m.to_lowercase().contains("error"));
    }
}
