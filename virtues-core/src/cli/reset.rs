//! `virtues reset` — wipe a box back to a fresh state for testing.
//!
//! HIDDEN, destructive, no `--keep-data` (yet). Drops the entire `public`
//! schema (all data PLUS the box's identity — CA, WireGuard keys, paired
//! devices, subscription link), re-runs migrations to an empty schema, then
//! clears the data lake. The encryption key in the env file is left untouched.
//! Because `box_secrets` is gone, the box must re-register with atlas and
//! re-pair every device afterward.
//!
//! Guards mirror `restore`/`uninstall`:
//!   1. Refuses if `systemctl is-active virtues` (unless `--force`) — a live
//!      server holds locks on the tables we're dropping.
//!   2. Typed-hostname confirmation (unless `--yes`) — proves WHICH box.
//!
//! Handled in `main.rs` (not `cli::run`) because it manages the schema itself
//! and must run against a bare pool, like restore/uninstall.

use std::path::Path;
use std::process::Command;

pub async fn run(yes: bool, force: bool) -> Result<(), crate::Error> {
    let database_url = crate::database::normalize_database_url()?;
    // Same precedence the client uses on a box (setup::recommended_config).
    let lake =
        std::env::var("STORAGE_PATH").unwrap_or_else(|_| "/var/lib/virtues/lake".to_string());

    if !force {
        check_service_inactive()?;
    }

    println!();
    println!("⚠  virtues reset — this DESTROYS everything on this box:");
    println!("     • the entire Postgres database: all data AND the box's");
    println!("       identity (CA, WireGuard keys, paired devices, billing link)");
    println!("     • the data lake at {lake}");
    println!("   The box will re-register with atlas and you must re-pair every");
    println!("   device afterward. The encryption key in the env file is kept.");
    println!("   Consider `virtues backup` first.");
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

    // ── Drop + recreate the `public` schema ─────────────────────────────────
    // CASCADE takes the migration ledger, every table, and the `vector`
    // extension with it; re-create gives migrations a clean slate.
    println!();
    println!("→ dropping all database objects…");
    {
        let db = crate::database::Database::new(&database_url)?;
        let pool = db.pool();
        sqlx::query("DROP SCHEMA public CASCADE")
            .execute(pool)
            .await
            .map_err(|e| crate::Error::Other(format!("drop schema: {e}")))?;
        sqlx::query("CREATE SCHEMA public")
            .execute(pool)
            .await
            .map_err(|e| crate::Error::Other(format!("create schema: {e}")))?;
        // Restore the default grants Postgres puts on a fresh `public`.
        let _ = sqlx::query("GRANT ALL ON SCHEMA public TO public")
            .execute(pool)
            .await;
    }

    // ── Re-migrate to an empty schema ───────────────────────────────────────
    println!("→ re-running migrations…");
    crate::database::Database::new(&database_url)?
        .initialize()
        .await
        .map_err(|e| crate::Error::Other(format!("migrate: {e}")))?;

    // ── Clear the data lake ─────────────────────────────────────────────────
    if Path::new(&lake).exists() {
        println!("→ clearing data lake at {lake}…");
        std::fs::remove_dir_all(&lake)
            .map_err(|e| crate::Error::Other(format!("remove lake: {e}")))?;
    }
    std::fs::create_dir_all(&lake)
        .map_err(|e| crate::Error::Other(format!("recreate lake: {e}")))?;

    println!();
    println!("✓ reset complete — box is back to a fresh state.");
    println!("  Next steps:");
    println!("    sudo systemctl start virtues   # seeds defaults on boot");
    println!("    virtues pair                   # re-pair this + other devices");
    Ok(())
}

/// Refuse while the server is up — it holds locks on the tables we drop.
/// Mirrors `restore::check_service_inactive`. Missing `systemctl` (dev macOS)
/// reads as inactive.
fn check_service_inactive() -> Result<(), crate::Error> {
    let out = Command::new("systemctl")
        .arg("is-active")
        .arg("virtues")
        .output();
    match out {
        Ok(o) if o.status.success() => Err(crate::Error::Other(
            "virtues.service is running. Stop it first (`sudo systemctl stop virtues`) \
             or re-run with --force."
                .to_string(),
        )),
        _ => Ok(()),
    }
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
