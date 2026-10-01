//! Periodic Timeline rebuild.
//!
//! The Timeline's stays, drives, signal gaps, nights and moments are derived
//! rows (`crate::timeline`), rolled up from the raw record like visits are.
//! This task keeps them current on the same fast clock as the entity
//! resolver. A rebuild replaces the tables whole in one transaction and is
//! idempotent, so a slow pass or a restart loses nothing.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::{interval, MissedTickBehavior};

use crate::database::Database;

/// The fast clock: the same 15 minutes as `entity_resolver`.
const TICK: Duration = Duration::from_secs(900);

/// Spawn the Timeline builder as a background tokio task. Errors are logged
/// and the loop continues; a database without the Timeline's tables is
/// skipped quietly.
pub fn spawn(db: Arc<Database>) {
    tokio::spawn(async move {
        let mut ticker = interval(TICK);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // The first tick fires at once, so a restart rebuilds straight away.
        loop {
            ticker.tick().await;
            match crate::timeline::rebuild(db.pool()).await {
                Ok(Some(stats)) => tracing::info!(
                    places = stats.places,
                    stops_on_water = stats.stops_on_water,
                    spans = stats.spans,
                    moments = stats.moments,
                    duration_ms = stats.duration_ms as u64,
                    "timeline rebuild complete"
                ),
                Ok(None) => tracing::debug!("timeline tables absent; rebuild skipped"),
                Err(e) => tracing::warn!(error = %e, "timeline rebuild failed (will retry next tick)"),
            }
        }
    });
}
