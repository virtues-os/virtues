//! Geospatial utilities
//!
//! Provides distance calculations and bounding box utilities for location-based queries.
//! Uses Haversine formula for accurate great-circle distance calculations.

use geo::HaversineDistance;
use geo::Point as GeoPoint;

/// Earth radius in meters (WGS84 semi-major axis)
pub const EARTH_RADIUS_METERS: f64 = 6_371_000.0;

/// Calculate Haversine distance between two points in meters
///
/// The Haversine formula calculates the great-circle distance between two points
/// on a sphere given their longitudes and latitudes.
///
/// # Arguments
/// * `lat1` - Latitude of first point in degrees
/// * `lon1` - Longitude of first point in degrees
/// * `lat2` - Latitude of second point in degrees
/// * `lon2` - Longitude of second point in degrees
///
/// # Returns
/// Distance in meters between the two points
pub fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let p1 = GeoPoint::new(lon1, lat1);
    let p2 = GeoPoint::new(lon2, lat2);
    p1.haversine_distance(&p2)
}

/// Calculate a bounding box for initial database filtering
///
/// This creates a rectangular bounding box around a center point that can be used
/// in SQL WHERE clauses for efficient pre-filtering before applying precise Haversine
/// distance calculations.
///
/// Note: This is an approximation that works well for short distances (< 100km).
/// The longitude approximation degrades at extreme latitudes.
///
/// # Arguments
/// * `lat` - Center latitude in degrees
/// * `lon` - Center longitude in degrees
/// * `radius_meters` - Search radius in meters
///
/// # Returns
/// Tuple of (min_lat, max_lat, min_lon, max_lon)
pub fn bounding_box(lat: f64, lon: f64, radius_meters: f64) -> (f64, f64, f64, f64) {
    // Approximate degrees per meter
    // 1 degree latitude ≈ 111.32 km at the equator
    let lat_delta = radius_meters / 111_320.0;

    // Longitude degrees vary by latitude (cos of latitude)
    // Handle edge case where cos(lat) could be very small near poles
    let cos_lat = lat.to_radians().cos().max(0.01);
    let lon_delta = radius_meters / (111_320.0 * cos_lat);

    (
        lat - lat_delta, // min_lat
        lat + lat_delta, // max_lat
        lon - lon_delta, // min_lon
        lon + lon_delta, // max_lon
    )
}

/// Find the nearest point from a list of candidates
///
/// # Arguments
/// * `center_lat` - Center latitude in degrees
/// * `center_lon` - Center longitude in degrees
/// * `candidates` - Iterator of (id, lat, lon) tuples
/// * `max_distance_meters` - Maximum distance to consider
///
/// # Returns
/// Option containing (id, distance_meters) of the nearest point within max_distance
pub fn find_nearest<T, I>(
    center_lat: f64,
    center_lon: f64,
    candidates: I,
    max_distance_meters: f64,
) -> Option<(T, f64)>
where
    I: IntoIterator<Item = (T, f64, f64)>,
{
    candidates
        .into_iter()
        .map(|(id, lat, lon)| {
            let dist = haversine_distance(center_lat, center_lon, lat, lon);
            (id, dist)
        })
        .filter(|(_, dist)| *dist <= max_distance_meters)
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_haversine_distance() {
        // San Francisco to Los Angeles (approx 559 km)
        let dist = haversine_distance(37.7749, -122.4194, 34.0522, -118.2437);
        assert!((dist - 559_000.0).abs() < 10_000.0); // Within 10km
    }

    #[test]
    fn test_haversine_distance_same_point() {
        let dist = haversine_distance(37.7749, -122.4194, 37.7749, -122.4194);
        assert!(dist < 1.0); // Should be essentially 0
    }

    #[test]
    fn test_haversine_distance_short() {
        // Two points about 100 meters apart
        // At 37.7749 lat, ~0.001 degrees longitude ≈ 88 meters
        let dist = haversine_distance(37.7749, -122.4194, 37.7749, -122.4184);
        assert!(dist > 80.0 && dist < 100.0);
    }

    #[test]
    fn test_bounding_box() {
        let (min_lat, max_lat, min_lon, max_lon) = bounding_box(37.7749, -122.4194, 1000.0);

        // 1km radius should give roughly ±0.009 degrees latitude
        assert!((max_lat - min_lat - 0.018).abs() < 0.001);

        // Box should be centered on the point
        assert!((((max_lat + min_lat) / 2.0) - 37.7749).abs() < 0.001);
        assert!((((max_lon + min_lon) / 2.0) - (-122.4194)).abs() < 0.001);
    }

    #[test]
    fn test_find_nearest() {
        let candidates = vec![
            ("a", 37.7749, -122.4194), // Center point
            ("b", 37.7759, -122.4194), // ~111m north
            ("c", 37.7849, -122.4194), // ~1.1km north
        ];

        let result = find_nearest(37.7749, -122.4194, candidates.clone(), 500.0);
        assert!(result.is_some());
        let (id, dist) = result.unwrap();
        assert_eq!(id, "a");
        assert!(dist < 1.0);

        // Test with offset center
        let result2 = find_nearest(37.7755, -122.4194, candidates, 500.0);
        assert!(result2.is_some());
    }
}
