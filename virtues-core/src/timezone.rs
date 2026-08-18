//! Timezone resolution.
//!
//! Two distinct timezones (see docs/timezone-model.md):
//!
//! 1. **`home_timezone`** — the timezone of the box's physical location, read
//!    from the server's own system clock ([`system_timezone`]). Stable anchor +
//!    fallback floor. Stored on `app_user_profile.home_timezone`.
//! 2. **per-day user-location tz** — *where the owner was that day*, resolved
//!    from the GPS track via [`coords_to_tz`] and fixed at the day's start
//!    ("the timezone you woke up in"). Stored on `wiki_days.start_timezone`.

use std::sync::OnceLock;

use chrono::NaiveDate;
use sqlx::{PgPool, Row};

use crate::api::day_summary::day_boundaries_utc;

/// The box's own system timezone as an IANA string (e.g. `"America/Chicago"`),
/// read from `/etc/localtime`. This is "the location of the server."
///
/// Returns `None` if the system timezone can't be determined.
pub fn system_timezone() -> Option<String> {
    iana_time_zone::get_timezone().ok()
}

/// Resolve an IANA timezone name for a coordinate, fully offline.
///
/// Backed by a lazily-initialised `tzf_rs::DefaultFinder` (loads embedded
/// timezone-boundary data once; construction is comparatively expensive so it is
/// memoised). Returns `None` for coordinates with no resolvable zone (e.g. open
/// ocean) or empty results.
pub fn coords_to_tz(lat: f64, lon: f64) -> Option<String> {
    static FINDER: OnceLock<tzf_rs::DefaultFinder> = OnceLock::new();
    let finder = FINDER.get_or_init(tzf_rs::DefaultFinder::new);
    // tzf-rs takes (lng, lat).
    let name = finder.get_tz_name(lon, lat);
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// The IANA zone of the **first** located GPS point of `date`'s local day, or
/// `None` if the day has no located points (web-only, location off, or the point
/// has no resolvable zone).
///
/// This is the "where you woke up" signal, and it is deliberately the *first*
/// point (init), not the most-dwelt zone: the day's tz must be deterministic the
/// moment the day begins and must not drift as the day unfolds. The same value
/// is used live (today) and at the EOD lock, so they agree on travel days.
/// `home_tz` only roughly bounds the local-day window to locate the points.
/// See docs/timezone-model.md.
pub async fn first_point_timezone(
    pool: &PgPool,
    date: NaiveDate,
    home_tz: &str,
) -> Option<String> {
    let (start_str, end_str) = day_boundaries_utc(date, Some(home_tz));

    let row = sqlx::query(
        r#"
        SELECT latitude, longitude
        FROM data_location_point
        WHERE occurred_at >= $1::timestamptz AND occurred_at < $2::timestamptz
        ORDER BY occurred_at ASC
        LIMIT 1
        "#,
    )
    .bind(&start_str)
    .bind(&end_str)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;

    let lat: Option<f64> = row.try_get("latitude").ok();
    let lon: Option<f64> = row.try_get("longitude").ok();
    match (lat, lon) {
        (Some(lat), Some(lon)) => coords_to_tz(lat, lon),
        _ => None,
    }
}

/// Resolve the per-day "where the owner was" timezone for `date`, falling back to
/// `home_tz` when the day has no located points. Used at the EOD lock.
pub async fn resolve_day_timezone(pool: &PgPool, date: NaiveDate, home_tz: &str) -> String {
    first_point_timezone(pool, date, home_tz)
        .await
        .unwrap_or_else(|| home_tz.to_string())
}
