//! Timezone resolution.
//!
//! Two distinct timezones (see agents/record/timezone-model.md):
//!
//! 1. **`home_timezone`** — the timezone of the box's physical location, read
//!    from the server's own system clock ([`system_timezone`]). Stable anchor +
//!    fallback floor. Stored on `app_user_profile.home_timezone`.
//! 2. **per-day user-location tz** — *where the owner was that day*, resolved
//!    from the GPS track via [`coords_to_tz`] and fixed at the day's start
//!    ("the timezone you woke up in"). Stored on `wiki_days.start_timezone`.

use std::sync::OnceLock;

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgPool, Row};

use crate::api::day_summary::day_boundaries_utc;

/// The box's own system timezone as an IANA string (e.g. `"Europe/Berlin"`),
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
/// See agents/record/timezone-model.md.
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

/// The zone `date` is windowed in, the one every reader of a day uses:
///   1. `wiki_days.start_timezone`, once narration has locked it: a day keeps
///      the zone it was first windowed in;
///   2. else the zone of the day's first located point ("where you woke up");
///   3. else `home_timezone`, and UTC only when the profile cannot be read.
///
/// Falling through to the home zone on a day with no location is what step 1
/// exists to stop: re-resolving a locked past day would re-window it hours off.
/// See agents/record/timezone-model.md.
pub async fn day_timezone(pool: &PgPool, date: NaiveDate) -> crate::error::Result<String> {
    let home_tz = home_timezone_or_utc(pool, date).await;
    Ok(located_day_timezone(pool, date, &home_tz)
        .await?
        .unwrap_or(home_tz))
}

/// Steps 1 and 2 of [`day_timezone`]: the zone the record itself gives `date`,
/// or `None` when it gives none and the caller must pick a fallback.
pub async fn located_day_timezone(
    pool: &PgPool,
    date: NaiveDate,
    home_tz: &str,
) -> crate::error::Result<Option<String>> {
    let locked: Option<String> =
        sqlx::query_scalar("SELECT start_timezone FROM wiki_days WHERE date = $1")
            .bind(date)
            .fetch_optional(pool)
            .await?
            .flatten();
    if let Some(tz) = locked.filter(|t| !t.is_empty()) {
        return Ok(Some(tz));
    }
    Ok(first_point_timezone(pool, date, home_tz).await)
}

/// The box's home zone, or UTC when the profile cannot be read.
pub async fn home_timezone_or_utc(pool: &PgPool, date: NaiveDate) -> String {
    match crate::api::profile::get_timezone(pool).await {
        Ok(tz) => tz,
        Err(e) => {
            tracing::warn!(date = %date, error = %e, "couldn't read the home timezone; windowing the day in UTC");
            None
        }
    }
    .unwrap_or_else(|| "UTC".to_string())
}

/// Set the transaction's `TimeZone` to the owner's, so SQL someone else wrote
/// reads dates the way the owner does: a bare `'2026-10-06'`, `::date`,
/// `date_trunc('day', …)` and `to_char(…)` mean the owner's day and clock, not
/// UTC's. Returns the zone it set.
///
/// `preferred` is the zone the caller already knows the owner is in (a chat's
/// client clock); else `home_timezone`; else UTC. Each is checked against
/// Postgres's own `pg_timezone_names` first, because `set_config` with a name
/// it does not know raises, and a raise aborts the transaction the caller's
/// query was about to run in.
///
/// Call it before any `SET LOCAL ROLE`: the profile is an `app_*` table, which
/// the reader roles cannot see. `is_local`, so it reverts with the transaction
/// and never reaches the next borrower of a pooled connection.
pub async fn set_local_timezone(
    conn: &mut sqlx::PgConnection,
    preferred: Option<&str>,
) -> Result<String, sqlx::Error> {
    // One scan of pg_timezone_names (it reads the zone files, ~15ms), not one
    // per candidate.
    sqlx::query_scalar(
        "SELECT set_config('TimeZone', COALESCE(( \
             SELECT name FROM pg_timezone_names \
              WHERE name = $1 \
                 OR name = (SELECT home_timezone FROM app_user_profile LIMIT 1) \
              ORDER BY name = $1 DESC NULLS LAST \
              LIMIT 1), 'UTC'), true)",
    )
    .bind(preferred)
    .fetch_one(conn)
    .await
}

/// `date`'s local midnight in [`day_timezone`], to where the next day begins:
/// the next date's midnight in *its* zone. A day that ends in another zone
/// than it began in (a flight across zones) ends where the next one starts,
/// so no hour falls between two days and none is counted on both.
pub async fn day_window(
    pool: &PgPool,
    date: NaiveDate,
) -> crate::error::Result<(DateTime<Utc>, DateTime<Utc>)> {
    let tz = day_timezone(pool, date).await?;
    let (start, same_zone_end) = crate::api::day_summary::day_bounds(date, Some(&tz));
    let Some(next) = date.succ_opt() else {
        return Ok((start, same_zone_end));
    };
    let next_tz = day_timezone(pool, next).await?;
    let (end, _) = crate::api::day_summary::day_bounds(next, Some(&next_tz));
    // Past the date line a day could close before it opens; it keeps its own
    // midnight then.
    Ok((start, if end > start { end } else { same_zone_end }))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn lock(pool: &PgPool, date: &str, tz: &str) {
        sqlx::query("INSERT INTO wiki_days (id, date, start_timezone) VALUES ($1, $2::date, $3)")
            .bind(format!("day_{date}"))
            .bind(date)
            .bind(tz)
            .execute(pool)
            .await
            .unwrap();
    }

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_day_ends_where_the_next_begins(pool: PgPool) {
        // Woke up in Winnipeg (UTC-5 in June), the next day in Regina
        // (UTC-6): the first day runs 25 hours, to Regina's midnight.
        lock(&pool, "2026-06-10", "America/Winnipeg").await;
        lock(&pool, "2026-06-11", "America/Regina").await;
        lock(&pool, "2026-06-12", "America/Winnipeg").await;
        let d = |s: &str| s.parse::<NaiveDate>().unwrap();
        let (a, b) = day_window(&pool, d("2026-06-10")).await.unwrap();
        assert_eq!((a, b), (utc("2026-06-10T05:00:00Z"), utc("2026-06-11T06:00:00Z")));
        // Flying back: the next day starts at Winnipeg's midnight, an hour
        // earlier, and this one ends there; nothing is counted twice.
        let (c, e) = day_window(&pool, d("2026-06-11")).await.unwrap();
        assert_eq!((c, e), (b, utc("2026-06-12T05:00:00Z")));
    }
}
