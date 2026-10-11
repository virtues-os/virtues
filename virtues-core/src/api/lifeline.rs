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

/// How many days before a day page its numbers are compared with.
pub const BASELINE_DAYS: i64 = 30;
/// The pin that stands for whichever number sat furthest from its usual today.
pub const UNUSUAL: &str = "unusual";
/// The most numbers one day page shows.
pub const MAX_PINNED: usize = 5;
/// The fewest days before with a value that make a usual. The page draws no
/// usual from fewer, so nothing can stand out from fewer either.
const USUAL_MIN_DAYS: usize = 7;
/// The fewest distinct values among those days. A count that is nearly always
/// the same number has a middle half too narrow to measure a day against.
const USUAL_MIN_DISTINCT: usize = 4;
/// How far from the middle day, in widths of the middle half, a day has to sit
/// to stand out.
const UNUSUAL_MIN_SCORE: f64 = 1.0;

/// One number a day page can show: the day's value and the days before it,
/// so the page can say where the day sat against your own usual.
#[derive(Debug, Clone, Serialize)]
pub struct DayMeasure {
    /// `lane:id`, the form a pin is stored in.
    pub key: String,
    pub lane: String,
    pub label: String,
    pub unit: String,
    pub kind: String,
    /// The registry ontology it reads (`health_sleep`) and that ontology's
    /// display name: where the number comes from.
    pub ontology: String,
    pub source: String,
    /// The day's value. `None` when nothing was collected that day, which is
    /// not the same as zero.
    pub value: Option<f64>,
    /// The `BASELINE_DAYS` days before, oldest first, `None` the same way.
    pub before: Vec<Option<f64>>,
}

/// Every measure a day page can pin, without its days.
#[derive(Debug, Serialize)]
pub struct MeasureListing {
    pub key: String,
    pub lane: String,
    pub label: String,
    pub unit: String,
    pub kind: String,
}

/// The phone and computer apps carry their own copy of the page and update
/// apart from the server, so this answer only grows: `measures` and
/// `available` keep the shape an app older than `catalog` reads, and a newer
/// app reads `catalog` and `unusual`.
#[derive(Debug, Serialize)]
pub struct DayMeasures {
    pub date: chrono::NaiveDate,
    /// The pinned measures, in pin order, at most `MAX_PINNED`.
    pub measures: Vec<DayMeasure>,
    pub available: Vec<MeasureListing>,
    /// Every measure a day page can show, in registry order. The pins read
    /// their cells from here and the picker draws every row, so choosing a
    /// number needs no second request.
    pub catalog: Vec<DayMeasure>,
    /// When the pins include `unusual`: the `lane:id` that sat furthest from
    /// its usual. `None` when nothing stood out, when the day isn't over in
    /// its own timezone, or when `unusual` isn't pinned.
    pub unusual: Option<String>,
}

/// Every measure for one day and the days before it, and, when `pins`
/// includes `unusual` and the day is over, the one that sat furthest from its
/// usual. Each day is
/// its own local day, in the timezone that day was lived in. A pin the
/// registry no longer lists is ignored: a pin can outlive a measure.
pub async fn day_measures(
    pool: &PgPool,
    date: chrono::NaiveDate,
    pins: &[String],
) -> Result<DayMeasures> {
    use virtues_registry::ontologies::{lane_measures, registered_ontologies, MeasureKind};

    let mut starts = Vec::new();
    let mut ends = Vec::new();
    for back in (0..=BASELINE_DAYS).rev() {
        let day = date - chrono::Duration::days(back);
        let tz = crate::timezone::day_timezone(pool, day).await?;
        let (start, end) = crate::api::day_summary::day_bounds(day, Some(&tz));
        starts.push(start);
        ends.push(end);
    }

    let ontologies = registered_ontologies();
    let mut catalog = Vec::new();
    for m in lane_measures() {
        // Table, column, aggregate, filter and coverage are registry
        // constants, never request input; the windows are bound.
        //
        // Collection starts at the measure's first row it can judge (the rule
        // `Lane.first_seen` uses): a day before that is NULL, nothing was
        // measured. After it, a day whose rows all fall outside `coverage` is
        // NULL: spend over unsigned rows is unknown, not $0. A day with no
        // rows at all is a real zero only where the registry says so
        // (`empty_is_zero`): purchases are sparse by nature, sleep is not.
        // There is no trailing bound, so the days after the last purchase
        // stay zeros. Rows that all fail `filter` are a zero for a total and
        // no reading for a rate: an average of nothing is not 0 bpm.
        let and = m.filter.map(|f| format!(" AND ({f})")).unwrap_or_default();
        let judged = m.coverage.map(|c| format!(" AND ({c})")).unwrap_or_default();
        let outside = m
            .coverage
            .map(|c| {
                format!(
                    "WHEN EXISTS (SELECT 1 FROM {t} WHERE {ts} >= w.s AND {ts} < w.e) \
                     AND NOT EXISTS (SELECT 1 FROM {t} WHERE {ts} >= w.s AND {ts} < w.e AND ({c})) \
                     THEN NULL ",
                    t = m.table,
                    ts = m.timestamp_column,
                )
            })
            .unwrap_or_default();
        let unrecorded = if m.empty_is_zero {
            String::new()
        } else {
            format!(
                "WHEN NOT EXISTS (SELECT 1 FROM {t} WHERE {ts} >= w.s AND {ts} < w.e) THEN NULL ",
                t = m.table,
                ts = m.timestamp_column,
            )
        };
        let nothing = match m.kind {
            MeasureKind::Total => "0",
            MeasureKind::Rate => "NULL",
        };
        let sql = format!(
            "WITH f AS (SELECT min({ts}) AS first FROM {t} WHERE true{judged}) \
             SELECT CASE WHEN f.first IS NULL OR w.e <= f.first THEN NULL \
                    {outside}{unrecorded}\
                    ELSE COALESCE((SELECT ({agg})::float8 FROM {t} \
                                   WHERE {ts} >= w.s AND {ts} < w.e{and}), {nothing}) END \
             FROM unnest($1::timestamptz[], $2::timestamptz[]) WITH ORDINALITY AS w(s, e, n) \
             CROSS JOIN f \
             ORDER BY w.n",
            t = m.table,
            ts = m.timestamp_column,
            agg = m.agg,
        );
        let mut values: Vec<Option<f64>> = sqlx::query_scalar(&sql)
            .bind(&starts)
            .bind(&ends)
            .fetch_all(pool)
            .await
            .map_err(|e| Error::Database(format!("day measure {}:{}: {e}", m.lane, m.id)))?;
        let value = values.pop().flatten();
        // `every_measure_points_at_a_real_lane_table` holds this lookup to a
        // registered ontology; the table name is only a fallback for a reader.
        let (ontology, source) = ontologies
            .iter()
            .find(|o| o.table_name == m.table && o.lane == Some(m.lane))
            .map(|o| (o.name, o.display_name))
            .unwrap_or((m.table, m.table));
        catalog.push(DayMeasure {
            key: format!("{}:{}", m.lane, m.id),
            lane: m.lane.to_string(),
            label: m.label.to_string(),
            unit: m.unit.to_string(),
            kind: kind_str(m.kind).to_string(),
            ontology: ontology.to_string(),
            source: source.to_string(),
            value,
            before: values,
        });
    }

    // A day still under way holds half its totals: no visits by 9 AM is not
    // unusual, only early. It is scored once it is over in its own zone.
    let unusual = if pins.iter().any(|p| p == UNUSUAL)
        && crate::api::day_summary::day_is_over(pool, date).await?
    {
        most_unusual(&catalog, pins)
    } else {
        None
    };
    let measures = pins
        .iter()
        .filter_map(|p| catalog.iter().find(|m| &m.key == p))
        .take(MAX_PINNED)
        .cloned()
        .collect();
    let available = lane_measures()
        .iter()
        .map(|m| MeasureListing {
            key: format!("{}:{}", m.lane, m.id),
            lane: m.lane.to_string(),
            label: m.label.to_string(),
            unit: m.unit.to_string(),
            kind: kind_str(m.kind).to_string(),
        })
        .collect();
    Ok(DayMeasures { date, measures, available, catalog, unusual })
}

/// The `p` quantile of sorted values, interpolating between neighbours: the
/// same rule the page uses to draw the usual.
fn quantile(sorted: &[f64], p: f64) -> f64 {
    let i = (sorted.len() - 1) as f64 * p;
    let (lo, hi) = (i.floor() as usize, i.ceil() as usize);
    sorted[lo] + (sorted[hi] - sorted[lo]) * (i - lo as f64)
}

/// How far a day sat from its usual, in widths of the middle half of the days
/// before, or `None` when those days can't say what usual is.
fn unusual_score(m: &DayMeasure) -> Option<f64> {
    let today = m.value?;
    let mut days: Vec<f64> = m.before.iter().flatten().copied().collect();
    if days.len() < USUAL_MIN_DAYS {
        return None;
    }
    days.sort_by(f64::total_cmp);
    let mut distinct = days.clone();
    distinct.dedup();
    if distinct.len() < USUAL_MIN_DISTINCT {
        return None;
    }
    let spread = quantile(&days, 0.75) - quantile(&days, 0.25);
    // A middle half with no width says nothing about how far is far.
    if spread <= 0.0 {
        return None;
    }
    Some((today - quantile(&days, 0.5)).abs() / spread)
}

/// The `unusual` pin: the measure furthest from its usual today, among the
/// lanes the other pins don't already show. A pin with nothing in the days
/// it covers shows no number, so it doesn't claim its lane. Ties go to the
/// measure the registry lists first.
fn most_unusual(measures: &[DayMeasure], pins: &[String]) -> Option<String> {
    let shown: std::collections::HashSet<&str> = pins
        .iter()
        .filter(|p| p.as_str() != UNUSUAL)
        .filter_map(|p| measures.iter().find(|m| &m.key == p))
        .filter(|m| m.value.is_some() || m.before.iter().any(Option::is_some))
        .map(|m| m.lane.as_str())
        .collect();
    let mut best: Option<(&DayMeasure, f64)> = None;
    for m in measures.iter().filter(|m| !shown.contains(m.lane.as_str())) {
        let Some(score) = unusual_score(m) else { continue };
        if score >= UNUSUAL_MIN_SCORE && best.is_none_or(|(_, s)| score > s) {
            best = Some((m, score));
        }
    }
    best.map(|(m, _)| m.key.clone())
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
            ("narrative", "wiki_events is DERIVED from the lanes - a peer row double-counts them"),
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

    /// One measure over the last week, as a single bucket.
    async fn measure_total(pool: &PgPool, lane: &str, id: &str) -> f64 {
        let to = chrono::Utc::now();
        let out = get_lifeline(
            pool,
            to - chrono::Duration::days(7),
            to,
            1,
            Some(vec![lane.to_string()]),
            None,
            Some(vec![format!("{lane}:{id}")]),
        )
        .await
        .unwrap();
        let l = out.lanes.iter().find(|l| l.id == lane).unwrap();
        assert_eq!(l.measure, id, "{id} was not applied");
        l.density[0]
    }

    async fn seed_transactions(pool: &PgPool) {
        for (id, provider) in [("acct_plaid", "plaid"), ("acct_fk", "apple_finance"), ("acct_demo", "demo")] {
            sqlx::query(
                "INSERT INTO data_financial_account
                    (id, account_name, account_type, source_stream_id, source_table, source_provider)
                 VALUES ($1, 'Everyday Checking', 'checking', $1, 'test', $2)",
            )
            .bind(id)
            .bind(provider)
            .execute(pool)
            .await
            .unwrap();
        }
        // (account, provider, cents, plaid category, FinanceKit type)
        let rows: &[(&str, &str, i64, Option<&str>, Option<&str>)] = &[
            // Plaid: one purchase, and every way money moves without being one.
            ("acct_plaid", "plaid", 2500, Some("FOOD_AND_DRINK"), None),
            ("acct_plaid", "plaid", 40000, Some("LOAN_PAYMENTS"), None), // paying the card
            ("acct_plaid", "plaid", -40000, Some("LOAN_PAYMENTS"), None), // the card receiving it
            ("acct_plaid", "plaid", 10000, Some("TRANSFER_OUT"), None),
            ("acct_plaid", "plaid", -10000, Some("TRANSFER_IN"), None),
            ("acct_plaid", "plaid", -300000, Some("INCOME"), None),
            ("acct_plaid", "plaid", -1200, Some("GENERAL_MERCHANDISE"), None), // a refund
            // FinanceKit, signed at ingest.
            ("acct_fk", "apple_finance", 1800, None, Some("pointOfSale")),
            ("acct_fk", "apple_finance", 250, None, Some("interest")), // charged
            ("acct_fk", "apple_finance", -90000, None, Some("billPayment")),
            ("acct_fk", "apple_finance", 5000, None, Some("withdrawal")),
            ("acct_fk", "apple_finance", -35, None, Some("deposit")), // Daily Cash
            ("acct_fk", "apple_finance", -412, None, Some("interest")), // earned
            // FinanceKit from a phone that sent no direction: unsigned, untyped.
            ("acct_fk", "apple_finance", 7000, None, None),
            ("acct_fk", "apple_finance", 900, None, None),
            // A provider with no vocabulary here is read by sign alone.
            ("acct_demo", "demo", 1000, Some("GROCERY"), None),
        ];
        let at = chrono::Utc::now() - chrono::Duration::days(1);
        for (i, (acct, provider, cents, category, kind)) in rows.iter().enumerate() {
            sqlx::query(
                "INSERT INTO data_financial_transaction
                    (id, account_id, transaction_id, amount, merchant_category, transaction_type,
                     occurred_at, source_stream_id, source_table, source_provider)
                 VALUES ($1, $2, $1, $3, $4, $5, $6, $1, 'test', $7)",
            )
            .bind(format!("tx_{i}"))
            .bind(acct)
            .bind(cents)
            .bind(category)
            .bind(kind)
            .bind(at)
            .bind(provider)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    /// Spend is purchases. A card payment pays for purchases already counted
    /// on the card, a transfer is your own money moving, and a FinanceKit row
    /// with no direction cannot be told from a deposit.
    #[sqlx::test]
    async fn spend_counts_purchases_once(pool: PgPool) {
        seed_transactions(&pool).await;
        let spend = measure_total(&pool, "financial", "spend").await;
        // 25.00 + 18.00 + 2.50 interest charged + 10.00 demo
        assert!((spend - 55.50).abs() < 1e-9, "spend was {spend}");
    }

    /// Income is earnings. Signing FinanceKit turns card payments and Daily
    /// Cash into credits, and none of them is income.
    #[sqlx::test]
    async fn income_counts_earnings_only(pool: PgPool) {
        seed_transactions(&pool).await;
        let income = measure_total(&pool, "financial", "income").await;
        // 3000.00 Plaid INCOME + 4.12 FinanceKit interest earned
        assert!((income - 3004.12).abs() < 1e-9, "income was {income}");
    }

    /// The owner's sent rows carry '' as their handle, and so do short codes
    /// and blank senders. None of them is a person who messaged you.
    #[sqlx::test]
    async fn people_counts_senders_but_not_the_owner(pool: PgPool) {
        let at = chrono::Utc::now() - chrono::Duration::days(1);
        let rows: &[(Option<&str>, bool)] = &[
            (Some(""), true),                // the owner, writing back
            (Some("+15125550101"), false),
            (Some("+15125550101"), false),   // same person, counted once
            (Some("nick@example.com"), false),
            (Some(""), false),               // a short code
            (None, false),                   // not yet normalized
        ];
        for (i, (handle, from_me)) in rows.iter().enumerate() {
            sqlx::query(
                "INSERT INTO data_communication_message
                    (id, message_id, channel, from_identifier, from_handle, occurred_at,
                     source_stream_id, source_table, source_provider, metadata)
                 VALUES ($1, $1, 'imessage', 'x', $2, $3, $1, 'test', 'test',
                         jsonb_build_object('is_from_me', $4::bool))",
            )
            .bind(format!("msg_{i}"))
            .bind(handle)
            .bind(at)
            .bind(from_me)
            .execute(&pool)
            .await
            .unwrap();
        }
        assert_eq!(measure_total(&pool, "communication", "people").await, 2.0);
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

    /// One measure's cell out of a day's numbers.
    fn cell<'a>(d: &'a DayMeasures, key: &str) -> &'a DayMeasure {
        d.catalog
            .iter()
            .find(|m| m.key == key)
            .unwrap_or_else(|| panic!("{key} missing from the day's numbers"))
    }

    /// A day page's numbers: the day, the days before it, and the difference
    /// between a day nothing was collected (null) and a day of zero.
    #[sqlx::test]
    async fn day_measures_tell_nothing_collected_from_zero(pool: PgPool) {
        for (id, at, from_me) in [
            ("m1", "2026-09-23T15:00:00Z", true),
            ("m2", "2026-09-23T16:00:00Z", true),
            ("m3", "2026-09-22T15:00:00Z", true),
            ("m4", "2026-09-22T16:00:00Z", false),
            ("m5", "2026-09-18T15:00:00Z", false),
        ] {
            sqlx::query(
                "INSERT INTO data_communication_message
                    (id, message_id, channel, from_identifier, from_handle, occurred_at,
                     source_stream_id, source_table, source_provider, metadata)
                 VALUES ($1, $1, 'imessage', 'x', 'h', $2::timestamptz, $1, 'test', 'test',
                         jsonb_build_object('is_from_me', $3::bool))",
            )
            .bind(id)
            .bind(at)
            .bind(from_me)
            .execute(&pool)
            .await
            .unwrap();
        }

        let date = chrono::NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
        let pins = vec!["communication:sent".to_string(), "nowhere:nothing".to_string()];
        let d = day_measures(&pool, date, &pins).await.unwrap();

        let sent = cell(&d, "communication:sent");
        assert_eq!(sent.value, Some(2.0));
        assert_eq!(sent.before.len(), BASELINE_DAYS as usize);
        assert_eq!(sent.before[29], Some(1.0), "the day before");
        assert_eq!(sent.before[25], Some(0.0), "messages that day, none sent: a real zero");
        assert_eq!(sent.before[27], Some(0.0), "no messages, after collection began: a real zero");
        assert_eq!(sent.before[24], None, "before collection began: not a zero");
        assert_eq!(sent.before[0], None, "nothing collected: not a zero");
        assert_eq!(sent.ontology, "communication_message");
        assert_eq!(sent.source, "Messages");
        assert_eq!(d.unusual, None, "nothing is picked when `unusual` isn't pinned");
    }

    /// An app older than the catalog reads `measures` (its pins, in order,
    /// unknown ones skipped) and `available`. The answer grows; it never
    /// takes those away.
    #[sqlx::test]
    async fn an_older_app_still_reads_its_pins(pool: PgPool) {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
        let pins = vec![
            "communication:sent".to_string(),
            "nowhere:nothing".to_string(),
            UNUSUAL.to_string(),
            "health:steps".to_string(),
        ];
        let d = day_measures(&pool, date, &pins).await.unwrap();
        let keys: Vec<&str> = d.measures.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(keys, ["communication:sent", "health:steps"]);
        assert_eq!(d.measures[1].before.len(), BASELINE_DAYS as usize);
        assert_eq!(d.available.len(), virtues_registry::ontologies::lane_measures().len());
        assert!(d.available.iter().any(|m| m.key == "health:sleep"));

        let json = serde_json::to_value(&d).unwrap();
        for field in ["date", "measures", "available", "catalog", "unusual"] {
            assert!(json.get(field).is_some(), "the answer lost `{field}`");
        }
    }

    /// Every measure's SQL, coverage included, runs through `day_measures`.
    /// The registry crate cannot reach a database, so a typo in a predicate
    /// would otherwise first show up as a 500 on a day page. One call answers
    /// for all of them: the picker draws every row.
    #[sqlx::test]
    async fn every_measure_executes_for_a_day(pool: PgPool) {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
        let d = day_measures(&pool, date, &[UNUSUAL.to_string()]).await.unwrap();
        let all = virtues_registry::ontologies::lane_measures();
        assert_eq!(d.catalog.len(), all.len());
        for (cell, m) in d.catalog.iter().zip(all) {
            assert_eq!(cell.key, format!("{}:{}", m.lane, m.id), "registry order");
            assert_eq!(cell.before.len(), BASELINE_DAYS as usize, "{}", cell.key);
            assert_ne!(cell.source, m.table, "{} found no ontology", cell.key);
        }
        assert_eq!(d.unusual, None, "an empty record has nothing unusual");
    }

    /// A day still under way holds half-finished totals, so it picks nothing
    /// unusual; the same record on a day that is over does.
    #[sqlx::test]
    async fn a_day_under_way_has_nothing_unusual(pool: PgPool) {
        let utc_today = chrono::Utc::now().date_naive();
        let tz: chrono_tz::Tz = crate::timezone::day_timezone(&pool, utc_today)
            .await
            .unwrap()
            .parse()
            .unwrap_or(chrono_tz::UTC);
        let today = chrono::Utc::now().with_timezone(&tz).date_naive();
        let past = today - chrono::Duration::days(100);
        let mut n = 0;
        for date in [past, today] {
            for back in 0..=BASELINE_DAYS {
                let sent = if back == 0 { 20 } else { back % 4 + 1 };
                let at = (date - chrono::Duration::days(back)).and_hms_opt(12, 0, 0).unwrap().and_utc();
                for _ in 0..sent {
                    n += 1;
                    sqlx::query(
                        "INSERT INTO data_communication_message
                            (id, message_id, channel, from_identifier, from_handle, occurred_at,
                             source_stream_id, source_table, source_provider, metadata)
                         VALUES ($1, $1, 'imessage', 'x', 'h', $2, $1, 'test', 'test',
                                 jsonb_build_object('is_from_me', true))",
                    )
                    .bind(format!("m{n}"))
                    .bind(at)
                    .execute(&pool)
                    .await
                    .unwrap();
                }
            }
        }

        let pins = vec![UNUSUAL.to_string()];
        let over = day_measures(&pool, past, &pins).await.unwrap();
        assert!(over.unusual.is_some(), "twenty sent against a usual of one to four stands out");
        let under_way = day_measures(&pool, today, &pins).await.unwrap();
        assert_eq!(under_way.unusual, None, "today isn't over");
    }

    /// A day whose only rows fall outside a measure's coverage was not
    /// measured: spend over unsigned FinanceKit rows reads as nothing
    /// recorded, not $0, and stays out of the usual. Collection starts at the
    /// first row the measure can judge, so a quiet day after it is $0.
    #[sqlx::test]
    async fn rows_outside_coverage_read_as_not_measured(pool: PgPool) {
        sqlx::query(
            "INSERT INTO data_financial_account
                 (id, account_name, account_type, source_stream_id, source_table, source_provider)
             VALUES ('acct', 'Card', 'credit', 'acct', 'test', 'apple_finance')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO data_financial_transaction
                 (id, account_id, transaction_id, amount, currency, occurred_at,
                  source_stream_id, source_table, source_provider)
             VALUES ('t1', 'acct', 't1', 4862, 'USD', '2026-09-23T15:00:00Z',
                     't1', 'test', 'apple_finance')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let date = chrono::NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
        let d = day_measures(&pool, date, &[]).await.unwrap();
        let spend = cell(&d, "financial:spend");
        assert_eq!(spend.value, None, "unknown direction is not $0");
        assert!(spend.before.iter().all(Option::is_none), "nothing judged yet");

        sqlx::query(
            "INSERT INTO data_financial_transaction
                 (id, account_id, transaction_id, amount, currency, occurred_at,
                  source_stream_id, source_table, source_provider)
             VALUES ('t0', 'acct', 't0', 1200, 'USD', '2026-09-20T15:00:00Z',
                     't0', 'test', 'test')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let d = day_measures(&pool, date, &[]).await.unwrap();
        let spend = cell(&d, "financial:spend");
        assert_eq!(spend.value, None, "still unknown on the day itself");
        assert_eq!(spend.before[28], Some(0.0), "a quiet day after collection began is $0");
        assert_eq!(spend.before[26], None, "the day before collection began");
    }

    /// Nobody sleeps zero hours. A night with no sleep rows, after the record
    /// of sleep began and after it stopped, was not recorded.
    #[sqlx::test]
    async fn a_day_without_sleep_rows_is_not_recorded(pool: PgPool) {
        for (id, start, end, minutes) in [
            // Early-morning UTC starts, so each lands on one date in any US zone.
            ("s1", "2026-09-19T06:00:00Z", "2026-09-19T13:30:00Z", 450),
            ("s2", "2026-09-21T06:30:00Z", "2026-09-21T13:00:00Z", 390),
        ] {
            sqlx::query(
                "INSERT INTO data_health_sleep
                     (id, started_at, ended_at, duration_minutes,
                      source_stream_id, source_table, source_provider)
                 VALUES ($1, $2::timestamptz, $3::timestamptz, $4, $1, 'test', 'test')",
            )
            .bind(id)
            .bind(start)
            .bind(end)
            .bind(minutes)
            .execute(&pool)
            .await
            .unwrap();
        }

        let date = chrono::NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
        let d = day_measures(&pool, date, &[]).await.unwrap();
        let sleep = cell(&d, "health:sleep");
        assert_eq!(sleep.value, None, "the day itself, after the last night recorded");
        assert_eq!(sleep.before[26], Some(7.5), "Sep 19");
        assert_eq!(sleep.before[27], None, "between two recorded nights: not 0h");
        assert_eq!(sleep.before[28], Some(6.5), "Sep 21");
        assert_eq!(sleep.before[29], None, "after the last recorded night: not 0h");
        assert_eq!(sleep.before[25], None, "before the record of sleep began");
    }

    /// Silent chunks are kept as rows with empty text. Speech heard counts the
    /// chunks with words in them; a day of only silent chunks heard nothing,
    /// and a day with no chunks at all was not recorded.
    #[sqlx::test]
    async fn speech_heard_counts_only_rows_with_words(pool: PgPool) {
        for (id, at, text, seconds) in [
            ("t1", "2026-09-23T15:00:00Z", "Nick, are you coming to dinner?", 60.0),
            ("t2", "2026-09-23T15:01:00Z", "", 3000.0),
            ("t3", "2026-09-23T16:00:00Z", "...", 300.0),
            ("t4", "2026-09-23T17:00:00Z", "  ", 60.0),
            ("t5", "2026-09-23T18:00:00Z", "Ja, gern.", 120.0),
            ("t6", "2026-09-21T15:00:00Z", "", 600.0),
            ("t7", "2026-09-20T15:00:00Z", "Thanks, David.", 30.0),
        ] {
            sqlx::query(
                "INSERT INTO data_communication_transcription
                     (id, text, duration_seconds, started_at,
                      source_stream_id, source_table, source_provider)
                 VALUES ($1, $2, $3, $4::timestamptz, $1, 'test', 'test')",
            )
            .bind(id)
            .bind(text)
            .bind(seconds)
            .bind(at)
            .execute(&pool)
            .await
            .unwrap();
        }

        let date = chrono::NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
        let d = day_measures(&pool, date, &[]).await.unwrap();
        let talk = cell(&d, "communication:talk");
        assert_eq!(talk.value, Some(3.0), "only the two chunks with words, in minutes");
        assert_eq!(talk.before[27], Some(0.5), "Sep 20");
        assert_eq!(talk.before[28], Some(0.0), "silent chunks only: nothing heard");
        assert_eq!(talk.before[29], None, "no chunks at all: the microphone wasn't recording");
        assert_eq!(talk.before[26], None, "before the record of speech began");
    }

    /// A day of numbers for the picker, with `before` the 30 days before and
    /// `value` the day.
    fn measure(key: &str, value: Option<f64>, before: &[f64]) -> DayMeasure {
        let (lane, _) = key.split_once(':').unwrap();
        let mut days: Vec<Option<f64>> = vec![None; BASELINE_DAYS as usize - before.len()];
        days.extend(before.iter().copied().map(Some));
        DayMeasure {
            key: key.into(),
            lane: lane.into(),
            label: key.into(),
            unit: String::new(),
            kind: "total".into(),
            ontology: String::new(),
            source: String::new(),
            value,
            before: days,
        }
    }

    fn pins(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    /// The widest miss wins, measured in widths of each number's own middle
    /// half, so a step count and a message count compare fairly.
    #[test]
    fn the_most_unusual_is_the_furthest_from_its_own_usual() {
        let usual = [10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0];
        let ms = vec![
            // Middle 13.5, middle half 3.5 wide: 21 sits ~2.1 widths out.
            measure("health:steps", Some(21.0), &usual),
            // ~4.3 widths out.
            measure("activity:screen", Some(28.5), &usual),
            // Under one width out: not unusual at all.
            measure("financial:spend", Some(15.0), &usual),
        ];
        assert_eq!(most_unusual(&ms, &pins(&[UNUSUAL])).as_deref(), Some("activity:screen"));
        let ms = vec![measure("financial:spend", Some(15.0), &usual)];
        assert_eq!(most_unusual(&ms, &pins(&[UNUSUAL])), None, "nothing stood out");
    }

    /// The fifth number comes from a kind the other four don't show; a pin
    /// that has shown nothing in 31 days doesn't claim its lane.
    #[test]
    fn the_most_unusual_skips_lanes_the_other_pins_show() {
        let usual = [10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0];
        let ms = vec![
            measure("health:steps", Some(40.0), &usual),
            measure("health:sleep", None, &[]),
            measure("activity:screen", Some(21.0), &usual),
        ];
        assert_eq!(
            most_unusual(&ms, &pins(&["health:sleep", UNUSUAL])).as_deref(),
            Some("health:steps"),
            "sleep holds nothing, so health is still open"
        );
        assert_eq!(
            most_unusual(&ms, &pins(&["health:steps", UNUSUAL])).as_deref(),
            Some("activity:screen")
        );
        assert_eq!(most_unusual(&ms, &pins(&["health:steps", "activity:screen", UNUSUAL])), None);
    }

    /// A usual needs enough days, and enough different days, to measure
    /// against; and a day with nothing recorded can't be unusual.
    #[test]
    fn the_most_unusual_needs_a_usual_to_stand_out_from() {
        let few = [10.0, 11.0, 12.0, 13.0, 14.0, 15.0];
        assert_eq!(unusual_score(&measure("health:steps", Some(99.0), &few)), None, "six days");
        let same = [5.0, 5.0, 5.0, 6.0, 6.0, 7.0, 7.0, 5.0];
        assert_eq!(unusual_score(&measure("health:steps", Some(99.0), &same)), None, "three values");
        let flat = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0];
        assert_eq!(unusual_score(&measure("health:workouts", Some(1.0), &flat)), None, "no spread");
        let usual = [10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0];
        assert_eq!(unusual_score(&measure("health:steps", None, &usual)), None, "not recorded");
        let score = unusual_score(&measure("health:steps", Some(6.5), &usual)).unwrap();
        assert!((score - 2.0).abs() < 1e-9, "7 below a middle day of 13.5, over 3.5: {score}");
    }

    /// Every rate reads "not recorded" on a day without rows: an average of
    /// nothing is no reading, never zero.
    #[test]
    fn a_rate_is_never_zero() {
        use virtues_registry::ontologies::MeasureKind;
        for m in virtues_registry::ontologies::lane_measures() {
            if m.kind == MeasureKind::Rate {
                assert!(!m.empty_is_zero, "{} is a rate but reads an empty day as zero", m.id);
            }
        }
    }
}
