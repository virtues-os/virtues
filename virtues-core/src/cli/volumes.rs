//! `virtues volumes` — register and inspect backup destinations.

use std::path::Path;

use sqlx::PgPool;

use super::ui;
use crate::storage::volumes;

pub async fn list(pool: &PgPool) -> Result<(), crate::Error> {
    let vols = volumes::backup_volumes(pool).await?;
    ui::section("Backup destinations");
    if vols.is_empty() {
        ui::warn("none registered — this box has exactly one copy of its data");
        println!("  Register one with:  virtues volumes add /path/to/mounted/drive");
        return Ok(());
    }
    for v in &vols {
        let attached = volumes::resolve_mount(&v.fs_uuid);
        ui::kv(
            &v.name,
            &match (&attached, v.last_ok_at) {
                (Some(m), Some(t)) => format!(
                    "attached at {} · last backup {}",
                    m.display(),
                    ui::rel_time(t)
                ),
                (Some(m), None) => format!("attached at {} · never backed up", m.display()),
                (None, Some(t)) => format!("not attached · last backup {}", ui::rel_time(t)),
                (None, None) => "not attached · never backed up".to_string(),
            },
        );
        if let Some(err) = &v.last_error {
            ui::warn(&format!("  last error: {err}"));
        }
    }
    Ok(())
}

pub async fn add(pool: &PgPool, path: &Path, name: Option<String>) -> Result<(), crate::Error> {
    if !path.is_dir() {
        return Err(crate::Error::Other(format!(
            "{} is not a directory — pass any path on the mounted volume",
            path.display()
        )));
    }
    // The UUID, not the path, is what gets stored. A destination recorded as
    // `/media/backup` would eventually write to whatever got mounted there
    // instead — a failure nobody notices until the data is needed.
    let uuid = volumes::uuid_for_path(path).ok_or_else(|| {
        crate::Error::Other(format!(
            "could not determine the filesystem UUID for {}. On Linux this reads \
             /proc/self/mountinfo and /dev/disk/by-uuid; a filesystem with no UUID \
             (some FAT formats) cannot be tracked reliably across reboots and is \
             not supported as a destination.",
            path.display()
        ))
    })?;

    let id = crate::ids::generate_id("vol", &[&uuid]);
    let label = name.unwrap_or_else(|| path.display().to_string());
    // `prefix` is the only thing on the volume this box will ever touch.
    let prefix = format!("virtues/{}", hostname_slug());

    sqlx::query(
        "INSERT INTO storage_volume (id, name, kind, fs_uuid, prefix, state, last_seen_at) \
         VALUES ($1, $2, 'removable', $3, $4, 'present', NOW()) \
         ON CONFLICT (fs_uuid) DO UPDATE SET name = EXCLUDED.name, updated_at = NOW()",
    )
    .bind(&id)
    .bind(&label)
    .bind(&uuid)
    .bind(&prefix)
    .execute(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("register volume: {e}")))?;

    ui::ok(&format!("registered {label}"));
    ui::kv("uuid", &uuid);
    ui::kv("box directory", &prefix);
    println!();
    println!("  Nothing outside {prefix} on that volume is ever touched.");
    println!("  Back up with:  sudo -u virtues virtues backup --volume all");
    Ok(())
}

/// Namespaced so one drive can serve two boxes without either overwriting the
/// other's archives.
fn hostname_slug() -> String {
    let raw = std::process::Command::new("hostname")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let slug: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    if slug.trim_matches('-').is_empty() {
        "box".to_string()
    } else {
        slug
    }
}
