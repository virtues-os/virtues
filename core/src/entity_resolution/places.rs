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
//! 2. Auto-detect sampling rate (points/minute)
//! 3. Run spatial-temporal clustering
//! 4. Write location_visit records
//! 5. Link visits to place entities (create if new)
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
const TEMPORAL_GAP_MINUTES: i64 = 5; // Max time gap within visit (robust for iOS backgrounding)
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

    // Auto-detect sampling rate
    let sampling_rate = detect_sampling_rate(&points);
    tracing::debug!(points_per_minute = sampling_rate, "Detected sampling rate");

    // Run density-adaptive clustering
    let visits = cluster_location_points(&points, sampling_rate)?;

    tracing::debug!(visit_count = visits.len(), "Completed clustering");

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
    // Fetch transactions where metadata doesn't have merchant_org_id
    let rows = sqlx::query!(
        r#"
        SELECT
            id,
            merchant_name,
            merchant_category
        FROM data_financial_transaction
        WHERE timestamp >= $1
          AND timestamp < $2
          AND merchant_name IS NOT NULL
          AND merchant_name != ''
          AND (metadata IS NULL OR json_extract(metadata, '$.merchant_org_id') IS NULL)
        ORDER BY timestamp ASC
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
                id: row.id?,
                merchant_name: row.merchant_name?,
                merchant_category: row.merchant_category,
            })
        })
        .collect();

    Ok(transactions)
}

/// Resolve merchant to wiki_orgs and link to transaction
async fn resolve_and_link_merchant(db: &Database, txn: &TransactionRecord) -> Result<bool> {
    let merchant_name = txn.merchant_name.trim();
    if merchant_name.is_empty() {
        return Ok(false);
    }

    // Resolve or create organization for this merchant
    let org_id = resolve_or_create_merchant_org(db, merchant_name, txn.merchant_category.as_deref())
        .await?;

    // Update transaction metadata with org reference
    sqlx::query!(
        r#"
        UPDATE data_financial_transaction
        SET metadata = json_set(
            COALESCE(metadata, '{}'),
            '$.merchant_org_id', $1
        ),
        updated_at = datetime('now')
        WHERE id = $2
        "#,
        org_id,
        txn.id
    )
    .execute(db.pool())
    .await?;

    tracing::debug!(
        transaction_id = %txn.id,
        merchant_name = %merchant_name,
        org_id = %org_id,
        "Linked transaction to merchant organization"
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
        WHERE LOWER(canonical_name) = LOWER($1)
           OR LOWER(canonical_name) = LOWER($2)
        LIMIT 1
        "#,
        merchant_name,
        normalized_name
    )
    .fetch_optional(db.pool())
    .await?;

    if let Some(row) = existing {
        if let Some(org_id) = row.id {
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
    let metadata_json = serde_json::to_string(&metadata)?;

    sqlx::query!(
        r#"
        INSERT INTO wiki_orgs (
            id,
            canonical_name,
            organization_type,
            relationship_type,
            metadata
        ) VALUES ($1, $2, $3, 'vendor', $4)
        ON CONFLICT (id) DO NOTHING
        "#,
        org_id,
        normalized_name,
        organization_type,
        metadata_json
    )
    .execute(db.pool())
    .await?;

    tracing::info!(
        org_id = %org_id,
        canonical_name = %normalized_name,
        organization_type = %organization_type,
        "Created new merchant organization"
    );

    Ok(org_id)
}

/// Normalize merchant name for matching
///
/// Removes common suffixes, special characters, and normalizes capitalization.
fn normalize_merchant_name(name: &str) -> String {
    let mut normalized = name.to_string();

    // Remove common suffixes
    let suffixes = [
        " INC",
        " LLC",
        " LTD",
        " CORP",
        " CO",
        " #",
        "*",
        " - ",
        "  ",
    ];
    for suffix in &suffixes {
        if let Some(pos) = normalized.to_uppercase().find(suffix) {
            normalized.truncate(pos);
        }
    }

    // Title case
    normalized
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>()
                        + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
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
            timestamp,
            horizontal_accuracy
        FROM data_location_point
        WHERE timestamp >= $1
          AND timestamp < $2
          AND (horizontal_accuracy IS NULL OR horizontal_accuracy < $3)
        ORDER BY timestamp ASC
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
            let id = row.id.as_ref().and_then(|s| Uuid::parse_str(s).ok())?;
            let timestamp = DateTime::parse_from_rfc3339(&row.timestamp)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))?;
            Some(LocationPoint {
                id,
                latitude: row.latitude,
                longitude: row.longitude,
                timestamp,
                horizontal_accuracy: row.horizontal_accuracy,
                _speed: None,
            })
        })
        .collect();

    Ok(points)
}

/// Auto-detect sampling rate from point density
fn detect_sampling_rate(points: &[LocationPoint]) -> f64 {
    if points.len() < 10 {
        return 1.0; // Default to 1 point/min
    }

    // Calculate median time gap between consecutive points
    let mut gaps: Vec<f64> = points
        .windows(2)
        .filter_map(|w| {
            let gap_seconds = (w[1].timestamp - w[0].timestamp).num_seconds();
            if gap_seconds > 0 && gap_seconds < 600 {
                // Sanity check: 0-10 minutes
                Some(60.0 / gap_seconds as f64)
            } else {
                None
            }
        })
        .collect();

    if gaps.is_empty() {
        return 1.0;
    }

    gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    gaps[gaps.len() / 2] // Median points per minute
}

/// Cluster location points using density-adaptive spatial-temporal clustering
fn cluster_location_points(points: &[LocationPoint], points_per_minute: f64) -> Result<Vec<Visit>> {
    // Calculate density-adaptive parameters
    let min_cluster_size = (MIN_VISIT_DURATION_MINUTES as f64 * points_per_minute).round() as usize;
    let min_cluster_size = min_cluster_size.max(3); // At least 3 points

    tracing::debug!(min_cluster_size, "Calculated adaptive parameters");

    // Simple density-based clustering (DBSCAN-like)
    let mut visits = Vec::new();
    let mut visited = vec![false; points.len()];

    for i in 0..points.len() {
        if visited[i] {
            continue;
        }

        // Start a new potential cluster
        let mut cluster_points = vec![points[i].clone()];
        visited[i] = true;

        // Expand cluster
        let mut j = i + 1;
        while j < points.len() {
            if visited[j] {
                j += 1;
                continue;
            }

            let last_point = cluster_points.last().unwrap();
            let current_point = &points[j];

            // Check spatial distance
            let distance = haversine_distance(
                last_point.latitude,
                last_point.longitude,
                current_point.latitude,
                current_point.longitude,
            );

            // Check temporal gap
            let time_gap = (current_point.timestamp - last_point.timestamp).num_minutes();

            if distance <= SPATIAL_EPSILON_METERS && time_gap <= TEMPORAL_GAP_MINUTES {
                // Point belongs to cluster
                cluster_points.push(current_point.clone());
                visited[j] = true;
                j += 1;
            } else if time_gap > TEMPORAL_GAP_MINUTES {
                // Temporal gap too large - end cluster
                break;
            } else {
                // Spatial distance too large but temporal OK - skip point
                j += 1;
            }
        }

        // Check if cluster meets minimum size
        if cluster_points.len() >= min_cluster_size {
            let visit = create_visit_from_cluster(cluster_points)?;
            let duration_minutes = (visit.end_time - visit.start_time).num_minutes();

            // Filter by minimum duration
            if duration_minutes >= MIN_VISIT_DURATION_MINUTES {
                visits.push(visit);
            }
        }
    }

    Ok(visits)
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

/// Write visit idempotently to database and link to place entity
async fn write_visit_and_link_place(db: &Database, visit: &Visit) -> Result<()> {
    let visit_id = generate_visit_id(visit.centroid_lat, visit.centroid_lon, visit.start_time);

    // Find or create place entity for this location
    let place_id = resolve_or_create_place(db, visit.centroid_lat, visit.centroid_lon).await?;

    let duration_minutes = (visit.end_time - visit.start_time).num_minutes() as i32;

    let metadata = serde_json::json!({
        "point_count": visit.points.len(),
        "radius_meters": calculate_visit_radius(visit),
    });

    sqlx::query!(
        r#"
        INSERT INTO data_location_visit (
            id,
            place_id,
            latitude,
            longitude,
            arrival_time,
            departure_time,
            duration_minutes,
            source_stream_id,
            source_table,
            source_provider,
            metadata
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11
        )
        ON CONFLICT (id) DO UPDATE SET
            place_id = EXCLUDED.place_id,
            departure_time = EXCLUDED.departure_time,
            duration_minutes = EXCLUDED.duration_minutes,
            latitude = EXCLUDED.latitude,
            longitude = EXCLUDED.longitude,
            metadata = EXCLUDED.metadata,
            updated_at = datetime('now')
        WHERE data_location_visit.departure_time < EXCLUDED.departure_time
           OR data_location_visit.place_id IS NULL
        "#,
        visit_id,
        place_id,
        visit.centroid_lat,
        visit.centroid_lon,
        visit.start_time,
        visit.end_time,
        duration_minutes,
        visit.points.first().unwrap().id, // Use first point ID as source
        "location_point",
        "ios",
        metadata
    )
    .execute(db.pool())
    .await?;

    Ok(())
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
        let Some(place_id) = place.id.as_ref() else {
            continue;
        };
        let Some(place_lat) = place.latitude else {
            continue;
        };
        let Some(place_lon) = place.longitude else {
            continue;
        };

        let distance = haversine_distance(lat, lon, place_lat, place_lon);
        let place_radius = place.radius_m.unwrap_or(DEFAULT_PLACE_RADIUS_METERS);

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
    use chrono::Duration;

    #[test]
    fn test_haversine_distance() {
        // San Francisco to Los Angeles (approx 559 km)
        let dist = haversine_distance(37.7749, -122.4194, 34.0522, -118.2437);
        assert!((dist - 559_000.0).abs() < 10_000.0); // Within 10km
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
