//! Place Resolution via Location Clustering
//!
//! Transforms raw location_point primitives into semantic location_visit records
//! using density-adaptive HDBSCAN-like clustering, then links to entities_place.
//!
//! ## Process
//!
//! 1. Fetch location_point records in time window
//! 2. Auto-detect sampling rate (points/minute)
//! 3. Run spatial-temporal clustering
//! 4. Write location_visit records
//! 5. Link visits to place entities (create if new)

use chrono::{DateTime, Utc};
use geo::HaversineDistance;
use geo::Point as GeoPoint;
use uuid::Uuid;

use super::TimeWindow;
use crate::database::Database;
use crate::error::{Error, Result};

/// Spatial clustering parameters
const SPATIAL_EPSILON_METERS: f64 = 75.0; // Max distance within a cluster (better GPS drift tolerance)
const MIN_VISIT_DURATION_MINUTES: i64 = 5; // Minimum visit duration
const TEMPORAL_GAP_MINUTES: i64 = 5; // Max time gap within visit (robust for iOS backgrounding)
const MAX_HORIZONTAL_ACCURACY: f64 = 100.0; // Filter low-quality points

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

/// Resolve places for the given time window
///
/// Returns the number of visits created/updated.
pub async fn resolve_places(db: &Database, window: TimeWindow) -> Result<usize> {
    tracing::info!(
        start = %window.start,
        end = %window.end,
        "Resolving places via location clustering"
    );

    // 1. Fetch location points in window
    let points = fetch_location_points(db, window).await?;

    if points.is_empty() {
        tracing::debug!("No location points to cluster");
        return Ok(0);
    }

    tracing::debug!(point_count = points.len(), "Fetched location points");

    // 2. Auto-detect sampling rate
    let sampling_rate = detect_sampling_rate(&points);
    tracing::debug!(points_per_minute = sampling_rate, "Detected sampling rate");

    // 3. Run density-adaptive clustering
    let visits = cluster_location_points(&points, sampling_rate)?;

    tracing::debug!(visit_count = visits.len(), "Completed clustering");

    // 4. Write visits idempotently and link to place entities
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

    tracing::info!(
        visits_written = records_written,
        "Place resolution completed"
    );

    Ok(records_written)
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
/// This function checks if a place entity exists within 75 meters of the given coordinates.
/// If found, returns its ID. If not, creates a new place entity with reverse geocoded name.
async fn resolve_or_create_place(db: &Database, lat: f64, lon: f64) -> Result<Uuid> {
    // Check for existing place within 75 meters (match SPATIAL_EPSILON_METERS)
    // Use bounding box for efficient SQL filtering, then Haversine for precise distance
    let search_radius = 75.0;
    let (min_lat, max_lat, min_lon, max_lon) = crate::geo::bounding_box(lat, lon, search_radius);

    let candidates = sqlx::query!(
        r#"
        SELECT id, name, latitude, longitude
        FROM data_entities_place
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

    // Find nearest place using Haversine distance
    let nearest = crate::geo::find_nearest(
        lat,
        lon,
        candidates.iter().filter_map(|p| {
            // Only include places with valid IDs
            let id = p.id.as_ref()?;
            Some((id.clone(), p.latitude?, p.longitude?))
        }),
        search_radius,
    );

    if let Some((place_id, _distance)) = nearest {
        let place = candidates
            .iter()
            .find(|p| p.id.as_deref() == Some(&place_id));
        if let Some(p) = place {
            let id_str = p.id.as_ref().unwrap();
            tracing::debug!(
                place_id = %id_str,
                place_name = %p.name,
                "Found existing place entity"
            );
            return Uuid::parse_str(id_str)
                .map_err(|e| Error::Database(format!("Invalid UUID: {}", e)));
        }
    }

    // Create new place entity with reverse geocoded name
    let place_name = reverse_geocode_stub(lat, lon);
    let place_id = Uuid::new_v4();
    let place_id_str = place_id.to_string();

    sqlx::query!(
        r#"
        INSERT INTO data_entities_place (
            id,
            name,
            latitude,
            longitude,
            radius_m,
            metadata
        ) VALUES (
            $1, $2, $3, $4, 75.0, '{}'
        )
        "#,
        place_id_str,
        place_name,
        lat,
        lon
    )
    .execute(db.pool())
    .await?;

    tracing::info!(
        place_id = %place_id,
        place_name = %place_name,
        lat = %lat,
        lon = %lon,
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
