//! `virtues deprovision` — strip this box of every per-unit identity so its
//! disk can be imaged and cloned onto other boards.
//!
//! This is the LAST command run before a box ships (or before its boot medium
//! is `dd`'d to a master image). It is not a reset and not an uninstall: the
//! software stays installed and configured, only the things that must be
//! unique per unit are removed.
//!
//! **Why this exists.** A cloned image carries whatever identity the master
//! had. The box's iroh secret *is* its identity on the network, so two boxes
//! flashed from an un-deprovisioned master are literally the same box: the same
//! `EndpointId`, so a device paired to one can dial the other, and the relay
//! cannot tell them apart. The encryption key is the same story for data at
//! rest — one leaked master key would decrypt every unit ever shipped. Neither
//! failure is visible on the bench; both are catastrophic and unfixable in the
//! field, which is why this runs unconditionally before imaging rather than
//! being a step someone remembers.
//!
//! **What it removes**, and where each lives:
//!   * the whole database — `box_secrets` holds the iroh secret, `app_device`
//!     the pairings, `credentials` the source tokens (delegated to `reset`)
//!   * the data lake
//!   * `VIRTUES_ENCRYPTION_KEY` from the env file — re-minted per unit on
//!     first boot, NOT here, because a key minted here would be baked into
//!     the image and shared by every clone
//!   * `/etc/machine-id` — systemd regenerates it on next boot; a shared one
//!     collides in DHCP and journald
//!   * SSH host keys — sshd regenerates them; a shared host key means every
//!     unit is trivially impersonable
//!   * saved wifi — otherwise the master's credentials ship to every customer.
//!     In BOTH places they live: NetworkManager's own profiles, and the netplan
//!     YAML that Ubuntu actually stores them in (see `remove_netplan_wifi`)
//!   * chat-authored applets, code (state root) and data (`applet_*` schemas)
//!     alike — the owner's own writing, which would otherwise be cloned onto
//!     every unit built from this master
//!   * the journal
//!
//! **What it keeps:** the installed binary, the systemd units, the env file's
//! non-secret keys, the models, and the QAIRT libs. That is the whole point —
//! a deprovisioned box is one boot away from working, and that boot is the
//! customer's.
//!
//! Handled in `main.rs` against a bare pool, like `reset`/`restore`/`uninstall`.

use std::path::Path;
use std::process::Command;

/// Env keys that are per-unit secrets and must never survive into an image.
const PER_UNIT_ENV_KEYS: &[&str] = &["VIRTUES_ENCRYPTION_KEY"];

/// The box's env file — the same path `main.rs` loads at startup and the
/// installer writes. `VIRTUES_ENV_FILE` overrides it so the tests (and a dev
/// box with a different layout) don't have to touch `/var/lib`.
pub fn env_file_path() -> std::path::PathBuf {
    std::env::var("VIRTUES_ENV_FILE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/var/lib/virtues/virtues.env"))
}

pub async fn run(yes: bool, force: bool) -> Result<(), crate::Error> {
    let env_path = env_file_path();

    println!();
    println!("⚠  virtues deprovision — prepares this box to be imaged and cloned.");
    println!("   REMOVES, so each clone mints its own:");
    println!("     • the entire database — iroh identity, pairings, credentials");
    println!("     • the data lake");
    println!("     • VIRTUES_ENCRYPTION_KEY (re-minted on first boot)");
    println!("     • machine-id, SSH host keys, saved wifi networks");
    println!("     • chat-authored applets (their code AND their data)");
    println!("   KEEPS: the binary, systemd units, models, QAIRT libs.");
    println!();
    // When the network actually drops depends on where the profile lived, and
    // saying "immediately" was wrong on the one box this has ever run on. On
    // Ubuntu the credentials are netplan's, and NetworkManager runs from a copy
    // in /run — tmpfs — so the link stays up until the reboot that does not
    // bring it back. Being precise here is not pedantry: an operator who
    // believes the session is about to drop will not start, and one who
    // believes it is safe will reboot and lose the box.
    println!("   Saved wifi is removed. If the link survives this command it is");
    println!("   running from RAM — it will NOT come back after a reboot. Have a");
    println!("   console to hand, or use a wired session.");
    println!();

    if !yes {
        let host = hostname();
        let typed: String = dialoguer::Input::new()
            .with_prompt(format!("Type this box's hostname ('{host}') to confirm"))
            .allow_empty(true)
            .interact_text()
            .unwrap_or_default();
        if typed.trim() != host {
            println!("Hostname mismatch — aborting. Nothing was changed.");
            return Ok(());
        }
    }

    // ── 1. Database + lake ──────────────────────────────────────────────────
    // `reset` already drops every app object (taking `box_secrets`, and with it
    // the iroh secret) and clears the lake. Reuse it rather than restating the
    // schema surgery, which has real subtleties about not dropping the `vector`
    // extension. `yes = true` because we just took our own confirmation.
    //
    // **As the `virtues` user, in a subprocess, not in-process.** Box installs
    // talk to Postgres over the Unix socket with peer auth, so the OS user IS
    // the database identity — and this command must run as root, because
    // everything after it writes `/etc/machine-id`, `/etc/ssh` and
    // `/etc/netplan`. Called in-process it therefore tried to connect as role
    // `root`, which does not exist, and deprovision died at its first step.
    //
    // Which means the documented way to build a master — `sudo sh
    // tools/build-master.sh` — could not complete, and had never been run on
    // hardware to discover that. `maybe_reexec_as_service_user` in main.rs
    // handles this for `reset` and friends by becoming the service user for
    // the whole command; that is the wrong shape here, because the root half
    // is not optional. So only the database half drops privilege.
    println!("→ wiping database + data lake…");
    wipe_database_as_service_user(force).await?;

    // ── 2. The per-unit env secrets ─────────────────────────────────────────
    // Removed, never regenerated here: a key minted on the master is baked into
    // the image and shared by every clone, which is the exact failure this
    // command exists to prevent. First boot mints it.
    strip_env_keys(&env_path)?;

    // ── 3. Logs — BEFORE machine-id, and this order is load-bearing ─────────
    // journald names its directory after the machine-id
    // (`/var/log/journal/<machine-id>/`) and journalctl resolves the local
    // journal through it. Clear machine-id first and journalctl answers "No
    // journal files were found", `--vacuum-time` frees 0 B, and 520 MB of the
    // master's history stays on the card — which is exactly what it did, while
    // printing "✓ journal vacuumed", because the result was discarded.
    //
    // Both halves of that were bugs. The order is fixed here; the discarding is
    // fixed by `image-check` now measuring the directory afterwards, which is
    // what caught this on its first run.
    vacuum_journal();

    // ── 4. Host identity ────────────────────────────────────────────────────
    // Truncate rather than delete machine-id: systemd treats an empty file as
    // "first boot" and populates it, whereas a *missing* file makes some
    // early-boot units fail outright.
    if Path::new("/etc/machine-id").exists() {
        std::fs::write("/etc/machine-id", b"")
            .map_err(|e| crate::Error::Other(format!("truncate machine-id: {e}")))?;
        println!("  ✓ machine-id cleared");
    }
    let _ = std::fs::remove_file("/var/lib/dbus/machine-id");

    remove_glob("/etc/ssh", "ssh_host_", "SSH host keys")?;

    // Saved wifi — the master's credentials must not ship. This is what takes
    // the box off the network, so it is deliberately last among the wipes.
    remove_glob(
        "/etc/NetworkManager/system-connections",
        "",
        "saved network connections",
    )?;

    // …and the same credentials in the OTHER place they live, which this
    // command did not know about until a box was inspected rather than
    // reasoned about.
    //
    // On Ubuntu — which is what the Dragon runs — NetworkManager is a netplan
    // *renderer*, not the system of record. Join a network from the desktop and
    // netplan writes `/etc/netplan/90-NM-<uuid>.yaml` holding the SSID, the
    // password, and for an enterprise network the 802.1X `identity` too; NM's
    // own copy under `/etc/NetworkManager/system-connections` is never created,
    // and the one it runs from lives in `/run`, which is tmpfs.
    //
    // So the wipe above found an empty directory and reported success, and the
    // workshop's wifi password — a corporate 802.1X account, in our case —
    // would have been readable in the plain text of every unit shipped.
    // `image-check` cleared the box too, for the same reason: it looked in the
    // same empty place.
    //
    // Files carrying `access-points:` only. A netplan YAML may also describe
    // ethernet, which is generic and must survive; wifi stanzas are the ones
    // that carry secrets. (NM writes one connection per file, so in practice
    // the distinction is clean.) Nothing is re-applied afterwards: the live
    // connection in /run is deliberately left up so a remote operator keeps
    // their session, and tmpfs means it never reaches the image anyway.
    remove_netplan_wifi()?;

    // Network *history* rather than secrets: DHCP leases and seen-BSSID lists
    // name every network this master ever touched, with MACs. Not a credential
    // leak, but it is the workshop's map, and it ships with the card.
    remove_glob("/var/lib/NetworkManager", "", "network state files")?;

    // ── 3a. Authored applets ────────────────────────────────────────────────
    // Chat-authored applets are per-box runtime state and live in the STATE
    // ROOT, never in the shipped `applets/` tree (see CLAUDE.md). Which means
    // nothing above touched them: `reset` wipes the database, and these are
    // source files on disk.
    //
    // They are the owner's own writing — three of them on this machine, one a
    // weekly planner with another person's name in the slug — and they would
    // have been cloned onto every unit built from this master. The applet
    // schemas that hold their DATA are handled in `reset`; this is their CODE.
    remove_authored_applets();

    // ── 3b. The relocated Postgres cluster ──────────────────────────────────
    // On an appliance the cluster lives on the data disk and `/var/lib/postgresql`
    // is a symlink to it. That whole tree is per-unit state — it is where the
    // record lived — so it goes, and `virtues-firstboot` builds a fresh one on
    // each unit's own disk.
    //
    // Removed rather than left because of where the image comes from: the card
    // is imaged, and on the master the data dir is a plain directory ON the
    // card (the NVMe is claimed at first boot, which the master never had). So
    // a surviving cluster would ship inside every image — the master's
    // `postgres` superuser, its catalogs, its size — under a path each unit
    // then hides with a mount and never reads.
    remove_relocated_cluster();

    // Login records and shell history — the operator's own traces. `wtmp` and
    // `lastlog` are truncated rather than removed: the files are expected to
    // exist and login(1) writes to them either way.
    for f in ["/var/log/wtmp", "/var/log/btmp", "/var/log/lastlog"] {
        if Path::new(f).exists() {
            let _ = std::fs::write(f, b"");
        }
    }
    remove_shell_history();

    // ── 5. Arm first boot ───────────────────────────────────────────────────
    // The marker is what licenses `virtues-firstboot` to mint a NEW encryption
    // key. Without it, a box whose key went missing for any other reason (a
    // botched edit, a half-restored backup) would silently get a fresh one —
    // and every credential encrypted under the old key becomes undecryptable
    // with no error, because the ciphertext is still there and still parses.
    // Minting must therefore be something deprovision explicitly asked for,
    // never a repair that happens on its own.
    write_firstboot_marker()?;

    println!();
    println!("✓ deprovisioned — safe to image.");
    println!("  Power off WITHOUT booting again (a boot re-mints machine-id and");
    println!("  host keys, which then get baked into the image):");
    println!();
    println!("    sudo poweroff");
    println!();
    println!("  On first boot each unit mints its own encryption key, identity,");
    println!("  and host keys via the first-boot unit.");
    Ok(())
}

/// Where chat-authored applets live: the state root, never the shipped tree.
///
/// Public so `image_check` can ask the same question — a check that looks
/// somewhere else than the thing that cleans is a check that agrees with itself
/// and nothing else.
pub fn authored_applets_dir() -> std::path::PathBuf {
    // `var` returns Result, so the fallback closure takes the error.
    std::env::var("VIRTUES_APPLET_STATE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            env_file_path()
                .parent()
                .unwrap_or(Path::new("/var/lib/virtues"))
                .join("applets")
        })
        .join("user")
}

/// Delete every authored applet. Best-effort; absent on a box where nobody
/// wrote one.
fn remove_authored_applets() {
    let dir = authored_applets_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut n = 0usize;
    for e in entries.flatten() {
        let p = e.path();
        let ok = if p.is_dir() {
            std::fs::remove_dir_all(&p).is_ok()
        } else {
            std::fs::remove_file(&p).is_ok()
        };
        if ok {
            n += 1;
        }
    }
    if n > 0 {
        println!("  ✓ removed {n} authored applet(s) from {}", dir.display());
    }
}

/// `/var/lib/postgresql`, which on a relocated appliance is a symlink into the
/// data dir. Named here so `image_check` can ask the same question.
pub const PG_LINK: &str = "/var/lib/postgresql";

/// The relocated cluster directory, if this box has one.
///
/// `None` on a DIY box, where `/var/lib/postgresql` is a real directory owned
/// by the distro and emphatically not ours to delete.
pub fn relocated_cluster_dir() -> Option<std::path::PathBuf> {
    let md = std::fs::symlink_metadata(PG_LINK).ok()?;
    if !md.file_type().is_symlink() {
        return None;
    }
    std::fs::read_link(PG_LINK).ok()
}

/// Delete the relocated cluster, stopping Postgres first.
///
/// Best-effort and silent on a DIY box. Stopping first is not politeness: the
/// postmaster holds the data directory open and writes to it continuously, so
/// removing it underneath a running server produces a half-deleted cluster and
/// a process that will not exit cleanly.
fn remove_relocated_cluster() {
    let Some(dir) = relocated_cluster_dir() else {
        return;
    };
    // `postgresql@<ver>-main` is `PartOf=postgresql.service`, so stopping the
    // wrapper propagates to every instance.
    let _ = Command::new("systemctl").args(["stop", "postgresql"]).output();
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => println!("  ✓ removed the Postgres cluster at {}", dir.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => println!("  ⚠  could not remove {}: {e}", dir.display()),
    }

    // And the copy the relocation left behind. On a running box that directory
    // is a rollback — the cluster as it was before it moved to the data disk,
    // kept deliberately so a botched move can be undone. At imaging time the
    // rollback is meaningless (the cluster it would restore has just been
    // deleted) and what remains is simply the master's whole database, sitting
    // on the boot card, about to be `dd`'d onto every unit.
    //
    // image-check flagged it with a manual `rm -rf` and that was the wrong
    // division of labour: this is per-unit data, deprovision removes per-unit
    // data, and a step left to a human at the end of a long procedure is a step
    // that gets skipped.
    match std::fs::remove_dir_all(PG_PRE_MOVE) {
        Ok(()) => println!("  ✓ removed the pre-move Postgres copy at {PG_PRE_MOVE}"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => println!("  ⚠  could not remove {PG_PRE_MOVE}: {e}"),
    }
}

/// The rollback copy `relocate_postgres_to_data_dir` leaves on the boot disk.
/// Shared with `image-check`, which verifies this removal happened.
pub const PG_PRE_MOVE: &str = "/var/lib/postgresql.pre-move";

/// Path of the marker that licenses `virtues-firstboot` to mint a fresh
/// encryption key. Lives beside the env file so it travels with the image.
pub fn firstboot_marker_path() -> std::path::PathBuf {
    env_file_path()
        .parent()
        .unwrap_or(Path::new("/var/lib/virtues"))
        .join(".needs-firstboot")
}

/// Arm first boot. Written last, so a deprovision that died partway through
/// does not leave a box licensed to rotate a key it still needs.
fn write_firstboot_marker() -> Result<(), crate::Error> {
    let p = firstboot_marker_path();
    std::fs::write(
        &p,
        "# Written by `virtues deprovision`.\n\
         # Licenses virtues-firstboot to mint a per-unit encryption key on the\n\
         # next boot, then delete this file. Do NOT create it by hand on a box\n\
         # that holds data — minting a new key makes existing credentials\n\
         # undecryptable, silently.\n",
    )
    .map_err(|e| crate::Error::Other(format!("write {}: {e}", p.display())))?;
    println!("  ✓ armed first boot ({})", p.display());
    Ok(())
}

/// Drop the per-unit secret lines from the env file, leaving everything else
/// (DATABASE_URL, STATIC_DIR, the inference config…) exactly as written.
fn strip_env_keys(env_path: &Path) -> Result<(), crate::Error> {
    if !env_path.exists() {
        println!("  · no env file at {} — nothing to strip", env_path.display());
        return Ok(());
    }
    let body = std::fs::read_to_string(env_path)
        .map_err(|e| crate::Error::Other(format!("read {}: {e}", env_path.display())))?;

    let mut removed = 0usize;
    let kept: Vec<&str> = body
        .lines()
        .filter(|line| {
            let is_secret = PER_UNIT_ENV_KEYS
                .iter()
                .any(|k| line.trim_start().starts_with(&format!("{k}=")));
            if is_secret {
                removed += 1;
            }
            !is_secret
        })
        .collect();

    if removed == 0 {
        println!("  · env file already has no per-unit secrets");
        return Ok(());
    }

    let mut out = kept.join("\n");
    out.push('\n');
    std::fs::write(env_path, out)
        .map_err(|e| crate::Error::Other(format!("write {}: {e}", env_path.display())))?;
    println!("  ✓ removed {removed} per-unit secret(s) from {}", env_path.display());
    Ok(())
}

/// Netplan YAMLs that describe a wifi network, and therefore carry its
/// password. Shared with `image-check`, which must look exactly where this
/// looks — the two disagreeing is how the credentials survived in the first
/// place.
///
/// Detection is `access-points:`, the netplan key under which every wifi
/// stanza (and only a wifi stanza) hangs. Returned rather than deleted so the
/// read-only checker can use the same function.
pub fn netplan_wifi_files() -> Vec<std::path::PathBuf> {
    netplan_wifi_files_in("/etc/netplan")
}

fn netplan_wifi_files_in(dir: &str) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_yaml = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "yaml" || e == "yml");
        if !is_yaml {
            continue;
        }
        // Read failures are skipped, not fatal: a YAML this process cannot read
        // is one it also cannot delete, and image-check reports what remains.
        if let Ok(text) = std::fs::read_to_string(&path) {
            if text.contains("access-points:") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Run `virtues reset` as whoever owns the database, and fail loudly if it did
/// not work.
///
/// On a box that is `sudo -u virtues`; run as `virtues` already, or on a dev
/// machine with no box-install marker, it is this process's own reset. The
/// marker is the same one `maybe_reexec_as_service_user` keys on, so the two
/// agree about what "a box" means.
///
/// The exit status is checked. Deprovision's whole contract is that after it
/// returns, nothing per-unit is left — a database wipe that silently failed
/// would leave the iroh secret and every credential in an image about to be
/// cloned, which is precisely the catastrophe the command exists to prevent.
async fn wipe_database_as_service_user(force: bool) -> Result<(), crate::Error> {
    let on_box = Path::new("/var/lib/virtues/virtues.env").exists();
    let me = Command::new("id")
        .arg("-un")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    if !on_box || me == "virtues" {
        // Same process, current identity — dev machines and the service user.
        return crate::cli::reset::run(false, true, force).await;
    }

    let exe = std::env::current_exe()
        .map_err(|e| crate::Error::Other(format!("locate own binary: {e}")))?;
    let mut cmd = Command::new("sudo");
    cmd.arg("-u").arg("virtues").arg(exe).arg("reset").arg("--yes");
    if force {
        cmd.arg("--force");
    }
    let status = cmd
        .status()
        .map_err(|e| crate::Error::Other(format!("run reset as virtues: {e}")))?;
    if !status.success() {
        return Err(crate::Error::Other(
            "database wipe failed — NOT safe to image. The box identity (iroh \
             secret, credentials) is still in the database."
                .to_string(),
        ));
    }
    Ok(())
}

/// Drop the journal: 403 MB reaching back to 2025-11-25 across 22 boots on the
/// lab board — the master's entire operational history, shipped inside every
/// image. Not the setup phrase, which is logged as "setup phrase rejected" and
/// never with the words, but the networks, addresses, hostnames and traces.
///
/// Two steps, because neither is sufficient alone. `journalctl --rotate` is the
/// graceful half: it makes journald close the active file so the data can go.
/// But vacuuming only ever removes *archived* files, and journald immediately
/// opens a new active one — so on its own it always leaves something. The
/// explicit removal afterwards is what makes the result deterministic, which
/// matters here because the next thing to touch this card is `dd`.
///
/// Removing files journald holds open is safe: it keeps writing to the unlinked
/// inode until it restarts, the space is freed, and nothing reaches the image.
/// Each unit mints its own machine-id at first boot and gets a fresh directory.
fn vacuum_journal() {
    let _ = Command::new("journalctl")
        .args(["--rotate", "--vacuum-time=1s"])
        .output();

    // Per-machine subdirectories, not /var/log/journal itself — journald wants
    // the parent to exist and to keep its ownership and setgid bit.
    if let Ok(entries) = std::fs::read_dir("/var/log/journal") {
        for entry in entries.flatten() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    println!("  ✓ journal cleared");
}

/// Shell history for root and every real user. Whoever built the master typed
/// their way through it, and a command line is where secrets get typed by
/// accident — a password passed as an argument, a token pasted into a curl.
fn remove_shell_history() {
    let mut homes = vec![std::path::PathBuf::from("/root")];
    if let Ok(entries) = std::fs::read_dir("/home") {
        homes.extend(entries.flatten().map(|e| e.path()));
    }
    let mut n = 0usize;
    for home in homes {
        for name in [".bash_history", ".zsh_history", ".python_history", ".psql_history"] {
            if std::fs::remove_file(home.join(name)).is_ok() {
                n += 1;
            }
        }
    }
    if n > 0 {
        println!("  ✓ removed {n} shell history file(s)");
    }
}

fn remove_netplan_wifi() -> Result<(), crate::Error> {
    let files = netplan_wifi_files();
    if files.is_empty() {
        return Ok(());
    }
    let mut n = 0usize;
    for path in &files {
        if std::fs::remove_file(path).is_ok() {
            n += 1;
        }
    }
    if n > 0 {
        println!("  ✓ removed {n} netplan wifi config(s) (SSID + password)");
    }
    Ok(())
}

/// Remove every entry in `dir` whose file name starts with `prefix` (empty
/// prefix = everything). Missing directory is fine — not every box has sshd or
/// NetworkManager.
fn remove_glob(dir: &str, prefix: &str, label: &str) -> Result<(), crate::Error> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    let mut n = 0usize;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if prefix.is_empty() || name.starts_with(prefix) {
            let path = entry.path();
            let ok = if path.is_dir() {
                std::fs::remove_dir_all(&path).is_ok()
            } else {
                std::fs::remove_file(&path).is_ok()
            };
            if ok {
                n += 1;
            }
        }
    }
    if n > 0 {
        println!("  ✓ removed {n} {label}");
    }
    Ok(())
}

fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "virtues".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_only_the_secret_line() {
        let dir = std::env::temp_dir().join(format!("depro-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("virtues.env");
        std::fs::write(
            &p,
            "DATABASE_URL=postgres:///virtues\n\
             VIRTUES_ENCRYPTION_KEY=c2VjcmV0\n\
             ENVIRONMENT=production\n",
        )
        .unwrap();

        strip_env_keys(&p).unwrap();

        let out = std::fs::read_to_string(&p).unwrap();
        assert!(!out.contains("VIRTUES_ENCRYPTION_KEY"), "secret survived: {out}");
        assert!(out.contains("DATABASE_URL=postgres:///virtues"));
        assert!(out.contains("ENVIRONMENT=production"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The wifi YAML is found and the ethernet one is not — the whole point of
    /// matching on `access-points:` rather than deleting `/etc/netplan/*`. A
    /// unit whose ethernet config was removed with the wifi would ship unable
    /// to come up on a wired network, which is the one network the first-boot
    /// path is allowed to assume.
    ///
    /// The wifi fixture is shaped like what netplan actually wrote on the lab
    /// board: an 802.1X stanza, because the workshop network is corporate and
    /// that is the case where the leak costs an account rather than a PSK.
    #[test]
    fn finds_wifi_yaml_and_spares_ethernet() {
        let dir = std::env::temp_dir().join(format!("netplan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("90-NM-wifi.yaml"),
            "network:\n  wifis:\n    wlan0:\n      access-points:\n        \"workshop\":\n          auth:\n            key-management: eap\n            identity: \"someone@example.com\"\n            password: \"hunter2\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("01-ethernet.yaml"),
            "network:\n  ethernets:\n    eth0:\n      dhcp4: true\n",
        )
        .unwrap();
        // A non-YAML neighbour must be ignored rather than read as config.
        std::fs::write(dir.join("README"), "access-points: not a netplan file\n").unwrap();

        let found = netplan_wifi_files_in(dir.to_str().unwrap());

        assert_eq!(found.len(), 1, "expected only the wifi YAML, got {found:?}");
        assert!(found[0].ends_with("90-NM-wifi.yaml"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A box with no netplan directory at all (DIY on a distro that does not
    /// use it) is not a finding. `image-check` calls this on every box.
    #[test]
    fn missing_netplan_dir_is_not_a_finding() {
        assert!(netplan_wifi_files_in("/nonexistent/netplan").is_empty());
    }

    #[test]
    fn strip_is_idempotent_on_a_clean_file() {
        let dir = std::env::temp_dir().join(format!("depro2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("virtues.env");
        std::fs::write(&p, "DATABASE_URL=postgres:///virtues\n").unwrap();
        strip_env_keys(&p).unwrap();
        strip_env_keys(&p).unwrap();
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "DATABASE_URL=postgres:///virtues\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
