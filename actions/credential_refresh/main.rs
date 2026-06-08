//! Credential refresh — idle-credential warm-keeper.
//!
//! Authoritative refresh now happens just-in-time in the action runner
//! (`load_credentials` → `virtues_helpers::auth::ensure_fresh`). Every
//! dispatch sees a fresh token regardless of cron timing, so the old race
//! between this action and the syncs it was supposed to feed is gone.
//!
//! This cron sticks around as a warm-keeper: it sweeps `via_proxy` credentials
//! whose tokens are nearing expiry and refreshes them ahead of time, so the
//! *first* manual run after a long idle period doesn't pay refresh latency.
//! It's a nice-to-have, not load-bearing.
//!
//! Outcome accounting mirrors `ensure_fresh`'s return:
//! - `Refreshed` — token rotated.
//! - `Fresh` — already valid; counted as `skipped`.
//! - `NoRefreshable` — paste-once kinds (api_key, Plaid); counted as `skipped`.
//! - `Err(Proxy "upstream 4xx")` — provider rejected; row already flipped to
//!   `reauth_required` by `ensure_fresh`; counted as `reauth_required`.
//! - `Err(_)` — transient; row untouched; counted as `errored`.

use anyhow::Result;
use sqlx::PgPool;
use virtues_helpers::auth::{ensure_fresh, AuthError, RefreshOutcome};
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_actions::init_tracing();

    let input = read_input()?;
    let pool = connect_from_env("virtues-action-credential_refresh").await?;

    let stats = sweep_expiring(&pool).await?;

    let summary = format!(
        "refreshed {} of {} credentials ({} reauth, {} errors, {} skipped)",
        stats.refreshed,
        stats.scanned,
        stats.reauth_required,
        stats.errored,
        stats.skipped,
    );
    output(&summary, &input.config)
}

#[derive(Default)]
struct Stats {
    scanned: usize,
    refreshed: usize,
    reauth_required: usize,
    errored: usize,
    skipped: usize,
}

async fn sweep_expiring(pool: &PgPool) -> Result<Stats> {
    let ids: Vec<(String,)> = sqlx::query_as(
        r#"SELECT id FROM credentials
           WHERE status = 'active'
             AND next_refresh_at IS NOT NULL
             AND next_refresh_at < now()"#,
    )
    .fetch_all(pool)
    .await?;

    let mut stats = Stats {
        scanned: ids.len(),
        ..Default::default()
    };

    for (id,) in ids {
        match ensure_fresh(pool, &id).await {
            Ok(RefreshOutcome::Refreshed) => stats.refreshed += 1,
            Ok(_) => stats.skipped += 1,
            Err(AuthError::Proxy(msg)) if msg.contains("upstream 4") => {
                stats.reauth_required += 1;
            }
            Err(e) => {
                tracing::warn!(credential_id = %id, error = %e, "ensure_fresh failed in sweep");
                stats.errored += 1;
            }
        }
    }

    Ok(stats)
}
