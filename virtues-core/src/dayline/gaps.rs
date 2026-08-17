//! Gap classification: settle the detective's raw spine into its final shape —
//! **dwells, transit, and honest unknowns.**
//!
//! The detective emits a gapless 00:00–24:00 timeline, and `backfill_24h_events`
//! fills every hole with a single `Unknown` block. But two kinds of "Unknown" get
//! conflated:
//!
//!   - a genuine **dead zone** (no signal, phone off) — a real Unknown, keep it, and
//!   - the 4–6 minute **slivers** between two real events, which are just the
//!     detective drawing boundaries at exact data timestamps — usually the *drive*
//!     between two places, mislabeled "insufficient data".
//!
//! This deterministic pass (no LLM) walks the stored blocks and applies the shape
//! rules from `docs/event-timeline.md`:
//!
//!   > A block earns 15 minutes; a seam earns 3; a moment earns a mention, not a row.
//!
//!   - **Dwell / Unknown** are floored at 15 min. A sub-15-min Unknown is *absorbed*
//!     into a neighbour (the sliver disappears).
//!   - **Transit** is a seam between two genuinely different places (compared by
//!     resolved place **id**, so two same-named places still differ). It is exempt
//!     from the 15-min floor but has its own 3-min floor — below that a "move" is
//!     visit-boundary noise / GPS jitter and is absorbed like any sliver.
//!
//! Mode is descriptive; salience is decisive — so transit blocks are *kept and
//! scored* like any event (this pass runs before annotate/novelty). A silent drive
//! scores low and recedes; a labelled content-drive was already a named event from
//! the detective and never reaches this pass as Unknown.

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgPool, Row};

/// A block earns 15 minutes. A sub-15-min Unknown is absorbed into its neighbour.
const MIN_BLOCK_MINUTES: i64 = 15;
/// A seam earns 3. A sub-3-min "transit" is visit-boundary noise, not a move.
const MIN_TRANSIT_MINUTES: i64 = 3;

/// A stored auto event, the classifier's input unit. `place_*` is the resolved
/// place of the visit overlapping this block (None for a moving/dead span).
#[derive(Debug, Clone)]
struct Block {
    id: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    is_unknown: bool,
    is_transit: bool,
    is_sleep: bool,
    place_id: Option<String>,
    place_name: Option<String>,
}

impl Block {
    fn minutes(&self) -> i64 {
        (self.end - self.start).num_minutes()
    }
}

/// One mutation the classifier decides — carries everything the SQL apply needs, so
/// `classify` stays a pure function over the loaded snapshot.
#[derive(Debug, Clone, PartialEq)]
enum Op {
    /// Delete `block_id` and extend `neighbour_id` to cover its span. `into_prev`
    /// means extend the previous block's `end_time` (else pull the next block's
    /// `start_time` back); `boundary` is the new value for that column.
    Absorb {
        block_id: String,
        neighbour_id: String,
        into_prev: bool,
        boundary: DateTime<Utc>,
    },
    /// Flip `block_id` from Unknown to a transit event (empty movement the detective
    /// left for us — we supply the "A → B" label).
    MakeTransit {
        block_id: String,
        label: String,
        summary: String,
    },
    /// Set `is_transit` on a block the detective already *named* (e.g. "Transit to
    /// office", or "call with Tony on the drive home") — keep its label, just record
    /// that it was movement so the flag is consistent however the block was labelled.
    FlagTransit {
        block_id: String,
    },
}

/// The pure decision: given the day's ordered blocks, decide the mutations. No I/O,
/// so it is unit-tested directly.
///
/// Only `is_unknown` blocks are reclassified; dwells, sleep, and existing transit
/// are left alone. Consecutive Unknowns cannot occur (backfill merges them and sleep
/// separates its head/tail with the Sleep block), so every Unknown's neighbours are
/// non-Unknown and absorbs never interact.
fn classify(blocks: &[Block]) -> Vec<Op> {
    let mut ops = Vec::new();
    for i in 0..blocks.len() {
        let b = &blocks[i];
        let prev = i.checked_sub(1).map(|j| &blocks[j]);
        let next = blocks.get(i + 1);

        // A location change is two DIFFERENT resolved place ids on either side.
        let location_change = match (prev.and_then(|p| p.place_id.as_ref()), next.and_then(|n| n.place_id.as_ref())) {
            (Some(a), Some(c)) => a != c,
            _ => false,
        };

        if !b.is_unknown {
            // A NAMED block with no dwell-place of its own, wedged between two
            // different places, is movement the detective already labelled ("Transit
            // to office", or a content-headlined drive). Flag it so is_transit is
            // consistent whoever named the move — but keep the detective's label.
            if !b.is_sleep && !b.is_transit && b.place_id.is_none() && location_change {
                ops.push(Op::FlagTransit { block_id: b.id.clone() });
            }
            continue;
        }
        let dur = b.minutes();

        if location_change {
            if dur >= MIN_TRANSIT_MINUTES {
                let a = prev.and_then(|p| p.place_name.clone());
                let c = next.and_then(|n| n.place_name.clone());
                let (label, summary) = match (a, c) {
                    (Some(a), Some(c)) => (format!("{a} → {c}"), format!("Moved from {a} to {c}.")),
                    _ => ("Transit".to_string(), "Movement between locations.".to_string()),
                };
                ops.push(Op::MakeTransit { block_id: b.id.clone(), label, summary });
            } else if let Some(op) = absorb(blocks, i) {
                ops.push(op); // sub-3-min location change = jitter
            }
        } else if dur < MIN_BLOCK_MINUTES {
            if let Some(op) = absorb(blocks, i) {
                ops.push(op);
            }
        }
        // else: >= 15 min, no location change → a real Unknown; keep it.
    }
    ops
}

/// Build an `Absorb` op for block `i`, preferring the previous neighbour — but
/// **never extend a sleep block** (sleep is authoritative and bounded by real data).
/// Falls back to the next neighbour; if both are sleep/absent, returns None (keep the
/// block rather than inflate sleep or hang off the day edge).
fn absorb(blocks: &[Block], i: usize) -> Option<Op> {
    let b = &blocks[i];
    let prev = i.checked_sub(1).map(|j| &blocks[j]);
    let next = blocks.get(i + 1);
    if let Some(p) = prev.filter(|p| !p.is_sleep) {
        Some(Op::Absorb { block_id: b.id.clone(), neighbour_id: p.id.clone(), into_prev: true, boundary: b.end })
    } else if let Some(n) = next.filter(|n| !n.is_sleep) {
        Some(Op::Absorb { block_id: b.id.clone(), neighbour_id: n.id.clone(), into_prev: false, boundary: b.start })
    } else {
        None
    }
}

/// Classify one day's gaps in place. Idempotent: a second run finds no sub-floor
/// Unknowns and no un-flagged movement, so it produces no ops.
pub async fn classify_day_gaps(pool: &PgPool, date: NaiveDate) -> crate::error::Result<u32> {
    let day_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM wiki_days WHERE date = $1")
            .bind(date)
            .fetch_optional(pool)
            .await?;
    let Some(day_id) = day_id else { return Ok(0) };

    // The day's auto blocks, gapless and ordered.
    let rows = sqlx::query(
        "SELECT id, start_time, end_time, is_unknown, is_transit, is_sleep \
         FROM wiki_events \
         WHERE day_id = $1 AND is_user_added = FALSE \
         ORDER BY start_time",
    )
    .bind(&day_id)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(0);
    }

    let mut blocks: Vec<Block> = rows
        .iter()
        .map(|r| Block {
            id: r.get("id"),
            start: r.get("start_time"),
            end: r.get("end_time"),
            is_unknown: r.get("is_unknown"),
            is_transit: r.get("is_transit"),
            is_sleep: r.get("is_sleep"),
            place_id: None,
            place_name: None,
        })
        .collect();

    // The day's UTC window, straight off the blocks (they already cover 00:00–24:00
    // local, stored as UTC) — no need to re-resolve the timezone here.
    let win_start = blocks.first().unwrap().start;
    let win_end = blocks.last().unwrap().end;

    // Visits for the day, each with its RESOLVED place id + name
    // (data_location_visit → wiki_refs[place] → wiki_places).
    let visits = sqlx::query(
        "SELECT er.entity_id AS place_id, p.name AS place_name, v.arrival_time, v.departure_time \
         FROM data_location_visit v \
         JOIN wiki_refs er \
           ON er.source_table = 'data_location_visit' AND er.source_id = v.id \
          AND er.entity_type = 'place' \
         JOIN wiki_places p ON p.id = er.entity_id \
         WHERE v.departure_time >= $1::timestamptz AND v.arrival_time <= $2::timestamptz",
    )
    .bind(win_start)
    .bind(win_end)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Attach each block's place = the visit with the largest overlap of its window.
    for b in blocks.iter_mut() {
        let mut best: Option<(i64, String, String)> = None;
        for v in &visits {
            let arr: DateTime<Utc> = v.get("arrival_time");
            let dep: DateTime<Utc> = v
                .try_get::<Option<DateTime<Utc>>, _>("departure_time")
                .ok()
                .flatten()
                .unwrap_or(win_end);
            let ov = ((b.end.min(dep)) - (b.start.max(arr))).num_minutes();
            if ov > 0 && best.as_ref().map_or(true, |(o, _, _)| ov > *o) {
                let pid: String = v.get("place_id");
                let pname: Option<String> = v.try_get("place_name").ok().flatten();
                best = Some((ov, pid, pname.unwrap_or_default()));
            }
        }
        if let Some((_, pid, pname)) = best {
            b.place_id = Some(pid);
            b.place_name = Some(pname).filter(|s| !s.is_empty());
        }
    }

    let ops = classify(&blocks);
    let n = ops.len() as u32;
    for op in ops {
        apply(pool, op).await;
    }

    if n > 0 {
        tracing::info!(date = %date, ops = n, "gap classification: slivers absorbed, transit labelled");
    }
    Ok(n)
}

/// Apply one op. Only ever touches `is_user_added = FALSE` rows. Best-effort per the
/// same convention as `dayline::sleep` — a single failed surgery must not abort the
/// nightly chain.
async fn apply(pool: &PgPool, op: Op) {
    match op {
        Op::Absorb { block_id, neighbour_id, into_prev, boundary } => {
            let extend = if into_prev {
                "UPDATE wiki_events SET end_time = $1 WHERE id = $2 AND is_user_added = FALSE"
            } else {
                "UPDATE wiki_events SET start_time = $1 WHERE id = $2 AND is_user_added = FALSE"
            };
            let _ = sqlx::query(extend).bind(boundary).bind(&neighbour_id).execute(pool).await;
            let _ = sqlx::query("DELETE FROM wiki_events WHERE id = $1 AND is_user_added = FALSE")
                .bind(&block_id)
                .execute(pool)
                .await;
        }
        Op::MakeTransit { block_id, label, summary } => {
            let _ = sqlx::query(
                "UPDATE wiki_events \
                 SET kind = 'transit', auto_label = $1, event_summary = $2 \
                 WHERE id = $3 AND is_user_added = FALSE",
            )
            .bind(&label)
            .bind(&summary)
            .bind(&block_id)
            .execute(pool)
            .await;
        }
        Op::FlagTransit { block_id } => {
            let _ = sqlx::query(
                "UPDATE wiki_events SET kind = 'transit' WHERE id = $1 AND is_user_added = FALSE",
            )
            .bind(&block_id)
            .execute(pool)
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(id: &str, start_min: i64, end_min: i64, is_unknown: bool, place: Option<&str>) -> Block {
        let base = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        Block {
            id: id.to_string(),
            start: base + chrono::Duration::minutes(start_min),
            end: base + chrono::Duration::minutes(end_min),
            is_unknown,
            is_transit: false,
            is_sleep: false,
            place_id: place.map(|p| format!("place_{p}")),
            place_name: place.map(|p| p.to_string()),
        }
    }

    #[test]
    fn sub_15_same_place_sliver_is_absorbed() {
        // desk work → (5-min unknown, same place) → desk work. The sliver vanishes
        // into the previous block.
        let blocks = vec![
            block("a", 0, 60, false, Some("office")),
            block("gap", 60, 65, true, None), // no place; neighbours are same place
            block("b", 65, 120, false, Some("office")),
        ];
        let ops = classify(&blocks);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0], Op::Absorb {
            block_id: "gap".into(),
            neighbour_id: "a".into(),
            into_prev: true,
            boundary: blocks[1].end,
        });
    }

    #[test]
    fn location_change_over_3min_becomes_transit() {
        // office (ends) → 6-min gap → restaurant (starts). The drive.
        let blocks = vec![
            block("a", 0, 60, false, Some("office")),
            block("gap", 60, 66, true, None),
            block("b", 66, 120, false, Some("restaurant")),
        ];
        let ops = classify(&blocks);
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            Op::MakeTransit { block_id, label, .. } => {
                assert_eq!(block_id, "gap");
                assert_eq!(label, "office → restaurant");
            }
            other => panic!("expected MakeTransit, got {other:?}"),
        }
    }

    #[test]
    fn sub_3min_location_change_is_jitter_absorbed() {
        // two different places 2 min apart — visit-boundary noise, not a move.
        let blocks = vec![
            block("a", 0, 60, false, Some("home")),
            block("gap", 60, 62, true, None),
            block("b", 62, 120, false, Some("garage")),
        ];
        let ops = classify(&blocks);
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], Op::Absorb { .. }), "sub-3min location change absorbs, not transit");
    }

    #[test]
    fn long_same_place_unknown_is_kept() {
        // 40-min dead zone at a known place — a real Unknown, not a sliver.
        let blocks = vec![
            block("a", 0, 60, false, Some("home")),
            block("gap", 60, 100, true, None),
            block("b", 100, 160, false, Some("home")),
        ];
        assert!(classify(&blocks).is_empty());
    }

    #[test]
    fn never_extends_sleep_neighbour() {
        // a short unknown wedged after Sleep must NOT extend the sleep block.
        let mut sleep = block("sleep", 0, 400, false, None);
        sleep.is_sleep = true;
        let blocks = vec![
            sleep,
            block("gap", 400, 405, true, None),
            block("morning", 405, 500, false, Some("home")),
        ];
        let ops = classify(&blocks);
        assert_eq!(ops.len(), 1);
        // absorbs forward into "morning", never back into "sleep".
        assert_eq!(ops[0], Op::Absorb {
            block_id: "gap".into(),
            neighbour_id: "morning".into(),
            into_prev: false,
            boundary: blocks[1].start,
        });
    }

    #[test]
    fn detective_named_movement_gets_flagged_not_relabelled() {
        // The detective labelled the commute itself ("Transit to office") — a NAMED
        // block with no place of its own between two different places. Flag it,
        // keep the label.
        let blocks = vec![
            block("home", 0, 60, false, Some("home")),
            block("commute", 60, 64, false, None), // named, no own place
            block("office", 64, 120, false, Some("office")),
        ];
        let ops = classify(&blocks);
        assert_eq!(ops, vec![Op::FlagTransit { block_id: "commute".into() }]);
    }

    #[test]
    fn named_block_with_its_own_place_is_not_transit() {
        // a quick store stop between home and office HAS its own place → a dwell,
        // not transit. Must not be flagged.
        let blocks = vec![
            block("home", 0, 60, false, Some("home")),
            block("store", 60, 75, false, Some("store")),
            block("office", 75, 120, false, Some("office")),
        ];
        assert!(classify(&blocks).is_empty());
    }

    #[test]
    fn idempotent_on_settled_timeline() {
        // no unknown slivers, no un-flagged movement (both dwells have their own
        // place) → nothing to do.
        let blocks = vec![
            block("a", 0, 60, false, Some("office")),
            block("b", 60, 120, false, Some("restaurant")),
        ];
        assert!(classify(&blocks).is_empty());
    }
}
