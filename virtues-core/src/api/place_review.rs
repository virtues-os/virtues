//! "Help Virtues out": each place's title, the places the map can't name as
//! questions for the person, and their answers (`dayback/src/main.js:
//! 2113-2201`, `dayback/build.py:181-216` and `521-575`).
//!
//! A place's title is, in order: the name the person gave; Home or Work, as
//! the person set them; the map's clear match (`maps::naming`), unless the
//! person said it was wrong; else the city it lies in. A clear match is
//! accepted without asking and listed for review; every other place is a
//! question, the place with the most time first.
//!
//! Home and Work are never assumed. Where the time went only picks the place
//! to ask about first: "Is this home?" for the place with the most nights,
//! "Is this where you work?" for the place with the most time otherwise,
//! among places stayed at twice or more. A "no" is kept, and the next place
//! is asked about.
//!
//! An answer is written to the place's `wiki_places` row, the row "Name this
//! place" writes: a name and its category in their columns, "just passing
//! through" and a rejected match in its metadata. Answered once, a place is
//! never asked again. The map's match is worked out when a place is read and
//! never stored.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc, Weekday};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

use crate::error::{Error, Result};
use crate::maps::naming::{Naming, Tiles};

/// A summary longer than this is cut to it (`build.py:573`).
const SNIPPET_CHARS: usize = 104;
/// Slept at this long, a place is someone's home: the question is whose
/// (`build.py:579`, `resid`).
const OVERNIGHT_MS: i64 = 60 * 60_000;
/// The local small hours that make a stay a night there (`build.py:181-193`).
const SMALL_HOURS: (u32, u32) = (1, 5);

/// A place with stays, and where its time went.
struct Place {
    id: String,
    name: String,
    is_named: bool,
    passing_through: bool,
    rejected_match: Option<String>,
    /// The person said it's where they work.
    is_work: bool,
    /// The person said it isn't home, or isn't where they work.
    not_home: bool,
    not_work: bool,
    /// The time-weighted centre of its stays.
    latitude: f64,
    longitude: f64,
    tz: Tz,
    /// Its stays, as `(start, end)`.
    stays: Vec<(DateTime<Utc>, DateTime<Utc>)>,
    total_ms: i64,
    /// Time in stays that run through the local small hours.
    night_ms: i64,
}

impl Place {
    fn hours(&self) -> f64 {
        (self.total_ms as f64 / 360_000.0).round() / 10.0
    }
}

/// Every place with stays, each stay counted at the place it links to now
/// (the newest link, as the Timeline's day reads it).
async fn places(pool: &PgPool) -> Result<Vec<Place>> {
    let rows = sqlx::query(
        r#"SELECT p.id, p.name, COALESCE(p.is_named, false) AS is_named,
                  COALESCE(p.metadata->>'passing_through' = 'true', false) AS passing_through,
                  p.metadata->>'rejected_match' AS rejected_match,
                  COALESCE(p.metadata->>'is_work' = 'true', false) AS is_work,
                  COALESCE(p.metadata->>'not_home' = 'true', false) AS not_home,
                  COALESCE(p.metadata->>'not_work' = 'true', false) AS not_work,
                  v.latitude, v.longitude, v.started_at,
                  COALESCE(v.ended_at, v.started_at + make_interval(mins => COALESCE(v.duration_minutes, 0))) AS ended_at
           FROM data_location_visit v
           JOIN LATERAL (
               SELECT er.entity_id FROM wiki_refs er
               WHERE er.source_table = 'data_location_visit' AND er.source_id = v.id AND er.entity_type = 'place'
               ORDER BY er.created_at DESC LIMIT 1
           ) er ON TRUE
           JOIN wiki_places p ON p.id = er.entity_id
           WHERE v.deleted_at_source IS NULL AND NOT v.is_archived
           ORDER BY p.id, v.started_at"#,
    )
    .fetch_all(pool)
    .await?;
    let mut out: Vec<Place> = Vec::new();
    let mut sums: Vec<(f64, f64, f64)> = Vec::new();
    for r in &rows {
        let id: String = r.try_get("id")?;
        let (lat, lon): (f64, f64) = (r.try_get("latitude")?, r.try_get("longitude")?);
        let (s, e): (DateTime<Utc>, DateTime<Utc>) = (r.try_get("started_at")?, r.try_get("ended_at")?);
        if out.last().is_none_or(|p| p.id != id) {
            out.push(Place {
                id,
                name: r.try_get("name")?,
                is_named: r.try_get("is_named")?,
                passing_through: r.try_get("passing_through")?,
                rejected_match: r.try_get("rejected_match")?,
                is_work: r.try_get("is_work")?,
                not_home: r.try_get("not_home")?,
                not_work: r.try_get("not_work")?,
                latitude: lat,
                longitude: lon,
                tz: chrono_tz::UTC,
                stays: Vec::new(),
                total_ms: 0,
                night_ms: 0,
            });
            sums.push((0.0, 0.0, 0.0));
        }
        let (p, sum) = (out.last_mut().expect("pushed above"), sums.last_mut().expect("pushed above"));
        let w = (e - s).num_milliseconds().max(1) as f64;
        *sum = (sum.0 + lat * w, sum.1 + lon * w, sum.2 + w);
        p.stays.push((s, e));
    }
    for (p, (la, lo, w)) in out.iter_mut().zip(sums) {
        (p.latitude, p.longitude) = (la / w, lo / w);
        p.tz = crate::timezone::coords_to_tz(p.latitude, p.longitude)
            .and_then(|z| z.parse().ok())
            .unwrap_or(chrono_tz::UTC);
        for &(s, e) in &p.stays {
            let dur = (e - s).num_milliseconds().max(0);
            p.total_ms += dur;
            if covers_small_hours(s, e, p.tz) {
                p.night_ms += dur;
            }
        }
    }
    Ok(out)
}

fn local(t: DateTime<Utc>, tz: Tz) -> NaiveDateTime {
    t.with_timezone(&tz).naive_local()
}

/// Whether a stay runs through any local 01:00-05:00: the person slept there
/// (`build.py:181-193`), however late they arrived.
fn covers_small_hours(s: DateTime<Utc>, e: DateTime<Utc>, tz: Tz) -> bool {
    let mut day = local(s, tz).date();
    let last = local(e, tz).date();
    while day <= last {
        let at = |h: u32| day.and_hms_opt(h, 0, 0).and_then(|t| tz.from_local_datetime(&t).earliest());
        if let (Some(a), Some(b)) = (at(SMALL_HOURS.0), at(SMALL_HOURS.1)) {
            if s < b && e > a {
                return true;
            }
        }
        let Some(next) = day.succ_opt() else { break };
        day = next;
    }
    false
}

/// The places to ask "Is this home?" and "Is this where you work?" about,
/// as ids (`build.py:204-206`, as questions rather than answers): with no
/// Home set, the place with the most nights not already said no to; with no
/// Work set, the place with the most time otherwise, among places stayed at
/// twice or more and not said no to.
fn home_and_work_to_ask(places: &[Place], home: Option<&str>) -> (Option<String>, Option<String>) {
    let ask_home = match home {
        Some(_) => None,
        None => places.iter().filter(|p| p.night_ms > 0 && !p.not_home).max_by_key(|p| p.night_ms).map(|p| p.id.clone()),
    };
    let ask_work = if places.iter().any(|p| p.is_work) {
        None
    } else {
        places
            .iter()
            .filter(|p| Some(p.id.as_str()) != home && Some(&p.id) != ask_home.as_ref())
            .filter(|p| p.stays.len() >= 2 && !p.not_work)
            .max_by_key(|p| p.total_ms)
            .map(|p| p.id.clone())
    };
    (ask_home, ask_work)
}

async fn set_home(pool: &PgPool) -> Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, Option<String>>("SELECT home_place_id FROM app_user_profile LIMIT 1")
        .fetch_optional(pool)
        .await?
        .flatten())
}

/// A place's title, and where it came from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlaceTitle {
    pub title: String,
    /// `named` (the person's own), `home` and `work` (as the person set
    /// them), `map` (a clear match), `city`, or `none`.
    pub source: &'static str,
}

fn title(p: &Place, naming: &Naming, home: Option<&str>) -> PlaceTitle {
    let t = |title: &str, source| PlaceTitle { title: title.to_string(), source };
    if p.is_named {
        return t(&p.name, "named");
    }
    if home == Some(p.id.as_str()) {
        return t("Home", "home");
    }
    if p.is_work {
        return t("Work", "work");
    }
    match (&naming.name, &naming.city) {
        (Some(n), _) if naming.tier == "confident" && p.rejected_match.as_deref() != Some(n) => t(n, "map"),
        (_, Some(c)) => t(c, "city"),
        _ => t("Unnamed place", "none"),
    }
}

/// The titles of `ids`, by place id: what a stay at each place is called.
pub async fn titles(pool: &PgPool, ids: &[String]) -> Result<HashMap<String, PlaceTitle>> {
    let all = places(pool).await?;
    let home = set_home(pool).await?;
    let mut tiles = Tiles::default();
    let mut out = HashMap::new();
    for p in all.iter().filter(|p| ids.contains(&p.id)) {
        let naming = tiles.name(p.latitude, p.longitude).await;
        out.insert(p.id.clone(), title(p, &naming, home.as_deref()));
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
pub struct Review {
    /// Places the map can't name, the most time first.
    pub questions: Vec<Question>,
    /// Places named without asking (Home, Work, a clear match) or by the
    /// person: the list a wrong one is flagged from.
    pub confirmed: Vec<Confirmed>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Question {
    /// `home`: "Is this home?"; `work`: "Is this where you work?"; `place`:
    /// what the place was.
    pub ask: &'static str,
    pub id: String,
    pub latitude: f64,
    pub longitude: f64,
    pub hours: f64,
    pub stop_count: usize,
    /// Slept at: the question is whose home it was.
    pub overnight: bool,
    pub naming: Naming,
    /// The map's match the person said was wrong, left out of the choices.
    pub rejected_match: Option<String>,
    /// The place's title until it's answered: its city.
    pub title: String,
    /// When the person was there, in words: "the weekend of Jul 25-26,
    /// mostly overnight".
    pub recall: String,
    /// A line from a conversation there, to jog the memory.
    pub snippet: Option<String>,
    /// The day with the most time there: "See that full day".
    pub best_day: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Confirmed {
    pub id: String,
    pub title: String,
    /// `named`, `home`, `work` or `map`.
    pub source: &'static str,
    pub latitude: f64,
    pub longitude: f64,
    pub hours: f64,
    pub stop_count: usize,
}

/// Every place's standing: a question, or confirmed.
pub async fn review(pool: &PgPool) -> Result<Review> {
    let mut all = places(pool).await?;
    all.sort_by(|a, b| b.total_ms.cmp(&a.total_ms).then(a.id.cmp(&b.id)));
    let home = set_home(pool).await?;
    let (ask_home, ask_work) = home_and_work_to_ask(&all, home.as_deref());
    let mut tiles = Tiles::default();
    let (mut questions, mut confirmed) = (Vec::new(), Vec::new());
    for p in &all {
        let naming = tiles.name(p.latitude, p.longitude).await;
        let t = title(p, &naming, home.as_deref());
        let ask = if ask_home.as_ref() == Some(&p.id) {
            "home"
        } else if ask_work.as_ref() == Some(&p.id) {
            "work"
        } else {
            "place"
        };
        match t.source {
            "named" | "home" | "work" | "map" => confirmed.push(Confirmed {
                id: p.id.clone(),
                title: t.title,
                source: t.source,
                latitude: p.latitude,
                longitude: p.longitude,
                hours: p.hours(),
                stop_count: p.stays.len(),
            }),
            _ if p.passing_through && ask == "place" => {}
            _ => {
                let (recall, best_day) = recall_line(&p.stays, p.tz);
                questions.push(Question {
                    ask,
                    id: p.id.clone(),
                    latitude: p.latitude,
                    longitude: p.longitude,
                    hours: p.hours(),
                    stop_count: p.stays.len(),
                    overnight: p.night_ms > OVERNIGHT_MS,
                    naming,
                    rejected_match: p.rejected_match.clone(),
                    title: t.title,
                    recall,
                    snippet: snippet(pool, &p.stays).await?,
                    best_day,
                });
            }
        }
    }
    // Home first, then Work: the two answers every other title leans on.
    questions.sort_by_key(|q| match q.ask {
        "home" => 0,
        "work" => 1,
        _ => 2,
    });
    Ok(Review { questions, confirmed })
}

/// One line from a conversation begun at the place: the most confident of
/// two or more voices, else the most confident of any (`build.py:563-574`).
async fn snippet(pool: &PgPool, stays: &[(DateTime<Utc>, DateTime<Utc>)]) -> Result<Option<String>> {
    let (starts, ends): (Vec<_>, Vec<_>) = stays.iter().copied().unzip();
    let row = sqlx::query(
        "SELECT t.summary, t.title FROM data_communication_transcription t \
         WHERE t.deleted_at_source IS NULL AND NOT COALESCE(t.is_archived, false) \
           AND EXISTS (SELECT 1 FROM unnest($1::timestamptz[], $2::timestamptz[]) w(s, e) \
                       WHERE t.started_at >= w.s AND t.started_at < w.e) \
         ORDER BY (COALESCE(t.speaker_count, 0) >= 2) DESC, t.confidence DESC NULLS LAST LIMIT 1",
    )
    .bind(&starts)
    .bind(&ends)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| {
        let summary: Option<String> = r.try_get("summary").ok()?;
        let title: Option<String> = r.try_get("title").ok()?;
        let text = summary.filter(|s| !s.trim().is_empty()).or(title)?;
        Some(cut(&text.split_whitespace().collect::<Vec<_>>().join(" ")))
    }))
}

fn cut(s: &str) -> String {
    if s.chars().count() <= SNIPPET_CHARS {
        return s.to_string();
    }
    let head: String = s.chars().take(SNIPPET_CHARS).collect();
    format!("{}…", head.trim_end())
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

fn md(d: NaiveDate) -> String {
    format!("{} {}", MONTHS[d.month0() as usize], d.day())
}

fn dow(d: NaiveDate) -> &'static str {
    match d.weekday() {
        Weekday::Mon => "Mon",
        Weekday::Tue => "Tue",
        Weekday::Wed => "Wed",
        Weekday::Thu => "Thu",
        Weekday::Fri => "Fri",
        Weekday::Sat => "Sat",
        Weekday::Sun => "Sun",
    }
}

/// The stays at a place in words, and the day with the most time there
/// (`build.py:530-562`): "Thu, Jul 23 - in the afternoon", "the weekend of
/// Jul 25-26, mostly overnight", "5 days between Jul 21 and Aug 3, evenings".
fn recall_line(stays: &[(DateTime<Utc>, DateTime<Utc>)], tz: Tz) -> (String, Option<NaiveDate>) {
    let mut by_date: HashMap<NaiveDate, i64> = HashMap::new();
    let (mut night, mut total, mut weighted, mut weights) = (0, 0, 0.0, 0.0);
    for &(a, d) in stays {
        let dur = (d - a).num_milliseconds().max(0);
        total += dur;
        *by_date.entry(local(a, tz).date()).or_default() += dur;
        if covers_small_hours(a, d, tz) {
            night += dur;
        }
        let mid = local(a + (d - a) / 2, tz).hour();
        weighted += f64::from(mid) * dur as f64;
        weights += dur as f64;
    }
    let mut dates: Vec<NaiveDate> = by_date.keys().copied().collect();
    dates.sort();
    let (Some(&first), Some(&last)) = (dates.first(), dates.last()) else {
        return (String::new(), None);
    };
    let best_day = by_date.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))).map(|(d, _)| *d);
    let one = dates.len() == 1;
    let night_share = if total > 0 { night as f64 / total as f64 } else { 0.0 };
    let mean_hour = if weights > 0.0 { weighted / weights } else { 12.0 };
    let when = if night_share >= 0.5 {
        if one { "overnight".to_string() } else { "mostly overnight".to_string() }
    } else {
        let part = match mean_hour {
            h if h < 11.0 => "morning",
            h if h < 14.0 => "midday",
            h if h < 17.0 => "afternoon",
            h if h < 21.0 => "evening",
            _ => "night",
        };
        match (one, part) {
            (true, "midday") => "at midday".into(),
            (_, "night") => "at night".into(),
            (true, p) => format!("in the {p}"),
            (false, "midday") => "around midday".into(),
            (false, p) => format!("{p}s"),
        }
    };
    if one {
        return (format!("{}, {} - {}", dow(first), md(first), when), best_day);
    }
    let n = dates.len();
    let contiguous = (last - first).num_days() == n as i64 - 1;
    let weekend = dates.iter().any(|d| matches!(d.weekday(), Weekday::Sat | Weekday::Sun));
    let range = if first.month() == last.month() {
        format!("{}-{}", md(first), last.day())
    } else {
        format!("{} - {}", md(first), md(last))
    };
    let base = if contiguous && n <= 4 && weekend {
        format!("the weekend of {range}")
    } else if contiguous {
        range
    } else {
        format!("{n} days between {} and {}", md(first), md(last))
    };
    (format!("{base}, {when}"), best_day)
}

/// One answer to a place question.
#[derive(Debug, Clone, Deserialize)]
pub struct Answer {
    /// `name`: what the place was (and its category); `passing`: just passing
    /// through; `not_this`: the map's match is wrong; `home` / `not_home` and
    /// `work` / `not_work`: whether it's home, or where the person works;
    /// `reopen`: forget every answer about it and ask again.
    pub kind: String,
    pub name: Option<String>,
    pub category: Option<String>,
}

/// The resolver's name for a place nobody has named
/// (`entity_resolution/places.rs`, `reverse_geocode_stub`).
fn stub(lat: f64, lon: f64) -> String {
    format!("Location {:.4}, {:.4}", lat, lon)
}

/// Writes an answer to the place's `wiki_places` row.
pub async fn answer(pool: &PgPool, id: &str, a: &Answer) -> Result<()> {
    let name = a.name.as_deref().map(str::trim).filter(|n| !n.is_empty());
    if !matches!(a.kind.as_str(), "name" | "passing" | "not_this" | "home" | "not_home" | "work" | "not_work" | "reopen") {
        return Err(Error::InvalidInput(format!("unknown answer `{}`", a.kind)));
    }
    if matches!(a.kind.as_str(), "name" | "not_this") && name.is_none() {
        return Err(Error::InvalidInput(format!("a `{}` answer needs a name", a.kind)));
    }
    let q = match a.kind.as_str() {
        "name" => sqlx::query(
            "UPDATE wiki_places SET name = $2, category = COALESCE($3, category), is_named = true, \
             metadata = (COALESCE(metadata, '{}'::jsonb) - 'passing_through' - 'rejected_match') || '{\"source\": \"user\"}'::jsonb, \
             updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .bind(name)
        .bind(a.category.as_deref().filter(|c| !c.trim().is_empty())),
        "passing" => sqlx::query(
            "UPDATE wiki_places SET metadata = COALESCE(metadata, '{}'::jsonb) || '{\"passing_through\": true}'::jsonb, \
             updated_at = NOW() WHERE id = $1",
        )
        .bind(id),
        "home" => {
            // Home is the profile's, the one place every reader of Home shares.
            let mut tx = pool.begin().await?;
            let cleared = sqlx::query(
                "UPDATE wiki_places SET metadata = COALESCE(metadata, '{}'::jsonb) - 'not_home', updated_at = NOW() WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
            if cleared.rows_affected() == 0 {
                return Err(Error::NotFound(format!("No place {id}")));
            }
            if sqlx::query("UPDATE app_user_profile SET home_place_id = $1").bind(id).execute(&mut *tx).await?.rows_affected() == 0 {
                return Err(Error::NotFound("No profile to set Home on".into()));
            }
            tx.commit().await?;
            return Ok(());
        }
        "not_home" | "work" | "not_work" => sqlx::query(
            "UPDATE wiki_places SET metadata = COALESCE(metadata, '{}'::jsonb) || jsonb_build_object($2::text, true), \
             updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .bind(match a.kind.as_str() {
            "work" => "is_work",
            k => k,
        }),
        "not_this" => sqlx::query(
            "UPDATE wiki_places SET metadata = COALESCE(metadata, '{}'::jsonb) || jsonb_build_object('rejected_match', $2::text), \
             updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .bind(name),
        _ => {
            let (lat, lon): (f64, f64) = sqlx::query_as("SELECT latitude, longitude FROM wiki_places WHERE id = $1")
                .bind(id)
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| Error::NotFound(format!("No place {id}")))?;
            sqlx::query("UPDATE app_user_profile SET home_place_id = NULL WHERE home_place_id = $1")
                .bind(id)
                .execute(pool)
                .await?;
            sqlx::query(
                "UPDATE wiki_places SET name = CASE WHEN is_named THEN $2 ELSE name END, is_named = false, \
                 metadata = COALESCE(metadata, '{}'::jsonb) - 'passing_through' - 'rejected_match' - 'source' \
                     - 'is_work' - 'not_home' - 'not_work', \
                 updated_at = NOW() WHERE id = $1",
            )
            .bind(id)
            .bind(stub(lat, lon))
        }
    };
    if q.execute(pool).await?.rows_affected() == 0 {
        return Err(Error::NotFound(format!("No place {id}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chicago() -> Tz {
        "America/Chicago".parse().unwrap()
    }
    fn at(m: u32, d: u32, h: u32) -> DateTime<Utc> {
        chicago().with_ymd_and_hms(2026, m, d, h, 0, 0).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn one_afternoon_reads_as_its_day_and_part() {
        let (line, best) = recall_line(&[(at(7, 23, 14), at(7, 23, 16))], chicago());
        assert_eq!(line, "Thu, Jul 23 - in the afternoon");
        assert_eq!(best, NaiveDate::from_ymd_opt(2026, 7, 23));
    }

    #[test]
    fn a_weekend_of_nights_reads_as_the_weekend_mostly_overnight() {
        let stays = [(at(7, 24, 22), at(7, 25, 9)), (at(7, 25, 22), at(7, 26, 9))];
        let (line, best) = recall_line(&stays, chicago());
        assert_eq!(line, "the weekend of Jul 24-25, mostly overnight");
        assert_eq!(best, NaiveDate::from_ymd_opt(2026, 7, 24), "a tie goes to the earlier day");
    }

    #[test]
    fn scattered_days_read_as_a_count() {
        let stays = [(at(7, 21, 18), at(7, 21, 20)), (at(7, 28, 18), at(7, 28, 19)), (at(8, 3, 17), at(8, 3, 20))];
        let (line, best) = recall_line(&stays, chicago());
        assert_eq!(line, "3 days between Jul 21 and Aug 3, evenings");
        assert_eq!(best, NaiveDate::from_ymd_opt(2026, 8, 3));
        assert_eq!(recall_line(&[], chicago()), (String::new(), None));
    }

    #[test]
    fn a_night_is_any_stay_through_the_small_hours() {
        assert!(covers_small_hours(at(7, 24, 22), at(7, 25, 7), chicago()), "evening to morning");
        assert!(covers_small_hours(at(7, 25, 3), at(7, 25, 9), chicago()), "arrived at 3 AM");
        assert!(!covers_small_hours(at(7, 25, 6), at(7, 25, 23), chicago()), "a whole day, no night");
    }

    fn place(id: &str, stays: usize, total_h: i64, night_h: i64) -> Place {
        Place {
            id: id.into(),
            name: format!("Location {id}"),
            is_named: false,
            passing_through: false,
            rejected_match: None,
            is_work: false,
            not_home: false,
            not_work: false,
            latitude: 0.0,
            longitude: 0.0,
            tz: chrono_tz::UTC,
            stays: vec![(at(7, 1, 9), at(7, 1, 10)); stays],
            total_ms: total_h * 3_600_000,
            night_ms: night_h * 3_600_000,
        }
    }

    #[test]
    fn home_and_work_are_asked_about_never_assumed() {
        let ps = [place("bed", 9, 120, 100), place("desk", 6, 60, 0), place("cafe", 1, 70, 0)];
        assert_eq!(home_and_work_to_ask(&ps, None), (Some("bed".into()), Some("desk".into())), "one long stay is no Work");
        assert_eq!(home_and_work_to_ask(&ps, Some("bed")), (None, Some("desk".into())), "Home set: only Work is asked");
        let mut no = ps;
        no[0].not_home = true;
        no[1].not_work = true;
        assert_eq!(home_and_work_to_ask(&no, None), (None, Some("bed".into())), "a no moves on to the next place");
        no[2].is_work = true;
        assert_eq!(home_and_work_to_ask(&no, None).1, None, "Work set: not asked");
    }

    #[test]
    fn a_title_is_the_persons_then_home_or_work_then_the_map_then_the_city() {
        let naming = |tier: &'static str, name: Option<&str>, city: Option<&str>| Naming {
            tier,
            name: name.map(str::to_string),
            city: city.map(str::to_string),
            ..Naming::default()
        };
        let mut p = place("p", 2, 3, 0);
        let clear = naming("confident", Some("Corner Cafe"), Some("Riverton"));
        assert_eq!(title(&p, &clear, None).title, "Corner Cafe");
        assert_eq!(title(&p, &naming("ambiguous", None, Some("Riverton")), None).source, "city");
        assert_eq!(title(&p, &naming("none", None, None), None).title, "Unnamed place");
        assert_eq!(title(&p, &clear, Some("p")).title, "Home", "the person's Home beats the map");
        p.is_work = true;
        assert_eq!(title(&p, &clear, None).title, "Work");
        p.is_work = false;
        p.rejected_match = Some("Corner Cafe".into());
        assert_eq!(title(&p, &clear, None).title, "Riverton", "a rejected match steps aside");
        p.is_named = true;
        p.name = "The Farm".into();
        assert_eq!(title(&p, &clear, Some("p")).title, "The Farm", "the person's name beats all");
    }

    #[test]
    fn a_long_snippet_is_cut_on_a_character() {
        assert_eq!(cut("short"), "short");
        let c = cut(&"word ".repeat(40));
        assert!(c.ends_with('…') && c.chars().count() <= SNIPPET_CHARS + 1);
    }

    /// A place in the open sea (no map, so no match) with one stay.
    async fn sea_place(pool: &PgPool) -> String {
        for q in [
            "INSERT INTO wiki_places (id, name, latitude, longitude, radius_m) VALUES ('place_sea', 'Location 0.5000, -30.5000', 0.5, -30.5, 100)",
            "INSERT INTO data_location_visit (id, latitude, longitude, started_at, ended_at, source_stream_id, source_table, source_provider) \
             VALUES ('visit_sea', 0.5, -30.5, '2026-09-12 14:00Z', '2026-09-12 15:30Z', 's', 'data_location_visit', 'test')",
            "INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id) \
             VALUES ('ref_sea', 'place', 'place_sea', 'data_location_visit', 'visit_sea')",
        ] {
            sqlx::query(q).execute(pool).await.unwrap();
        }
        "place_sea".into()
    }
    async fn row(pool: &PgPool, id: &str) -> (String, bool, Option<String>, Option<String>, Option<String>) {
        sqlx::query_as(
            "SELECT name, is_named, category, metadata->>'source', metadata->>'passing_through' FROM wiki_places WHERE id = $1",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
    }
    fn named(name: &str) -> Answer {
        Answer { kind: "name".into(), name: Some(name.into()), category: Some("dining".into()) }
    }

    #[sqlx::test]
    async fn a_name_is_written_to_the_place_and_never_asked_again(pool: PgPool) {
        let id = sea_place(&pool).await;
        let r = review(&pool).await.unwrap();
        let q = r.questions.iter().find(|q| q.id == id).expect("asked");
        assert_eq!((q.hours, q.stop_count, q.overnight), (1.5, 1, false));
        assert_eq!(q.best_day, NaiveDate::from_ymd_opt(2026, 9, 12));
        answer(&pool, &id, &named("  The Boathouse ")).await.unwrap();
        let (name, is_named, category, source, _) = row(&pool, &id).await;
        assert_eq!((name.as_str(), is_named, category.as_deref(), source.as_deref()), ("The Boathouse", true, Some("dining"), Some("user")));
        let r = review(&pool).await.unwrap();
        assert!(r.questions.iter().all(|q| q.id != id));
        assert!(r.confirmed.iter().any(|c| c.id == id && c.source == "named" && c.title == "The Boathouse"));
        assert_eq!(titles(&pool, &[id.clone()]).await.unwrap()[&id].title, "The Boathouse");
    }

    #[sqlx::test]
    async fn passing_through_is_kept_and_reopening_asks_again(pool: PgPool) {
        let id = sea_place(&pool).await;
        answer(&pool, &id, &Answer { kind: "passing".into(), name: None, category: None }).await.unwrap();
        assert_eq!(row(&pool, &id).await.4.as_deref(), Some("true"));
        assert!(review(&pool).await.unwrap().questions.iter().all(|q| q.id != id));

        answer(&pool, &id, &named("Somewhere")).await.unwrap();
        answer(&pool, &id, &Answer { kind: "reopen".into(), name: None, category: None }).await.unwrap();
        let (name, is_named, _, source, passing) = row(&pool, &id).await;
        assert_eq!((name.as_str(), is_named, source, passing), ("Location 0.5000, -30.5000", false, None, None));
        assert!(review(&pool).await.unwrap().questions.iter().any(|q| q.id == id));
    }

    #[sqlx::test]
    async fn home_is_asked_until_the_person_answers_and_a_no_is_kept(pool: PgPool) {
        let id = sea_place(&pool).await;
        sqlx::query("UPDATE data_location_visit SET started_at = '2026-09-12 02:00Z', ended_at = '2026-09-12 09:00Z'")
            .execute(&pool)
            .await
            .unwrap();
        let asked = |r: &Review| r.questions.iter().find(|q| q.id == id).map(|q| q.ask);
        assert_eq!(asked(&review(&pool).await.unwrap()), Some("home"), "the most nights: asked, not assumed");
        assert_eq!(titles(&pool, &[id.clone()]).await.unwrap()[&id].title, "Unnamed place", "no Home until the person says so");

        answer(&pool, &id, &Answer { kind: "not_home".into(), name: None, category: None }).await.unwrap();
        assert_eq!(asked(&review(&pool).await.unwrap()), Some("place"), "a no is kept: asked what it was instead");

        answer(&pool, &id, &Answer { kind: "home".into(), name: None, category: None }).await.unwrap();
        let r = review(&pool).await.unwrap();
        assert!(r.confirmed.iter().any(|c| c.id == id && c.source == "home" && c.title == "Home"));
        assert_eq!(titles(&pool, &[id.clone()]).await.unwrap()[&id].title, "Home");

        answer(&pool, &id, &Answer { kind: "reopen".into(), name: None, category: None }).await.unwrap();
        assert_eq!(asked(&review(&pool).await.unwrap()), Some("home"), "reopened: Home is asked again");
    }

    #[sqlx::test]
    async fn a_wrong_match_is_kept_and_an_answer_needs_a_name(pool: PgPool) {
        let id = sea_place(&pool).await;
        answer(&pool, &id, &Answer { kind: "not_this".into(), name: Some("Riverside".into()), category: None }).await.unwrap();
        let rejected: Option<String> = sqlx::query_scalar("SELECT metadata->>'rejected_match' FROM wiki_places WHERE id = $1")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rejected.as_deref(), Some("Riverside"));
        assert!(answer(&pool, &id, &Answer { kind: "name".into(), name: Some("  ".into()), category: None }).await.is_err());
        assert!(answer(&pool, &id, &Answer { kind: "shrug".into(), name: None, category: None }).await.is_err());
        assert!(answer(&pool, "place_none", &named("X")).await.is_err());
    }
}
