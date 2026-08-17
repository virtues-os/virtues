//! `virtues image-check` — is this disk safe to clone?
//!
//! The gate between [`crate::cli::deprovision`] and `dd`, and the reason it is
//! a separate command is that deprovision cannot be its own witness. It prints
//! "✓ deprovisioned — safe to image", and until this existed that sentence was
//! the entire assurance: nothing re-read the disk afterwards, and nothing
//! stopped an operator imaging a box that had been *booted* since (a boot
//! re-mints machine-id and SSH host keys, which then travel into every clone).
//!
//! ## What a survived secret costs
//!
//! Every finding here is invisible on the bench and unfixable in the field.
//!
//! * **The iroh secret is the box's identity.** Two units flashed from a master
//!   that still had one are not similar boxes; they are the same box. A device
//!   paired to one dials the other, and the relay cannot tell them apart.
//! * **One encryption key decrypts every unit ever shipped.** It protects
//!   stored credentials, and a master's key baked into an image is a single
//!   key for the whole fleet.
//! * **A shared machine-id** collides in DHCP and journald; **shared SSH host
//!   keys** make every unit trivially impersonable.
//! * **Saved wifi** ships the workshop's network password to customers.
//!
//! ## Read-only, on purpose
//!
//! This never fixes anything. A check that repairs what it finds is a check
//! that can be run once, pass, and tell you nothing about the disk you are
//! actually about to clone — and the fix (`deprovision`) is destructive enough
//! that it must stay an explicit act. Findings print the command that resolves
//! them.

use std::path::Path;

use super::ui;

/// One thing that must not be true of a disk about to be cloned.
struct Finding {
    what: &'static str,
    detail: String,
    fix: &'static str,
}

pub async fn run() -> i32 {
    ui::section("Image check");
    println!();

    let mut findings: Vec<Finding> = Vec::new();

    // ── Per-unit secrets in the env file ────────────────────────────────────
    let env_path = crate::cli::deprovision::env_file_path();
    if let Ok(body) = std::fs::read_to_string(&env_path) {
        for key in ["VIRTUES_ENCRYPTION_KEY"] {
            if body
                .lines()
                .any(|l| l.trim_start().starts_with(&format!("{key}=")))
            {
                findings.push(Finding {
                    what: "per-unit secret",
                    detail: format!("{key} is still in {}", env_path.display()),
                    fix: "sudo virtues deprovision",
                });
            }
        }
    }

    // ── The first-boot marker ───────────────────────────────────────────────
    // Its ABSENCE is the finding. The marker is what licenses
    // `virtues-firstboot` to mint a fresh encryption key on each unit; without
    // it, every clone boots with no key and no permission to make one.
    let marker = crate::cli::deprovision::firstboot_marker_path();
    if !marker.exists() {
        findings.push(Finding {
            what: "first boot not armed",
            detail: format!("{} is missing — clones would boot with no encryption key and no licence to mint one", marker.display()),
            fix: "sudo virtues deprovision",
        });
    }

    // ── Host identity ───────────────────────────────────────────────────────
    // A non-empty machine-id means this box has BOOTED since it was
    // deprovisioned. That is the failure deprovision's own closing note warns
    // about and cannot itself prevent, because it happens after it exits.
    if let Ok(id) = std::fs::read_to_string("/etc/machine-id") {
        if !id.trim().is_empty() {
            findings.push(Finding {
                what: "machine-id present",
                detail: "this box has booted since it was deprovisioned — machine-id and SSH host keys have been re-minted and would be baked into the image".to_string(),
                fix: "sudo virtues deprovision   (then power off WITHOUT booting again)",
            });
        }
    }

    if let Some(n) = count_prefixed("/etc/ssh", "ssh_host_") {
        if n > 0 {
            findings.push(Finding {
                what: "SSH host keys",
                detail: format!("{n} key file(s) in /etc/ssh — every clone would be impersonable as every other"),
                fix: "sudo virtues deprovision",
            });
        }
    }

    // ── The workshop's own network ──────────────────────────────────────────
    if let Some(n) = count_prefixed("/etc/NetworkManager/system-connections", "") {
        if n > 0 {
            findings.push(Finding {
                what: "saved wifi",
                detail: format!("{n} saved connection(s) — the workshop's network password would ship to customers"),
                fix: "sudo virtues deprovision",
            });
        }
    }

    // ── A leftover pre-move copy of the cluster ─────────────────────────────
    // `relocate_postgres_to_data_dir` keeps the original as a rollback and
    // tells the operator to remove it. On a box that is a rollback; inside an
    // image it is the master's whole database, shipped to every customer.
    if Path::new(PG_PRE_MOVE).exists() {
        findings.push(Finding {
            what: "pre-move Postgres copy",
            detail: format!("{PG_PRE_MOVE} is still on the boot disk — this is the cluster from before it was relocated, and it would ship inside the image"),
            fix: "rm -rf /var/lib/postgresql.pre-move",
        });
    }

    // ── The database ────────────────────────────────────────────────────────
    // `box_secrets` holds the iroh secret. Checked by looking rather than by
    // trusting that deprovision ran: this is the single most expensive thing
    // to get wrong, and looking again is the whole point of a separate command.
    //
    // Two ways to pass, and the first is the stronger one. On a relocated
    // appliance, deprovision removes the CLUSTER, not just the database — so
    // there is no server left to ask, and "Postgres is unreachable" is the
    // expected, correct end state rather than a failure to check. Asking first
    // whether the cluster exists is what tells those two apart.
    match cluster_state() {
        ClusterState::Absent => {
            ui::ok("no Postgres cluster on this disk — nothing to leak");
        }
        ClusterState::Present => match applet_schemas_present().await {
            Some(n) if n > 0 => findings.push(Finding {
                what: "applet schemas",
                detail: format!("{n} `applet_*` schema(s) still in the database — an authored applet's DATA, which a wipe scoped to `public` does not touch"),
                fix: "sudo virtues deprovision",
            }),
            _ => {}
        },
    }
    match cluster_state() {
        ClusterState::Absent => {}
        ClusterState::Present => match database_is_present().await {
            Some(true) => findings.push(Finding {
                what: "database still exists",
                detail: "the `virtues` database is present — box_secrets holds the iroh secret, which IS this box's network identity. Clones would all be the same box.".to_string(),
                fix: "sudo virtues deprovision",
            }),
            Some(false) => {}
            // A cluster exists but will not answer. Unlike the Absent case
            // above, this genuinely is "I could not check the most important
            // thing", and that must never render as a tick.
            None => findings.push(Finding {
                what: "database unreadable",
                detail: "a Postgres cluster exists here but would not answer — this check cannot pass without knowing whether the `virtues` database is in it".to_string(),
                fix: "start postgresql, then re-run",
            }),
        },
    }

    // ── Chat-authored applets ───────────────────────────────────────────────
    // The owner's own writing, and the finding that prompted this check to
    // exist at all: `reset` wiped only the `public` schema, so applet DATA
    // survived a deprovision, and nothing ever removed their CODE from the
    // state root. Three of them were sitting on the box that would have been
    // the first master — a calorie diary, a weekly planner, a readings log.
    //
    // Checked here as well as fixed there, because a cleaner nobody audits is
    // how it silently stops working: someone adds a fourth place applets can
    // live, and only this notices.
    if let Some(n) = count_prefixed(
        &crate::cli::deprovision::authored_applets_dir().display().to_string(),
        "",
    ) {
        if n > 0 {
            findings.push(Finding {
                what: "authored applets",
                detail: format!("{n} chat-authored applet(s) still in the state root — this is the owner's own writing, and it would be cloned onto every unit"),
                fix: "sudo virtues deprovision",
            });
        }
    }

    // ── The lake ────────────────────────────────────────────────────────────
    let lake = crate::cli::deprovision::env_file_path()
        .parent()
        .unwrap_or(Path::new("/var/lib/virtues"))
        .join("lake");
    if let Some(n) = count_prefixed(&lake.display().to_string(), "") {
        if n > 0 {
            findings.push(Finding {
                what: "data lake not empty",
                detail: format!("{n} entr(y/ies) under {}", lake.display()),
                fix: "sudo virtues deprovision",
            });
        }
    }

    println!();
    if findings.is_empty() {
        ui::ok("No per-unit identity found. This disk is safe to image.");
        println!();
        println!("  Power off WITHOUT booting again — a boot re-mints machine-id and");
        println!("  host keys, and this check would then fail:");
        println!();
        println!("    sudo poweroff");
        println!();
        return 0;
    }

    for f in &findings {
        ui::err(&format!("{}: {}", f.what, f.detail));
        println!("      fix: {}", f.fix);
    }
    println!();
    ui::err(&format!(
        "{} finding(s) — DO NOT image this disk.",
        findings.len()
    ));
    println!();
    1
}

/// The pre-move copy `relocate_postgres_to_data_dir` leaves behind.
const PG_PRE_MOVE: &str = "/var/lib/postgresql.pre-move";

enum ClusterState {
    /// A cluster directory with a `PG_VERSION` in it.
    Present,
    /// Nothing to ask, and on a deprovisioned appliance that is the goal.
    Absent,
}

/// Is there a Postgres cluster on this disk at all?
///
/// Looks for `PG_VERSION`, the file `initdb` writes and every Postgres tool
/// treats as "a cluster lives here" — rather than for the directory, which
/// exists as an empty shell after a `remove_dir_all` race or a fresh `mkdir`.
///
/// Covers both layouts: a relocated appliance (`/var/lib/postgresql` is a
/// symlink into the data dir) and a DIY box (it is a real directory), because
/// the path resolves the same way from here either way.
fn cluster_state() -> ClusterState {
    cluster_state_of(Path::new(crate::cli::deprovision::PG_LINK))
}

fn cluster_state_of(root: &Path) -> ClusterState {
    let Ok(entries) = std::fs::read_dir(root) else {
        // Missing, or a symlink into a data dir that is not mounted. Both mean
        // no cluster is reachable on this disk.
        return ClusterState::Absent;
    };
    for e in entries.flatten() {
        // `<root>/<major>/<cluster>/PG_VERSION`, e.g. `18/main/PG_VERSION`.
        if let Ok(inner) = std::fs::read_dir(e.path()) {
            for c in inner.flatten() {
                if c.path().join("PG_VERSION").exists() {
                    return ClusterState::Present;
                }
            }
        }
    }
    ClusterState::Absent
}

/// How many `applet_*` schemas are left?
///
/// `None` when Postgres cannot be asked, which the caller already treats as its
/// own finding via the database check — no need to double-report it.
async fn applet_schemas_present() -> Option<i64> {
    let out = std::process::Command::new("sudo")
        .args([
            "-u",
            "postgres",
            "psql",
            "-d",
            "virtues",
            "-tAc",
            // Underscore escaped: unescaped it is a single-character wildcard.
            r"SELECT count(*) FROM pg_namespace WHERE nspname LIKE 'applet\_%'",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// Does the `virtues` database exist?
///
/// `None` means we could not find out, which this command treats as a finding
/// rather than a pass.
async fn database_is_present() -> Option<bool> {
    let out = std::process::Command::new("sudo")
        .args([
            "-u",
            "postgres",
            "psql",
            "-tAc",
            "SELECT 1 FROM pg_database WHERE datname='virtues'",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim() == "1")
}

/// How many entries in `dir` start with `prefix`? `None` if the directory is
/// absent — which is a pass, not an error: not every box has sshd.
fn count_prefixed(dir: &str, prefix: &str) -> Option<usize> {
    let entries = std::fs::read_dir(dir).ok()?;
    Some(
        entries
            .flatten()
            .filter(|e| {
                prefix.is_empty() || e.file_name().to_string_lossy().starts_with(prefix)
            })
            .count(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("imgchk-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_deprovisioned_appliance_has_no_cluster() {
        // Deprovision removes the whole cluster on a relocated box, so there is
        // nothing to ask — and that must read as a PASS, not as "Postgres is
        // unreachable". Getting this backwards would fail the gate on every
        // correctly prepared image.
        let d = tmp("empty");
        assert!(matches!(cluster_state_of(&d), ClusterState::Absent));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_symlink_into_an_unmounted_data_dir_is_absent() {
        // The state a freshly imaged unit is in before firstboot claims its
        // disk: the link exists, the target does not.
        let d = tmp("dangling");
        let link = d.join("postgresql");
        std::os::unix::fs::symlink(d.join("nowhere"), &link).unwrap();
        assert!(matches!(cluster_state_of(&link), ClusterState::Absent));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_empty_version_directory_is_not_a_cluster() {
        // `PG_VERSION` is the file initdb writes and every Postgres tool keys
        // on. Testing for the DIRECTORY instead would call a bare `mkdir -p
        // 18/main` — which is exactly what firstboot does moments before
        // initdb runs — a cluster, and then demand an answer from a server
        // that was never going to start.
        let d = tmp("shell");
        std::fs::create_dir_all(d.join("18/main")).unwrap();
        assert!(matches!(cluster_state_of(&d), ClusterState::Absent));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_real_cluster_is_present() {
        let d = tmp("real");
        std::fs::create_dir_all(d.join("18/main")).unwrap();
        std::fs::write(d.join("18/main/PG_VERSION"), "18\n").unwrap();
        assert!(matches!(cluster_state_of(&d), ClusterState::Present));
        let _ = std::fs::remove_dir_all(&d);
    }
}
