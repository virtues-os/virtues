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
    // agents/record/observability.md); the tail is deliberately NOT a
    // field on it — it is already in this journal, a few lines up.
    tracing::error!(
        kind = "box.crashed",
        service_result = %service_result,
        exit_code = %exit_code,
        exit_status = %exit_status,
        // The RELEASE version, not `CARGO_PKG_VERSION` — which is pinned at
        // "0.1.0" in Cargo.toml and told every crash record on a
        // v0.1.7-staging.78 box that it was 0.1.0. The first question about a
        // crash is which build died.
        version = %crate::codename::version(),
        "the server exited abnormally"
    );

    if !diag::enabled() {
        return Ok(());
    }

    let payload = json!({
        "box_id": diag::box_id(),
        "version": crate::codename::version(),
        "service_result": service_result,
        "exit_code": exit_code,
        "exit_status": exit_status,
        // Filtered, not raw — see `tail_journal`. Named for what it is so the
        // receiving end does not read absence as silence.
        "journal_tail_filtered": tail,
        "ts": chrono::Utc::now().to_rfc3339(),
    });

    // Best-effort post. Failure here is silent — the daemon already
    // crashed; making the post-stop hook also fail just adds noise.
    if let Err(e) = diag::send("/diag/crash", &payload).await {
        tracing::info!("crash beacon post failed: {e}");
    }
    Ok(())
}

/// The journal lines worth sending off the box after a crash.
///
/// # Why this is filtered
///
/// This is the one thing a running box sends anywhere, and it used to be the
/// last 50 lines of the unit's journal, unread and unfiltered. A crashing
/// box's last 50 lines are also the likeliest place in the whole system for a
/// file path, a search term, a person's name or an error quoting user text to
/// appear — and `agents/build/virtues-api.md` answers "can you see my notes,
/// location, or health data?" with "that data never leaves your box". That was
/// true of every table and one careless `tracing::info!` away from false.
///
/// So we send the lines that explain a crash and not the lines that narrate
/// the work: see [`keep_line`].
///
/// # Why not `journalctl -p err`
///
/// Because it would send almost nothing, silently. systemd stamps a priority
/// on lines a service writes to stderr, and absent an explicit `<N>` syslog
/// prefix — which the tracing formatter does not write — every line lands at
/// `info` no matter what level we logged it at. Filtering by journal priority
/// would therefore drop our own ERROR lines. The level we care about lives
/// INSIDE the message, which is why this matches on content.
///
/// Returns an empty string if `journalctl` isn't available (it won't be in
/// dev) or errors — we never block on log capture.
fn tail_journal(lines: usize) -> String {
    // Read well past what we intend to send: the filter is the point, and
    // after it a 50-line read might yield two lines.
    let scan = lines * 8;
    let out = Command::new("journalctl")
        .arg("-u")
        .arg("virtues.service")
        .arg("--no-pager")
        .arg("-o")
        .arg("cat") // the message only — journald's own prefix adds nothing here
        .arg("-n")
        .arg(scan.to_string())
        .output();
    let raw = match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return String::new(),
    };

    let kept: Vec<&str> = raw.lines().filter(|l| keep_line(l)).collect();
    // Send NOTHING rather than falling back to the unfiltered tail when the
    // filter finds nothing. A fallback would undo the whole point on exactly
    // the boxes that are chattiest.
    let start = kept.len().saturating_sub(lines);
    kept[start..].join("\n")
}

/// Does this journal line belong in a crash beacon?
///
/// Two keepers:
///
/// 1. **Anything that is not one of our JSON log records.** A Rust panic does
///    not go through `tracing` at all — the panic hook writes plain text to
///    stderr — so the single most valuable line in a crash is exactly the one
///    a "JSON with level ERROR" rule would throw away. Backtraces, loader
///    failures and OOM messages arrive the same way.
/// 2. **Our own records at ERROR**, except those re-emitted from an applet
///    subprocess (`source` starting `applet:`). An applet's error is a real
///    error and belongs in the journal — but it runs in a different process,
///    it is rarely why the daemon died, and it is the likeliest line in the
///    system to quote a filename or a fragment of the user's own content.
///    Applet lines began arriving at their true level once the runner started
///    unwrapping them, so without this they would have quietly re-entered the
///    beacon that had just been narrowed to keep them out.
///
/// Everything else — our INFO, WARN and DEBUG records — is dropped. That is
/// the bulk of the volume and the part that narrates the user's actual life:
/// which applet ran for whom, what was searched, which file was read.
///
/// Known residual, deliberately accepted: a line written by an OLDER build
/// (before the JSON switch) is plain text and therefore kept under rule 1. The
/// window is small — the daemon that just crashed ships with this binary, so
/// its own lines are JSON — and closing it would mean guessing at the shape of
/// text we no longer emit.
fn keep_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        // One of ours: keep only what we called an error, and only when it
        // came from the daemon itself.
        Ok(v) if v.get("level").is_some() => {
            let from_applet = v
                .get("source")
                .and_then(|s| s.as_str())
                .is_some_and(|s| s.starts_with("applet:"));
            !from_applet && matches!(v.get("level").and_then(|l| l.as_str()), Some("ERROR"))
        }
        // JSON, but not one of our records (an applet echoing a payload, say).
        // Not ours to interpret, and not evidence of our crash.
        Ok(_) => false,
        // Not JSON: a panic, a backtrace, a linker or OOM message.
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_survives_the_filter() {
        // The most important line in a crash, and the one a naive
        // "JSON with level ERROR" rule throws away: the panic hook writes
        // plain text, never through tracing.
        assert!(keep_line(
            "thread 'tokio-runtime-worker' panicked at src/agent/mod.rs:88:9:"
        ));
        assert!(keep_line("note: run with `RUST_BACKTRACE=1` to display a backtrace"));
        assert!(keep_line("   3: core::panicking::panic_fmt"));
    }

    #[test]
    fn our_errors_are_kept_and_our_chatter_is_not() {
        let err = r#"{"timestamp":"t","level":"ERROR","message":"pool timed out"}"#;
        let info = r#"{"timestamp":"t","level":"INFO","message":"narrating the day"}"#;
        let warn = r#"{"timestamp":"t","level":"WARN","message":"slow query"}"#;
        assert!(keep_line(err));
        assert!(!keep_line(info));
        assert!(!keep_line(warn));
    }

    #[test]
    fn the_users_life_does_not_leave_the_box() {
        // The whole reason for the filter. These are the shapes that would
        // have ridden along in an unfiltered tail.
        for line in [
            r#"{"level":"INFO","message":"read /var/lib/virtues/lake/notes/therapy.md"}"#,
            r#"{"level":"INFO","message":"search: divorce lawyer"}"#,
            r#"{"level":"DEBUG","message":"ai call","feature":"day_summary"}"#,
            r#"{"level":"WARN","span":{"chat_id":"c1"},"message":"retrying"}"#,
        ] {
            assert!(!keep_line(line), "should not leave the box: {line}");
        }
    }

    #[test]
    fn blank_lines_and_foreign_json_are_dropped() {
        assert!(!keep_line(""));
        assert!(!keep_line("   "));
        // Valid JSON that is not one of our records — an applet echoing a
        // payload, say. Not evidence of our crash, and not ours to interpret.
        assert!(!keep_line(r#"{"records":[{"name":"someone"}]}"#));
    }
}

#[cfg(test)]
mod applet_source_tests {
    use super::*;

    #[test]
    fn an_applet_error_stays_on_the_box() {
        // Applet lines now arrive at their true level, so ERROR ones would
        // have slipped back into the beacon that was just narrowed.
        let line = r#"{"level":"ERROR","message":"failed to read /var/lib/virtues/lake/notes/private.md","kind":"applet.log","source":"applet:applet_document_extraction"}"#;
        assert!(!keep_line(line));
    }

    #[test]
    fn the_daemons_own_error_still_goes() {
        let line = r#"{"level":"ERROR","message":"pool timed out","kind":"db.pool"}"#;
        assert!(keep_line(line));
        // And one that names a source which is not an applet.
        let boxed = r#"{"level":"ERROR","message":"boom","source":"device:dev_1"}"#;
        assert!(keep_line(boxed));
    }
}
