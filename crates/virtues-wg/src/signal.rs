//! Reconcile signalling — the box's (rootless) app tells the privileged
//! `virtues-wireguard` daemon to reconcile `wg0` *now* instead of waiting for
//! its backstop poll, so a new pairing or a revoke takes effect in ~1s rather
//! than up to one poll interval later.
//!
//! Mechanism is Postgres `LISTEN`/`NOTIFY`: the daemon `LISTEN`s on
//! [`RECONCILE_CHANNEL`]; the app fires [`notify_reconcile`] after it mutates
//! the durable peer set (pair-consume, credential/device revoke). Pure SQL, no
//! platform deps, so the rootless app can call it on any OS — the daemon (and
//! the kernel side) stays Linux-only.

use anyhow::Result;
use sqlx::PgPool;

/// The Postgres `LISTEN`/`NOTIFY` channel the daemon waits on for prompt
/// reconciliation. The 15s poll in the daemon is the backstop if a notification
/// is missed (dropped LISTEN connection, restart window).
pub const RECONCILE_CHANNEL: &str = "wg_reconcile";

/// Fire a reconcile notification at the daemon. Best-effort at the call site
/// (the poll backstop guarantees eventual convergence), but the error is
/// returned so callers can log it. Uses `pg_notify(text, text)` — which takes
/// the channel as a bind parameter, unlike the literal-only `NOTIFY` statement.
pub async fn notify_reconcile(pool: &PgPool) -> Result<()> {
    sqlx::query("SELECT pg_notify($1, '')")
        .bind(RECONCILE_CHANNEL)
        .execute(pool)
        .await?;
    Ok(())
}
