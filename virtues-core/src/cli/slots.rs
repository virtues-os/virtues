//! Atomic release slots — the box's whole-release install layout.
//!
//! A release is staged as one immutable directory and activated by flipping a
//! single symlink; every well-known path routes through that link, so a
//! release changes binary + web + actions **together, atomically**, and
//! rollback is one flip back. This replaces the in-place per-component swaps
//! whose partial failures left boxes half-upgraded (the 2026-07-09 brick).
//!
//! ```text
//! /usr/local/share/virtues/
//!   releases/<slot-id>/        one whole release: virtues, llama-server?,
//!                              virtues-qnnd?, web/, actions/, actions-bin/
//!   current -> releases/<id>   THE flip point (atomic rename)
//!   web     -> current/web     stable paths — env vars & unit files never
//!   actions -> current/actions change; they resolve through `current`
//! /usr/local/bin/virtues       -> ../share/virtues/current/virtues
//! /usr/local/bin/llama-server  -> ../share/virtues/current/llama-server  (if shipped)
//! /usr/local/bin/virtues-qnnd  -> ../share/virtues/current/virtues-qnnd  (if shipped)
//! /usr/local/libexec/virtues   -> ../share/virtues/current/actions-bin
//! ```
//!
//! The INSTALLER creates this layout (there is no legacy-layout conversion —
//! the slot layout shipped before any external box existed); `virtues
//! upgrade` refuses politely on a box without it. Keep-count is 2: current +
//! one previous. Rollback needs exactly one prior release; more is hoarding.

use std::fs;
use std::path::{Path, PathBuf};

/// How many release slots to keep (the current one + N-1 previous).
pub const KEEP_SLOTS: usize = 2;

/// The slot layout rooted at a base dir (`/usr/local/share/virtues` in
/// production; a temp dir in tests).
pub struct SlotLayout {
    base: PathBuf,
}

impl SlotLayout {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    /// The production layout.
    pub fn system() -> Self {
        Self::new("/usr/local/share/virtues")
    }

    pub fn releases_dir(&self) -> PathBuf {
        self.base.join("releases")
    }

    pub fn current_link(&self) -> PathBuf {
        self.base.join("current")
    }

    pub fn slot_dir(&self, slot_id: &str) -> PathBuf {
        self.releases_dir().join(slot_id)
    }

    /// Is the slot layout present on this box? (`current` exists and is a
    /// symlink.) Upgrade refuses without it — the installer owns creation.
    pub fn exists(&self) -> bool {
        self.current_link().is_symlink()
    }

    /// The release dir `current` points at, absolute. `None` if the layout is
    /// absent or the link dangles.
    pub fn current_slot(&self) -> Option<PathBuf> {
        let link = self.current_link();
        let target = fs::read_link(&link).ok()?;
        let abs = if target.is_absolute() { target } else { self.base.join(target) };
        abs.canonicalize().ok().filter(|p| p.is_dir())
    }

    /// Atomically repoint `current` at `slot`: create a temp symlink next to
    /// it and `rename` over — rename on the same filesystem is atomic, so a
    /// crash at any instant leaves `current` pointing at a complete release
    /// (old or new), never nothing.
    pub fn flip(&self, slot: &Path) -> std::io::Result<()> {
        let link = self.current_link();
        let tmp = self.base.join(".current.tmp");
        let _ = fs::remove_file(&tmp);
        std::os::unix::fs::symlink(slot, &tmp)?;
        fs::rename(&tmp, &link)
    }

    /// The newest release that is NOT current — the rollback target.
    /// Ordered by directory mtime (set when the slot was staged).
    pub fn previous_slot(&self) -> Option<PathBuf> {
        let current = self.current_slot();
        self.slots_by_age()
            .into_iter()
            .find(|p| current.as_ref().map(|c| c != p).unwrap_or(true))
    }

    /// All slots, newest first (by mtime).
    fn slots_by_age(&self) -> Vec<PathBuf> {
        let mut slots: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(self.releases_dir())
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| {
                let t = e.metadata().ok()?.modified().ok()?;
                // Canonicalize so comparisons against `current_slot()` (which
                // canonicalizes) can't be foiled by symlinked tmp paths.
                Some((t, e.path().canonicalize().ok()?))
            })
            .collect();
        slots.sort_by(|a, b| b.0.cmp(&a.0));
        slots.into_iter().map(|(_, p)| p).collect()
    }

    /// Delete all slots beyond the newest `keep`, never touching the one
    /// `current` resolves to. Best-effort.
    pub fn prune(&self, keep: usize) {
        let current = self.current_slot();
        let mut kept = 0usize;
        for slot in self.slots_by_age() {
            let is_current = current.as_ref().map(|c| c == &slot).unwrap_or(false);
            if is_current || kept < keep {
                if !is_current {
                    kept += 1;
                }
                continue;
            }
            let _ = fs::remove_dir_all(&slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("slots-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("releases")).unwrap();
        d
    }

    fn mk_slot(base: &Path, id: &str) -> PathBuf {
        let p = base.join("releases").join(id);
        fs::create_dir_all(&p).unwrap();
        // Distinct mtimes: creation order == age order.
        std::thread::sleep(std::time::Duration::from_millis(20));
        p
    }

    #[test]
    fn flip_is_atomic_and_repoints() {
        let base = scratch("flip");
        let layout = SlotLayout::new(&base);
        let a = mk_slot(&base, "a");
        let b = mk_slot(&base, "b");

        assert!(!layout.exists());
        layout.flip(&a).unwrap();
        assert!(layout.exists());
        assert_eq!(layout.current_slot().unwrap(), a.canonicalize().unwrap());
        // Flip over an existing link (the upgrade case).
        layout.flip(&b).unwrap();
        assert_eq!(layout.current_slot().unwrap(), b.canonicalize().unwrap());
    }

    #[test]
    fn previous_is_newest_non_current() {
        let base = scratch("prev");
        let layout = SlotLayout::new(&base);
        let old = mk_slot(&base, "old");
        let mid = mk_slot(&base, "mid");
        let new = mk_slot(&base, "new");
        layout.flip(&new).unwrap();
        assert_eq!(layout.previous_slot().unwrap(), mid.canonicalize().unwrap());
        // After rolling back to mid, previous becomes new (flip-forward target).
        layout.flip(&mid).unwrap();
        assert_eq!(layout.previous_slot().unwrap(), new.canonicalize().unwrap());
        let _ = old;
    }

    #[test]
    fn prune_keeps_current_plus_n() {
        let base = scratch("prune");
        let layout = SlotLayout::new(&base);
        let a = mk_slot(&base, "a");
        let b = mk_slot(&base, "b");
        let c = mk_slot(&base, "c");
        layout.flip(&c).unwrap();
        layout.prune(1); // current + 1 previous
        assert!(!a.exists(), "oldest pruned");
        assert!(b.exists(), "previous kept");
        assert!(c.exists(), "current kept");

        // Current is never pruned even with keep=0.
        layout.prune(0);
        assert!(c.exists());
    }
}
