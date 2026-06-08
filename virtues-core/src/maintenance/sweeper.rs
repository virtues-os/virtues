//! Auth-table sweeper.
//!
//! Every 10 minutes the daemon prunes auth tables that would otherwise grow
//! unbounded:
//!
//!   - `app_pair_token` rows past `expires_at + 1m grace` get deleted. The
//!     1-minute grace covers the consume re-claim window the auth model
//!     promises (a user whose network died mid-consume retries with the
//!     same token; the row is still around for that retry).
//!
//!   - `app_sudo_request` rows past `expires_at`, in any non-pending
//!     terminal state (`consumed`, `denied`, `expired`), get deleted. The
//!     `pending` state is kept past TTL until a separate pass flips it to
//!     `expired` — handled inline by `verify_and_consume` when the gated
//!     handler attempts to use it.
//!
//!   - `app_auth_event` rows older than 90 days are moved to
//!     `app_auth_event_archive`. The live table stays small for
//!     incident-response queries while the archive keeps full history.
//!
//! No job queue, no worker pool, no PID file — one tokio task spawned by
//! `server::run`. Stops when the daemon stops.

use std::time::Duration;

use sqlx::PgPool;
use tokio::time::{interval, MissedTickBehavior};

const TICK: Duration = Duration::from_secs(600); // 10 minutes
const PAIR_TOKEN_GRACE: &str = "1 minute";
const EVENT_RETENTION_DAYS: i64 = 90;

/// Spawn the sweeper as a background tokio task. Logs cleanup counts at
/// info level when work was done; silent on no-op ticks. Errors are logged
/// and the loop continues — a transient DB error shouldn't take the
/// daemon down with it.
pub fn spawn(pool: PgPool) {
    tokio::spawn(async move {
        // `MissedTickBehavior::Skip` so a long-running iteration (e.g. a
        // big archive batch) doesn't queue catch-up ticks. The sweeper is
        // idempotent — skipping a tick is fine.
        let mut tick = interval(TICK);
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // Burn the immediate first tick — interval fires once at start.
        // We don't want the very first work to land before the server is
        // accepting traffic.
        tick.tick().await;
        loop {
            tick.tick().await;
            match run_once(&pool).await {
                Ok(Counts { pair_tokens: 0, sudo_requests: 0, archived: 0 }) => {
                    // quiet
                }
                Ok(c) => tracing::info!(
                    pair_tokens = c.pair_tokens,
                    sudo_requests = c.sudo_requests,
                    archived = c.archived,
                    "sweeper: cleaned"
                ),
                Err(e) => tracing::warn!("sweeper tick failed: {e:#}"),
            }
        }
    });
}

#[derive(Debug, Default)]
struct Counts {
    pair_tokens: u64,
    sudo_requests: u64,
    archived: u64,
}

async fn run_once(pool: &PgPool) -> Result<Counts, sqlx::Error> {
    let pair_tokens = sqlx::query(&format!(
        "DELETE FROM app_pair_token \
         WHERE expires_at < now() - interval '{PAIR_TOKEN_GRACE}'"
    ))
    .execute(pool)
    .await?
    .rows_affected();

    let sudo_requests = sqlx::query(
        "DELETE FROM app_sudo_request \
         WHERE expires_at < now() AND status <> 'pending'",
    )
    .execute(pool)
    .await?
    .rows_affected();

    // Move-then-delete in one statement using a CTE. Postgres guarantees
    // RETURNING from the DELETE is visible to the INSERT only inside the
    // same statement, so this is atomic — no partial archive states.
    let archived = sqlx::query(
        "WITH moved AS (
             DELETE FROM app_auth_event
             WHERE occurred_at < now() - make_interval(days => $1::int)
             RETURNING id, user_id, device_id, event_type, detail, ip, user_agent, occurred_at
         )
         INSERT INTO app_auth_event_archive
             (id, user_id, device_id, event_type, detail, ip, user_agent, occurred_at)
         SELECT id, user_id, device_id, event_type, detail, ip, user_agent, occurred_at
         FROM moved",
    )
    .bind(EVENT_RETENTION_DAYS as i32)
    .execute(pool)
    .await?
    .rows_affected();

    // Roll the 24h auto-top-up failure window. The breaker is "3 failures
    // in 24h"; if the most recent failure or breaker-trip is more than 24h
    // old, the counter resets. The user's `auto_topup_enabled` setting is
    // left as-is — if the breaker tripped it off, the user has to flip it
    // back on themselves (forces a deliberate "I've fixed my card" action).
    let _ = sqlx::query(
        "UPDATE app_user_profile \
         SET auto_topup_failures_24h = 0 \
         WHERE id = '00000000-0000-0000-0000-000000000001' \
           AND auto_topup_failures_24h > 0 \
           AND (auto_topup_disabled_at IS NULL \
                OR auto_topup_disabled_at < now() - interval '24 hours')",
    )
    .execute(pool)
    .await?;

    Ok(Counts {
        pair_tokens,
        sudo_requests,
        archived,
    })
}
