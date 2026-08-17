//! `virtues deprovision` — strip this box of every per-unit identity so its
//! disk can be imaged and cloned onto other boards.
//!
//! This is the LAST command run before a box ships (or before its eMMC is
//! `dd`'d to a master image). It is not a reset and not an uninstall: the
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
//!   * saved NetworkManager connections — otherwise the master's wifi
//!     credentials ship to every customer
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
    println!("   KEEPS: the binary, systemd units, models, QAIRT libs.");
    println!();
    println!("   This box will lose network access the moment its saved wifi is");
    println!("   removed. Run it from the console, or from a wired session.");
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
    println!("→ wiping database + data lake…");
    crate::cli::reset::run(false, true, force).await?;

    // ── 2. The per-unit env secrets ─────────────────────────────────────────
    // Removed, never regenerated here: a key minted on the master is baked into
    // the image and shared by every clone, which is the exact failure this
    // command exists to prevent. First boot mints it.
    strip_env_keys(&env_path)?;

    // ── 3. Host identity ────────────────────────────────────────────────────
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

    // ── 4. Logs ─────────────────────────────────────────────────────────────
    let _ = Command::new("journalctl")
        .args(["--rotate", "--vacuum-time=1s"])
        .output();
    println!("  ✓ journal vacuumed");

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
