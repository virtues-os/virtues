//! Strava API response types
//!
//! Based on the Strava V3 API: https://developers.strava.com/docs/reference/

use serde::{Deserialize, Serialize};

/// Summary representation of an activity (from GET /athlete/activities)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryActivity {
    pub id: i64,
    pub name: String,
    pub sport_type: String,
    pub start_date: String, // ISO 8601
    pub elapsed_time: i64,  // seconds
    pub distance: Option<f64>, // meters
    pub total_elevation_gain: Option<f64>,
    pub average_speed: Option<f64>, // m/s
    pub max_speed: Option<f64>,
    pub average_heartrate: Option<f64>,
    pub max_heartrate: Option<f64>,
    pub kilojoules: Option<f64>,
    pub suffer_score: Option<f64>,
    pub gear_id: Option<String>,
    pub map: Option<ActivityMap>,
    #[serde(rename = "type")]
    pub activity_type: String,
}

/// Map/route data for an activity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityMap {
    pub id: Option<String>,
    pub summary_polyline: Option<String>,
}
