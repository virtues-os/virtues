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

    // ── The database ────────────────────────────────────────────────────────
    // `box_secrets` holds the iroh secret. Checked by asking Postgres rather
    // than by trusting that deprovision ran: this is the single most expensive
    // thing to get wrong, and the whole point of a separate command is to look
    // again.
    match database_is_present().await {
        Some(true) => findings.push(Finding {
            what: "database still exists",
            detail: "the `virtues` database is present — box_secrets holds the iroh secret, which IS this box's network identity. Clones would all be the same box.".to_string(),
            fix: "sudo virtues deprovision",
        }),
        Some(false) => {}
        // Postgres unreachable. Say so rather than passing: "I could not check
        // the most important thing" must never render as a tick.
        None => findings.push(Finding {
            what: "database unreadable",
            detail: "could not ask Postgres whether the `virtues` database exists — this check cannot pass without an answer".to_string(),
            fix: "start postgresql, then re-run",
        }),
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
