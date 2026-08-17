//! `virtues reset` — wipe a box back to a fresh state for testing.
//!
//! HIDDEN, destructive, no `--keep-data` (yet). Drops the entire `public`
//! schema (all data PLUS the box's identity — iroh secret, paired
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

use std::process::Command;

pub async fn run(keep_data: bool, yes: bool, force: bool) -> Result<(), crate::Error> {
    let database_url = crate::database::normalize_database_url()?;

    // `--keep-data`: just re-open onboarding. No schema/lake changes, safe on a
    // live box — so it skips the service-stopped check entirely.
    if keep_data {
        return run_keep_data(&database_url, yes).await;
    }

    let lake = crate::storage::lake::lake_root();

    if !force {
        check_service_inactive()?;
    }

    println!();
    println!("⚠  virtues reset — this DESTROYS everything on this box:");
    println!("     • the entire Postgres database: all data AND the box's");
    println!("       identity (iroh secret, paired devices, billing link)");
    println!("     • the data lake at {}", lake.display());
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

    // ── Drop the app's objects, KEEP extensions + the schema ────────────────
    // We deliberately do NOT `DROP SCHEMA public` or `DROP OWNED BY` — both can
    // take the `vector` extension with them, and only a superuser can recreate
    // it (the installer made it as postgres; `virtues` can't), which would leave
    // the box unmigratable. Instead drop every table (CASCADE clears sequences +
    // FKs + the migration ledger) and every user-defined type that isn't part of
    // an extension. The `vector` extension, its types, and the schema all
    // survive, so re-migration's `CREATE EXTENSION IF NOT EXISTS` is a no-op.
    // Works regardless of who owns the schema and needs no superuser.
    //
    // **And the `applet_*` schemas, which this used to miss entirely.** An
    // authored applet gets a schema of its own (`faces`/`applet_schema`), so a
    // wipe scoped to `public` left every one of them — tables, rows and all —
    // standing in a database that had just reported itself erased. On this
    // machine that was three schemas of the owner's own life: a calorie diary,
    // a weekly planner, a readings log.
    //
    // The consequence was not a tidiness problem. `deprovision` delegates the
    // database wipe here, and deprovision is what runs immediately before a
    // disk is imaged — so the owner's personal applet data would have been
    // cloned onto every unit shipped from that master, invisibly, since nothing
    // downstream looks inside the database it was told had been emptied.
    //
    // `LIKE 'applet\_%'` with the underscore escaped: unescaped, `_` is a
    // single-character wildcard and would also match a schema called
    // `appletx...`. Nothing is named that today, which is exactly when this
    // sort of thing gets written wrong and stays wrong.
    println!();
    println!("→ dropping all database objects (keeping extensions)…");
    {
        let db = crate::database::Database::new(&database_url)?;
        sqlx::query(
            "DO $$ \
             DECLARE r RECORD; \
             BEGIN \
               FOR r IN SELECT tablename FROM pg_tables WHERE schemaname = 'public' LOOP \
                 EXECUTE format('DROP TABLE IF EXISTS public.%I CASCADE', r.tablename); \
               END LOOP; \
               FOR r IN \
                 SELECT t.typname FROM pg_type t \
                 JOIN pg_namespace n ON n.oid = t.typnamespace \
                 WHERE n.nspname = 'public' AND t.typtype IN ('e','c') \
                   AND NOT EXISTS ( \
                     SELECT 1 FROM pg_depend d WHERE d.objid = t.oid AND d.deptype = 'e') \
               LOOP \
                 EXECUTE format('DROP TYPE IF EXISTS public.%I CASCADE', r.typname); \
               END LOOP; \
               FOR r IN SELECT nspname FROM pg_namespace WHERE nspname LIKE 'applet\\_%' LOOP \
                 EXECUTE format('DROP SCHEMA IF EXISTS %I CASCADE', r.nspname); \
               END LOOP; \
             END $$",
        )
        .execute(db.pool())
        .await
        .map_err(|e| crate::Error::Other(format!("drop app objects: {e}")))?;
    }

    // ── Re-migrate to an empty schema ───────────────────────────────────────
    println!("→ re-running migrations…");
    crate::database::Database::new(&database_url)?
        .initialize()
        .await
        .map_err(|e| crate::Error::Other(format!("migrate: {e}")))?;

    // ── Clear the data lake ─────────────────────────────────────────────────
    if lake.exists() {
        println!("→ clearing data lake at {}…", lake.display());
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

/// `--keep-data`: re-open onboarding without deleting anything. Revoke every
/// paired device, so `claimed` flips false and setup reappears. Keeps indexed
/// data, source connections, the subscription, the box identity, and the schema — and runs fine against a live server (these are the
/// same UPDATEs the dashboard does when you remove a device).
async fn run_keep_data(database_url: &str, yes: bool) -> Result<(), crate::Error> {
    println!();
    println!("⚠  virtues reset --keep-data — re-opens onboarding, deletes nothing:");
    println!("     • revokes all paired devices (you'll re-pair the Mac / phone)");
    println!("   Keeps: your data, connected sources, subscription, box identity.");
    println!();

    if !yes {
        let ok = dialoguer::Confirm::new()
            .with_prompt("Re-open onboarding now (revoke devices, keep data)?")
            .default(false)
            .interact()
            .unwrap_or(false);
        if !ok {
            println!("Aborted — nothing was changed.");
            return Ok(());
        }
    }

    let db = crate::database::Database::new(database_url)?;
    let mut tx = db
        .pool()
        .begin()
        .await
        .map_err(|e| crate::Error::Database(format!("begin: {e}")))?;

    let devices = sqlx::query("UPDATE app_device SET revoked_at = now() WHERE revoked_at IS NULL")
        .execute(&mut *tx)
        .await
        .map_err(|e| crate::Error::Database(format!("revoke devices: {e}")))?
        .rows_affected();

    // No credentials statement. `credentials` has no `device_id` column and
    // never has — see `api::pair::revoke_all_devices` for the full account.
    // This made `virtues reset --keep-data` fail outright, in the same way and
    // for the same reason.
    let creds = 0u64;

    tx.commit()
        .await
        .map_err(|e| crate::Error::Database(format!("commit: {e}")))?;

    println!();
    println!("✓ onboarding re-opened — revoked {devices} device(s) + {creds} credential(s).");
    println!("  Re-pair from the app; your data + subscription are intact.");
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
