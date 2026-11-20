//! Let go of billed connections after the subscription lapses.
//!
//! Some providers bill Virtues for each connection while it exists: Plaid
//! charges a monthly fee per linked bank until the bank is removed. The
//! catalog marks those sources `release_on_lapse`. Once a day this asks
//! virtues-api whether the box's subscription is live. The first day it is
//! not, the box notes the date; 30 days later it revokes those sources' active
//! connections, which hands each grant back to the OAuth proxy to release
//! (`api::credentials::revoke_credential_because`). A live subscription at any
//! check clears the date.
//!
//! The date is kept on the box because virtues-api deletes a lapsed wallet
//! after a short grace, so it cannot say how long ago a subscription ended.
//! It lives in the `virtues_api` credential's metadata, the row that holds the
//! box's link to its account.

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;

use crate::error::Result;

/// How long a lapsed box keeps its billed connections before letting go.
pub const RELEASE_AFTER_DAYS: i64 = 30;

/// The `status_reason` a released connection is revoked with. The Sources
/// page reads it to say why the connection went away.
pub const RELEASE_REASON: &str = "subscription_ended";

const MARKER: &str = "no_subscription_since";

/// What one daily check does.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// The subscription is live: forget any lapse date.
    Clear,
    /// First check without a live subscription: note today.
    Mark,
    /// Lapsed, but not for long enough yet.
    Wait,
    /// Lapsed for `RELEASE_AFTER_DAYS` or more: let go.
    Release,
}

fn step(live: bool, since: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Step {
    match (live, since) {
        (true, _) => Step::Clear,
        (false, None) => Step::Mark,
        (false, Some(t)) if now - t >= Duration::days(RELEASE_AFTER_DAYS) => Step::Release,
        (false, Some(_)) => Step::Wait,
    }
}

/// Run a check a few minutes after start, then daily. Off in a dev checkout,
/// which reports itself subscribed and must not revoke a developer's banks.
pub fn spawn(pool: PgPool) {
    if std::env::var("ENVIRONMENT").is_ok_and(|e| e == "dev") {
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(240)).await;
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(24 * 3600));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            if let Err(e) = check_once(&pool).await {
                tracing::warn!("lapse check failed: {e:#}");
            }
        }
    });
}

/// One daily check. An unlinked box has no subscription to lapse and no key
/// the proxy would accept, so it does nothing. A check that cannot reach
/// virtues-api changes nothing: an outage must not count toward a lapse.
pub async fn check_once(pool: &PgPool) -> Result<()> {
    if !crate::virtues_api::renew::has_api_key(pool).await? {
        return Ok(());
    }
    let client = crate::virtues_api::client::BearerClient::from_env(pool.clone());
    let resp = match client.get_json("/v1/usage").await {
        Ok(r) if r.is_success() => r,
        Ok(r) => {
            tracing::info!(status = r.status, "lapse check: virtues-api did not answer usage; skipping");
            return Ok(());
        }
        Err(e) => {
            tracing::info!(error = %e, "lapse check: virtues-api unreachable; skipping");
            return Ok(());
        }
    };
    let now = Utc::now();
    let live = resp.body["expires_at"]
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .is_some_and(|t| t.with_timezone(&Utc) > now);

    match step(live, read_marker(pool).await?, now) {
        Step::Clear => set_marker(pool, None).await,
        Step::Mark => set_marker(pool, Some(now)).await,
        Step::Wait => Ok(()),
        Step::Release => release(pool).await,
    }
}

async fn read_marker(pool: &PgPool) -> Result<Option<DateTime<Utc>>> {
    let raw: Option<Option<String>> = sqlx::query_scalar(
        "SELECT metadata->>$1 FROM credentials WHERE kind = 'source' AND key = 'virtues_api' LIMIT 1",
    )
    .bind(MARKER)
    .fetch_optional(pool)
    .await?;
    // absent-ok: the query error is returned above; this parses the stored
    // date, and an unreadable one counts as no date, so the next check notes
    // today and the 30 days start over rather than releasing early.
    Ok(raw
        .flatten()
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|t| t.with_timezone(&Utc)))
}

async fn set_marker(pool: &PgPool, since: Option<DateTime<Utc>>) -> Result<()> {
    match since {
        Some(t) => sqlx::query(
            "UPDATE credentials SET metadata = metadata || jsonb_build_object($1::text, $2::text) \
             WHERE kind = 'source' AND key = 'virtues_api'",
        )
        .bind(MARKER)
        .bind(t.to_rfc3339())
        .execute(pool)
        .await?,
        None => sqlx::query(
            "UPDATE credentials SET metadata = metadata - $1 \
             WHERE kind = 'source' AND key = 'virtues_api' AND metadata ? $1",
        )
        .bind(MARKER)
        .execute(pool)
        .await?,
    };
    Ok(())
}

/// Revoke every active connection of a `release_on_lapse` source.
async fn release(pool: &PgPool) -> Result<()> {
    let keys: Vec<String> = crate::applet_templates::list_sources_sorted()
        .into_iter()
        .filter(|s| s.release_on_lapse)
        .map(|s| s.id)
        .collect();
    if keys.is_empty() {
        return Ok(());
    }
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM credentials WHERE kind = 'source' AND status = 'active' AND key = ANY($1)",
    )
    .bind(&keys)
    .fetch_all(pool)
    .await?;
    for id in ids {
        match crate::api::credentials::revoke_credential_because(pool, &id, RELEASE_REASON).await {
            Ok(()) => tracing::info!(credential_id = %id, "released after the subscription lapsed"),
            Err(e) => tracing::warn!(credential_id = %id, error = %e, "could not release after the subscription lapsed"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(days_ago: i64) -> DateTime<Utc> {
        Utc::now() - Duration::days(days_ago)
    }

    #[test]
    fn a_live_subscription_clears_the_date() {
        assert_eq!(step(true, Some(at(40)), Utc::now()), Step::Clear);
        assert_eq!(step(true, None, Utc::now()), Step::Clear);
    }

    #[test]
    fn the_first_lapsed_check_notes_the_date() {
        assert_eq!(step(false, None, Utc::now()), Step::Mark);
    }

    #[test]
    fn connections_are_released_only_after_thirty_days() {
        assert_eq!(step(false, Some(at(29)), Utc::now()), Step::Wait);
        assert_eq!(step(false, Some(at(30)), Utc::now()), Step::Release);
    }
}
