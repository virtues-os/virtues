//! Files outside the database that belong to one box and its owner.
//!
//! `virtues deprovision` removes every one of them, `virtues reset` removes
//! the ones that copy rows it drops, and `virtues image-check` fails while any
//! of them holds a file. All three read [`all`], so a writer that leaves a
//! per-box secret on the disk is added here once and is then cleaned and
//! checked everywhere.
//!
//! Each path comes from the function its writer uses, so the list cannot
//! point somewhere the writer does not write. The database, the lake, the env
//! file's key and the authored applets are not here: the commands handle each
//! of those in steps of their own.

use std::path::{Path, PathBuf};

/// What `virtues reset` does with a file on the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnReset {
    /// Removed, because it copies rows that reset drops.
    Remove,
    /// Kept, the way reset keeps `VIRTUES_ENCRYPTION_KEY`. A reset is the same
    /// owner starting over, and this file belongs to the owner rather than to
    /// the database.
    Keep,
}

/// One per-box file or folder.
pub struct BoxFile {
    /// What it is, as the commands name it.
    pub what: &'static str,
    /// What it costs when it stays on a disk that is imaged or handed on.
    pub why: &'static str,
    pub path: PathBuf,
    pub on_reset: OnReset,
}

/// Every per-box file, at the path its writer uses.
pub fn all() -> Vec<BoxFile> {
    vec![
        BoxFile {
            what: "backup recipient",
            why: "the next owner's backups would be encrypted to this owner's recovery key, so only this owner could open them",
            path: crate::cli::backup::recipient_path(),
            on_reset: OnReset::Keep,
        },
        BoxFile {
            what: "shared pages and door key",
            why: "the owner's shared pages, and the key that lets whoever holds it answer as this box's shared links",
            path: crate::api::publications::state_dir(),
            on_reset: OnReset::Remove,
        },
        BoxFile {
            what: "applet run files",
            why: "decrypted keys and source tokens from an applet run the server did not finish",
            path: crate::applet_runner::RunFile::dir(),
            on_reset: OnReset::Remove,
        },
    ]
}

/// Whether `path` is a file, or a folder with a file anywhere under it.
/// Symlinks count and are not followed. Sockets and empty folders do not
/// count: the server makes the door's `core.sock` at every start, and it
/// carries nothing.
pub fn holds_a_file(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.is_dir() {
        return meta.is_file() || meta.file_type().is_symlink();
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        // A folder this process cannot read is one it cannot clear either,
        // so it counts as holding something.
        return true;
    };
    entries.flatten().any(|e| holds_a_file(&e.path()))
}

/// Remove `path`, a file or a whole folder. Returns whether anything was
/// there; a path that does not exist is not an error.
pub fn remove(path: &Path) -> std::io::Result<bool> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    if meta.is_dir() {
        std::fs::remove_dir_all(path)?;
    } else {
        std::fs::remove_file(path)?;
    }
    Ok(true)
}

/// Remove each of `files` and print what went. Stops at the first one that
/// cannot be removed.
pub fn remove_each<'a>(files: impl IntoIterator<Item = &'a BoxFile>) -> Result<(), crate::Error> {
    for f in files {
        match remove(&f.path) {
            Ok(true) => println!("  ✓ removed the {} at {}", f.what, f.path.display()),
            Ok(false) => {}
            Err(e) => {
                return Err(crate::Error::Other(format!(
                    "could not remove the {} at {}: {e}",
                    f.what,
                    f.path.display()
                )))
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_holds_a_file_only_when_one_is_inside_it() {
        // Under /tmp, because a socket's path must fit in about 100 bytes and
        // the per-user temporary folder on macOS is long.
        let root = tempfile::tempdir_in("/tmp").unwrap();
        let dir = root.path().join("publications");
        assert!(!holds_a_file(&dir), "absent");

        std::fs::create_dir_all(dir.join("door")).unwrap();
        std::fs::create_dir_all(dir.join("bundles")).unwrap();
        assert!(!holds_a_file(&dir), "empty folders");

        let _socket = std::os::unix::net::UnixListener::bind(dir.join("door").join("core.sock")).unwrap();
        assert!(!holds_a_file(&dir), "a socket carries nothing");

        std::fs::write(dir.join("door").join("door.key"), [7u8; 32]).unwrap();
        assert!(holds_a_file(&dir));
        assert!(holds_a_file(&dir.join("door").join("door.key")), "a file on its own");
    }

    #[test]
    fn remove_takes_a_file_or_a_whole_folder_and_allows_absence() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("backup-recipient");
        std::fs::write(&file, "age1example\n").unwrap();
        let dir = root.path().join("applet-env");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.env"), "EXAMPLE_KEY=\"x\"\n").unwrap();

        assert!(remove(&file).unwrap());
        assert!(remove(&dir).unwrap());
        assert!(!file.exists() && !dir.exists());
        assert!(!remove(&file).unwrap(), "already gone");
    }

    /// The same owner keeps the recipient through a reset, and the copies of
    /// dropped rows go.
    #[test]
    fn reset_keeps_only_what_belongs_to_the_owner() {
        let kept: Vec<_> = all().into_iter().filter(|f| f.on_reset == OnReset::Keep).map(|f| f.what).collect();
        assert_eq!(kept, ["backup recipient"]);
    }
}
