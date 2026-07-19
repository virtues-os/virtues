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

use crate::api::pair::{ensure_standing_code, mint_standing_code, STANDING_ROTATE_INTERVAL_MIN};

/// Spawn the rotator as a background tokio task. Mints on boot (if none valid),
/// then rotates on the interval. Errors are logged and the loop continues — a
/// transient DB/crypto error shouldn't take the daemon down with it.
pub fn spawn(pool: PgPool) {
    tokio::spawn(async move {
        // Mint immediately if no valid standing code exists, so a freshly
        // started box (or one whose codes expired while it was off) has one at
        // once — before the first interval elapses.
        if let Err(e) = ensure_standing_code(&pool).await {
            tracing::warn!("pair_rotator: initial ensure failed: {e:#}");
        }

        let mut tick = interval(Duration::from_secs((STANDING_ROTATE_INTERVAL_MIN * 60) as u64));
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // Burn the immediate first tick — we just ensured a code above.
        tick.tick().await;
        loop {
            tick.tick().await;
            match mint_standing_code(&pool).await {
                Ok(_) => tracing::debug!("pair_rotator: rotated standing code"),
                Err(e) => tracing::warn!("pair_rotator: rotation failed: {e:#}"),
            }
        }
    });
}
