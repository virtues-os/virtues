//! `virtues report-crash` — systemd `ExecStopPost=` hook.
//!
//! systemd invokes this after the daemon exits. It runs even on success,
//! so the first check is whether the exit was actually a crash; we only
//! act on non-zero or signal-terminated exits. We tail the last 50 lines
//! of the unit's journal so a reader has enough context to triage without
//! having to ask the user for logs.
//!
//! **The local record comes first.** This used to do exactly one thing with
//! a crash — POST it to atlas — so a box whose owner had set
//! `VIRTUES_DIAG=off`, or whose network was down at the moment it died,
//! kept no record at all that it had crashed. The owner's own machine was
//! the one place the fact did not exist. Now the `box.crashed` line is
//! written to the journal unconditionally, and the beacon is a second,
//! optional step on top of it.
//!
//! Writing a crash record *into* the journal we just read from is not
//! circular: `ExecStopPost` runs as a separate process after the daemon is
//! gone, so the line lands after the tail it quotes, and journald's own
//! rate limiting bounds a restart loop.
//!
//! Safety: this command MUST exit 0 in every scenario, even when the
//! beacon failed to send. systemd interprets a non-zero `ExecStopPost`
//! exit as a "post-stop hook failed" event and logs it; the user
//! shouldn't see noise just because their network blipped.

use std::process::Command;

use serde_json::json;

use super::diag;

/// Entry point invoked from `main.rs`. Always returns `Ok(())` — the
/// outer dispatch maps that to `process::exit(0)`.
pub async fn run() -> Result<(), crate::Error> {
    // systemd sets these for ExecStopPost. See systemd.service(5).
    let service_result = std::env::var("SERVICE_RESULT").unwrap_or_else(|_| "unknown".to_string());
    let exit_code = std::env::var("EXIT_CODE").unwrap_or_else(|_| "?".to_string());
    let exit_status = std::env::var("EXIT_STATUS").unwrap_or_else(|_| "?".to_string());

    // Only beacon on real crashes: SERVICE_RESULT in {"signal", "core-dump",
    // "watchdog", "exit-code"} with a non-zero status. A clean
    // `systemctl stop` produces SERVICE_RESULT=success and we exit silently.
    let is_crash = matches!(
        service_result.as_str(),
        "signal" | "core-dump" | "watchdog" | "abort" | "oom-kill"
    ) || (service_result == "exit-code" && exit_status != "0");
    if !is_crash {
        return Ok(());
    }

    let tail = tail_journal(50);

    // The local record. Unconditional, and before the beacon: a box always
    // knows it crashed, whether or not it is allowed to tell anyone.
    // `kind` is the observability vocabulary's event name (see
    // agents/plan/observability-plan.md); the tail is deliberately NOT a
    // field on it — it is already in this journal, a few lines up.
    tracing::error!(
        kind = "box.crashed",
        service_result = %service_result,
        exit_code = %exit_code,
        exit_status = %exit_status,
        version = env!("CARGO_PKG_VERSION"),
        "the server exited abnormally"
    );

    if !diag::enabled() {
        return Ok(());
    }

    let payload = json!({
        "box_id": diag::box_id(),
        "version": env!("CARGO_PKG_VERSION"),
        "service_result": service_result,
        "exit_code": exit_code,
        "exit_status": exit_status,
        "journal_tail": tail,
        "ts": chrono::Utc::now().to_rfc3339(),
    });

    // Best-effort post. Failure here is silent — the daemon already
    // crashed; making the post-stop hook also fail just adds noise.
    if let Err(e) = diag::send("/diag/crash", &payload).await {
        tracing::info!("crash beacon post failed: {e}");
    }
    Ok(())
}

/// Tail the last N lines of the `virtues.service` journal. Returns an
/// empty string if `journalctl` isn't available (won't be in dev) or if
/// it errors — we never block on log capture.
fn tail_journal(lines: usize) -> String {
    let out = Command::new("journalctl")
        .arg("-u")
        .arg("virtues.service")
        .arg("--no-pager")
        .arg("-n")
        .arg(lines.to_string())
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => String::new(),
    }
}
