//! Dayline — Shape of Your Day
//!
//! Per-event z-scored signals:
//! - **Novelty (global)** (Novel ↑ / Routine ↓): kernel-weighted centroid distance — "rare in your life at all"
//! - **Novelty (local)** (LOF): density-relative unusualness — "off-pattern for its kind"
//! - **Autonomic** (Stress ↑ / Recovery ↓): embedding-weighted HR comparison, physiological response

pub mod annotate;
pub mod autonomic_scoring;
pub mod context;
pub mod embedding_ops;
pub mod gaps;
pub mod novelty;
pub mod sleep;
pub mod topic_entity_novelty;

use crate::error::Result;
use sqlx::PgPool;

/// Recompute every event score, across every day that has events.
///
/// This exists because scores can be **invalidated wholesale**, and until now
/// nothing put them back.
///
/// `virtues reindex` nulls `wiki_events.embedding` and every score derived from
/// it — correctly, because a new embedding model puts vectors in a different
/// geometry and the old novelty numbers are meaningless in it. But it then
/// rebuilt only the *search* index and walked away. The nightly cron scores
/// exactly ONE day: the one it runs for. So a reindex silently destroyed the
/// novelty, autonomic, topic and entity scores of every past day — 82 of 83 on
/// this box — and nothing ever restored them.
///
/// It is the same shape as the bug that made the day pipeline useless for months:
/// one step quietly destroying what another produced, with no error anywhere. The
/// lesson is the same too. **Whatever invalidates scores must restore them.**
///
/// Order matters, and it is the cron's order for the same reasons: annotate
/// before scoring (autonomic baselines on `avg_hr`, which annotation writes), and
/// novelty before topic/entity (both stand on the event embedding).
pub async fn rescore_all_days(pool: &PgPool) -> Result<(u32, u32)> {
    let dates: Vec<chrono::NaiveDate> = sqlx::query_scalar(
        "SELECT DISTINCT d.date FROM wiki_days d \
         JOIN wiki_events e ON e.day_id = d.id \
         ORDER BY d.date",
    )
    .fetch_all(pool)
    .await?;

    let mut scored = 0u32;
    for date in &dates {
        // Annotation first: it writes `avg_hr`, which autonomic scoring baselines
        // against. Reverse them and the baseline is empty by construction — which
        // is exactly how autonomic scoring returned Ok(0) for every user, every
        // day, for months.
        annotate::annotate_events_for_day(pool, *date).await?;
        scored += novelty::compute_novelty_for_day(pool, *date).await?;
        autonomic_scoring::compute_autonomic_for_day(pool, *date).await?;
        topic_entity_novelty::compute_topic_entity_novelty(pool, *date).await?;
    }

    Ok((dates.len() as u32, scored))
}
