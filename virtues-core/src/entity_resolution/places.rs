//! Place Resolution
//!
//! Resolves places from multiple sources to canonical wiki_places entities.
//!
//! ## Sources
//!
//! 1. **Location Clustering** - GPS points → location_visit → wiki_places
//! 2. **Transaction Merchants** - Merchant names → wiki_orgs (with optional place links)
//!
//! ## Location Clustering Process
//!
//! 1. Fetch location_point records in time window
//! 2. Split them into stays (`detect_stays`), bridging the quiet stretches of a
//!    still phone up to `MAX_QUIET_GAP_MINUTES`
//! 3. Write location_visit records, merging with the stays already stored
//! 4. Link visits to place entities (create if new)
//!
//! `rebuild_visits` recomputes all of history the same way whenever
//! `TIMELINE_VERSION` moves.
//!
//! ## Merchant Resolution Process
//!
//! 1. Fetch transactions without org/place links
//! 2. Resolve merchant_name to wiki_orgs (create if new)
//! 3. Optionally link to wiki_places if location context available

use chrono::{DateTime, Utc};
use geo::HaversineDistance;
use geo::Point as GeoPoint;
use uuid::Uuid;

use super::TimeWindow;
use crate::database::Database;
use crate::error::Result;
use crate::ids;

/// Spatial clustering parameters
const SPATIAL_EPSILON_METERS: f64 = 100.0; // Max distance within a cluster
const MIN_VISIT_DURATION_MINUTES: i64 = 10; // Minimum visit duration (filters traffic/parking noise)
const TEMPORAL_GAP_MINUTES: i64 = 5; // Stored stays this close in time and space are one stay
/// Longest silence between two fixes at the same spot that still counts as
/// staying put. A still phone reports about every 15 minutes, so this is three
/// missed reports: past it, the phone stopped and the gap is unknown.
const MAX_QUIET_GAP_MINUTES: i64 = 45;
/// Stamped into every visit's metadata. Visits written by an older stay
/// detector carry a lower number (or none), and `rebuild_visits` recomputes
/// them all from the stored fixes. Bump it whenever the detector changes.
const TIMELINE_VERSION: i64 = 2;
const MAX_HORIZONTAL_ACCURACY: f64 = 100.0; // Filter low-quality points
const DEFAULT_PLACE_RADIUS_METERS: f64 = 100.0; // Default radius for new places

/// Location point for clustering
#[derive(Debug, Clone)]
struct LocationPoint {
    id: Uuid,
    latitude: f64,
    longitude: f64,
    timestamp: DateTime<Utc>,
    horizontal_accuracy: Option<f64>,
    _speed: Option<f64>,
}

/// Clustered visit
#[derive(Debug, Clone)]
struct Visit {
    points: Vec<LocationPoint>,
    centroid_lat: f64,
    centroid_lon: f64,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
}

/// Resolve places from all sources in the given time window
///
/// Returns the total number of records processed.
pub async fn resolve_places(db: &Database, window: TimeWindow) -> Result<usize> {
    tracing::info!(
        start = %window.start,
        end = %window.end,
        "Resolving places from all sources"
    );

    let mut total_resolved = 0;

    // 1. Resolve from location clustering
    total_resolved += resolve_location_visits(db, window).await?;

    // 2. Resolve transaction merchants to organizations
    total_resolved += resolve_transaction_merchants(db, window).await?;

    tracing::info!(
        total_resolved,
        "Place resolution completed"
    );

    Ok(total_resolved)
}

/// Resolve places via location clustering
async fn resolve_location_visits(db: &Database, window: TimeWindow) -> Result<usize> {
    let points = fetch_location_points(db, window).await?;

    if points.is_empty() {
        tracing::debug!("No location points to cluster");
        return Ok(0);
    }

    tracing::debug!(point_count = points.len(), "Fetched location points");

    let visits = detect_stays(&points)?;

    tracing::debug!(ref_count = visits.len(), "Completed clustering");

    // Write visits idempotently and link to place entities
    let mut records_written = 0;
    for visit in &visits {
        match write_visit_and_link_place(db, visit).await {
            Ok(_) => records_written += 1,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to write visit"
                );
            }
        }
    }

    tracing::debug!(
        visits_written = records_written,
        "Location clustering completed"
    );

    Ok(records_written)
}

/// Resolve transaction merchants to wiki_orgs
///
/// For each unique merchant name, creates or finds a wiki_orgs entity.
/// Links transactions to the organization via metadata.
async fn resolve_transaction_merchants(db: &Database, window: TimeWindow) -> Result<usize> {
    // Fetch transactions without org resolution
    let transactions = fetch_unresolved_transactions(db, window).await?;

    if transactions.is_empty() {
        tracing::debug!("No transactions to resolve for merchants");
        return Ok(0);
    }

    tracing::debug!(
        transaction_count = transactions.len(),
        "Fetched transactions for merchant resolution"
    );

    let mut total_resolved = 0;
    for txn in transactions {
        match resolve_and_link_merchant(db, &txn).await {
            Ok(true) => total_resolved += 1,
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(
                    transaction_id = %txn.id,
                    merchant = %txn.merchant_name,
                    error = %e,
                    "Failed to resolve merchant"
                );
            }
        }
    }

    tracing::debug!(
        merchants_resolved = total_resolved,
        "Transaction merchant resolution completed"
    );

    Ok(total_resolved)
}

/// Transaction record for merchant resolution
#[derive(Debug)]
struct TransactionRecord {
    id: String,
    merchant_name: String,
    merchant_category: Option<String>,
}

/// Fetch transactions without merchant organization resolution
async fn fetch_unresolved_transactions(
    db: &Database,
    window: TimeWindow,
) -> Result<Vec<TransactionRecord>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            t.id,
            t.merchant_name,
            t.merchant_category
        FROM data_financial_transaction t
        WHERE t.occurred_at >= $1
          AND t.occurred_at < $2
          AND t.merchant_name IS NOT NULL
          AND t.merchant_name != ''
          AND NOT EXISTS (
              SELECT 1 FROM wiki_refs er
              WHERE er.source_table = 'data_financial_transaction'
                AND er.source_id = t.id
                AND er.role = 'merchant'
          )
        ORDER BY t.occurred_at ASC
        LIMIT 500
        "#,
        window.start,
        window.end
    )
    .fetch_all(db.pool())
    .await?;

    let transactions = rows
        .into_iter()
        .filter_map(|row| {
            Some(TransactionRecord {
                id: row.id,
                merchant_name: row.merchant_name?,
                merchant_category: row.merchant_category,
            })
        })
        .collect();

    Ok(transactions)
}

/// Resolve merchant to wiki_orgs and link to transaction via wiki_refs
async fn resolve_and_link_merchant(db: &Database, txn: &TransactionRecord) -> Result<bool> {
    let merchant_name = txn.merchant_name.trim();
    if merchant_name.is_empty() {
        return Ok(false);
    }

    // Resolve or create organization for this merchant
    let org_id = resolve_or_create_merchant_org(db, merchant_name, txn.merchant_category.as_deref())
        .await?;

    // Get transaction timestamp for the entity reference
    let timestamp: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT occurred_at FROM data_financial_transaction WHERE id = $1",
    )
    .bind(&txn.id)
    .fetch_optional(db.pool())
    .await?
    .flatten();

    // Link via wiki_refs
    let ref_id = ids::generate_id("eref", &[&txn.id, &org_id, "merchant"]);
    sqlx::query!(
        r#"
        INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, role, occurred_at)
        VALUES ($1, 'organization', $2, 'data_financial_transaction', $3, 'merchant', $4)
        ON CONFLICT (entity_id, source_table, source_id, role) DO NOTHING
        "#,
        ref_id,
        org_id,
        txn.id,
        timestamp
    )
    .execute(db.pool())
    .await?;

    tracing::debug!(
        transaction_id = %txn.id,
        merchant_name = %merchant_name,
        org_id = %org_id,
        "Linked transaction to merchant organization via wiki_refs"
    );

    Ok(true)
}

/// Resolve or create a merchant organization in wiki_orgs
async fn resolve_or_create_merchant_org(
    db: &Database,
    merchant_name: &str,
    category: Option<&str>,
) -> Result<String> {
    // Normalize merchant name for matching
    let normalized_name = normalize_merchant_name(merchant_name);

    // Check if organization exists
    let existing = sqlx::query!(
        r#"
        SELECT id
        FROM wiki_orgs
        WHERE LOWER(name) = LOWER($1)
           OR LOWER(name) = LOWER($2)
        LIMIT 1
        "#,
        merchant_name,
        normalized_name
    )
    .fetch_optional(db.pool())
    .await?;

    if let Some(row) = existing {
        {
            let org_id = row.id;
            tracing::debug!(
                merchant_name = %merchant_name,
                org_id = %org_id,
                "Found existing merchant organization"
            );
            return Ok(org_id);
        }
    }

    // Create new organization
    let org_id = ids::generate_id(ids::WIKI_ORG_PREFIX, &[&normalized_name]);

    let organization_type = category
        .map(|c| categorize_merchant(c))
        .unwrap_or("merchant");

    let metadata = serde_json::json!({
        "source": "transaction_merchant",
        "original_name": merchant_name,
        "category": category,
    });

    sqlx::query!(
        r#"
        INSERT INTO wiki_orgs (
            id,
            name,
            organization_type,
            relationship_type,
            metadata
        ) VALUES ($1, $2, $3, 'vendor', $4)
        ON CONFLICT (id) DO NOTHING
        "#,
        org_id,
        normalized_name,
        organization_type,
        metadata,
    )
    .execute(db.pool())
    .await?;

    tracing::info!(
        org_id = %org_id,
        name = %normalized_name,
        organization_type = %organization_type,
        "Created new merchant organization"
    );

    Ok(org_id)
}

/// Normalize a merchant descriptor down to the business it names.
///
/// THIS FUNCTION USED TO MERGE UNRELATED BUSINESSES. It walked a list it called
/// "suffixes" and truncated at the FIRST occurrence of each — `find`, not
/// `ends_with` — and the list contained `"*"` and `" CO"`. So:
///
///   "SQ *BLUE BOTTLE"     → truncate at `*` → "Sq"
///   "TST* PIZZERIA"       → truncate at `*` → "Tst"
///   "BLUE BOTTLE COFFEE"  → `" CO"` matches inside " COFFEE" → "Blue Bottle"
///
/// Every Square, Toast and PayPal charge on the box therefore resolved to ONE
/// shared `wiki_orgs` row, which then accumulated refs from unrelated
/// transactions. Silent graph corruption, from a deterministic writer, on the
/// one path the doctrine trusts precisely because it does not guess.
///
/// Two fixes, in order:
///
///   1. A processor tag is a PREFIX and the merchant is what follows it. Take
///      the text after the marker, never before.
///   2. A corporate suffix only counts as a suffix — matched on whole trailing
///      words, so "COFFEE" is never mistaken for "CO".
fn normalize_merchant_name(name: &str) -> String {
    /// Trailing words that name a legal form rather than a business.
    const CORP_SUFFIXES: &[&str] = &[
        "INC", "INC.", "LLC", "L.L.C.", "LTD", "LTD.", "CORP", "CORP.", "CO", "CO.", "PLC",
    ];

    let original = name.trim();

    // 1. Payment-processor tags: "SQ *", "TST*", "PAYPAL *", "SP ". The marker
    //    introduces the merchant, so everything BEFORE it is the processor and
    //    everything after is the name we want. Split on the LAST marker, since
    //    some descriptors carry two ("PP*SQ *SHOP").
    let mut s = match original.rfind('*') {
        Some(pos) => original[pos + 1..].trim().to_string(),
        None => original.to_string(),
    };

    // 2. Store and reference numbers: "STARBUCKS #1234", "SHELL - 4471".
    for sep in [" #", "#", " - "] {
        if let Some(pos) = s.find(sep) {
            s.truncate(pos);
        }
    }

    // 3. Trailing legal-form words, repeatedly — "ACME CO LTD" loses both.
    //    Compared whole-word against the last token, which is the difference
    //    between this and the bug above.
    loop {
        let trimmed = s.trim_end();
        let Some((head, last)) = trimmed.rsplit_once(char::is_whitespace) else {
            break;
        };
        if CORP_SUFFIXES.contains(&last.to_uppercase().as_str()) {
            s = head.to_string();
        } else {
            s = trimmed.to_string();
            break;
        }
    }

    let titled = s
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    // Never return nothing. A descriptor that is entirely processor tag and
    // punctuation would otherwise mint an org with an empty name, and empty
    // names collide with each other — the very failure this rewrite exists to
    // end.
    if titled.trim().is_empty() {
        return original.to_string();
    }
    titled
}

/// Categorize merchant based on Plaid category
fn categorize_merchant(category: &str) -> &'static str {
    let cat_lower = category.to_lowercase();
    if cat_lower.contains("restaurant") || cat_lower.contains("food") {
        "restaurant"
    } else if cat_lower.contains("grocery") || cat_lower.contains("supermarket") {
        "grocery"
    } else if cat_lower.contains("gas") || cat_lower.contains("fuel") {
        "gas_station"
    } else if cat_lower.contains("shop") || cat_lower.contains("retail") || cat_lower.contains("store") {
        "retail"
    } else if cat_lower.contains("travel") || cat_lower.contains("airline") || cat_lower.contains("hotel") {
        "travel"
    } else if cat_lower.contains("healthcare") || cat_lower.contains("medical") || cat_lower.contains("pharmacy") {
        "healthcare"
    } else if cat_lower.contains("subscription") || cat_lower.contains("streaming") {
        "subscription"
    } else {
        "merchant"
    }
}

/// Fetch location points from database in time window
async fn fetch_location_points(db: &Database, window: TimeWindow) -> Result<Vec<LocationPoint>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id,
            latitude,
            longitude,
            occurred_at,
            horizontal_accuracy
        FROM data_location_point
        WHERE occurred_at >= $1
          AND occurred_at < $2
          AND (horizontal_accuracy IS NULL OR horizontal_accuracy < $3)
        ORDER BY occurred_at ASC
        "#,
        window.start,
        window.end,
        MAX_HORIZONTAL_ACCURACY
    )
    .fetch_all(db.pool())
    .await?;

    let points = rows
        .into_iter()
        .filter_map(|row| {
            let id = Uuid::parse_str(&row.id).ok()?;
            Some(LocationPoint {
                id,
                latitude: row.latitude,
                longitude: row.longitude,
                timestamp: row.occurred_at,
                horizontal_accuracy: row.horizontal_accuracy,
                _speed: None,
            })
        })
        .collect();

    Ok(points)
}

/// Split a time-ordered run of fixes into stays.
///
/// The phone samples densely only while it moves. Standing still, it drops to
/// a battery-saving mode that reports a single still-check fix about every 15
/// minutes, and any real departure wakes dense sampling again within a few
/// hundred meters. So silence between two fixes at the same spot means the
/// person stayed, up to `MAX_QUIET_GAP_MINUTES`; past that the phone itself
/// stopped reporting and the gap is unknown, not a stay. A clusterer that
/// treated every quiet stretch over a few minutes as a departure saw almost
/// none of the stays this phone reports, since most of a still day is quiet.
///
/// Stays are found in one forward pass and never overlap: each one claims every
/// fix up to where it ends. A single fix far from the stay, with the next back
/// inside it, is GPS jitter and is absorbed; two far fixes in a row are a
/// departure.
fn detect_stays(points: &[LocationPoint]) -> Result<Vec<Visit>> {
    let quiet = chrono::Duration::minutes(MAX_QUIET_GAP_MINUTES);
    let near = |lat: f64, lon: f64, p: &LocationPoint| {
        haversine_distance(lat, lon, p.latitude, p.longitude) <= SPATIAL_EPSILON_METERS
    };

    let mut stays = Vec::new();
    let mut i = 0;
    while i < points.len() {
        let mut members = vec![points[i].clone()];
        let (mut lat, mut lon) = (points[i].latitude, points[i].longitude);
        let mut j = i + 1;
        while j < points.len() {
            let last = members.last().map(|m| m.timestamp).unwrap_or(points[i].timestamp);
            let p = &points[j];
            if p.timestamp - last > quiet {
                break;
            }
            if near(lat, lon, p) {
                members.push(p.clone());
                let n = members.len() as f64;
                lat += (p.latitude - lat) / n;
                lon += (p.longitude - lon) / n;
                j += 1;
                continue;
            }
            let jitter = points
                .get(j + 1)
                .is_some_and(|q| q.timestamp - last <= quiet && near(lat, lon, q));
            if !jitter {
                break;
            }
            j += 1;
        }

        let span = members[members.len() - 1].timestamp - members[0].timestamp;
        if members.len() >= 2 && span.num_minutes() >= MIN_VISIT_DURATION_MINUTES {
            stays.push(create_visit_from_cluster(members)?);
            i = j;
        } else {
            i += 1;
        }
    }
    Ok(stays)
}

/// Create a visit from a cluster of points
fn create_visit_from_cluster(points: Vec<LocationPoint>) -> Result<Visit> {
    // Calculate weighted centroid (weight by accuracy)
    let mut total_weight = 0.0;
    let mut weighted_lat = 0.0;
    let mut weighted_lon = 0.0;

    for point in &points {
        let weight = 1.0 / point.horizontal_accuracy.unwrap_or(50.0);
        weighted_lat += point.latitude * weight;
        weighted_lon += point.longitude * weight;
        total_weight += weight;
    }

    let centroid_lat = weighted_lat / total_weight;
    let centroid_lon = weighted_lon / total_weight;

    let start_time = points.first().unwrap().timestamp;
    let end_time = points.last().unwrap().timestamp;

    Ok(Visit {
        points,
        centroid_lat,
        centroid_lon,
        start_time,
        end_time,
    })
}

/// Calculate Haversine distance between two points (in meters)
fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let p1 = GeoPoint::new(lon1, lat1);
    let p2 = GeoPoint::new(lon2, lat2);
    p1.haversine_distance(&p2)
}

/// Generate deterministic visit ID
fn generate_visit_id(centroid_lat: f64, centroid_lon: f64, start_time: DateTime<Utc>) -> Uuid {
    // Round coordinates to ~10 meter precision (4 decimal places)
    let lat_rounded = (centroid_lat * 10000.0).round() / 10000.0;
    let lon_rounded = (centroid_lon * 10000.0).round() / 10000.0;

    // Round start time to nearest minute
    let timestamp_secs = start_time.timestamp();
    let rounded_secs = (timestamp_secs / 60) * 60;
    let start_rounded = DateTime::from_timestamp(rounded_secs, 0).unwrap_or(start_time);

    // Create deterministic UUID v5
    let hash_input = format!(
        "{}:{}:{}",
        lat_rounded,
        lon_rounded,
        start_rounded.to_rfc3339()
    );

    Uuid::new_v5(&Uuid::NAMESPACE_OID, hash_input.as_bytes())
}

/// Two stored visits closer than this are the same stay seen twice; farther
/// apart, they are rival claims on the same minutes. Twice the clustering
/// epsilon: a re-cluster of one stay can move its centroid by up to a cluster
/// radius, and a stay near a place's edge resolves to a different place from
/// one pass to the next.
const SAME_STAY_METERS: f64 = 2.0 * SPATIAL_EPSILON_METERS;

/// A stored visit the candidate overlaps, as the planner sees it.
#[derive(Debug, Clone)]
struct StoredVisit {
    id: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    latitude: f64,
    longitude: f64,
}

/// What writing one clustered stay should do to the stored rows.
#[derive(Debug, PartialEq)]
enum VisitWrite {
    /// Nothing survives that is worth writing.
    Skip,
    /// Extend `keeper` to `span` and delete `absorb` — the same stay, stored twice.
    Extend {
        keeper: String,
        absorb: Vec<String>,
        span: (DateTime<Utc>, DateTime<Utc>),
    },
    /// A new stay over `span`.
    Insert { span: (DateTime<Utc>, DateTime<Utc>) },
}

/// Decide how a clustered stay lands, given every stored visit near it in time.
///
/// A person is in one place at a time, so the visit table is a partition of the
/// timeline: no two rows may cover the same minute. Stored visits within
/// `SAME_STAY_METERS` that overlap or nearly touch the candidate are the same
/// stay and are unioned into one row. Farther ones that truly overlap are
/// rivals, and the stored rival keeps its minutes: the candidate is clipped
/// around it and keeps its longest remaining piece, or is dropped when that
/// piece is shorter than a visit.
///
/// Identity used to hang on the `wiki_refs` place link. A visit whose ref was
/// lost (its place deleted, a ref wipe) became invisible to the merge, so every
/// re-cluster over it inserted another overlapping row, and a stay that
/// resolved to a neighboring place on a later pass did the same. Matching on
/// time and distance cannot lose a row that way.
fn plan_visit_write(
    candidate: (DateTime<Utc>, DateTime<Utc>),
    centroid: (f64, f64),
    stored: &[StoredVisit],
) -> VisitWrite {
    let gap = chrono::Duration::minutes(TEMPORAL_GAP_MINUTES);
    let (mut same, mut rivals): (Vec<&StoredVisit>, Vec<&StoredVisit>) = (Vec::new(), Vec::new());
    for s in stored {
        let meters = haversine_distance(centroid.0, centroid.1, s.latitude, s.longitude);
        if meters <= SAME_STAY_METERS {
            if s.start <= candidate.1 + gap && s.end >= candidate.0 - gap {
                same.push(s);
            }
        } else if s.start < candidate.1 && s.end > candidate.0 {
            rivals.push(s);
        }
    }
    same.sort_by(|a, b| (a.start, &a.id).cmp(&(b.start, &b.id)));

    let union = same.iter().fold(candidate, |(a, b), s| (a.min(s.start), b.max(s.end)));
    let span = longest_uncovered(union, rivals.iter().map(|r| (r.start, r.end)));
    let long_enough = |(a, b): (DateTime<Utc>, DateTime<Utc>)| {
        (b - a).num_minutes() >= MIN_VISIT_DURATION_MINUTES
    };

    match (same.first(), span.filter(|s| long_enough(*s))) {
        (Some(keeper), Some(span)) => VisitWrite::Extend {
            keeper: keeper.id.clone(),
            absorb: same.iter().skip(1).map(|s| s.id.clone()).collect(),
            span,
        },
        // The same stay already sits in rival territory: leave the stored rows
        // exactly as they are rather than guess which side is wrong.
        (Some(_), None) => VisitWrite::Skip,
        (None, Some(span)) => VisitWrite::Insert { span },
        (None, None) => VisitWrite::Skip,
    }
}

/// The longest piece of `span` that none of `covered` touches.
fn longest_uncovered(
    span: (DateTime<Utc>, DateTime<Utc>),
    covered: impl Iterator<Item = (DateTime<Utc>, DateTime<Utc>)>,
) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let mut covered: Vec<_> = covered.filter(|(a, b)| a < b).collect();
    covered.sort();
    let mut best: Option<(DateTime<Utc>, DateTime<Utc>)> = None;
    let mut cursor = span.0;
    let mut consider = |a: DateTime<Utc>, b: DateTime<Utc>| {
        if a < b && best.is_none_or(|(x, y)| b - a > y - x) {
            best = Some((a, b));
        }
    };
    for (a, b) in covered {
        if a >= span.1 {
            break;
        }
        consider(cursor, a.min(span.1));
        cursor = cursor.max(b);
    }
    consider(cursor, span.1);
    best
}

/// Write a visit as ONE row per stay — matching and extending an existing visit
/// rather than minting a new one every time the clusterer re-runs.
///
/// The maintenance loop re-clusters a 30-hour window every 15 minutes, and
/// each pass sees a slightly different set of points, so the same stay comes
/// back with a drifted start, end and centroid. Every drift that was not
/// recognized as the stay already stored became another overlapping row;
/// `plan_visit_write` is where that recognition lives. Re-running over a mess
/// collapses it, within the window the pass covers.
async fn write_visit_and_link_place(db: &Database, visit: &Visit) -> Result<()> {
    let pool = db.pool();
    let gap = chrono::Duration::minutes(TEMPORAL_GAP_MINUTES);

    let stored: Vec<StoredVisit> = sqlx::query_as::<_, (String, DateTime<Utc>, DateTime<Utc>, f64, f64)>(
        r#"
        SELECT id, started_at, COALESCE(ended_at, started_at), latitude, longitude
        FROM data_location_visit
        WHERE started_at <= $2
          AND COALESCE(ended_at, started_at) >= $1
        ORDER BY started_at, id
        "#,
    )
    .bind(visit.start_time - gap)
    .bind(visit.end_time + gap)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id, start, end, latitude, longitude)| StoredVisit { id, start, end, latitude, longitude })
    .collect();

    let plan = plan_visit_write(
        (visit.start_time, visit.end_time),
        (visit.centroid_lat, visit.centroid_lon),
        &stored,
    );
    let (keeper, span) = match plan {
        VisitWrite::Skip => return Ok(()),
        VisitWrite::Extend { keeper, absorb, span } => {
            if !absorb.is_empty() {
                sqlx::query(
                    "DELETE FROM wiki_refs \
                     WHERE source_table = 'data_location_visit' AND source_id = ANY($1)",
                )
                .bind(&absorb)
                .execute(pool)
                .await?;
                sqlx::query("DELETE FROM data_location_visit WHERE id = ANY($1)")
                    .bind(&absorb)
                    .execute(pool)
                    .await?;
                tracing::debug!(kept = %keeper, absorbed = absorb.len(), "merged overlapping visits");
            }
            (Some(keeper), span)
        }
        VisitWrite::Insert { span } => (None, span),
    };

    // A clipped span keeps only the points inside it.
    let points: Vec<&LocationPoint> = visit
        .points
        .iter()
        .filter(|p| p.timestamp >= span.0 && p.timestamp <= span.1)
        .collect();
    let Some(first_point) = points.first() else {
        return Ok(());
    };
    let metadata = serde_json::json!({
        "point_count": points.len(),
        "radius_meters": calculate_visit_radius(visit),
        "timeline": TIMELINE_VERSION,
    });
    let duration_minutes = (span.1 - span.0).num_minutes() as i32;

    let place_id = resolve_or_create_place(db, visit.centroid_lat, visit.centroid_lon).await?;

    let visit_id: String = match keeper {
        Some(keeper) => {
            sqlx::query(
                "UPDATE data_location_visit \
                 SET started_at = $2, ended_at = $3, duration_minutes = $4, \
                     latitude = $5, longitude = $6, metadata = $7, updated_at = now() \
                 WHERE id = $1",
            )
            .bind(&keeper)
            .bind(span.0)
            .bind(span.1)
            .bind(duration_minutes)
            .bind(visit.centroid_lat)
            .bind(visit.centroid_lon)
            .bind(&metadata)
            .execute(pool)
            .await?;
            keeper
        }
        None => {
            let candidate_id =
                generate_visit_id(visit.centroid_lat, visit.centroid_lon, span.0).to_string();
            // Conflict on `source_stream_id`, NOT `id`. `id` is derived from
            // (centroid, started_at) and DRIFTS every time re-clustering nudges
            // either, while `source_stream_id` (the first point) holds the UNIQUE
            // constraint. Guarding `id` let a re-clustered stay sail past the
            // guard and die on `data_location_visit_source_stream_id_key`, and
            // then no visit was written at all.
            sqlx::query_scalar(
                "INSERT INTO data_location_visit \
                 (id, latitude, longitude, started_at, ended_at, duration_minutes, \
                  source_stream_id, source_table, source_provider, metadata) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, 'location_point', 'ios', $8) \
                 ON CONFLICT (source_stream_id) DO UPDATE SET \
                   started_at   = LEAST(data_location_visit.started_at, EXCLUDED.started_at), \
                   ended_at = GREATEST(data_location_visit.ended_at, EXCLUDED.ended_at), \
                   duration_minutes = GREATEST(0, (EXTRACT(EPOCH FROM \
                     GREATEST(data_location_visit.ended_at, EXCLUDED.ended_at) \
                     - LEAST(data_location_visit.started_at, EXCLUDED.started_at)) / 60)::int), \
                   latitude = EXCLUDED.latitude, \
                   longitude = EXCLUDED.longitude, \
                   metadata = EXCLUDED.metadata, \
                   updated_at = now() \
                 RETURNING id",
            )
            .bind(&candidate_id)
            .bind(visit.centroid_lat)
            .bind(visit.centroid_lon)
            .bind(span.0)
            .bind(span.1)
            .bind(duration_minutes)
            .bind(first_point.id.to_string())
            .bind(&metadata)
            .fetch_one(pool)
            .await?
        }
    };

    // Link the visit to its place — unless it already has one. A kept row keeps
    // the place it was first resolved to; a second place ref would make one
    // visit two places. The row's own id, never `candidate_id`: on conflict the
    // row keeps its ORIGINAL id, and binding the drifted one orphaned refs.
    let ref_id = ids::generate_id("eref", &[&visit_id, &place_id, "location"]);
    sqlx::query(
        "INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, role, occurred_at) \
         SELECT $1, 'place', $2, 'data_location_visit', $3, 'location', $4 \
         WHERE NOT EXISTS ( \
           SELECT 1 FROM wiki_refs \
           WHERE source_table = 'data_location_visit' AND source_id = $3 AND entity_type = 'place') \
         ON CONFLICT (entity_id, source_table, source_id, role) DO NOTHING",
    )
    .bind(&ref_id)
    .bind(&place_id)
    .bind(&visit_id)
    .bind(span.0)
    .execute(pool)
    .await?;

    Ok(())
}

/// Which stored visit each recomputed stay takes over.
#[derive(Debug, Default, PartialEq)]
struct VisitRebuild {
    /// One entry per stay, in order: the stored visit whose id it keeps, if any.
    keep: Vec<Option<String>>,
    /// Stored visits no stay took over.
    delete: Vec<String>,
}

/// Match recomputed stays to stored visits so a rebuild keeps ids where it can.
///
/// Chat citations and place refs point at visit ids, so a stay that covers a
/// stored visit takes over that visit's id instead of minting a new one. Each
/// stored visit goes to at most one stay, and the largest overlap is matched
/// first. Stored visits no stay covers are deleted: every stay is derived from
/// the stored fixes, so a row the fixes no longer support is not a stay.
fn plan_visit_rebuild(
    stays: &[(DateTime<Utc>, DateTime<Utc>)],
    stored: &[StoredVisit],
) -> VisitRebuild {
    let mut by_start: Vec<usize> = (0..stored.len()).collect();
    by_start.sort_by_key(|&k| stored[k].start);
    let longest = stored.iter().map(|s| s.end - s.start).max().unwrap_or_default();

    let mut pairs: Vec<(chrono::Duration, usize, usize)> = Vec::new();
    for (i, &(a, b)) in stays.iter().enumerate() {
        let lo = by_start.partition_point(|&k| stored[k].start < a - longest);
        let hi = by_start.partition_point(|&k| stored[k].start < b);
        for &k in &by_start[lo..hi] {
            let overlap = b.min(stored[k].end) - a.max(stored[k].start);
            if overlap > chrono::Duration::zero() {
                pairs.push((overlap, i, k));
            }
        }
    }
    pairs.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));

    let mut keep: Vec<Option<String>> = vec![None; stays.len()];
    let mut taken = vec![false; stored.len()];
    for (_, i, k) in pairs {
        if keep[i].is_none() && !taken[k] {
            keep[i] = Some(stored[k].id.clone());
            taken[k] = true;
        }
    }
    let mut delete: Vec<String> = stored
        .iter()
        .zip(&taken)
        .filter(|(_, t)| !**t)
        .map(|(s, _)| s.id.clone())
        .collect();
    delete.sort();
    VisitRebuild { keep, delete }
}

/// Recompute every stored visit from the stored fixes, once per detector
/// version.
///
/// The rolling pass only revisits its last 30 hours, so a detector fix never
/// reaches history on its own. Guarded on `TIMELINE_VERSION`: it runs while
/// any visit was written by an older detector, rewrites them all in one
/// transaction (a box never shows a half-rebuilt timeline), and is a no-op
/// afterwards. Stays keep the ids of the visits they replace; see
/// `plan_visit_rebuild`.
pub async fn rebuild_visits(db: &Database) -> Result<usize> {
    let pool = db.pool();
    let stale: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM data_location_visit \
                         WHERE COALESCE((metadata->>'timeline')::bigint, 0) < $1)",
    )
    .bind(TIMELINE_VERSION)
    .fetch_one(pool)
    .await?;
    if !stale {
        return Ok(0);
    }

    let everything = TimeWindow::new(
        DateTime::from_timestamp(0, 0).unwrap_or_default(),
        Utc::now() + chrono::Duration::days(1),
    );
    let stays = detect_stays(&fetch_location_points(db, everything).await?)?;

    let mut tx = pool.begin().await?;
    let stored: Vec<StoredVisit> = sqlx::query_as::<_, (String, DateTime<Utc>, DateTime<Utc>, f64, f64)>(
        "SELECT id, started_at, COALESCE(ended_at, started_at), latitude, longitude \
         FROM data_location_visit FOR UPDATE",
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|(id, start, end, latitude, longitude)| StoredVisit { id, start, end, latitude, longitude })
    .collect();
    let before = stored.len();
    let spans: Vec<_> = stays.iter().map(|v| (v.start_time, v.end_time)).collect();
    let plan = plan_visit_rebuild(&spans, &stored);

    sqlx::query(
        "DELETE FROM wiki_refs WHERE source_table = 'data_location_visit' AND source_id = ANY($1)",
    )
    .bind(&plan.delete)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM data_location_visit WHERE id = ANY($1)")
        .bind(&plan.delete)
        .execute(&mut *tx)
        .await?;

    // A kept row takes its stay's first fix as `source_stream_id`, which is
    // UNIQUE, and another kept row may hold that value until its own turn.
    // Park every kept row on a value no fix can have first.
    let kept: Vec<String> = plan.keep.iter().flatten().cloned().collect();
    sqlx::query(
        "UPDATE data_location_visit SET source_stream_id = 'rebuild:' || id WHERE id = ANY($1)",
    )
    .bind(&kept)
    .execute(&mut *tx)
    .await?;

    for (stay, keep) in stays.iter().zip(&plan.keep) {
        let first_point = stay.points[0].id.to_string();
        let metadata = serde_json::json!({
            "point_count": stay.points.len(),
            "radius_meters": calculate_visit_radius(stay),
            "timeline": TIMELINE_VERSION,
        });
        let duration_minutes = (stay.end_time - stay.start_time).num_minutes() as i32;
        match keep {
            Some(id) => {
                sqlx::query(
                    "UPDATE data_location_visit \
                     SET started_at = $2, ended_at = $3, duration_minutes = $4, latitude = $5, \
                         longitude = $6, source_stream_id = $7, metadata = $8, updated_at = now() \
                     WHERE id = $1",
                )
                .bind(id)
                .bind(stay.start_time)
                .bind(stay.end_time)
                .bind(duration_minutes)
                .bind(stay.centroid_lat)
                .bind(stay.centroid_lon)
                .bind(&first_point)
                .bind(&metadata)
                .execute(&mut *tx)
                .await?;
            }
            None => {
                sqlx::query(
                    "INSERT INTO data_location_visit \
                     (id, latitude, longitude, started_at, ended_at, duration_minutes, \
                      source_stream_id, source_table, source_provider, metadata) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, 'location_point', 'ios', $8)",
                )
                // Keyed on the first fix, not `generate_visit_id`: a kept row
                // may already hold the (centroid, start) id this stay would
                // derive, while no two stays share a fix.
                .bind(Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("visit:{first_point}").as_bytes()).to_string())
                .bind(stay.centroid_lat)
                .bind(stay.centroid_lon)
                .bind(stay.start_time)
                .bind(stay.end_time)
                .bind(duration_minutes)
                .bind(&first_point)
                .bind(&metadata)
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    tx.commit().await?;

    let linked = link_unplaced_visits(db).await?;
    tracing::info!(
        before,
        after = stays.len(),
        kept = kept.len(),
        linked,
        "rebuilt visits from stored fixes"
    );
    Ok(stays.len())
}

/// Give every visit with no place link one, the way the rolling pass does for
/// the visits it writes.
async fn link_unplaced_visits(db: &Database) -> Result<usize> {
    let unplaced: Vec<(String, f64, f64, DateTime<Utc>)> = sqlx::query_as(
        "SELECT v.id, v.latitude, v.longitude, v.started_at FROM data_location_visit v \
         WHERE NOT EXISTS (SELECT 1 FROM wiki_refs r \
             WHERE r.source_table = 'data_location_visit' AND r.source_id = v.id \
               AND r.entity_type = 'place') \
         ORDER BY v.started_at",
    )
    .fetch_all(db.pool())
    .await?;
    for (id, lat, lon, started_at) in &unplaced {
        let place_id = resolve_or_create_place(db, *lat, *lon).await?;
        sqlx::query(
            "INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, role, occurred_at) \
             VALUES ($1, 'place', $2, 'data_location_visit', $3, 'location', $4) \
             ON CONFLICT (entity_id, source_table, source_id, role) DO NOTHING",
        )
        .bind(ids::generate_id("eref", &[id, &place_id, "location"]))
        .bind(&place_id)
        .bind(id)
        .bind(started_at)
        .execute(db.pool())
        .await?;
    }
    Ok(unplaced.len())
}

/// Find or create a place entity for the given coordinates
///
/// This function checks if a place entity exists within its configured radius.
/// Each place has its own radius_m (defaults to 100m). If found, returns its ID.
/// If not, creates a new place entity with reverse geocoded name.
///
/// Returns the place entity ID (format: place_{hash16}).
async fn resolve_or_create_place(db: &Database, lat: f64, lon: f64) -> Result<String> {
    // Use a generous bounding box to fetch candidates, then filter by each place's radius
    let max_search_radius = 500.0; // Fetch places within 500m, then check individual radii
    let (min_lat, max_lat, min_lon, max_lon) =
        crate::geo::bounding_box(lat, lon, max_search_radius);

    let candidates = sqlx::query!(
        r#"
        SELECT id, name, latitude, longitude, radius_m
        FROM wiki_places
        WHERE latitude IS NOT NULL
          AND longitude IS NOT NULL
          AND latitude BETWEEN $1 AND $2
          AND longitude BETWEEN $3 AND $4
        "#,
        min_lat,
        max_lat,
        min_lon,
        max_lon
    )
    .fetch_all(db.pool())
    .await?;

    // Find the nearest place where we're within that place's radius
    let mut best_match: Option<(&str, &str, f64)> = None; // (id, name, distance)

    for place in &candidates {
        let place_id = place.id.as_str();
        let Some(place_lat) = place.latitude else {
            continue;
        };
        let Some(place_lon) = place.longitude else {
            continue;
        };

        let distance = haversine_distance(lat, lon, place_lat, place_lon);
        let place_radius = place.radius_m;

        // Check if we're within this place's radius
        if distance <= place_radius {
            // Keep the closest match
            if best_match.is_none() || distance < best_match.unwrap().2 {
                best_match = Some((place_id, &place.name, distance));
            }
        }
    }

    if let Some((place_id, place_name, distance)) = best_match {
        tracing::debug!(
            place_id = %place_id,
            place_name = %place_name,
            distance_m = %distance,
            "Found existing place entity"
        );
        return Ok(place_id.to_string());
    }

    // Create new place entity with reverse geocoded name
    let place_name = reverse_geocode_stub(lat, lon);
    // Generate ID with proper prefix (place_{hash16})
    let place_id = ids::generate_id(
        ids::WIKI_PLACE_PREFIX,
        &[&lat.to_string(), &lon.to_string()],
    );

    sqlx::query!(
        r#"
        INSERT INTO wiki_places (
            id,
            name,
            latitude,
            longitude,
            radius_m,
            metadata
        ) VALUES (
            $1, $2, $3, $4, $5, '{}'
        )
        "#,
        place_id,
        place_name,
        lat,
        lon,
        DEFAULT_PLACE_RADIUS_METERS
    )
    .execute(db.pool())
    .await?;

    tracing::info!(
        place_id = %place_id,
        place_name = %place_name,
        lat = %lat,
        lon = %lon,
        radius_m = %DEFAULT_PLACE_RADIUS_METERS,
        "Created new place entity"
    );

    Ok(place_id)
}

/// Reverse geocode coordinates to human-readable name
///
/// Generates a coordinate-based label. For production use with proper place names,
/// integrate a reverse geocoding service (Nominatim, Google Places, Mapbox).
fn reverse_geocode_stub(lat: f64, lon: f64) -> String {
    format!("Location {:.4}, {:.4}", lat, lon)
}

/// Calculate radius of visit (max distance from centroid)
fn calculate_visit_radius(visit: &Visit) -> f64 {
    visit
        .points
        .iter()
        .map(|p| {
            haversine_distance(
                visit.centroid_lat,
                visit.centroid_lon,
                p.latitude,
                p.longitude,
            )
        })
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact descriptors that used to collapse into one org.
    #[test]
    fn processor_tags_do_not_eat_the_merchant() {
        // Was "Sq" — and so was every other Square charge on the box.
        assert_eq!(normalize_merchant_name("SQ *BLUE BOTTLE"), "Blue Bottle");
        assert_eq!(normalize_merchant_name("TST* PIZZERIA DELFINA"), "Pizzeria Delfina");
        assert_eq!(normalize_merchant_name("PAYPAL *STEAM GAMES"), "Steam Games");
        // Two tags, innermost wins.
        assert_eq!(normalize_merchant_name("PP*SQ *CORNER SHOP"), "Corner Shop");
    }

    /// " CO" used to match inside " COFFEE".
    #[test]
    fn a_suffix_must_actually_be_a_suffix() {
        assert_eq!(normalize_merchant_name("BLUE BOTTLE COFFEE"), "Blue Bottle Coffee");
        assert_eq!(normalize_merchant_name("COSTCO WHOLESALE"), "Costco Wholesale");
        assert_eq!(normalize_merchant_name("CORNER STORE"), "Corner Store");
        // But a real trailing legal form still goes, including stacked ones.
        assert_eq!(normalize_merchant_name("ACME CO"), "Acme");
        assert_eq!(normalize_merchant_name("ACME CO LTD"), "Acme");
        assert_eq!(normalize_merchant_name("INITECH INC."), "Initech");
    }

    #[test]
    fn store_numbers_are_dropped() {
        assert_eq!(normalize_merchant_name("STARBUCKS #1234"), "Starbucks");
        assert_eq!(normalize_merchant_name("SHELL - 4471"), "Shell");
    }

    /// An empty name would collide with every other empty name — the failure
    /// this rewrite exists to end, reintroduced by the fix itself.
    #[test]
    fn never_normalizes_to_nothing() {
        assert_eq!(normalize_merchant_name("SQ *"), "SQ *");
        assert_eq!(normalize_merchant_name("   "), "");
    }

    #[test]
    fn test_haversine_distance() {
        // San Francisco to Los Angeles (approx 559 km)
        let dist = haversine_distance(37.7749, -122.4194, 34.0522, -118.2437);
        assert!((dist - 559_000.0).abs() < 10_000.0); // Within 10km
    }

    fn at(minute: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + minute * 60, 0).unwrap()
    }

    fn point(minute: i64, latitude: f64, longitude: f64) -> LocationPoint {
        LocationPoint {
            id: Uuid::new_v4(),
            latitude,
            longitude,
            timestamp: at(minute),
            horizontal_accuracy: Some(10.0),
            _speed: None,
        }
    }

    fn stored(id: &str, start: i64, end: i64, latitude: f64, longitude: f64) -> StoredVisit {
        StoredVisit { id: id.into(), start: at(start), end: at(end), latitude, longitude }
    }

    // ~0.0027° of latitude ≈ 300 m: outside one cluster, outside one stay.
    const HERE: (f64, f64) = (30.0, -97.0);
    const NEAR: (f64, f64) = (30.0005, -97.0); // ≈ 55 m
    const FAR: (f64, f64) = (30.0027, -97.0);

    /// A still phone reports once every ~15 minutes. Those quiet stretches
    /// are the stay, not departures between a dozen ten-minute fragments.
    #[test]
    fn quiet_still_reports_are_one_stay() {
        let mut points: Vec<LocationPoint> = (0..5).map(|m| point(m, HERE.0, HERE.1)).collect();
        points.extend((1..=12).map(|k| point(k * 15, NEAR.0, NEAR.1)));
        let stays = detect_stays(&points).unwrap();
        assert_eq!(stays.len(), 1);
        assert_eq!((stays[0].start_time, stays[0].end_time), (at(0), at(180)));
    }

    /// Past the quiet cap the phone stopped reporting: the gap is unknown, so
    /// the stay ends at the last fix before it instead of spanning the outage.
    #[test]
    fn silence_past_the_cap_is_not_a_stay() {
        let points = vec![
            point(0, HERE.0, HERE.1),
            point(15, HERE.0, HERE.1),
            point(15 + MAX_QUIET_GAP_MINUTES + 1, HERE.0, HERE.1),
            point(30 + MAX_QUIET_GAP_MINUTES + 1, HERE.0, HERE.1),
        ];
        let stays = detect_stays(&points).unwrap();
        assert_eq!(stays.len(), 2);
        assert_eq!(stays[0].end_time, at(15));
        assert_eq!(stays[1].start_time, at(15 + MAX_QUIET_GAP_MINUTES + 1));
    }

    /// One wild fix is jitter; two in a row are a departure.
    #[test]
    fn one_far_fix_is_jitter_two_are_a_departure() {
        let mut points: Vec<LocationPoint> = (0..20).map(|m| point(m, HERE.0, HERE.1)).collect();
        points[10] = point(10, FAR.0, FAR.1);
        points.extend((20..40).map(|m| point(m, FAR.0, FAR.1)));
        let stays = detect_stays(&points).unwrap();
        assert_eq!(stays.len(), 2);
        assert_eq!((stays[0].start_time, stays[0].end_time), (at(0), at(19)));
        assert_eq!(stays[1].start_time, at(20));
    }

    /// Fixes alternating between two spots never yield two stays over the
    /// same minutes.
    #[test]
    fn stays_never_overlap() {
        let points: Vec<LocationPoint> = (0..40)
            .map(|m| if m % 2 == 0 { point(m, HERE.0, HERE.1) } else { point(m, FAR.0, FAR.1) })
            .collect();
        let stays = detect_stays(&points).unwrap();
        for w in stays.windows(2) {
            assert!(w[0].end_time < w[1].start_time, "two stays claim the same minutes");
        }
    }

    /// A stay whose stored row lost its place ref, re-clustered with a drifted
    /// centroid, is the same stay — not a second row beside it.
    #[test]
    fn a_drifted_recluster_extends_the_stored_stay() {
        let rows = [stored("v1", 0, 60, HERE.0, HERE.1), stored("v2", 15, 60, HERE.0, HERE.1)];
        let plan = plan_visit_write((at(10), at(75)), NEAR, &rows);
        assert_eq!(
            plan,
            VisitWrite::Extend { keeper: "v1".into(), absorb: vec!["v2".into()], span: (at(0), at(75)) }
        );
    }

    /// Another place already holds those minutes: the candidate keeps what is left.
    #[test]
    fn a_rival_stay_clips_the_candidate() {
        let rows = [stored("away", 0, 30, FAR.0, FAR.1)];
        assert_eq!(
            plan_visit_write((at(20), at(90)), HERE, &rows),
            VisitWrite::Insert { span: (at(30), at(90)) }
        );
        // Nothing worth a visit survives the clip.
        assert_eq!(plan_visit_write((at(0), at(35)), HERE, &rows), VisitWrite::Skip);
    }

    #[test]
    fn longest_uncovered_picks_the_biggest_gap() {
        let got = longest_uncovered(
            (at(0), at(100)),
            [(at(10), at(20)), (at(60), at(70)), (at(15), at(40))].into_iter(),
        );
        assert_eq!(got, Some((at(70), at(100))));
        assert_eq!(longest_uncovered((at(0), at(10)), [(at(0), at(10))].into_iter()), None);
    }

    /// A rebuilt stay keeps the id of the stored visit it covers most, so
    /// citations to it still resolve; stored rows no stay covers go.
    #[test]
    fn rebuild_keeps_ids_by_largest_overlap() {
        let rows = vec![
            stored("old_a", 0, 30, HERE.0, HERE.1),
            stored("old_b", 10, 120, HERE.0, HERE.1),
            stored("orphan", 300, 320, FAR.0, FAR.1),
        ];
        let plan = plan_visit_rebuild(&[(at(0), at(130)), (at(140), at(200))], &rows);
        assert_eq!(plan.keep, vec![Some("old_b".into()), None]);
        assert_eq!(plan.delete, vec!["old_a".to_string(), "orphan".into()]);
    }

    /// Two stays both overlapping one stored row: only one may take its id.
    #[test]
    fn rebuild_never_gives_one_id_to_two_stays() {
        let rows = vec![stored("wide", 0, 200, HERE.0, HERE.1)];
        let plan = plan_visit_rebuild(&[(at(0), at(50)), (at(60), at(200))], &rows);
        assert_eq!(plan.keep, vec![None, Some("wide".into())]);
        assert!(plan.delete.is_empty());
    }

    #[test]
    fn test_generate_visit_id_deterministic() {
        let lat = 37.7749;
        let lon = -122.4194;
        let time = Utc::now();

        let id1 = generate_visit_id(lat, lon, time);
        let id2 = generate_visit_id(lat, lon, time);

        assert_eq!(id1, id2);
    }
}
