//! The lifeline — a life at a glance, bucketed.
//!
//! The console is a QUERY problem before it is a chart problem, so the endpoint
//! shape is the decision that matters and everything visual follows from it.
//! A life-scale view cannot ship rows to the client: the measured corpus is
//! ~330k rows across all lanes today and a real box grows without bound. So the
//! server buckets and the client draws densities.
//!
//! **Lanes come from the registry's `lane` field**, declared per ontology. They
//! keep the names the data already uses — location, health, communication,
//! activity, financial — because a lane called "Place" or "Attention" reads
//! well and matches nothing you could grep for.
//!
//! Excluded, deliberately: `calendar` (intent, not evidence — and the one
//! source that routinely lies), `environment` (conditions, not conduct),
//! `wiki_events` (DERIVED from the lanes, so drawing it as a peer
//! double-counts them), and the record's own artifacts.
//!
//! **A lane also reports when it started.** The lanes have wildly different
//! reach — communication goes back to 2017, location has been collecting for
//! three weeks — and drawing a flat zero across the years before a collector
//! existed says "nothing happened" when the truth is "nothing was watching".
//! `first_seen` is what lets a client draw that difference.

use serde::Serialize;
use sqlx::PgPool;

use crate::error::{Error, Result};

/// Bucket ceiling. A viewport has on the order of a thousand pixels, and a
/// bucket narrower than a pixel is work nobody can see.
const MAX_BUCKETS: i32 = 2_000;

#[derive(Debug, Serialize)]
pub struct Lifeline {
    pub from: chrono::DateTime<chrono::Utc>,
    pub to: chrono::DateTime<chrono::Utc>,
    pub buckets: i32,
    pub lanes: Vec<Lane>,
}

#[derive(Debug, Serialize)]
pub struct Lane {
    /// The registry `domain`.
    pub id: String,
    /// Which tables fed it — so a reader can tell an empty lane from a missing one.
    pub sources: Vec<String>,
    /// One value per bucket, left to right. Always `buckets` long, zeros
    /// included: a sparse map would make the client reconstruct the axis, and
    /// the axis is the one thing the server already knows exactly.
    ///
    /// `f64` because a measure is rarely a count — hours asleep, dollars,
    /// average bpm.
    pub density: Vec<f64>,
    /// The largest bucket, so a client can scale a lane without a second pass.
    pub peak: f64,
    /// The smallest non-empty bucket.
    ///
    /// Only meaningful for a `rate`, and there it is essential: a resting heart
    /// rate of 60 drawn from a zero baseline spends 40% of the row saying
    /// "alive". A band between floor and peak uses the row for the variation,
    /// which is the only part anyone is looking at.
    pub floor: f64,
    /// The earliest record this lane has ever held, or `None` if it holds none.
    ///
    /// Everything before this is OUTSIDE the lane's coverage, not empty within
    /// it. A client must render the two differently or the chart asserts that a
    /// life had no location for eight years, when what it had was no collector.
    pub first_seen: Option<chrono::DateTime<chrono::Utc>>,
    /// Which measure produced `density` — `records` when none was asked for.
    pub measure: String,
    pub measure_label: String,
    pub unit: String,
    /// `total` or `rate`; decides both rescaling and how the lane is drawn.
    pub kind: String,
    /// What else this lane could plot, so the client needs no second endpoint
    /// to offer the menu.
    pub available: Vec<MeasureInfo>,
}

/// One entry in a lane's measure menu.
#[derive(Debug, Serialize)]
pub struct MeasureInfo {
    pub id: String,
    pub label: String,
    pub unit: String,
    pub kind: String,
}

/// The default: every row in the lane, counted. Honest, and nearly meaningless
/// for four lanes of five — which is why measures exist — but it is the only
/// thing that works before a lane's shape is known.
const RECORDS: &str = "records";

fn kind_str(k: virtues_registry::ontologies::MeasureKind) -> &'static str {
    match k {
        virtues_registry::ontologies::MeasureKind::Total => "total",
        virtues_registry::ontologies::MeasureKind::Rate => "rate",
    }
}

/// Lanes and their member tables, straight from the registry.
fn lanes_from_registry() -> Vec<(String, Vec<(&'static str, &'static str)>)> {
    use std::collections::BTreeMap;
    let mut by_domain: BTreeMap<String, Vec<(&'static str, &'static str)>> = BTreeMap::new();

    for o in virtues_registry::ontologies::registered_ontologies() {
        // One declaration, in the registry, visible to every consumer — rather
        // than a blocklist here that the next new domain walks straight past.
        let Some(lane) = o.lane else { continue };
        by_domain
            .entry(lane.to_string())
            .or_default()
            .push((o.table_name, o.timestamp_column));
    }

    // One table can appear under two ontologies; count it once per lane.
    for members in by_domain.values_mut() {
        members.sort();
        members.dedup_by(|a, b| a.0 == b.0);
    }
    by_domain.into_iter().collect()
}

/// The full span of the record — the default window.
///
/// A lifeline defaulted to "the last year" is not a lifeline. On a real box the
/// corpus reaches back to **2017** (3,144 days) while the collectors that
/// produce location and activity only started months ago, so a 365-day window
/// showed one year of a nine-year life with everything recent crushed against
/// the right edge and half the chart blank. The window has to come from the
/// data.
pub async fn corpus_span(
    pool: &PgPool,
) -> Result<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> {
    let mut earliest: Option<chrono::DateTime<chrono::Utc>> = None;
    let mut latest: Option<chrono::DateTime<chrono::Utc>> = None;

    for (_, members) in lanes_from_registry() {
        for (table, ts) in members {
            let row: Option<(Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>)> =
                sqlx::query_as(&format!("SELECT min({ts}), max({ts}) FROM {table}"))
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| Error::Database(format!("span {table}: {e}")))?;
            if let Some((lo, hi)) = row {
                if let Some(lo) = lo {
                    earliest = Some(earliest.map_or(lo, |e| e.min(lo)));
                }
                if let Some(hi) = hi {
                    latest = Some(latest.map_or(hi, |e| e.max(hi)));
                }
            }
        }
    }

    // Clamp to now. A recurring calendar entry projects decades forward — this
    // box holds events dated 2087 — and `max()` across lanes would happily make
    // that the right edge of the chart, compressing a real life into the first
    // 12% of the canvas. The future is not part of the record.
    let now = chrono::Utc::now();
    let to = latest.map(|t| t.min(now)).unwrap_or(now);
    // A box with no data at all still needs a window a chart can draw.
    let from = earliest.unwrap_or(to - chrono::Duration::days(365));
    Ok((from, to))
}

/// Per-lane, per-bucket density over a window.
///
/// `measures` selects a non-default measure per lane, as `lane:measure_id`
/// pairs. An unknown lane or id is ignored rather than refused: these come from
/// a URL a person can edit and share, and a stale link should degrade to the
/// default view, not to an error page.
pub async fn get_lifeline(
    pool: &PgPool,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
    buckets: i32,
    only: Option<Vec<String>>,
    expand: Option<Vec<String>>,
    measures: Option<Vec<String>>,
) -> Result<Lifeline> {
    use virtues_registry::ontologies::{measures_for_lane, LaneMeasure};

    if to <= from {
        return Err(Error::InvalidInput("`to` must be after `from`".into()));
    }
    let buckets = buckets.clamp(1, MAX_BUCKETS);
    let wanted = only.unwrap_or_default();
    let expand = expand.unwrap_or_default();

    // lane -> measure id, from `health:heart_rate` pairs.
    let chosen: std::collections::HashMap<String, String> = measures
        .unwrap_or_default()
        .iter()
        .filter_map(|s| s.split_once(':').map(|(l, m)| (l.to_string(), m.to_string())))
        .collect();

    // Vertical resolution: an expanded lane is replaced by one row per member
    // table, so "health" becomes sleep, heart rate, steps, workouts. The same
    // query runs either way — a lane is just a set of tables, and expanding
    // means running it per table instead of over the union.
    let mut plan: Vec<(String, Vec<(&'static str, &'static str)>)> = Vec::new();
    for (domain, members) in lanes_from_registry() {
        if !wanted.is_empty() && !wanted.contains(&domain) {
            continue;
        }
        // A measure already names a single table, so expanding under one would
        // split a lane into rows that cannot all answer it.
        if expand.contains(&domain) && members.len() > 1 && !chosen.contains_key(&domain) {
            for (table, ts) in members {
                // Named for the part that differs: `data_health_sleep` reads as
                // "sleep" under a health lane, and the full table name is noise
                // repeated down the column.
                let short = table
                    .strip_prefix("data_")
                    .and_then(|t| t.strip_prefix(&format!("{domain}_")))
                    .unwrap_or(table)
                    .to_string();
                plan.push((format!("{domain}/{short}"), vec![(table, ts)]));
            }
        } else {
            plan.push((domain, members));
        }
    }

    let mut lanes = Vec::new();
    for (domain, members) in plan {
        let root = domain.split('/').next().unwrap_or(&domain).to_string();

        // The menu this row can offer. On a member row it is only the measures
        // that read that member's table — offering "spend" beside `health/sleep`
        // would be a control that cannot work.
        let tables: Vec<&str> = members.iter().map(|(t, _)| *t).collect();
        let available: Vec<LaneMeasure> = measures_for_lane(&root)
            .into_iter()
            .filter(|m| tables.contains(&m.table))
            .collect();

        let picked: Option<LaneMeasure> = chosen
            .get(&domain)
            .and_then(|id| available.iter().find(|m| m.id == id).copied());

        // Table, column and aggregate all come from the registry as
        // compile-time constants, never from a request; the window and bucket
        // count are bound. `width_bucket` does the arithmetic in Postgres so a
        // lane is one round trip regardless of how many tables feed it.
        let bucket_of = |expr: &str| {
            format!(
                "width_bucket(EXTRACT(EPOCH FROM {expr}), \
                              EXTRACT(EPOCH FROM $1::timestamptz), \
                              EXTRACT(EPOCH FROM $2::timestamptz), $3)"
            )
        };

        let sql = match &picked {
            Some(m) => {
                let and = m.filter.map(|f| format!(" AND ({f})")).unwrap_or_default();
                format!(
                    "SELECT {b} AS b, ({agg})::float8 AS v \
                     FROM {table} \
                     WHERE {ts} >= $1 AND {ts} < $2{and} \
                     GROUP BY 1",
                    b = bucket_of(m.timestamp_column),
                    agg = m.agg,
                    table = m.table,
                    ts = m.timestamp_column,
                )
            }
            None => {
                let unions: Vec<String> = members
                    .iter()
                    .map(|(table, ts)| {
                        format!("SELECT {ts} AS ts FROM {table} WHERE {ts} >= $1 AND {ts} < $2")
                    })
                    .collect();
                format!(
                    "SELECT {b} AS b, count(*)::float8 AS v FROM ({u}) x GROUP BY 1",
                    b = bucket_of("ts"),
                    u = unions.join(" UNION ALL "),
                )
            }
        };

        let rows: Vec<(i32, Option<f64>)> = sqlx::query_as(&sql)
            .bind(from)
            .bind(to)
            .bind(buckets)
            .fetch_all(pool)
            .await
            .map_err(|e| Error::Database(format!("lifeline lane {domain}: {e}")))?;

        let mut density = vec![0f64; buckets as usize];
        for (b, v) in rows {
            // width_bucket returns 1..=buckets inside the range, and
            // 0 / buckets+1 for values on the edges — which the WHERE already
            // excludes, but clamping here means a boundary row can never panic.
            let idx = (b - 1).clamp(0, buckets - 1) as usize;
            // A `rate` groups per bucket, so one row per bucket and += is a
            // plain assignment; a `records` union can emit the same bucket
            // once per table.
            density[idx] += v.unwrap_or(0.0);
        }
        let peak = density.iter().copied().fold(0f64, f64::max);
        let floor = density
            .iter()
            .copied()
            .filter(|v| *v > 0.0)
            .fold(f64::INFINITY, f64::min);
        let floor = if floor.is_finite() { floor } else { 0.0 };

        // When this lane started existing — not when it started having data in
        // the current window. Asked of the whole table, deliberately, and under
        // the measure's own filter: a lane plotting `income` has no coverage
        // before the first refund, whatever the account table says.
        let coverage: Vec<(&str, &str, Option<&str>)> = match &picked {
            Some(m) => vec![(m.table, m.timestamp_column, m.filter)],
            None => members.iter().map(|(t, ts)| (*t, *ts, None)).collect(),
        };
        let mut first_seen: Option<chrono::DateTime<chrono::Utc>> = None;
        for (table, ts, filter) in coverage {
            let where_ = filter.map(|f| format!(" WHERE ({f})")).unwrap_or_default();
            let lo: Option<Option<chrono::DateTime<chrono::Utc>>> =
                sqlx::query_scalar(&format!("SELECT min({ts}) FROM {table}{where_}"))
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| Error::Database(format!("first_seen {table}: {e}")))?;
            if let Some(Some(lo)) = lo {
                first_seen = Some(first_seen.map_or(lo, |f| f.min(lo)));
            }
        }

        lanes.push(Lane {
            id: domain,
            sources: members.iter().map(|(t, _)| t.to_string()).collect(),
            density,
            peak,
            floor,
            first_seen,
            measure: picked.map(|m| m.id.to_string()).unwrap_or_else(|| RECORDS.into()),
            measure_label: picked
                .map(|m| m.label.to_string())
                .unwrap_or_else(|| "records".into()),
            unit: picked.map(|m| m.unit.to_string()).unwrap_or_default(),
            kind: picked.map(|m| kind_str(m.kind)).unwrap_or("total").to_string(),
            available: available
                .iter()
                .map(|m| MeasureInfo {
                    id: m.id.to_string(),
                    label: m.label.to_string(),
                    unit: m.unit.to_string(),
                    kind: kind_str(m.kind).to_string(),
                })
                .collect(),
        });
    }

    Ok(Lifeline { from, to, buckets, lanes })
}

// ───────────────────────────────────────────────────────────────────────────
// Ground — where the window was spent
// ───────────────────────────────────────────────────────────────────────────
//
// A density bar is worthless for location. "How many GPS pings in March" is
// not a question anyone has; the question about location is always WHERE, and
// no amount of bar chart answers it. So the location lane gets a second view,
// keyed to the same window the lanes are drawn over: brush a stretch of the
// timeline and this is the ground it covered.
//
// **Clustered by coordinate, not by name.** `data_location_visit.place_name`
// is NULL on every one of the 462 rows a real box holds — the collector has
// never populated it. Grouping by name would therefore return one bucket
// called nothing. Rounding to three decimal places (~110 m at this latitude)
// puts every arrival at the same doorway in one stay, which is what the column
// was supposed to do.
//
// **No basemap.** Not a missing feature — sending a person's coordinate
// history to a tile server to have their own life drawn back at them would
// contradict the product. The trace IS the map: 100k points over three weeks
// draw the road network on their own.

/// A place the window was spent, found by clustering arrivals.
#[derive(Debug, Serialize)]
pub struct Stay {
    pub lat: f64,
    pub lon: f64,
    /// Arrivals that fell in this cluster.
    pub visits: i64,
    pub minutes: f64,
    pub first: Option<chrono::DateTime<chrono::Utc>>,
    pub last: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
pub struct Ground {
    /// `[lat_min, lat_max, lon_min, lon_max]`, or `None` when the window holds
    /// no location at all.
    pub bbox: Option<[f64; 4]>,
    /// The trace, thinned to something a canvas can draw. Flat pairs rather
    /// than objects: 4,000 points as `{lat,lon}` is 140 KB of punctuation.
    pub track: Vec<[f64; 2]>,
    /// Total points before thinning, so the client can say what it is showing.
    pub track_total: i64,
    pub stays: Vec<Stay>,
}

/// How many track points to ship. A canvas at this size cannot resolve more,
/// and the shape of a city is legible long before the last thousand.
const MAX_TRACK: i64 = 4_000;
/// Cluster grain, in degrees. ~110 m of latitude.
const STAY_GRAIN: f64 = 0.001;
/// Stays closer than this are the same doorway.
///
/// A grid has edges, and a door that happens to sit on one gets split in two:
/// on a real box `30.258843,-97.752608` and `30.258934,-97.752385` came back as
/// separate places 25 m apart, one with 119 hours and one with 50. Rounding
/// alone cannot fix that — a finer grid moves the seam, it does not remove it —
/// so the grid is followed by a merge that does not care where the seam was.
const MERGE_METRES: f64 = 150.0;

/// Metres between two coordinates, flat-earth.
///
/// Exact enough well inside a kilometre, which is the only range this is asked
/// about, and it avoids a haversine for a comparison against 150 m.
fn metres_between(a: (f64, f64), b: (f64, f64)) -> f64 {
    let mid = ((a.0 + b.0) / 2.0).to_radians();
    let dy = (b.0 - a.0) * 111_320.0;
    let dx = (b.1 - a.1) * 111_320.0 * mid.cos();
    (dx * dx + dy * dy).sqrt()
}

/// Fold neighbouring grid cells into single places, biggest first.
///
/// Greedy rather than proper clustering: the input is at most 80 rows already
/// sorted by time spent, and seeding from the longest stay means a doorway
/// absorbs its own spillover rather than two halves negotiating a centre.
fn merge_stays(mut input: Vec<Stay>) -> Vec<Stay> {
    let mut out: Vec<Stay> = Vec::with_capacity(input.len());
    input.sort_by(|a, b| b.minutes.total_cmp(&a.minutes));

    for s in input {
        match out
            .iter_mut()
            .find(|o| metres_between((o.lat, o.lon), (s.lat, s.lon)) <= MERGE_METRES)
        {
            Some(o) => {
                // Centroid weighted by time, so the place lands where the hours
                // were actually spent rather than halfway between two counts.
                let w = o.minutes + s.minutes;
                if w > 0.0 {
                    o.lat = (o.lat * o.minutes + s.lat * s.minutes) / w;
                    o.lon = (o.lon * o.minutes + s.lon * s.minutes) / w;
                }
                o.minutes += s.minutes;
                o.visits += s.visits;
                o.first = match (o.first, s.first) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                o.last = match (o.last, s.last) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                };
            }
            None => out.push(s),
        }
    }
    out
}

pub async fn get_ground(
    pool: &PgPool,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> Result<Ground> {
    if to <= from {
        return Err(Error::InvalidInput("`to` must be after `from`".into()));
    }

    let stays: Vec<Stay> = sqlx::query_as!(
        Stay,
        r#"
        SELECT round(avg(latitude)::numeric, 6)::float8  AS "lat!",
               round(avg(longitude)::numeric, 6)::float8 AS "lon!",
               count(*)                                  AS "visits!",
               COALESCE(sum(duration_minutes), 0)::float8 AS "minutes!",
               min(arrival_time)                         AS "first",
               max(COALESCE(departure_time, arrival_time)) AS "last"
        FROM data_location_visit
        WHERE arrival_time >= $1 AND arrival_time < $2
          AND latitude IS NOT NULL AND longitude IS NOT NULL
        GROUP BY round((latitude / $3)::numeric), round((longitude / $3)::numeric)
        ORDER BY 4 DESC
        LIMIT 80
        "#,
        from,
        to,
        STAY_GRAIN,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("ground stays: {e}")))?;

    let stays = merge_stays(stays);

    let track_total: i64 = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM data_location_point
           WHERE timestamp >= $1 AND timestamp < $2 AND latitude IS NOT NULL"#,
        from,
        to,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("ground count: {e}")))?;

    // Every nth point in time order. Deterministic, unlike TABLESAMPLE, so
    // panning back to a window redraws the same trace rather than a new
    // shimmer of it.
    let step = (track_total / MAX_TRACK).max(1);
    let rows = sqlx::query!(
        r#"
        SELECT latitude AS "lat!", longitude AS "lon!"
        FROM (
            SELECT latitude, longitude,
                   row_number() OVER (ORDER BY timestamp) AS rn
            FROM data_location_point
            WHERE timestamp >= $1 AND timestamp < $2
              AND latitude IS NOT NULL AND longitude IS NOT NULL
        ) s
        WHERE rn % $3 = 0
        "#,
        from,
        to,
        step,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("ground track: {e}")))?;

    let track: Vec<[f64; 2]> = rows.into_iter().map(|r| [r.lat, r.lon]).collect();

    // The extent of everything drawn — stays included, because a stay outside
    // the thinned track would otherwise fall off the edge of the canvas.
    let mut bbox: Option<[f64; 4]> = None;
    let mut grow = |lat: f64, lon: f64| {
        bbox = Some(match bbox {
            None => [lat, lat, lon, lon],
            Some([a, b, c, d]) => [a.min(lat), b.max(lat), c.min(lon), d.max(lon)],
        });
    };
    for p in &track {
        grow(p[0], p[1]);
    }
    for s in &stays {
        grow(s.lat, s.lon);
    }

    Ok(Ground { bbox, track, track_total, stays })
}

// ───────────────────────────────────────────────────────────────────────────
// The feed — the records themselves
// ───────────────────────────────────────────────────────────────────────────
//
// The point of selecting a stretch of time is to SEE WHAT IS IN IT. A panel of
// sums is the same answer chat already gives badly; the reason to draw a
// timeline at all is that a range on it can hand back the rows. So the lanes
// are the index and this is the text: brush three weeks of 2019 and read the
// messages.
//
// **One resolution, all the way down.** The same endpoint answers a decade and
// an afternoon — a decade just has more rows behind the same `limit`. Nothing
// switches modes as you zoom; the window narrows and the feed sharpens.
//
// **The registry already knew how to render every row.** `DaySourceConfig`
// declares `label_sql`, `preview_sql` and `id_sql` per ontology because the day
// pipeline needed exactly this: one line a person can read. Eighteen ontologies
// carry it. Writing a second rendering table here would have meant two
// descriptions of one row, drifting apart.
//
// **Continuous ontologies are excluded on purpose.** Heart rate has 22,911 rows
// and not one of them is a thing that happened; a feed of `72 bpm` repeated
// forever is noise wearing the costume of detail. `TemporalType` already draws
// that line, so this reads it rather than guessing.

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Record {
    pub id: String,
    pub ontology: String,
    pub lane: String,
    /// The day pipeline's `source_type` — `message:imessage`, `transaction`.
    pub kind: String,
    pub label: Option<String>,
    pub preview: Option<String>,
    pub at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct Feed {
    pub records: Vec<Record>,
    /// Whether another page exists, learned by asking for one more row than
    /// wanted. A true count means counting 169k rows to render 50.
    pub has_more: bool,
}

/// Hard ceiling on one page.
const MAX_FEED: i64 = 200;

/// Ontologies that produce rows a person would read, in lanes.
fn feedable() -> Vec<virtues_registry::ontologies::OntologyDescriptor> {
    use virtues_registry::ontologies::TemporalType;
    virtues_registry::ontologies::registered_ontologies()
        .into_iter()
        .filter(|o| {
            o.lane.is_some()
                && o.day_source.is_some()
                && matches!(o.temporal_type, TemporalType::Discrete)
        })
        .collect()
}

pub async fn get_feed(
    pool: &PgPool,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
    only: Option<Vec<String>>,
    limit: i64,
    offset: i64,
) -> Result<Feed> {
    if to <= from {
        return Err(Error::InvalidInput("`to` must be after `from`".into()));
    }
    let limit = limit.clamp(1, MAX_FEED);
    let offset = offset.max(0);
    let wanted = only.unwrap_or_default();

    let mut branches: Vec<String> = Vec::new();
    for o in feedable() {
        let lane = o.lane.unwrap();
        if !wanted.is_empty() && !wanted.contains(&lane.to_string()) {
            continue;
        }
        let d = o.day_source.as_ref().unwrap();
        let ts = o.timestamp_column;
        // `extra_where` carries its own leading `AND` on two of the eighteen
        // (`AND t.is_open = false`) and not on the rest. Strip it and add our
        // own, so both spellings survive and the clause is still parenthesised
        // — an unbracketed `a OR b` appended to a range filter would widen the
        // window instead of narrowing it.
        let and = d
            .extra_where
            .map(|w| {
                let w = w.trim();
                let w = w.strip_prefix("AND ").or_else(|| w.strip_prefix("and ")).unwrap_or(w);
                format!(" AND ({w})")
            })
            .unwrap_or_default();

        // Each branch is ordered and cut on its OWN index before the union
        // sorts. Without this the planner reads every row in the window from
        // all eighteen tables — 169k messages to show fifty — and a wide
        // selection takes seconds.
        branches.push(format!(
            "(SELECT ({id})::text AS id, '{name}' AS ontology, '{lane}' AS lane, \
                     ({kind})::text AS kind, ({label})::text AS label, \
                     ({preview})::text AS preview, t.{ts} AS at \
              FROM {table} t \
              WHERE t.{ts} >= $1 AND t.{ts} < $2{and} \
              ORDER BY t.{ts} DESC LIMIT $3)",
            id = d.id_sql,
            name = o.name,
            kind = d.source_type_sql.unwrap_or("''"),
            label = d.label_sql,
            preview = d.preview_sql,
            table = o.table_name,
            ts = ts,
        ));
    }

    if branches.is_empty() {
        return Ok(Feed { records: Vec::new(), has_more: false });
    }

    // One extra row, purely to answer "is there more".
    let want = limit + 1;
    let sql = format!(
        "SELECT id, ontology, lane, kind, label, preview, at FROM ({}) u \
         ORDER BY at DESC OFFSET $4 LIMIT $5",
        branches.join(" UNION ALL ")
    );

    let mut records: Vec<Record> = sqlx::query_as(&sql)
        .bind(from)
        .bind(to)
        .bind(offset + want)
        .bind(offset)
        .bind(want)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("lifeline feed: {e}")))?;

    let has_more = records.len() as i64 > limit;
    records.truncate(limit as usize);
    Ok(Feed { records, has_more })
}

// ───────────────────────────────────────────────────────────────────────────
// The processed layer
// ───────────────────────────────────────────────────────────────────────────
//
// Raw reaches back to 2017 and holds 330k rows; the interpreted layer — days
// segmented into events, with summaries — covers about three weeks. That gap
// is not a defect to hide, it is the single most useful fact about the record,
// and a reader who cannot see it will assume the timeline means the same thing
// everywhere on it. So `coverage` is returned even when the window holds
// nothing: the answer to "why is this empty" is "nothing has been processed
// here", and that sentence needs the dates to be worth reading.

#[derive(Debug, Serialize)]
pub struct Interpreted {
    pub id: String,
    /// `event` or `day`.
    pub kind: String,
    /// The segmenter's own classification — sleep, transit, unknown. The spine
    /// draws a night's sleep differently from a meeting, and guessing that from
    /// the label would be reading tea leaves when the column already says it.
    pub tag: Option<String>,
    pub label: Option<String>,
    pub summary: Option<String>,
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
pub struct Processed {
    pub items: Vec<Interpreted>,
    /// The span over which ANY interpretation exists, whatever the window.
    pub coverage: Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>,
    pub days_processed: i64,
}

pub async fn get_processed(
    pool: &PgPool,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
    limit: i64,
) -> Result<Processed> {
    if to <= from {
        return Err(Error::InvalidInput("`to` must be after `from`".into()));
    }
    let limit = limit.clamp(1, MAX_FEED);

    let items = sqlx::query_as!(
        Interpreted,
        r#"
        SELECT e.id                                       AS "id!",
               'event'                                    AS "kind!",
               CASE WHEN e.is_sleep    THEN 'sleep'
                    WHEN e.is_transit  THEN 'transit'
                    WHEN e.is_unknown  THEN 'unknown'
                    ELSE e.kind END                        AS "tag",
               COALESCE(e.user_label, e.auto_label)        AS "label",
               e.event_summary                             AS "summary",
               e.start_time                                AS "start!",
               e.end_time                                  AS "end"
        FROM wiki_events e
        WHERE e.start_time >= $1 AND e.start_time < $2
          AND COALESCE(e.user_hidden, false) = false
        ORDER BY e.start_time DESC
        LIMIT $3
        "#,
        from,
        to,
        limit,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("processed items: {e}")))?;

    let span = sqlx::query!(
        r#"SELECT min(start_time) AS "lo", max(COALESCE(end_time, start_time)) AS "hi",
                  count(DISTINCT day_id) AS "days!"
           FROM wiki_events WHERE COALESCE(user_hidden, false) = false"#
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("processed coverage: {e}")))?;

    Ok(Processed {
        items,
        coverage: span.lo.zip(span.hi),
        days_processed: span.days,
    })
}

// ───────────────────────────────────────────────────────────────────────────
// The day-clock — a life as its own rhythm
// ───────────────────────────────────────────────────────────────────────────
//
// **Why this and not a bar chart.** A density chart has one kind of mark, so it
// shows WEATHER and never LANDMARKS: you can see that something happened and
// never what. Nothing in it is findable by eye. This is the chronobiologist's
// actogram — time of day against date — and it is made of landmarks:
//
//   · sleep is a dark band, and you can watch it drift over years
//   · a TRIP dislocates the whole band by the time difference and puts it back
//   · weekends beat through as texture
//   · a bad month goes ragged
//
// **One timezone, deliberately.** Rendering each record in the zone it was
// recorded in would straighten the band back out and destroy the single most
// legible thing here. Fixing the whole raster to one zone is what makes two
// weeks in Tokyo a visible dislocation rather than a statistic.
//
// **Only activation signals.** See `activity_sources`: a watch samples a pulse
// all night, and including it would fill the exact rows the band is made of.

#[derive(Debug, Serialize)]
pub struct Clock {
    pub from: chrono::DateTime<chrono::Utc>,
    pub to: chrono::DateTime<chrono::Utc>,
    pub columns: i32,
    /// `columns * 24`, row-major by column: `cells[col * 24 + hour]`.
    ///
    /// Flat, because 28,800 numbers as `{col,hour,n}` objects is a quarter of a
    /// megabyte of punctuation for the same information.
    pub cells: Vec<i32>,
    /// The busiest single cell — the ceiling for a global scale.
    pub peak: i32,
    /// The busiest cell within each column, so a client can normalise a day
    /// against its own shape without a second pass. A quiet Sunday and a loud
    /// Monday should show the same rhythm at different volumes; scaling
    /// everything to the global peak would render the Sunday as empty.
    pub column_peak: Vec<i32>,
    /// Which zone the hours are in, echoed back for the axis labels.
    pub timezone: String,
}

/// Wider than this and a column is thinner than a pixel.
const MAX_COLUMNS: i32 = 1_400;

pub async fn get_clock(
    pool: &PgPool,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
    columns: i32,
    timezone: &str,
) -> Result<Clock> {
    if to <= from {
        return Err(Error::InvalidInput("`to` must be after `from`".into()));
    }
    let columns = columns.clamp(1, MAX_COLUMNS);

    // A bad zone name would abort the query at run time; a bad zone name that
    // reached the string below would do it inside interpolated SQL. Bound as a
    // parameter and validated first, so neither can happen.
    let known: bool = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM pg_timezone_names WHERE name = $1) AS "e!""#,
        timezone
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("timezone check: {e}")))?;
    let tz = if known { timezone } else { "UTC" };

    let sources = virtues_registry::ontologies::activity_sources();
    let unions: Vec<String> = sources
        .iter()
        .map(|s| {
            let and = s.filter.map(|f| format!(" AND ({f})")).unwrap_or_default();
            format!(
                "SELECT {ts} AS ts FROM {table} WHERE {ts} >= $1 AND {ts} < $2{and}",
                ts = s.timestamp_column,
                table = s.table
            )
        })
        .collect();

    let sql = format!(
        "SELECT width_bucket(EXTRACT(EPOCH FROM ts), \
                             EXTRACT(EPOCH FROM $1::timestamptz), \
                             EXTRACT(EPOCH FROM $2::timestamptz), $3) AS col, \
                EXTRACT(HOUR FROM ts AT TIME ZONE $4)::int AS hr, \
                count(*)::int AS n \
         FROM ({}) x GROUP BY 1, 2",
        unions.join(" UNION ALL ")
    );

    let rows: Vec<(i32, i32, i32)> = sqlx::query_as(&sql)
        .bind(from)
        .bind(to)
        .bind(columns)
        .bind(tz)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("clock: {e}")))?;

    let mut cells = vec![0i32; (columns as usize) * 24];
    let mut column_peak = vec![0i32; columns as usize];
    let mut peak = 0i32;
    for (col, hr, n) in rows {
        // width_bucket returns 0 and columns+1 for the edges, which the WHERE
        // already excludes; clamping means a boundary row can never panic.
        let c = (col - 1).clamp(0, columns - 1) as usize;
        let h = hr.clamp(0, 23) as usize;
        let v = &mut cells[c * 24 + h];
        *v += n;
        column_peak[c] = column_peak[c].max(*v);
        peak = peak.max(*v);
    }

    Ok(Clock { from, to, columns, cells, peak, column_peak, timezone: tz.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The five, by the names the data already uses.
    #[test]
    fn the_lanes_are_the_five() {
        let lanes = lanes_from_registry();
        let mut ids: Vec<&str> = lanes.iter().map(|(d, _)| d.as_str()).collect();
        ids.sort();
        assert_eq!(
            ids,
            vec!["activity", "communication", "financial", "health", "location"],
            "lanes drifted"
        );
    }

    /// Each exclusion is a different reason, and each has been got wrong once.
    #[test]
    fn the_exclusions_hold() {
        let lanes = lanes_from_registry();
        let ids: Vec<&str> = lanes.iter().map(|(d, _)| d.as_str()).collect();
        for (excluded, why) in [
            ("narrative", "wiki_events is DERIVED from the lanes — a peer row double-counts them"),
            ("calendar", "intent, not evidence; it is the one source that routinely lies"),
            ("environment", "weather is a condition you were in, not something you did"),
            ("app", "pages and chats are the record's own artifacts"),
            ("wiki", "articles are the record's own artifacts"),
            ("content", "empty on every box measured, and it is not one idea"),
        ] {
            assert!(!ids.contains(&excluded), "{excluded} is not a lane: {why}");
        }
    }

    /// A table registered under two ontologies must not be counted twice.
    #[test]
    fn a_table_appears_once_per_lane() {
        for (domain, members) in lanes_from_registry() {
            let mut tables: Vec<&str> = members.iter().map(|(t, _)| *t).collect();
            let before = tables.len();
            tables.sort();
            tables.dedup();
            assert_eq!(before, tables.len(), "{domain} counts a table twice");
        }
    }

    /// Every measure must name a table the registry actually registers, with
    /// the timestamp column that table really uses. These strings go straight
    /// into SQL, so a typo here is a 500 at request time and nowhere earlier.
    #[test]
    fn every_measure_points_at_a_real_lane_table() {
        let all = virtues_registry::ontologies::registered_ontologies();
        for m in virtues_registry::ontologies::lane_measures() {
            let o = all
                .iter()
                .find(|o| o.table_name == m.table && o.lane == Some(m.lane))
                .unwrap_or_else(|| panic!("measure {} names {} which is not in lane {}",
                                          m.id, m.table, m.lane));
            assert_eq!(
                o.timestamp_column, m.timestamp_column,
                "measure {} disagrees with the registry about {}'s time column",
                m.id, m.table
            );
        }
    }

    /// Ids appear in URLs people share; two lanes may reuse one, a lane may not.
    #[test]
    fn measure_ids_are_unique_within_a_lane() {
        for (lane, _) in lanes_from_registry() {
            let ms = virtues_registry::ontologies::measures_for_lane(&lane);
            let mut ids: Vec<&str> = ms.iter().map(|m| m.id).collect();
            let before = ids.len();
            ids.sort();
            ids.dedup();
            assert_eq!(before, ids.len(), "{lane} declares a measure id twice");
        }
    }

    /// A rate is an average, and an average that Postgres could return as a sum
    /// would silently make "heart rate" grow with the zoom level.
    #[test]
    fn rates_aggregate_with_avg() {
        use virtues_registry::ontologies::MeasureKind;
        for m in virtues_registry::ontologies::lane_measures() {
            if m.kind == MeasureKind::Rate {
                assert!(
                    m.agg.starts_with("avg("),
                    "{} is a rate but aggregates with `{}`",
                    m.id,
                    m.agg
                );
            }
        }
    }

    #[sqlx::test]
    async fn density_is_dense_and_bucketed(pool: PgPool) {
        let to = chrono::Utc::now();
        let from = to - chrono::Duration::days(7);
        let out = get_lifeline(&pool, from, to, 24, None, None, None).await.unwrap();

        assert!(!out.lanes.is_empty());
        for lane in &out.lanes {
            assert_eq!(
                lane.density.len(),
                24,
                "{} must return every bucket, zeros included",
                lane.id
            );
            assert_eq!(lane.measure, RECORDS, "{} defaulted to a measure", lane.id);
        }
    }

    /// Every declared measure has to survive a real round trip. The aggregates
    /// are interpolated SQL over columns nothing else in the codebase reads —
    /// `metadata->>'is_from_me'`, `sum(-amount)` — and an empty table still
    /// parses and plans the query, which is the half this catches.
    #[sqlx::test]
    async fn every_measure_executes(pool: PgPool) {
        let to = chrono::Utc::now();
        let from = to - chrono::Duration::days(30);
        for m in virtues_registry::ontologies::lane_measures() {
            let out = get_lifeline(
                &pool,
                from,
                to,
                12,
                Some(vec![m.lane.to_string()]),
                None,
                Some(vec![format!("{}:{}", m.lane, m.id)]),
            )
            .await
            .unwrap_or_else(|e| panic!("measure {} failed: {e}", m.id));

            let lane = out.lanes.iter().find(|l| l.id == m.lane).unwrap();
            assert_eq!(lane.measure, m.id, "{} was not applied", m.id);
            assert_eq!(lane.density.len(), 12);
        }
    }

    /// A URL outlives the code it was written against. A view saved when
    /// `spend` existed must not 500 after `spend` is renamed.
    #[sqlx::test]
    async fn an_unknown_measure_falls_back_rather_than_failing(pool: PgPool) {
        let to = chrono::Utc::now();
        let from = to - chrono::Duration::days(7);
        let out = get_lifeline(
            &pool,
            from,
            to,
            8,
            None,
            None,
            Some(vec!["financial:no_such_thing".into(), "nonsense".into()]),
        )
        .await
        .unwrap();
        let lane = out.lanes.iter().find(|l| l.id == "financial").unwrap();
        assert_eq!(lane.measure, RECORDS);
    }

    /// The exact split a real box produced: one doorway, 25 m apart, two rows.
    #[test]
    fn a_doorway_split_by_the_grid_is_put_back_together() {
        let at = |lat: f64, lon: f64, minutes: f64, visits: i64| Stay {
            lat,
            lon,
            visits,
            minutes,
            first: None,
            last: None,
        };
        let out = merge_stays(vec![
            at(30.258843, -97.752608, 7140.0, 57),
            at(30.258934, -97.752385, 3000.0, 33),
            // A genuinely different place, 20 km north-west. Must survive.
            at(30.438352, -97.921789, 360.0, 2),
        ]);

        assert_eq!(out.len(), 2, "the doorway did not merge, or the airport did");
        assert_eq!(out[0].visits, 90);
        assert_eq!(out[0].minutes, 10_140.0);
        // Centroid pulled toward the heavier half, not the midpoint.
        assert!(
            out[0].lon < -97.7525,
            "centroid ignored the weighting: {}",
            out[0].lon
        );
    }

    /// Merging must never invent or lose time.
    #[test]
    fn merging_conserves_visits_and_minutes() {
        let mk = |lat: f64, lon: f64| Stay {
            lat,
            lon,
            visits: 3,
            minutes: 60.0,
            first: None,
            last: None,
        };
        let input = vec![
            mk(30.0, -97.0),
            mk(30.0001, -97.0001),
            mk(31.0, -97.0),
            mk(31.0, -96.0),
        ];
        let out = merge_stays(input);
        assert_eq!(out.iter().map(|s| s.visits).sum::<i64>(), 12);
        assert_eq!(out.iter().map(|s| s.minutes).sum::<f64>(), 240.0);
        assert_eq!(out.len(), 3, "only the two neighbours should have merged");
    }

    #[sqlx::test]
    async fn ground_is_empty_but_shaped_on_a_bare_box(pool: PgPool) {
        let to = chrono::Utc::now();
        let g = get_ground(&pool, to - chrono::Duration::days(30), to).await.unwrap();
        assert!(g.bbox.is_none());
        assert!(g.stays.is_empty());
        assert_eq!(g.track_total, 0);
    }

    /// A feed of `72 bpm` repeated 22,911 times is noise dressed as detail.
    #[test]
    fn the_feed_carries_only_rows_a_person_would_read() {
        let names: Vec<&str> = feedable().iter().map(|o| o.name).collect();
        for sampled in [
            "health_heart_rate",
            "health_hrv",
            "health_steps",
            "location_point",
        ] {
            assert!(
                !names.contains(&sampled),
                "{sampled} is a measurement, not an event — it has no readable row"
            );
        }
        for eventful in ["communication_message", "financial_transaction", "location_visit"] {
            assert!(names.contains(&eventful), "{eventful} dropped out of the feed");
        }
    }

    /// Every branch is generated SQL over `label_sql`/`preview_sql`/`id_sql`
    /// strings that nothing else executes as a SELECT list. An empty database
    /// still parses and plans all eighteen, which is the half that breaks.
    #[sqlx::test]
    async fn every_feed_branch_parses(pool: PgPool) {
        let to = chrono::Utc::now();
        let feed = get_feed(&pool, to - chrono::Duration::days(365), to, None, 50, 0)
            .await
            .expect("feed failed to build");
        assert!(feed.records.is_empty());
        assert!(!feed.has_more);
    }

    /// Narrowing to one lane must not silently return everything.
    #[sqlx::test]
    async fn the_feed_can_be_narrowed_to_a_lane(pool: PgPool) {
        let to = chrono::Utc::now();
        for lane in ["communication", "financial", "location", "health", "activity"] {
            get_feed(
                &pool,
                to - chrono::Duration::days(30),
                to,
                Some(vec![lane.to_string()]),
                10,
                0,
            )
            .await
            .unwrap_or_else(|e| panic!("lane {lane} failed: {e}"));
        }
        // A lane nobody has heard of yields nothing rather than everything.
        let none = get_feed(
            &pool,
            to - chrono::Duration::days(30),
            to,
            Some(vec!["invented".into()]),
            10,
            0,
        )
        .await
        .unwrap();
        assert!(none.records.is_empty());
    }

    /// "Nothing has been processed here" is only a useful sentence with dates.
    #[sqlx::test]
    async fn processed_answers_even_when_it_has_nothing(pool: PgPool) {
        let to = chrono::Utc::now();
        let p = get_processed(&pool, to - chrono::Duration::days(30), to, 50)
            .await
            .unwrap();
        assert!(p.items.is_empty());
        assert!(p.coverage.is_none());
        assert_eq!(p.days_processed, 0);
    }

    /// The band is made of sleep. A stream that fires while you are asleep
    /// fills exactly the rows the band is made of, and the picture is gone.
    #[test]
    fn the_clock_reads_no_stream_that_runs_while_you_sleep() {
        let tables: Vec<&str> = virtues_registry::ontologies::activity_sources()
            .iter()
            .map(|s| s.table)
            .collect();
        for passive in [
            "data_health_heart_rate",
            "data_health_hrv",
            "data_health_steps",
            "data_health_sleep",
            "data_location_point",
        ] {
            assert!(!tables.contains(&passive), "{passive} would erase the sleep band");
        }
        assert!(tables.contains(&"data_communication_message"));
        assert!(tables.contains(&"data_activity_app_session"));
    }

    /// An arriving text says nothing about whether anyone was awake to read it.
    #[test]
    fn only_outbound_messages_count_as_being_awake() {
        let msg = virtues_registry::ontologies::activity_sources()
            .into_iter()
            .find(|s| s.table == "data_communication_message")
            .expect("messages dropped out of the clock");
        assert_eq!(msg.filter, Some("metadata->>'is_from_me' = 'true'"));
    }

    #[sqlx::test]
    async fn the_clock_is_a_full_dense_raster(pool: PgPool) {
        let to = chrono::Utc::now();
        let c = get_clock(&pool, to - chrono::Duration::days(30), to, 40, "America/Chicago")
            .await
            .unwrap();
        assert_eq!(c.columns, 40);
        assert_eq!(c.cells.len(), 40 * 24, "every hour of every column, zeros included");
        assert_eq!(c.column_peak.len(), 40);
        assert_eq!(c.timezone, "America/Chicago");
    }

    /// A zone name arrives from a browser and is interpolated nowhere, but it
    /// still reaches Postgres — an unknown one must degrade, not 500.
    #[sqlx::test]
    async fn an_unknown_timezone_falls_back_to_utc(pool: PgPool) {
        let to = chrono::Utc::now();
        let c = get_clock(&pool, to - chrono::Duration::days(2), to, 8, "Mars/Olympus")
            .await
            .unwrap();
        assert_eq!(c.timezone, "UTC");
    }

    #[sqlx::test]
    async fn a_backwards_window_is_refused(pool: PgPool) {
        let now = chrono::Utc::now();
        assert!(
            get_lifeline(&pool, now, now - chrono::Duration::days(1), 10, None, None, None)
                .await
                .is_err()
        );
    }
}
