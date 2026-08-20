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
//!   current  -> releases/<id>  THE flip point (atomic rename)
//!   prepared -> releases/<id>  downloaded + preflighted, NOT yet activated
//!   web      -> current/web    stable paths — env vars & unit files never
//!   actions  -> current/actions change; they resolve through `current`
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
//!
//! `prepared` is the third pointer, and it is a POINTER rather than a state
//! file on purpose: the fact that a release is staged is a fact about the
//! filesystem, so it lives in the filesystem, next to the thing it describes.
//! A state file can outlive the slot it names (someone clears `releases/`, a
//! disk fills mid-write) and then claims a release is ready that isn't there.
//! A dangling symlink is self-evidently dangling.
//!
//! It also has to exist for pruning to be correct. `prune` keeps the newest N
//! by mtime, and a prepared slot is by definition the newest thing on disk —
//! but so is the slot an upgrade just activated. Without a pointer saying "this
//! one is spoken for", background preparation on a box that upgrades often
//! would either evict the prepared release or evict the rollback target,
//! depending on the order things happened in.

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

    /// Where a downloaded-and-preflighted release waits for someone to say go.
    pub fn prepared_link(&self) -> PathBuf {
        self.base.join("prepared")
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
        self.resolve(&self.current_link())
    }

    /// The staged-but-not-activated release, absolute. `None` when nothing is
    /// prepared — including when the link exists but its slot has been removed,
    /// which is exactly the case a state file would get wrong.
    pub fn prepared_slot(&self) -> Option<PathBuf> {
        self.resolve(&self.prepared_link())
    }

    /// Resolve one of the pointer symlinks to a real directory.
    fn resolve(&self, link: &Path) -> Option<PathBuf> {
        let target = fs::read_link(link).ok()?;
        let abs = if target.is_absolute() { target } else { self.base.join(target) };
        abs.canonicalize().ok().filter(|p| p.is_dir())
    }

    /// Point `prepared` at a staged slot. Same atomic symlink-and-rename as
    /// [`flip`] — a half-written pointer would be indistinguishable from a
    /// release that is ready when it is not.
    pub fn set_prepared(&self, slot: &Path) -> std::io::Result<()> {
        let tmp = self.base.join(".prepared.tmp");
        let _ = fs::remove_file(&tmp);
        std::os::unix::fs::symlink(slot, &tmp)?;
        fs::rename(&tmp, self.prepared_link())
    }

    /// Drop the `prepared` pointer, leaving the slot itself for `prune` to
    /// reclaim in age order. Called once a prepared release is activated (it is
    /// `current` now, not pending) and when a newer release supersedes it.
    pub fn clear_prepared(&self) {
        let _ = fs::remove_file(self.prepared_link());
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

    /// Delete all slots beyond the newest `keep`, never touching the ones
    /// `current` or `prepared` resolve to. Best-effort.
    ///
    /// Both pointers are exempt rather than counted, because they answer
    /// different questions: `current` is what the box runs, `prepared` is what
    /// it is about to run, and `keep` is how much history to hold for rollback.
    /// Counting a prepared slot against the history budget would mean that
    /// staging an update silently discards the release you would roll back to —
    /// paying for the next upgrade with the safety net of the last one.
    pub fn prune(&self, keep: usize) {
        let current = self.current_slot();
        let prepared = self.prepared_slot();
        let mut kept = 0usize;
        for slot in self.slots_by_age() {
            let spoken_for = current.as_ref() == Some(&slot) || prepared.as_ref() == Some(&slot);
            if spoken_for || kept < keep {
                if !spoken_for {
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

    #[test]
    fn prepared_survives_prune_without_costing_the_rollback_target() {
        let base = scratch("prepared-prune");
        let layout = SlotLayout::new(&base);
        let old = mk_slot(&base, "old");
        let prev = mk_slot(&base, "prev");
        let cur = mk_slot(&base, "cur");
        let staged = mk_slot(&base, "staged"); // newest — a background prepare
        layout.flip(&cur).unwrap();
        layout.set_prepared(&staged).unwrap();

        layout.prune(KEEP_SLOTS - 1);

        assert!(staged.exists(), "prepared release kept");
        assert!(cur.exists(), "current kept");
        // The whole point of exempting `prepared`: preparing an update must not
        // spend the rollback target to pay for itself.
        assert!(prev.exists(), "rollback target kept");
        assert!(!old.exists(), "genuinely old slot reclaimed");
    }

    #[test]
    fn prepared_resolves_and_clears() {
        let base = scratch("prepared-ptr");
        let layout = SlotLayout::new(&base);
        let s = mk_slot(&base, "s");
        assert!(layout.prepared_slot().is_none(), "nothing prepared yet");

        layout.set_prepared(&s).unwrap();
        assert_eq!(layout.prepared_slot().unwrap(), s.canonicalize().unwrap());

        // Re-pointing is the supersede case: a newer release replaces an older
        // prepared one without an intervening clear.
        let t = mk_slot(&base, "t");
        layout.set_prepared(&t).unwrap();
        assert_eq!(layout.prepared_slot().unwrap(), t.canonicalize().unwrap());

        layout.clear_prepared();
        assert!(layout.prepared_slot().is_none());
    }

    /// A pointer beats a state file precisely here: the slot can vanish under
    /// it, and the answer must become "nothing is prepared" rather than a claim
    /// that a missing release is ready to install.
    #[test]
    fn prepared_pointing_at_a_removed_slot_reads_as_nothing_prepared() {
        let base = scratch("prepared-dangle");
        let layout = SlotLayout::new(&base);
        let s = mk_slot(&base, "s");
        layout.set_prepared(&s).unwrap();
        fs::remove_dir_all(&s).unwrap();
        assert!(layout.prepared_slot().is_none());
    }
}
