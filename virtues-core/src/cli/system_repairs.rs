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
//! WHERE the root context actually is took two releases to get right, so
//! spell it out. During `virtues upgrade`, the orchestrator (the OLD binary,
//! root) spawns the NEW binary as `virtues migrate` — also root, but only for
//! an instant: main.rs's `maybe_reexec_as_service_user` immediately re-execs
//! every DB-touching subcommand, `migrate` included, as the `virtues` user
//! (Postgres peer auth). v0.1.5-staging.72 hooked repairs *after* that
//! demotion, in the Migrate arm of cli/mod.rs — present on every box, root-
//! gated, and silently skipped on all of them. The working hooks are:
//!
//!   1. main.rs, in the re-exec guard, before the drop — the NEW binary's
//!      root instant, which is what heals a box on the upgrade that first
//!      carries a repair;
//!   2. upgrade.rs, after migrate succeeds — the orchestrator's own pass,
//!      which keeps firing on future upgrades once this release is the old
//!      half.
//!
//!   (The Migrate arm's call stays as a third site: inert today because of
//!   the demotion, live again if the re-exec ever stops covering `migrate`.
//!   The server can't ever do this work: `virtues.service` runs as
//!   `User=virtues`, which cannot chown a root-owned directory to itself.)
//!
//! Hook 1 runs for `virtues migrate --check` too, which applies no migration.
//! The installer and first boot run that as root, so a new box gets the
//! repairs before its server first starts, and so does an upgrade's
//! preflight, which runs it under the staged binary.
//!
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
                //
                // First boot reads stdout for "`db-logins` could not finish"
                // to decide whether to retry (`virtues-firstboot.sh` §3), and
                // the installer forwards each "system repair `" line.
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
const REPAIRS: &[(&str, fn(&std::path::Path) -> Outcome)] = &[
    ("state-ownership", state_ownership),
    ("dragon-watchdog", dragon_watchdog),
    ("db-logins", db_logins),
];

/// The two low-privilege Postgres roles can log in, with the passwords the
/// server derives from this box's encryption key (`database::restricted`).
/// Model- and applet-written SQL runs on those logins, and a box refuses to
/// run that SQL without them.
///
/// Root is needed because the server's role has no CREATEROLE and so cannot
/// alter the roles itself. Checked by logging in over TCP the way the server
/// will, so a `pg_hba.conf` that refuses the login reports as Failed instead
/// of Converged. The installer and first boot run this after the key exists
/// and before the server first starts (see the module doc), and every upgrade
/// runs it again before the new server starts.
///
/// Debian's `pg_hba.conf` takes a password on 127.0.0.1. Fedora's uses
/// `ident` there, which these roles cannot pass because they have no OS user,
/// so when the login still fails after its password is set, the repair adds
/// [`LOGINS_TCP_LINE`] and checks again.
///
/// Sandboxed applet code has no network, so it logs in as the writer on the
/// Postgres socket instead. A box's `pg_hba.conf` maps socket logins to OS
/// users (`peer`), and the sandbox's throwaway user is no Postgres role, so
/// the repair adds [`WRITER_SOCKET_LINE`] for that and checks the login works.
#[cfg(target_os = "linux")]
fn db_logins(data_dir: &std::path::Path) -> Outcome {
    use crate::database::restricted::{alter_login_sql, password, Login};
    use std::io::Write;
    use std::process::{Command, Stdio};

    let Some(key) = crate::box_env::read(&data_dir.join("virtues.env"))
        .and_then(|mut env| env.remove("VIRTUES_ENCRYPTION_KEY"))
        .filter(|k| !k.is_empty())
    else {
        return Outcome::Failed("virtues.env has no VIRTUES_ENCRYPTION_KEY to derive the logins from".into());
    };
    // The password goes through the environment, never argv, which any user
    // can read in `ps`.
    let can_log_in = |login: Login| {
        Command::new("psql")
            .args(["-h", "127.0.0.1", "-U", login.role(), "-d", "virtues", "-w", "-tAc", "SELECT 1"])
            .env("PGPASSWORD", password(&key, login))
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    let as_postgres = |sql: &str| {
        Command::new("runuser")
            .args(["-u", "postgres", "--", "psql", "-d", "postgres", "-tAc", sql])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    // Put `line` in pg_hba.conf and reload, unless it is there already.
    // Returns the file, and whether this added the line.
    let add_hba_line = |line: &str, why: &str| -> Result<(std::path::PathBuf, bool), String> {
        let Some(hba) = as_postgres("SHOW hba_file").filter(|p| !p.is_empty()) else {
            return Err("could not ask Postgres where pg_hba.conf is".into());
        };
        let hba = std::path::PathBuf::from(hba);
        let text = std::fs::read_to_string(&hba).map_err(|e| format!("could not read {}: {e}", hba.display()))?;
        let Some(updated) = with_hba_line(&text, line, why) else {
            return Ok((hba, false));
        };
        replace_keeping_owner(&hba, &updated).map_err(|e| format!("could not write {}: {e}", hba.display()))?;
        if as_postgres("SELECT pg_reload_conf()").is_none() {
            return Err(format!("added `{line}` to {} but could not reload Postgres", hba.display()));
        }
        Ok((hba, true))
    };

    let mut done = Vec::new();
    let missing: Vec<Login> = Login::ALL.into_iter().filter(|l| !can_log_in(*l)).collect();
    for login in &missing {
        // The SQL goes on stdin for the same reason.
        let child = Command::new("runuser")
            .args(["-u", "postgres", "--", "psql", "-v", "ON_ERROR_STOP=1", "-d", "postgres", "-q"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => return Outcome::Failed(format!("could not run psql as postgres: {e}")),
        };
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(format!("{};\n", alter_login_sql(&key, *login)).as_bytes()) {
                return Outcome::Failed(format!("could not write to psql: {e}"));
            }
        }
        match child.wait_with_output() {
            Ok(o) if o.status.success() => {}
            Ok(o) => {
                return Outcome::Failed(format!(
                    "ALTER ROLE {} failed: {}",
                    login.role(),
                    String::from_utf8_lossy(&o.stderr).trim()
                ))
            }
            Err(e) => return Outcome::Failed(format!("psql as postgres did not finish: {e}")),
        }
    }
    if !missing.is_empty() {
        let roles = missing.iter().map(|l| l.role()).collect::<Vec<_>>().join(", ");
        done.push(format!("set the login for {roles}"));
        if !missing.iter().all(|l| can_log_in(*l)) {
            let hba = match add_hba_line(LOGINS_TCP_LINE, "chat and applet SQL logs in with a password on 127.0.0.1.") {
                Ok((hba, added)) => {
                    if added {
                        done.push(format!("let them log in with a password on 127.0.0.1 ({})", hba.display()));
                    }
                    hba
                }
                Err(why) => return Outcome::Failed(format!("set the login for {roles}, but {why}")),
            };
            if !missing.iter().all(|l| can_log_in(*l)) {
                return Outcome::Failed(format!(
                    "set the login for {roles}, but Postgres still refuses it on 127.0.0.1 (check {})",
                    hba.display()
                ));
            }
        }
    }

    let writer_on_socket = || {
        Command::new("psql")
            .arg(format!(
                "host={} user={} dbname=virtues",
                crate::database::restricted::SOCKET_DIR,
                Login::Writer.role()
            ))
            .args(["-w", "-tAc", "SELECT 1"])
            .env("PGPASSWORD", password(&key, Login::Writer))
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if !writer_on_socket() {
        let hba = match add_hba_line(WRITER_SOCKET_LINE, "sandboxed applet code logs in as the writer with its password.") {
            Ok((hba, added)) => {
                if added {
                    done.push(format!("let the applet writer log in on the socket ({})", hba.display()));
                }
                hba
            }
            Err(why) => return Outcome::Failed(why),
        };
        if !writer_on_socket() {
            return Outcome::Failed(format!(
                "Postgres refuses the applet writer on the socket (check {})",
                hba.display()
            ));
        }
    }

    if done.is_empty() {
        Outcome::Converged
    } else {
        Outcome::Repaired(done.join("; "))
    }
}

/// The `pg_hba.conf` line that lets sandboxed applet code log in as the
/// writer on the socket with its password.
#[cfg(any(target_os = "linux", test))]
const WRITER_SOCKET_LINE: &str = "local virtues virtues_applet_writer scram-sha-256";

/// The `pg_hba.conf` line that lets both restricted roles log in with their
/// passwords on 127.0.0.1, where the server connects them.
#[cfg(any(target_os = "linux", test))]
const LOGINS_TCP_LINE: &str = "host virtues virtues_face_reader,virtues_applet_writer 127.0.0.1/32 scram-sha-256";

/// `hba` with `line`, under a comment that says `why`, ahead of its first
/// rule, or `None` when it already has the line. Postgres takes the first
/// rule that matches, so the line has to come before the `peer` and `ident`
/// rules a box ships with.
#[cfg(any(target_os = "linux", test))]
fn with_hba_line(hba: &str, line: &str, why: &str) -> Option<String> {
    let has = hba.lines().any(|l| l.split_whitespace().collect::<Vec<_>>().join(" ") == line);
    if has {
        return None;
    }
    let block = format!("# Virtues: {why}\n{line}\n");
    let mut out = String::with_capacity(hba.len() + block.len());
    let mut placed = false;
    for existing in hba.split_inclusive('\n') {
        let t = existing.trim();
        if !placed && !t.is_empty() && !t.starts_with('#') {
            out.push_str(&block);
            placed = true;
        }
        out.push_str(existing);
    }
    if !placed {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&block);
    }
    Some(out)
}

/// Write `path` through a file beside it and a rename, with the original's
/// owner and mode, so a failed write cannot leave Postgres a half file.
#[cfg(target_os = "linux")]
fn replace_keeping_owner(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let meta = std::fs::metadata(path)?;
    let tmp = path.with_extension("virtues-new");
    std::fs::write(&tmp, contents)?;
    std::os::unix::fs::chown(&tmp, Some(meta.uid()), Some(meta.gid()))?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(meta.mode() & 0o7777))?;
    std::fs::rename(&tmp, path)
}

/// The systemd drop-in that arms a Dragon's hardware watchdog. The installer
/// writes the same bytes on a fresh install (its bring-up runs as `virtues`,
/// so this repair can't); uninstall removes it.
pub const WATCHDOG_DROPIN: &str = "/etc/systemd/system.conf.d/10-virtues-watchdog.conf";
pub const WATCHDOG_CONF: &str = include_str!("watchdog.conf");

/// Every Dragon has systemd pinging its hardware watchdog, so a hung kernel
/// reboots the box instead of waiting for someone to pull the plug.
///
/// Not hypothetical: on 2026-09-24 a context binary too large for the cDSP to
/// map crashed the cDSP, and this kernel's FastRPC driver (6.18.2-3-qcom) then
/// freed reserved DMA pages as the process exited. `BUG: Bad page state` sixty
/// times, and the box stopped answering on every interface. There was no OOM
/// and no panic, so nothing rebooted it until someone power-cycled it. `qnnd`
/// shares that cDSP, so every Dragon can hit it.
///
/// Keyed on the manifest's `profile == "dragon"`, not `appliance()`. The
/// profile is set whenever the box runs our NPU stack, appliance image or DIY,
/// while `appliance()` falls back to whether a display unit exists and reads
/// false on a headless Dragon. A server where we drive no NPU keeps its own
/// systemd settings. Skipped on a board with no `/dev/watchdog0`.
///
/// `daemon-reload` is enough to apply a `system.conf.d` change (checked on the
/// lab Dragon with `wdctl`); no re-exec. 30 s sits inside `qcom_wdt`'s 32 s
/// hardware maximum, and systemd pings at half that.
#[cfg(target_os = "linux")]
fn dragon_watchdog(_data_dir: &std::path::Path) -> Outcome {
    use std::path::Path;
    use std::process::Command;

    let is_dragon = crate::install_manifest::get()
        .as_ref()
        .is_some_and(|m| m.profile == "dragon");
    if !is_dragon || !Path::new("/dev/watchdog0").exists() {
        return Outcome::Converged;
    }
    if std::fs::read_to_string(WATCHDOG_DROPIN).ok().as_deref() == Some(WATCHDOG_CONF) {
        return Outcome::Converged;
    }
    if let Err(e) = std::fs::create_dir_all("/etc/systemd/system.conf.d") {
        return Outcome::Failed(format!("could not create system.conf.d: {e}"));
    }
    if let Err(e) = std::fs::write(WATCHDOG_DROPIN, WATCHDOG_CONF) {
        return Outcome::Failed(format!("could not write {WATCHDOG_DROPIN}: {e}"));
    }
    match Command::new("systemctl").arg("daemon-reload").status() {
        Ok(s) if s.success() => {
            Outcome::Repaired("hardware watchdog on: a hung box reboots itself within 30 s".into())
        }
        Ok(s) => Outcome::Failed(format!(
            "wrote {WATCHDOG_DROPIN} but daemon-reload exited {s}; it applies at next boot"
        )),
        Err(e) => Outcome::Failed(format!(
            "wrote {WATCHDOG_DROPIN} but could not run daemon-reload ({e}); it applies at next boot"
        )),
    }
}

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

    // Detect first, repair second, as two invocations: the log wants the list
    // of what was wrong, and threading a cap into the REPAIRING pass (a la
    // `| head`) lets SIGPIPE kill it after the cap — silently fixing a prefix
    // and leaving the rest, the same shape of half-truth this repair exists to
    // end. The full list is held in memory, which is fine: mass wrong
    // ownership is repaired at its sources (restore chowns after unpacking,
    // firstboot after seeding), so what reaches this sweep is drift, not bulk.
    let mut detect = Command::new("find");
    prunes(&mut detect);
    detect.arg("-print");
    let (found, detect_ok) = match detect.output() {
        Ok(o) => (
            String::from_utf8_lossy(&o.stdout).trim().to_string(),
            o.status.success(),
        ),
        Err(e) => return Outcome::Failed(format!("could not run find: {e}")),
    };
    if found.is_empty() {
        // Nothing found by a find that also FAILED is not convergence — e.g.
        // `-user virtues` hard-errors before traversal on a machine with no
        // such user. Reading that as "all good" is the swallowed-error shape
        // this repo keeps paying for.
        if !detect_ok {
            return Outcome::Failed("ownership scan failed before finding anything".to_string());
        }
        return Outcome::Converged;
    }

    // `-h`: `find ! -user` tests the symlink ITSELF, so repair the symlink
    // itself — plain chown would dereference and change the target's owner
    // while leaving the link that failed the test untouched.
    let mut repair = Command::new("find");
    prunes(&mut repair);
    repair.args(["-exec", "chown", "-h", "virtues:virtues", "{}", "+"]);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_writer_socket_line_goes_ahead_of_the_first_rule_once() {
        let hba = "# comment\n\nlocal   all   postgres   peer\nlocal all all peer\nhost all all 127.0.0.1/32 scram-sha-256\n";
        let updated = with_hba_line(hba, WRITER_SOCKET_LINE, "why").unwrap();
        let rules: Vec<&str> = updated.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).collect();
        assert_eq!(rules[0], WRITER_SOCKET_LINE);
        assert_eq!(rules[1], "local   all   postgres   peer");
        assert!(updated.starts_with("# comment\n"), "comments above the rules stay first");
        assert!(with_hba_line(&updated, WRITER_SOCKET_LINE, "why").is_none(), "added once");
        let spaced = updated.replace(WRITER_SOCKET_LINE, "local  virtues  virtues_applet_writer  scram-sha-256");
        assert!(with_hba_line(&spaced, WRITER_SOCKET_LINE, "why").is_none(), "spacing does not matter");
    }

    /// Fedora's `postgresql-setup --initdb` writes `ident` for 127.0.0.1,
    /// which a role with no OS user cannot pass. The password line has to
    /// come first, and adding the socket line later keeps both.
    #[test]
    fn the_tcp_line_goes_ahead_of_an_ident_rule() {
        let hba = "local all all peer\nhost all all 127.0.0.1/32 ident\nhost all all ::1/128 ident\n";
        let updated = with_hba_line(hba, LOGINS_TCP_LINE, "why").unwrap();
        let updated = with_hba_line(&updated, WRITER_SOCKET_LINE, "why").unwrap();
        let rules: Vec<&str> = updated.lines().filter(|l| !l.starts_with('#')).collect();
        let tcp = rules.iter().position(|l| *l == LOGINS_TCP_LINE).unwrap();
        let ident = rules.iter().position(|l| l.contains("127.0.0.1/32 ident")).unwrap();
        assert!(tcp < ident, "{updated}");
        assert!(rules.contains(&WRITER_SOCKET_LINE));
        assert!(with_hba_line(&updated, LOGINS_TCP_LINE, "why").is_none(), "added once");
    }

    /// The lines name the roles `database::restricted` connects as.
    #[test]
    fn the_hba_lines_name_the_restricted_roles() {
        use crate::database::restricted::Login;
        let roles = Login::ALL.iter().map(|l| l.role()).collect::<Vec<_>>().join(",");
        assert_eq!(LOGINS_TCP_LINE.split_whitespace().nth(2), Some(roles.as_str()));
        assert_eq!(WRITER_SOCKET_LINE.split_whitespace().nth(2), Some(Login::Writer.role()));
    }
}
