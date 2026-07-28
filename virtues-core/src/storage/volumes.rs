//! Backup destinations — where a second copy of the archive lives.
//!
//! A volume is identified by its **filesystem UUID**, never by where it happens
//! to be mounted. Mount points move between boots and between drives, so a
//! destination recorded as `/mnt/backup` would eventually start writing to
//! whatever got mounted there instead. `/dev/disk/by-uuid/<uuid>` is the stable
//! name; the mount path is resolved from it at use time and cached only so the
//! UI has something to show.
//!
//! **Absence is never an outage.** A drive that is not plugged in resolves to
//! `None`, the run is skipped with a warning, and the box carries on. That is
//! the entire reason backup destinations are tractable while live storage on
//! removable media is not: the lake must be present or the app is broken, a
//! backup target need only be present eventually.

use std::path::{Path, PathBuf};

/// Roles a volume may serve. Only one exists in this version, deliberately —
/// see `docs/backup-plan.md` for why tiering is excluded.
pub const ROLE_BACKUP: &str = "backup";

/// A registered destination, as stored in `storage_volume`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Volume {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub roles: Vec<String>,
    pub fs_uuid: String,
    pub mount_path: Option<String>,
    pub prefix: String,
    pub state: String,
    pub last_ok_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_error: Option<String>,
}

impl Volume {
    /// The directory this box owns on the volume, once it is mounted.
    ///
    /// Everything outside this path belongs to the owner: their photos, another
    /// box's archives, whatever else they keep on the drive. Nothing here reads,
    /// writes, or prunes outside it, which is why the drive stays usable for its
    /// other purposes and why formatting it is unnecessary.
    pub fn root(&self) -> Option<PathBuf> {
        resolve_mount(&self.fs_uuid).map(|m| m.join(&self.prefix))
    }

    pub fn serves_backup(&self) -> bool {
        self.roles.iter().any(|r| r == ROLE_BACKUP)
    }
}

/// Live mount point for a filesystem UUID, or `None` when the volume is absent.
///
/// `None` is the normal case for a drive nobody has plugged in, not an error.
pub fn resolve_mount(fs_uuid: &str) -> Option<PathBuf> {
    let device = std::fs::canonicalize(format!("/dev/disk/by-uuid/{fs_uuid}")).ok()?;
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
    mount_point_for_source(&mountinfo, &device)
}

/// Find the mount point serving `source` in `/proc/self/mountinfo`.
///
/// Split out from `resolve_mount` so the parsing is testable without a real
/// block device. Format, per mount:
///
/// ```text
/// 36 35 8:1 / /mnt/backup rw,relatime - ext4 /dev/sda1 rw
/// ^                       ^                   ^
/// id                      mount point         source
/// ```
///
/// The optional fields before `-` are variable in number, which is exactly the
/// trap: splitting on whitespace and indexing from the left finds the mount
/// point, but the source has to be located relative to the `-` separator.
fn mount_point_for_source(mountinfo: &str, source: &Path) -> Option<PathBuf> {
    for line in mountinfo.lines() {
        // `continue`, not `?`. A `?` here would abandon the whole scan on the
        // first unparseable line rather than the line itself — and since a
        // partial read of /proc truncates the LAST line, a bail-out would be
        // silent and order-dependent. Matches `source_for_mount` below.
        let Some((left, right)) = line.split_once(" - ") else {
            continue;
        };
        let left: Vec<&str> = left.split_whitespace().collect();
        let right: Vec<&str> = right.split_whitespace().collect();
        // left[4] = mount point, right[1] = mount source.
        let (Some(mount), Some(src)) = (left.get(4), right.get(1)) else {
            continue;
        };
        if Path::new(src) == source {
            return Some(PathBuf::from(unescape_mountinfo(mount)));
        }
    }
    None
}

/// mountinfo octal-escapes space, tab, newline and backslash in paths. A drive
/// mounted at `/media/My Backup Drive` appears as `/media/My\040Backup\040Drive`,
/// and using that verbatim would look for a directory that does not exist —
/// which for removable media is the common case, not an exotic one.
fn unescape_mountinfo(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let octal: String = chars.clone().take(3).collect();
        match u8::from_str_radix(&octal, 8) {
            Ok(byte) if octal.len() == 3 => {
                out.push(byte as char);
                for _ in 0..3 {
                    chars.next();
                }
            }
            _ => out.push('\\'),
        }
    }
    out
}

/// Filesystem UUID of the volume holding `path`, for registering a destination
/// the operator named by mount point.
///
/// Walks `/dev/disk/by-uuid` and matches on the resolved device, so the UUID we
/// store is the one the kernel will hand back later.
pub fn uuid_for_path(path: &Path) -> Option<String> {
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
    let canonical = std::fs::canonicalize(path).ok()?;
    let source = source_for_mount(&mountinfo, &canonical)?;
    for entry in std::fs::read_dir("/dev/disk/by-uuid").ok()?.flatten() {
        // Skip entries that will not resolve instead of abandoning the scan.
        // `/dev/disk/by-uuid` routinely keeps a dangling symlink after a
        // hot-unplug, and directory order is arbitrary — with `?` here, whether
        // a healthy drive could be registered came down to which entry the
        // kernel happened to list first.
        let Ok(resolved) = std::fs::canonicalize(entry.path()) else {
            continue;
        };
        if resolved == source {
            return Some(entry.file_name().to_string_lossy().into_owned());
        }
    }
    None
}

/// The device backing whichever mount actually holds `path` — the LONGEST
/// matching mount point, since `/` and `/media` both prefix `/media/backup`.
fn source_for_mount(mountinfo: &str, path: &Path) -> Option<PathBuf> {
    let mut best: Option<(usize, PathBuf)> = None;
    for line in mountinfo.lines() {
        let Some((left, right)) = line.split_once(" - ") else {
            continue;
        };
        let left: Vec<&str> = left.split_whitespace().collect();
        let right: Vec<&str> = right.split_whitespace().collect();
        let (Some(mount), Some(src)) = (left.get(4), right.get(1)) else {
            continue;
        };
        let mount = PathBuf::from(unescape_mountinfo(mount));
        if path.starts_with(&mount) {
            let depth = mount.as_os_str().len();
            if best.as_ref().is_none_or(|(d, _)| depth > *d) {
                best = Some((depth, PathBuf::from(src)));
            }
        }
    }
    best.map(|(_, src)| src)
}

/// Every registered destination that may hold backups.
pub async fn backup_volumes(pool: &sqlx::PgPool) -> Result<Vec<Volume>, crate::Error> {
    sqlx::query_as::<_, Volume>(
        "SELECT id, name, kind, roles, fs_uuid, mount_path, prefix, state, \
                last_ok_at, last_error \
         FROM storage_volume \
         WHERE $1 = ANY(roles) \
         ORDER BY created_at",
    )
    .bind(ROLE_BACKUP)
    .fetch_all(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("list storage volumes: {e}")))
}

/// Record what a probe saw. `mount_path` is refreshed on every probe precisely
/// because it is not identity — the same drive legitimately appears elsewhere
/// after a reboot.
pub async fn record_probe(
    pool: &sqlx::PgPool,
    id: &str,
    mount: Option<&Path>,
    capacity: Option<u64>,
    free: Option<u64>,
) -> Result<(), crate::Error> {
    sqlx::query(
        "UPDATE storage_volume \
         SET state = $2, mount_path = $3, capacity_bytes = $4, free_bytes = $5, \
             probed_at = NOW(), last_seen_at = CASE WHEN $2 = 'present' THEN NOW() \
                                                    ELSE last_seen_at END, \
             updated_at = NOW() \
         WHERE id = $1",
    )
    .bind(id)
    .bind(if mount.is_some() { "present" } else { "absent" })
    .bind(mount.map(|m| m.to_string_lossy().into_owned()))
    .bind(capacity.map(|v| v as i64))
    .bind(free.map(|v| v as i64))
    .execute(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("record volume probe: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real shape, including the variable optional fields before `-` that make
    // left-indexing for the source wrong.
    const MOUNTINFO: &str = "\
25 30 0:23 / /proc rw,nosuid - proc proc rw
36 25 8:1 / / rw,relatime - ext4 /dev/sda1 rw
41 36 8:17 / /media/My\\040Backup rw,relatime shared:5 master:2 - exfat /dev/sdb1 rw
42 36 8:33 / /var/lib/virtues rw,relatime - ext4 /dev/sdc1 rw
";

    #[test]
    fn finds_the_mount_point_for_a_device() {
        assert_eq!(
            mount_point_for_source(MOUNTINFO, Path::new("/dev/sdc1")),
            Some(PathBuf::from("/var/lib/virtues"))
        );
    }

    #[test]
    fn decodes_escaped_mount_points() {
        // A removable drive named in Finder almost always has a space in it,
        // so this is the common case rather than an edge one.
        assert_eq!(
            mount_point_for_source(MOUNTINFO, Path::new("/dev/sdb1")),
            Some(PathBuf::from("/media/My Backup"))
        );
    }

    #[test]
    fn a_malformed_line_does_not_abandon_the_scan() {
        // This used `?`, which returned from the whole function on the first
        // line without a " - " separator. Whether a mounted drive was found
        // then depended on where the bad line happened to sit.
        let with_junk = format!("41 36 8:17 / /media/x rw shared:5\n{MOUNTINFO}");
        assert_eq!(
            mount_point_for_source(&with_junk, Path::new("/dev/sdc1")),
            Some(PathBuf::from("/var/lib/virtues"))
        );
        assert_eq!(
            source_for_mount(&with_junk, Path::new("/var/lib/virtues/lake")),
            Some(PathBuf::from("/dev/sdc1"))
        );
    }

    #[test]
    fn an_absent_device_is_none_not_an_error() {
        assert_eq!(mount_point_for_source(MOUNTINFO, Path::new("/dev/sdz9")), None);
    }

    #[test]
    fn the_longest_matching_mount_wins() {
        // /var/lib/virtues is under /, and both match. Picking the shorter one
        // would attribute the box's data to the root filesystem.
        assert_eq!(
            source_for_mount(MOUNTINFO, Path::new("/var/lib/virtues/lake")),
            Some(PathBuf::from("/dev/sdc1"))
        );
        assert_eq!(
            source_for_mount(MOUNTINFO, Path::new("/home/adam")),
            Some(PathBuf::from("/dev/sda1"))
        );
    }

    #[test]
    fn unescape_leaves_ordinary_paths_alone() {
        assert_eq!(unescape_mountinfo("/mnt/backup"), "/mnt/backup");
        assert_eq!(unescape_mountinfo("/mnt/a\\040b\\011c"), "/mnt/a b\tc");
        // A trailing lone backslash must not panic or eat the string.
        assert_eq!(unescape_mountinfo("/mnt/x\\"), "/mnt/x\\");
    }

    #[test]
    fn roles_gate_what_a_volume_may_hold() {
        let mut v = Volume {
            id: "vol_1".into(),
            name: "Archive".into(),
            kind: "removable".into(),
            roles: vec![ROLE_BACKUP.into()],
            fs_uuid: "u".into(),
            mount_path: None,
            prefix: "virtues/box".into(),
            state: "absent".into(),
            last_ok_at: None,
            last_error: None,
        };
        assert!(v.serves_backup());
        v.roles.clear();
        assert!(!v.serves_backup());
    }
}
