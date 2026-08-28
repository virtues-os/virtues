//! System repairs — migration-like updates for the box itself, not the schema.
//!
//! SQL migrations are a **history**: numbered, applied exactly once, recorded,
//! and refused on divergence. That model fits a schema because a schema only
//! changes when a migration changes it. The box's filesystem is not like that —
//! ownership, modes and layout regress under us (a card-flash with an old
//! firstboot, a restored backup, a hand-run `chown`, a well-meaning `cp` as
//! root), so an applied-once ledger would confidently record a repair that is
//! no longer true. Repairs are therefore **convergent invariants**: each one
//! states something that must hold on every healthy box, checks it on every
//! run, and repairs it when it doesn't hold. Running them twice is the same as
//! running them once; running them on an already-correct box changes nothing.
//!
//! They run from the `migrate` arm of the CLI, and that placement is the whole
//! trick: `virtues upgrade` runs the **new** binary's `migrate` as root right
//! after the symlink flip, which is the one root-context moment an upgrade
//! gives code shipped *in* the release. (Code in `upgrade.rs` itself is the
//! OLD binary — a fix there only runs when upgrading from the release after
//! this one. The server can't do it either: `virtues.service` runs as
//! `User=virtues`, which cannot chown a root-owned directory back to itself.)
//! `virtues migrate` is also a documented power-user command that runs
//! un-sudo'd, so everything here is gated on euid 0 and skips quietly without
//! it — a repair that can only half-run without privilege must not half-run.
//!
//! Repairs never fail the migrate that carries them. The upgrade flow rolls
//! the whole release back on a non-zero `migrate` exit, and a box that could
//! not be repaired is still a box that upgraded — no worse than it was, and
//! now running a binary whose boot probe will say loudly what is wrong.
//! Firstboot carries the same sweep for boxes cut from new images; this is the
//! vehicle for the fleet that already exists and only ever upgrades in place.

#[cfg(target_os = "linux")]
pub fn run() {
    // Root and a state root, or nothing to do. Keying "on a box" off the data
    // dir existing (not the install manifest) so a half-provisioned box still
    // gets repaired — the manifest is written late in install.
    if unsafe { libc::geteuid() } != 0 {
        return;
    }
    let data_dir = crate::data_disk::data_dir();
    if !data_dir.is_dir() {
        return;
    }
    for (name, repair) in REPAIRS {
        match repair(&data_dir) {
            Outcome::Converged => {}
            Outcome::Repaired(what) => {
                super::ui::ok(&format!("system repair `{name}`: {what}"));
            }
            Outcome::Failed(why) => {
                // A warning, never an error: see the module doc. The boot-time
                // lake probe (`storage::health_check`) is the surface that
                // keeps shouting if this stays broken.
                super::ui::warn(&format!("system repair `{name}` could not finish: {why}"));
            }
        }
    }
}

/// Dev Macs (and anything else non-Linux) compile repairs out entirely; every
/// invariant here is about an installed box's layout. Same shape as the
/// `ble_provision` stub.
#[cfg(not(target_os = "linux"))]
pub fn run() {}

#[cfg(target_os = "linux")]
enum Outcome {
    /// The invariant already held; say nothing.
    Converged,
    /// Something was wrong and is now fixed — the message says what.
    Repaired(String),
    /// The invariant does not hold and the repair could not make it hold.
    Failed(String),
}

/// The invariants. Order is the run order; each entry must be safe to run on
/// every box, every upgrade, forever — retiring one is a deliberate act, not
/// a consequence of it having "already run".
#[cfg(target_os = "linux")]
const REPAIRS: &[(&str, fn(&std::path::Path) -> Outcome)] =
    &[("state-ownership", state_ownership)];

/// Everything under the data dir is written by the `virtues` service user and
/// must be owned by it — except the Postgres cluster (owned by `postgres`;
/// chowning it breaks `pg_filenode.map` and surfaces as a confusing role
/// error) and the journal (journald requires `root:systemd-journal`; taking it
/// would create the same disease this repair exists to cure, one directory
/// over).
///
/// Not hypothetical: boxes flashed from the v0.1.3/v0.1.4 masters booted with
/// a root-owned `lake` — firstboot seeded it root-side and never chowned it —
/// and every ingest applet failed with `Permission denied (os error 13)` on
/// every run, for days, while every surface reported the source as merely
/// idle. Those boxes upgrade in place and never re-flash, so the fixed
/// firstboot in newer images can't reach them; this can.
///
/// A list-free sweep with prunes, deliberately copying the installer's and
/// firstboot's shape rather than naming `lake`: enumerating siblings means the
/// NEXT directory added gets forgotten, which is exactly how the bug happened.
/// Shells out to `find`/`chown` like `restore::give_to_service_user` — a pure
/// Rust walk would need the uid/gid lookup anyway, and `find -exec {} +` is
/// the idiom every other owner of this invariant already uses.
#[cfg(target_os = "linux")]
fn state_ownership(data_dir: &std::path::Path) -> Outcome {
    use std::process::Command;

    let dir = data_dir.display().to_string();
    let postgres = data_dir.join("postgresql").display().to_string();
    let journal = data_dir.join("journal").display().to_string();
    let journal_glob = format!("{journal}/*");
    let prunes = |cmd: &mut Command| {
        cmd.arg(&dir)
            .args(["-path", &postgres, "-prune", "-o"])
            .args(["(", "-path", &journal, "-o", "-path", &journal_glob, ")", "-prune", "-o"])
            .args(["!", "-user", "virtues"]);
    };

    // Detect first, repair second, as two invocations: the detection pass is
    // capped for the log, and piping the REPAIRING find through a cap lets
    // SIGPIPE kill it after the cap — silently fixing a prefix and leaving the
    // rest, the same shape of half-truth this repair exists to end.
    let mut detect = Command::new("find");
    prunes(&mut detect);
    detect.arg("-print");
    let found = match detect.output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(e) => return Outcome::Failed(format!("could not run find: {e}")),
    };
    if found.is_empty() {
        return Outcome::Converged;
    }

    let mut repair = Command::new("find");
    prunes(&mut repair);
    repair.args(["-exec", "chown", "virtues:virtues", "{}", "+"]);
    match repair.output() {
        Ok(o) if o.status.success() => {
            let paths: Vec<&str> = found.lines().collect();
            let shown = paths.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
            let extra = paths.len().saturating_sub(5);
            Outcome::Repaired(if extra > 0 {
                format!(
                    "{} path(s) were not owned by virtues — repaired ({shown}, +{extra} more)",
                    paths.len()
                )
            } else {
                format!("{} path(s) were not owned by virtues — repaired ({shown})", paths.len())
            })
        }
        Ok(o) => Outcome::Failed(format!(
            "chown sweep exited {} ({})",
            o.status,
            String::from_utf8_lossy(&o.stderr).trim()
        )),
        Err(e) => Outcome::Failed(format!("could not run chown sweep: {e}")),
    }
}
