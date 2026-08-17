//! `sudo virtues uninstall` — remove a box install from this machine.
//!
//! Design rules (from the onboarding-rewrite doctrine):
//!
//! 1. **The manifest is COMPUTED, not hardcoded-then-deleted.** We probe each
//!    artifact the installer can create, print exactly what was found, and
//!    delete exactly that list. An uninstall that guesses is how you delete
//!    the wrong thing on a customized install.
//!
//!    That rule was true of the *probe* and false of the list it probed
//!    from — a `const UNITS` here that still named `virtues-wireguard`
//!    (deleted with the move to the relay, and actively retired by
//!    `cli::upgrade`) and had never heard of the display, first-boot or
//!    captive-redirect units an appliance install writes. So uninstalling an
//!    appliance left a kiosk enabled against a server that no longer existed,
//!    a polkit grant for a deleted user, and a wildcard-DNS drop-in. The list
//!    now comes from `install.json`, written by the thing that created the
//!    artifacts; see `crate::install_manifest`.
//! 2. **Two confirmation factors:** root via sudo (the password) + typing the
//!    box's hostname (proves you know WHICH machine you're wiping — the
//!    GitHub-delete-repo pattern). `--force` skips the typed phrase for
//!    scripted dev/CI loops; root is still required.
//! 3. **Two tiers:** default = full purge. `--keep-data` removes the moving
//!    parts (binaries, units, avahi, WG iface) but keeps `/var/lib/virtues`
//!    (env + ENCRYPTION KEY + lake) and the Postgres db/role + system user,
//!    so a later reinstall picks the box back up where it left off. This is
//!    the dev-loop tier.
//! 4. **Shared infra is left alone.** Postgres the *server*, avahi, and
//!    WireGuard packages all stay — we only remove what is ours. The
//!    llama-server sidecars (units + binary) ARE ours (the installer put
//!    them there; nothing else uses them), so they go. The GGUFs live
//!    under the data dir and follow its tier (`--purge-models` removes
//!    them even with `--keep-data`; they re-download on reinstall).
//!
//! Everything is best-effort with per-item reporting: a missing artifact is a
//! skip, not an error, so the command is idempotent and safe to re-run.

use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;

use super::ui;

/// Units to look for when `install.json` is absent or says nothing — a box
/// installed before the manifest carried a unit list.
///
/// Every unit the installer has ever written, so an old box is still cleaned
/// up completely. `virtues-wireguard` stays in THIS list and only this one: it
/// is exactly the legacy artifact a pre-relay box still has, and the fallback
/// is where legacy belongs. It must not come back to the declared list.
const LEGACY_UNITS: &[&str] = &[
    "virtues-display.service",
    "virtues.service",
    "virtues-embed.service",
    "virtues-rerank.service",
    "virtues-qnnd.service",
    "virtues-captive-redirect.service",
    "virtues-firstboot.service",
    "virtues-wireguard.service",
];

/// Files outside `/etc/systemd/system` that exist only because we put them
/// there. Same fallback role as [`LEGACY_UNITS`].
const LEGACY_EXTRA_FILES: &[&str] = &[
    "/usr/local/lib/virtues/display.py",
    "/usr/local/sbin/virtues-firstboot.sh",
    "/etc/polkit-1/rules.d/50-virtues-network.rules",
    "/etc/NetworkManager/dnsmasq-shared.d/00-virtues-captive.conf",
];

const BINARIES: &[&str] = &[
    "/usr/local/bin/virtues",
    "/usr/local/bin/virtues-wireguard",
    "/usr/local/bin/llama-server",
    "/usr/local/bin/virtues-qnnd",
];
const WEB_DIR: &str = "/usr/local/share/virtues";
const AVAHI_SERVICE: &str = "/etc/avahi/services/virtues.service";
const DATA_DIR: &str = "/var/lib/virtues";
const MODELS_DIR: &str = "/var/lib/virtues/models";

struct Manifest {
    units: Vec<String>,
    binaries: Vec<&'static str>,
    /// Kiosk shim, first-boot script, polkit rule, dnsmasq drop-in.
    extra_files: Vec<String>,
    web_dir: bool,
    avahi: bool,
    data_dir: bool,
    pg: bool,
    system_user: bool,
    models_dir: bool,
}

pub async fn run(keep_data: bool, purge_models: bool, force: bool) -> Result<()> {
    // Root gate — we touch /usr/local, /etc/systemd, postgres, userdel.
    if !is_root() {
        return Err(anyhow!("uninstall must run as root: sudo virtues uninstall"));
    }

    let m = probe(purge_models);
    if m.is_empty() {
        ui::skip("nothing to remove — no Virtues install artifacts found");
        return Ok(());
    }

    print_manifest(&m, keep_data);

    if !force {
        let host = hostname();
        println!();
        println!("  This cannot be undone.{}", if keep_data {
            "  (data is kept: re-install to recover the box)"
        } else {
            "  ALL box data — including the encryption key — will be destroyed."
        });
        if !keep_data {
            println!("  Consider `virtues backup` first.");
        }
        let typed: String = dialoguer::Input::new()
            .with_prompt(format!("Type this box's hostname ('{host}') to confirm"))
            .allow_empty(true)
            .interact_text()
            .unwrap_or_default();
        if typed.trim() != host {
            println!("Hostname mismatch — aborting. Nothing was removed.");
            return Ok(());
        }
    }

    println!();

    // ── Stop & remove systemd units ─────────────────────────────────────
    for unit in &m.units {
        run_quiet("systemctl", &["stop", unit]);
        run_quiet("systemctl", &["disable", unit]);
        let path = format!("/etc/systemd/system/{unit}");
        report(std::fs::remove_file(&path).is_ok(), &format!("removed {path}"));
    }
    if !m.units.is_empty() {
        run_quiet("systemctl", &["daemon-reload"]);
    }

    // ── Files the units point at ────────────────────────────────────────
    // After the units, never before: removing the kiosk shim out from under a
    // running `virtues-display` would leave cage restarting into a missing
    // script every 5s for as long as the rest of this takes.
    for f in &m.extra_files {
        report(std::fs::remove_file(f).is_ok(), &format!("removed {f}"));
    }
    // NetworkManager and polkit both re-read their drop-in directories on
    // their own schedule; nudge them so the grant is gone now rather than at
    // the next reload.
    if m.extra_files.iter().any(|f| f.contains("NetworkManager")) {
        run_quiet("sh", &["-c", "systemctl reload NetworkManager 2>/dev/null || true"]);
    }

    // ── Binaries + web UI + mDNS advertisement ──────────────────────────
    for bin in &m.binaries {
        report(std::fs::remove_file(bin).is_ok(), &format!("removed {bin}"));
    }
    if m.web_dir {
        report(
            std::fs::remove_dir_all(WEB_DIR).is_ok(),
            &format!("removed {WEB_DIR}"),
        );
    }
    if m.avahi {
        report(
            std::fs::remove_file(AVAHI_SERVICE).is_ok(),
            &format!("removed {AVAHI_SERVICE}"),
        );
        run_quiet("sh", &["-c", "systemctl reload avahi-daemon 2>/dev/null || true"]);
    }

    // ── GGUF models (opt-in with --keep-data; full purge removes the whole
    //    data dir below anyway). They re-download on the next install. ────
    if m.models_dir {
        report(
            std::fs::remove_dir_all(MODELS_DIR).is_ok(),
            &format!("removed {MODELS_DIR} (GGUFs re-download on reinstall)"),
        );
    }

    // ── Data tier (skipped with --keep-data) ────────────────────────────
    if keep_data {
        ui::skip(&format!("kept {DATA_DIR} (env, encryption key, lake)"));
        ui::skip("kept Postgres database/role 'virtues' + system user");
    } else {
        if m.pg {
            report(
                run_quiet("sudo", &["-u", "postgres", "dropdb", "--if-exists", "virtues"]),
                "dropped Postgres database 'virtues'",
            );
            report(
                run_quiet("sudo", &["-u", "postgres", "dropuser", "--if-exists", "virtues"]),
                "dropped Postgres role 'virtues'",
            );
        }
        if m.data_dir {
            report(
                std::fs::remove_dir_all(DATA_DIR).is_ok(),
                &format!("removed {DATA_DIR} (env, encryption key, lake)"),
            );
        }
        if m.system_user {
            report(run_quiet("userdel", &["virtues"]), "removed system user 'virtues'");
        }
    }

    println!();
    if keep_data {
        ui::ok("Virtues removed (data kept). Reinstall any time:");
    } else {
        ui::ok("Virtues fully removed. Reinstall any time:");
    }
    println!(
        "       {}",
        console::style("curl -fsSL https://virtues.com/sh | sudo sh").cyan()
    );
    println!();
    Ok(())
}

/// Every unit that might belong to us, declared list ∪ legacy list.
///
/// The union, not a choice between them: the manifest describes the install as
/// it is configured NOW, and a box that was once an appliance and is no longer
/// one — or that predates the unit list — still has the older artifacts on
/// disk. Taking both and then filtering on existence removes everything that
/// is actually there and nothing that isn't.
fn candidate_units() -> Vec<String> {
    let mut out: Vec<String> = LEGACY_UNITS.iter().map(|u| u.to_string()).collect();
    if let Some(m) = crate::install_manifest::get().as_ref() {
        for u in &m.units {
            // The manifest stores bare names; units are addressed with the
            // suffix. Tolerate both so a manifest written either way works.
            let name = if u.ends_with(".service") {
                u.clone()
            } else {
                format!("{u}.service")
            };
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// Same union, for the files that live outside the unit directory.
fn candidate_extra_files() -> Vec<String> {
    let mut out: Vec<String> = LEGACY_EXTRA_FILES.iter().map(|f| f.to_string()).collect();
    if let Some(m) = crate::install_manifest::get().as_ref() {
        for f in &m.extra_files {
            if !out.contains(f) {
                out.push(f.clone());
            }
        }
    }
    out
}

/// Probe every artifact; only existing ones enter the manifest.
fn probe(purge_models: bool) -> Manifest {
    Manifest {
        units: candidate_units()
            .into_iter()
            .filter(|u| Path::new(&format!("/etc/systemd/system/{u}")).exists())
            .collect(),
        binaries: BINARIES
            .iter()
            .copied()
            .filter(|b| Path::new(b).exists())
            .collect(),
        extra_files: candidate_extra_files()
            .into_iter()
            .filter(|f| Path::new(f).exists())
            .collect(),
        web_dir: Path::new(WEB_DIR).exists(),
        avahi: Path::new(AVAHI_SERVICE).exists(),
        data_dir: Path::new(DATA_DIR).exists(),
        // Postgres db/role: probe via the postgres superuser; any failure
        // (postgres not installed, etc.) just means "nothing to drop".
        pg: Command::new("sudo")
            .args(["-u", "postgres", "psql", "-tAc", "SELECT 1 FROM pg_database WHERE datname='virtues'"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "1")
            .unwrap_or(false),
        system_user: Command::new("id")
            .arg("virtues")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false),
        models_dir: purge_models && Path::new(MODELS_DIR).exists(),
    }
}

impl Manifest {
    fn is_empty(&self) -> bool {
        self.units.is_empty()
            && self.binaries.is_empty()
            && self.extra_files.is_empty()
            && !self.web_dir
            && !self.avahi
            && !self.data_dir
            && !self.pg
            && !self.system_user
            && !self.models_dir
    }
}

/// Ledger column for the manifest ("mDNS advertisement" is the longest key).
const MANIFEST_COL: usize = 20;

fn print_manifest(m: &Manifest, keep_data: bool) {
    ui::section("Uninstall");
    println!();
    println!("    The following will be removed from this machine:");
    println!();
    for u in &m.units {
        ui::kv_at(MANIFEST_COL, "systemd unit", &format!("/etc/systemd/system/{u}  (stopped + disabled)"));
    }
    for b in &m.binaries {
        ui::kv_at(MANIFEST_COL, "binary", b);
    }
    if m.web_dir {
        ui::kv_at(MANIFEST_COL, "web UI", WEB_DIR);
    }
    if m.avahi {
        ui::kv_at(MANIFEST_COL, "mDNS advertisement", AVAHI_SERVICE);
    }
    for f in &m.extra_files {
        ui::kv_at(MANIFEST_COL, "installed file", f);
    }
    if m.models_dir {
        ui::kv_at(MANIFEST_COL, "GGUF models", &format!("{MODELS_DIR}  (re-download on reinstall)"));
    }
    if keep_data {
        if m.data_dir || m.pg || m.system_user {
            println!();
            ui::skip(&format!(
                "kept (--keep-data): {DATA_DIR}, Postgres db/role, system user"
            ));
        }
    } else {
        if m.pg {
            ui::kv_at(MANIFEST_COL, "Postgres", "database + role 'virtues'  (server stays)");
        }
        if m.data_dir {
            ui::kv_at(
                MANIFEST_COL,
                "data",
                &format!("{DATA_DIR}  (env, ENCRYPTION KEY, lake)"),
            );
        }
        if m.system_user {
            ui::kv_at(MANIFEST_COL, "system user", "virtues");
        }
    }
}

fn is_root() -> bool {
    Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false)
}

fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            Command::new("hostname")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "virtues".to_string())
}

/// Run a command, swallowing output; true on exit 0.
fn run_quiet(bin: &str, args: &[&str]) -> bool {
    Command::new(bin)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Per-item result line: `✓` on success, `·` when the artifact resisted
/// (already gone, command unavailable) — never a hard failure.
fn report(ok: bool, what: &str) {
    if ok {
        ui::ok(what);
    } else {
        ui::skip(&format!("skipped: {what}"));
    }
}
