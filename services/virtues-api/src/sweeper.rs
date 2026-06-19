//! Background housekeeping for virtues-api.
//!
//! Deletes dead rows:
//!   - expired entitlements (bearers rotate monthly; an expired one is
//!     just dead weight — kept a short grace in case of a late redeem)
//!   - spent or expired vouchers — redeemed rows go after **24h** for a
//!     privacy reason: the voucher hash is a potential join key against
//!     Atlas's Stripe ledger during its retention window. Quick deletion
//!     defangs subpoena-time correlation. See `voucher.rs` module docs.
//!
//! Privacy hardening (locked 2026-05-28, two-pool launch):
//!   - 24h redeemed-voucher deletion (was 7d)
//!   - hour-bucketed `redeemed_at` on api side (done in `voucher::redeem`)
//!   - fixed `{os: $19, chat: $20}` denominations (atlas-side)
//!
//! Together these close the (timing + amount) correlation surface that
//! voucher-hash-alone leaked. The cryptographic version (RFC 9474 blind
//! RSA) is a planned v2 upgrade; this is the launch posture.

use sqlx::PgPool;
use std::time::Duration;

const SWEEP_INTERVAL: Duration = Duration::from_secs(60 * 60); // hourly
const ENTITLEMENT_GRACE_DAYS: i64 = 7;
/// 24h after redemption, the voucher row is deleted. Hour-bucketed
/// `redeemed_at` + this short grace are the privacy-hardening pair.
const VOUCHER_REDEEMED_GRACE_HOURS: i64 = 24;

pub fn spawn(pool: PgPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SWEEP_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            run_once(&pool).await;
        }
    });
}

async fn run_once(pool: &PgPool) {
    // Expired entitlements, past grace.
    match sqlx::query(&format!(
        "DELETE FROM entitlements WHERE expires_at < now() - interval '{ENTITLEMENT_GRACE_DAYS} days'"
    ))
    .execute(pool)
    .await
    {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(deleted = r.rows_affected(), "swept expired entitlements")
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("entitlement sweep failed: {e:#}"),
    }

    // Vouchers: unredeemed-and-expired, OR redeemed past the 24h privacy
    // window. Aggressive redeemed-row deletion is the privacy hardening:
    // the voucher hash is a potential join key with Atlas's ledger during
    // its retention window, so we drop it as soon as replay-protection
    // allows.
    match sqlx::query(&format!(
        "DELETE FROM vouchers \
         WHERE (redeemed_at IS NULL AND voucher_expires_at < now()) \
            OR (redeemed_at IS NOT NULL AND redeemed_at < now() - interval '{VOUCHER_REDEEMED_GRACE_HOURS} hours')"
    ))
    .execute(pool)
    .await
    {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(deleted = r.rows_affected(), "swept dead vouchers")
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("voucher sweep failed: {e:#}"),
    }

    // Expired blocklist entries (blocks are TTL'd cooldowns). The in-memory
    // map is pruned separately by `Blocklist::spawn_pruner`; this clears the
    // restart snapshot so a reboot doesn't re-load stale blocks.
    match sqlx::query("DELETE FROM blocklist WHERE expires_at < now()")
        .execute(pool)
        .await
    {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(deleted = r.rows_affected(), "swept expired blocks")
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("blocklist sweep failed: {e:#}"),
    }
}
