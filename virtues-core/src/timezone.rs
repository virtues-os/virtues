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
/// The profile is an `app_*` table, which the restricted logins cannot see. A
/// transaction on one of them resolves the zone on the main pool with
/// [`resolve_timezone`] and sets it with [`apply_timezone`]. `is_local`, so it
/// reverts with the transaction and never reaches the next borrower of a
/// pooled connection.
pub async fn set_local_timezone(
    conn: &mut sqlx::PgConnection,
    preferred: Option<&str>,
) -> Result<String, sqlx::Error> {
    let zone = resolve_timezone(&mut *conn, preferred).await?;
    apply_timezone(conn, &zone).await
}

/// The zone [`set_local_timezone`] would set, without setting it.
pub async fn resolve_timezone<'c>(
    conn: impl sqlx::PgExecutor<'c>,
    preferred: Option<&str>,
) -> Result<String, sqlx::Error> {
    // One scan of pg_timezone_names (it reads the zone files, ~15ms), not one
    // per candidate.
    sqlx::query_scalar(
        "SELECT COALESCE(( \
             SELECT name FROM pg_timezone_names \
              WHERE name = $1 \
                 OR name = (SELECT home_timezone FROM app_user_profile LIMIT 1) \
              ORDER BY name = $1 DESC NULLS LAST \
              LIMIT 1), 'UTC')",
    )
    .bind(preferred)
    .fetch_one(conn)
    .await
}

/// Set a zone [`resolve_timezone`] returned for the rest of the transaction.
pub async fn apply_timezone(conn: &mut sqlx::PgConnection, zone: &str) -> Result<String, sqlx::Error> {
    sqlx::query_scalar("SELECT set_config('TimeZone', $1, true)")
        .bind(zone)
        .fetch_one(conn)
        .await
}

/// An instant as RFC 3339 on the owner's clock, with its offset
/// (`2026-10-13T22:12:00-05:00`), or in UTC when `zone` is not a zone chrono
/// knows. Tools hand the model times this way so it never converts a clock
/// time itself: a model given an evening message's UTC stamp quoted it
/// as the next morning. Postgres reads
/// the offset form back exactly, so a time pasted into a query still matches.
pub fn owner_rfc3339(at: DateTime<Utc>, zone: Option<&str>) -> String {
    match zone.and_then(|z| z.parse::<chrono_tz::Tz>().ok()) {
        Some(tz) => at.with_timezone(&tz).to_rfc3339(),
        None => at.to_rfc3339(),
    }
}

/// The same instant as a person reads it: `Tue Oct 13 2026, 10:12 PM`. The
/// weekday is spelled out because a model works it out from a date wrongly.
pub fn owner_label(at: DateTime<Utc>, zone: Option<&str>) -> String {
    const FORMAT: &str = "%a %b %-d %Y, %-I:%M %p";
    match zone.and_then(|z| z.parse::<chrono_tz::Tz>().ok()) {
        Some(tz) => at.with_timezone(&tz).format(FORMAT).to_string(),
        None => format!("{} UTC", at.format(FORMAT)),
    }
}

/// `date`'s local midnight to the next local midnight, in [`day_timezone`].
pub async fn day_window(
    pool: &PgPool,
    date: NaiveDate,
) -> crate::error::Result<(DateTime<Utc>, DateTime<Utc>)> {
    let tz = day_timezone(pool, date).await?;
    Ok(crate::api::day_summary::day_bounds(date, Some(&tz)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().to_utc()
    }

    /// An evening message is early-morning UTC; quoted raw, it moved to the
    /// next day.
    #[test]
    fn a_tool_time_is_on_the_owners_clock() {
        let sent = at("2026-10-14T03:12:00Z");
        assert_eq!(owner_rfc3339(sent, Some("America/Chicago")), "2026-10-13T22:12:00-05:00");
        assert_eq!(owner_label(sent, Some("America/Chicago")), "Tue Oct 13 2026, 10:12 PM");
        // Postgres and chrono read the offset form back as the same instant.
        assert_eq!(at(&owner_rfc3339(sent, Some("America/Chicago"))), sent);
    }

    /// Across a DST change each time carries its own offset.
    #[test]
    fn each_time_carries_its_own_offset() {
        assert!(owner_rfc3339(at("2026-11-02T15:00:00Z"), Some("America/Chicago")).ends_with("-06:00"));
        assert!(owner_rfc3339(at("2026-10-30T15:00:00Z"), Some("America/Chicago")).ends_with("-05:00"));
    }

    #[test]
    fn an_unknown_zone_stays_utc_and_says_so() {
        let sent = at("2026-10-14T03:12:00Z");
        assert_eq!(owner_rfc3339(sent, Some("Not/AZone")), "2026-10-14T03:12:00+00:00");
        assert_eq!(owner_label(sent, None), "Wed Oct 14 2026, 3:12 AM UTC");
    }
}
