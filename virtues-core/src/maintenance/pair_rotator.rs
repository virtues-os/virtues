//! Universal rotating pairing code.
//!
//! Keeps a fresh standing pair code alive at all times so the panel and
//! `virtues pair` always have a valid code to show. Mints a new code every
//! `STANDING_ROTATE_INTERVAL_MIN`; the previous code stays valid for
//! `STANDING_GRACE_MIN` (the overlap window), so a code read mid-rotation never
//! dies under the user. Expired rows are pruned by `maintenance::sweeper`.
//!
//! One tokio task spawned by `server::run`, mirroring `maintenance::sweeper`.

use std::time::Duration;

use sqlx::PgPool;
use tokio::time::{interval, MissedTickBehavior};

use crate::api::pair::{
    ensure_standing_code, expire_standing_codes, is_unclaimed, mint_standing_code,
    STANDING_ROTATE_INTERVAL_MIN,
};

/// Spawn the rotator as a background tokio task. Keeps a fresh standing code
/// alive ONLY WHILE THE BOX IS UNCLAIMED — that is the setup window, where
/// `virtues pair` and the box's own codeless `0x83` redemption need one. Once claimed, the standing code is
/// retired (an always-live multi-use code on a claimed box is a permanent
/// brute-forceable password; see `api::pair::expire_standing_codes`); the loop
/// keeps running so a reset back to unclaimed re-arms it. Errors are logged and
/// the loop continues — a transient DB/crypto error shouldn't take the daemon
/// down with it.
pub fn spawn(pool: PgPool) {
    tokio::spawn(async move {
        // Mint immediately if unclaimed and no valid code exists, so a freshly
        // started box in setup has one at once — before the first interval.
        if is_unclaimed(&pool).await {
            if let Err(e) = ensure_standing_code(&pool).await {
                tracing::warn!("pair_rotator: initial ensure failed: {e:#}");
            }
        }

        let mut tick = interval(Duration::from_secs((STANDING_ROTATE_INTERVAL_MIN * 60) as u64));
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // Burn the immediate first tick — we just ensured a code above.
        tick.tick().await;
        loop {
            tick.tick().await;
            if !is_unclaimed(&pool).await {
                // Claimed: no new standing code, and retire any that lingers.
                if let Err(e) = expire_standing_codes(&pool).await {
                    tracing::warn!("pair_rotator: could not expire standing code: {e:#}");
                }
                continue;
            }
            match mint_standing_code(&pool).await {
                Ok(_) => tracing::debug!("pair_rotator: rotated standing code"),
                Err(e) => tracing::warn!("pair_rotator: rotation failed: {e:#}"),
            }
        }
    });
}
