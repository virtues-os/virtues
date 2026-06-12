//! `sudo virtues uninstall` — remove a box install from this machine.
//!
//! Design rules (from the onboarding-rewrite doctrine):
//!
//! 1. **The manifest is COMPUTED, not hardcoded-then-deleted.** We probe each
//!    artifact the installer can create, print exactly what was found, and
//!    delete exactly that list. An uninstall that guesses is how you delete
//!    the wrong thing on a customized install.
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

/// Filesystem artifacts the installer creates, probed at runtime.
const UNITS: &[&str] = &[
    "virtues.service",
    "virtues-wireguard.service",
    "virtues-embed.service",
    "virtues-rerank.service",
];
const BINARIES: &[&str] = &[
    "/usr/local/bin/virtues",
    "/usr/local/bin/virtues-wireguard",
    "/usr/local/bin/llama-server",
];
const WEB_DIR: &str = "/usr/local/share/virtues";
const AVAHI_SERVICE: &str = "/etc/avahi/services/virtues.service";
const DATA_DIR: &str = "/var/lib/virtues";
const MODELS_DIR: &str = "/var/lib/virtues/models";
const WG_IFNAME: &str = "wg0";

struct Manifest {
    units: Vec<String>,
    binaries: Vec<&'static str>,
    web_dir: bool,
    avahi: bool,
    wg_iface: bool,
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
        println!("Nothing to remove — no Virtues install artifacts found.");
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

    // ── WG interface (the unit normally tears it down; belt & braces) ───
    if m.wg_iface {
        report(
            run_quiet("ip", &["link", "del", WG_IFNAME]),
            &format!("removed WireGuard interface {WG_IFNAME}"),
        );
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
        println!("  ∙ kept {DATA_DIR} (env, encryption key, lake)");
        println!("  ∙ kept Postgres database/role 'virtues' + system user");
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
        println!("✓ Virtues removed (data kept). Reinstall any time:");
    } else {
        println!("✓ Virtues fully removed. Reinstall any time:");
    }
    println!("    curl -fsSL https://get.virtues.com | sudo sh");
    Ok(())
}

/// Probe every artifact; only existing ones enter the manifest.
fn probe(purge_models: bool) -> Manifest {
    Manifest {
        units: UNITS
            .iter()
            .filter(|u| Path::new(&format!("/etc/systemd/system/{u}")).exists())
            .map(|u| u.to_string())
            .collect(),
        binaries: BINARIES
            .iter()
            .copied()
            .filter(|b| Path::new(b).exists())
            .collect(),
        web_dir: Path::new(WEB_DIR).exists(),
        avahi: Path::new(AVAHI_SERVICE).exists(),
        wg_iface: Path::new(&format!("/sys/class/net/{WG_IFNAME}")).exists(),
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
            && !self.web_dir
            && !self.avahi
            && !self.wg_iface
            && !self.data_dir
            && !self.pg
            && !self.system_user
            && !self.models_dir
    }
}

fn print_manifest(m: &Manifest, keep_data: bool) {
    println!();
    println!("The following will be removed from this machine:");
    for u in &m.units {
        println!("  • systemd unit       /etc/systemd/system/{u}  (stopped + disabled)");
    }
    for b in &m.binaries {
        println!("  • binary             {b}");
    }
    if m.web_dir {
        println!("  • web UI             {WEB_DIR}");
    }
    if m.avahi {
        println!("  • mDNS advertisement {AVAHI_SERVICE}");
    }
    if m.wg_iface {
        println!("  • WireGuard iface    {WG_IFNAME}");
    }
    if m.models_dir {
        println!("  • GGUF models        {MODELS_DIR}  (re-download on reinstall)");
    }
    if keep_data {
        if m.data_dir || m.pg || m.system_user {
            println!("  KEPT (--keep-data):  {DATA_DIR}, Postgres db/role, system user");
        }
    } else {
        if m.pg {
            println!("  • Postgres           database + role 'virtues'  (server stays)");
        }
        if m.data_dir {
            println!("  • data               {DATA_DIR}  (env, ENCRYPTION KEY, lake)");
        }
        if m.system_user {
            println!("  • system user        virtues");
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

/// Per-item result line: `✓` on success, `∙ skipped` when the artifact
/// resisted (already gone, command unavailable) — never a hard failure.
fn report(ok: bool, what: &str) {
    if ok {
        println!("  ✓ {what}");
    } else {
        println!("  ∙ skipped: {what}");
    }
}
