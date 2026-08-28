//! backup_volumes: ship a backup to every attached volume.
//!
//! Runs nightly. A volume that is not plugged in is skipped rather than failed —
//! a box whose backup drive is unplugged must keep working, and a cron that goes
//! red every night for an expected condition is a cron nobody reads.
//!
//! **Why this shells out instead of calling the library.** The action contract is
//! that stdout is one JSON document (`applet_runner` does `from_str` on the whole
//! thing). `virtues backup` writes human progress to stdout, so calling it
//! in-process would corrupt the contract, and threading a "quiet" flag through
//! the whole archive stack to avoid that would be a lot of invasive plumbing to
//! buy nothing. The CLI is already a stable interface, and `backup` already
//! shells out to pg_dump — so this runs the command and reads the structured
//! outcome from `storage_volume`, which is where the run records itself anyway.

use anyhow::{Context, Result};
use serde_json::json;
use sqlx::Row;
use virtues_helpers::{output, read_input};

/// Where the box's binary lives. Overridable so a dev checkout can point at a
/// `cargo build` target instead of the installed path.
fn virtues_bin() -> String {
    if let Ok(explicit) = std::env::var("VIRTUES_BIN") {
        if !explicit.is_empty() {
            return explicit;
        }
    }
    let installed = "/usr/local/bin/virtues";
    if std::path::Path::new(installed).exists() {
        return installed.to_string();
    }
    "virtues".to_string()
}

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let input = read_input()?;
    let pool = virtues_helpers::connect_from_env("virtues-action-backup_volumes").await?;

    // `all` unless the caller named one — a manual run against a specific drive
    // is a legitimate thing to want.
    let target = input
        .config
        .get("volume")
        .and_then(|v| v.as_str())
        .unwrap_or("all")
        .to_string();

    let registered: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM storage_volume")
            .fetch_one(&pool)
            .await
            .context("count backup volumes")?;
    if registered == 0 {
        // Not a failure. It IS worth saying out loud every night, because a box
        // with no destination has exactly one copy of everything on it.
        output(
            "no backup volume registered — this box has one copy of its data",
            &json!({ "registered": 0 }),
        )?;
        return Ok(());
    }

    let status = std::process::Command::new(virtues_bin())
        .args(["backup", "--volume", &target])
        .stdout(std::process::Stdio::null())
        .status()
        .context("running `virtues backup --volume`")?;

    // Read the outcome from the rows the run itself wrote, rather than parsing
    // human output that is free to change.
    let rows = sqlx::query(
        "SELECT name, last_ok_at, last_error, \
                EXTRACT(EPOCH FROM (NOW() - last_ok_at))::BIGINT AS age_secs \
         FROM storage_volume ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .context("read volume outcomes")?;

    let mut ok = 0;
    let mut failed = 0;
    let mut never = 0;
    let mut details = Vec::new();
    for row in &rows {
        let name: String = row.get("name");
        let last_ok: Option<chrono::DateTime<chrono::Utc>> = row.get("last_ok_at");
        let err: Option<String> = row.get("last_error");
        let age: Option<i64> = row.get("age_secs");
        match (&err, last_ok) {
            (Some(_), _) => failed += 1,
            (None, Some(_)) => ok += 1,
            (None, None) => never += 1,
        }
        details.push(json!({
            "volume": name,
            "last_ok_at": last_ok.map(|t| t.to_rfc3339()),
            "age_seconds": age,
            "error": err,
        }));
    }

    // Exit non-zero only when something actually broke. An unplugged drive
    // leaves last_error untouched and simply ages, which the UI surfaces as a
    // stale backup — the honest signal, and not a nightly red cron.
    if !status.success() && failed == 0 {
        failed = 1;
    }
    let summary = format!(
        "{ok} volume(s) backed up, {failed} failed, {never} never run"
    );
    output(&summary, &json!({ "volumes": details, "ok": ok, "failed": failed }))?;
    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}
