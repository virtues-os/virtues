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
//! Home and Work are never assumed, and asked about once. Where the time
//! went picks the place to ask about: "Is this home?" for the place with the
//! most nights, "Is this where you work?" for the place with the most time
//! otherwise, among places stayed at twice or more (`build.py:204-206`). On
//! "no" the person points to the right place on the map instead. Neither is
//! asked again unless a different place has led for most of the last four
//! weeks: then "Is this your home now?".
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
/// A tap on the map this near a place with stays is that place.
const TAP_M: f64 = 150.0;
/// A place has taken over Home or Work when it leads this many of the last
/// `RECENT_WEEKS` weeks.
const RECENT_WEEKS: i64 = 4;
const LEAD_WEEKS: usize = 3;

/// A place with stays, and where its time went.
#[derive(Clone)]
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

/// The place that led most of the last four weeks by `score` (time in that
/// week's stays, as the caller counts it), among `eligible` places.
fn taken_over<'a>(
    places: &'a [Place],
    now: DateTime<Utc>,
    score: impl Fn(&Place, DateTime<Utc>, DateTime<Utc>) -> i64,
    eligible: impl Fn(&Place) -> bool,
) -> Option<&'a Place> {
    let mut leads: HashMap<&str, usize> = HashMap::new();
    for k in 0..RECENT_WEEKS {
        let (a, b) = (now - chrono::Duration::weeks(k + 1), now - chrono::Duration::weeks(k));
        let lead = places
            .iter()
            .filter(|p| eligible(p))
            .map(|p| (p, score(p, a, b)))
            .filter(|(_, t)| *t > 0)
            .max_by_key(|(p, t)| (*t, std::cmp::Reverse(p.id.as_str())));
        if let Some((p, _)) = lead {
            *leads.entry(p.id.as_str()).or_default() += 1;
        }
    }
    let (id, n) = leads.into_iter().max_by_key(|(id, n)| (*n, std::cmp::Reverse(*id)))?;
    (n >= LEAD_WEEKS).then(|| places.iter().find(|p| p.id == id)).flatten()
}

/// Time in the stays begun in `[a, b)`, all of them or only the nights.
fn time_in(p: &Place, a: DateTime<Utc>, b: DateTime<Utc>, nights_only: bool) -> i64 {
    p.stays
        .iter()
        .filter(|(s, e)| *s >= a && *s < b && (!nights_only || covers_small_hours(*s, *e, p.tz)))
        .map(|(s, e)| (*e - *s).num_milliseconds().max(0))
        .sum()
}

/// What to ask about Home and Work, as `(place id, changed)`, `changed` when
/// the person already set it elsewhere (`build.py:204-206`, as questions).
/// Unset, the question is about the place with the most nights (Home) or the
/// most time otherwise among places stayed at twice or more (Work), unless
/// the person said no to it; then nothing, until a different place leads
/// most of the last four weeks. Set, only such a place is asked about.
fn to_ask(places: &[Place], home: Option<&str>, now: DateTime<Utc>) -> (Option<(String, bool)>, Option<(String, bool)>) {
    let pick = |lead: Option<&Place>, recent: Option<&Place>, set: Option<&str>, no: fn(&Place) -> bool| -> Option<(String, bool)> {
        match set {
            None => match lead {
                Some(l) if !no(l) => Some((l.id.clone(), false)),
                _ => recent.filter(|r| !no(r) && Some(r.id.as_str()) != lead.map(|l| l.id.as_str())).map(|r| (r.id.clone(), false)),
            },
            Some(h) => recent.filter(|r| r.id != h && !no(r)).map(|r| (r.id.clone(), true)),
        }
    };
    let lead_home = places.iter().filter(|p| p.night_ms > 0).max_by_key(|p| (p.night_ms, std::cmp::Reverse(p.id.as_str())));
    let recent_home = taken_over(places, now, |p, a, b| time_in(p, a, b, true), |_| true);
    let ask_home = pick(lead_home, recent_home, home, |p| p.not_home);

    let work = places.iter().find(|p| p.is_work).map(|p| p.id.as_str());
    // Never the place slept at most, set as Home or not, as the prototype
    // leaves Home out of Work.
    let slept = lead_home.map(|p| p.id.as_str());
    let eligible = |p: &Place| {
        p.stays.len() >= 2
            && Some(p.id.as_str()) != home
            && Some(p.id.as_str()) != slept
            && Some(&p.id) != ask_home.as_ref().map(|(id, _)| id)
    };
    let lead_work = places.iter().filter(|p| eligible(p)).max_by_key(|p| (p.total_ms, std::cmp::Reverse(p.id.as_str())));
    let recent_work = taken_over(places, now, |p, a, b| time_in(p, a, b, false), eligible);
    let ask_work = pick(lead_work, recent_work, work, |p| p.not_work);
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
    /// For `home` and `work`: the person set it elsewhere, and this place has
    /// led since: "Is this your home now?".
    pub changed: bool,
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
    /// A typical week there, from the first stay to the last (a week at the
    /// least): hours, and nights.
    pub hours_a_week: f64,
    pub nights_a_week: f64,
}

/// Hours and nights a week at a place, over the weeks from its first stay to
/// its last, a week at the least.
fn per_week(p: &Place) -> (f64, f64) {
    let (Some(first), Some(last)) = (p.stays.iter().map(|s| s.0).min(), p.stays.iter().map(|s| s.1).max()) else {
        return (0.0, 0.0);
    };
    // Whole days, first to last, both counted: three weeks of daily visits
    // are three weeks, not the 20 days and 3 hours between first and last.
    let days = (local(last, p.tz).date() - local(first, p.tz).date()).num_days() + 1;
    let weeks = (days as f64 / 7.0).max(1.0);
    let nights: std::collections::HashSet<NaiveDate> = p
        .stays
        .iter()
        .filter(|(s, e)| covers_small_hours(*s, *e, p.tz))
        .map(|(_, e)| local(*e, p.tz).date())
        .collect();
    let round = |v: f64| (v * 10.0).round() / 10.0;
    (round(p.total_ms as f64 / 3_600_000.0 / weeks), round(nights.len() as f64 / weeks))
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
    let (ask_home, ask_work) = to_ask(&all, home.as_deref(), Utc::now());
    let mut tiles = Tiles::default();
    let (mut questions, mut confirmed) = (Vec::new(), Vec::new());
    for p in &all {
        let naming = tiles.name(p.latitude, p.longitude).await;
        let t = title(p, &naming, home.as_deref());
        let (ask, changed) = match (&ask_home, &ask_work) {
            (Some((h, c)), _) if *h == p.id => ("home", *c),
            (_, Some((w, c))) if *w == p.id => ("work", *c),
            _ => ("place", false),
        };
        match t.source {
            "named" | "home" | "work" | "map" if ask == "place" => confirmed.push(Confirmed {
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
                let (hours_a_week, nights_a_week) = per_week(p);
                questions.push(Question {
                    ask,
                    changed,
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
                    hours_a_week,
                    nights_a_week,
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
    /// `home_here` / `work_here`: not this place, but the one at `lat`,
    /// `lon`, tapped on the map; `reopen`: forget every answer about it and
    /// ask again.
    pub kind: String,
    pub name: Option<String>,
    pub category: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

/// The place an answer set Home or Work on, when it wasn't the one asked
/// about.
#[derive(Debug, Clone, Serialize)]
pub struct Answered {
    pub place_id: Option<String>,
}

/// The place at a tap on the map: the nearest place within 150 m, else a new
/// one there, named as the resolver names a place nobody has named.
async fn place_at(pool: &PgPool, lat: f64, lon: f64) -> Result<String> {
    let near: Vec<(String, f64, f64)> = sqlx::query_as("SELECT id, latitude, longitude FROM wiki_places").fetch_all(pool).await?;
    let nearest = near
        .into_iter()
        .map(|(id, la, lo)| (crate::geo::haversine_distance(lat, lon, la, lo), id))
        .filter(|(d, _)| *d <= TAP_M)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, id)) = nearest {
        return Ok(id);
    }
    let id = crate::ids::generate_id(crate::ids::WIKI_PLACE_PREFIX, &["tapped", &format!("{lat:.5},{lon:.5}")]);
    sqlx::query("INSERT INTO wiki_places (id, name, latitude, longitude, radius_m) VALUES ($1, $2, $3, $4, 100.0) ON CONFLICT (id) DO NOTHING")
        .bind(&id)
        .bind(stub(lat, lon))
        .bind(lat)
        .bind(lon)
        .execute(pool)
        .await?;
    Ok(id)
}

/// The resolver's name for a place nobody has named
/// (`entity_resolution/places.rs`, `reverse_geocode_stub`).
fn stub(lat: f64, lon: f64) -> String {
    format!("Location {:.4}, {:.4}", lat, lon)
}

/// Writes an answer to the place's `wiki_places` row.
pub async fn answer(pool: &PgPool, id: &str, a: &Answer) -> Result<Answered> {
    let name = a.name.as_deref().map(str::trim).filter(|n| !n.is_empty());
    if !matches!(
        a.kind.as_str(),
        "name" | "passing" | "not_this" | "home" | "not_home" | "work" | "not_work" | "home_here" | "work_here" | "reopen"
    ) {
        return Err(Error::InvalidInput(format!("unknown answer `{}`", a.kind)));
    }
    if matches!(a.kind.as_str(), "name" | "not_this") && name.is_none() {
        return Err(Error::InvalidInput(format!("a `{}` answer needs a name", a.kind)));
    }
    if let ("home_here" | "work_here", Some(lat), Some(lon)) = (a.kind.as_str(), a.lat, a.lon) {
        // Not this place: it's the one tapped on the map.
        let role = &a.kind[..4];
        let no = Answer { kind: format!("not_{role}"), name: None, category: None, lat: None, lon: None };
        Box::pin(answer(pool, id, &no)).await?;
        let there = place_at(pool, lat, lon).await?;
        let yes = Answer { kind: role.to_string(), name: None, category: None, lat: None, lon: None };
        Box::pin(answer(pool, &there, &yes)).await?;
        return Ok(Answered { place_id: Some(there) });
    }
    if matches!(a.kind.as_str(), "home_here" | "work_here") {
        return Err(Error::InvalidInput(format!("a `{}` answer needs where on the map", a.kind)));
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
            return Ok(Answered { place_id: None });
        }
        "work" => {
            // One place is where the person works: the new one replaces it.
            let mut tx = pool.begin().await?;
            sqlx::query("UPDATE wiki_places SET metadata = metadata - 'is_work', updated_at = NOW() WHERE metadata ? 'is_work' AND id <> $1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            let set = sqlx::query(
                "UPDATE wiki_places SET metadata = (COALESCE(metadata, '{}'::jsonb) - 'not_work') || '{\"is_work\": true}'::jsonb, \
                 updated_at = NOW() WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
            if set.rows_affected() == 0 {
                return Err(Error::NotFound(format!("No place {id}")));
            }
            tx.commit().await?;
            return Ok(Answered { place_id: None });
        }
        "not_home" | "not_work" => sqlx::query(
            "UPDATE wiki_places SET metadata = COALESCE(metadata, '{}'::jsonb) || jsonb_build_object($2::text, true), \
             updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .bind(a.kind.as_str()),
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
    Ok(Answered { place_id: None })
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

    /// A place with a night there each of the last `nights` days before
    /// `now`, and a working day each of the last `days` days.
    fn lived(id: &str, nights: i64, days: i64, now: DateTime<Utc>) -> Place {
        let mut p = place(id, 0, 0, 0);
        p.stays.clear();
        for d in 1..=nights {
            let evening = now - chrono::Duration::days(d) - chrono::Duration::hours(10);
            p.stays.push((evening, evening + chrono::Duration::hours(10)));
        }
        for d in 1..=days {
            let morning = now - chrono::Duration::days(d) - chrono::Duration::hours(4);
            p.stays.push((morning, morning + chrono::Duration::hours(3)));
        }
        p.tz = chrono_tz::UTC;
        for &(a, b) in &p.stays {
            let dur = (b - a).num_milliseconds();
            p.total_ms += dur;
            if covers_small_hours(a, b, p.tz) {
                p.night_ms += dur;
            }
        }
        p
    }

    #[test]
    fn home_and_work_are_asked_once_never_assumed() {
        let now = chrono_tz::UTC.with_ymd_and_hms(2026, 10, 7, 12, 0, 0).unwrap().with_timezone(&Utc);
        // "now" is noon: a night runs 2 AM to noon, a working day 8 to 11 AM.
        let ps = [lived("bed", 60, 0, now), lived("desk", 0, 40, now), lived("cafe", 0, 1, now)];
        assert_eq!(to_ask(&ps, None, now), (Some(("bed".into(), false)), Some(("desk".into(), false))), "one visit is no Work");
        assert_eq!(to_ask(&ps, Some("bed"), now).0, None, "Home set: not asked again");
        let mut no = ps.clone();
        no[0].not_home = true;
        assert_eq!(to_ask(&no, None, now).0, None, "a no is final: the next place isn't asked");
        no[1].is_work = true;
        assert_eq!(to_ask(&no, None, now).1, None, "Work set: not asked");
    }

    #[test]
    fn a_place_that_takes_over_is_asked_about_again() {
        let now = chrono_tz::UTC.with_ymd_and_hms(2026, 10, 7, 12, 0, 0).unwrap().with_timezone(&Utc);
        // The old home has more nights in all, but the new one has had them
        // for the last four weeks.
        let mut old = lived("old", 0, 0, now);
        let start = now - chrono::Duration::days(300);
        for d in 0..200 {
            let e = start + chrono::Duration::days(d);
            old.stays.push((e, e + chrono::Duration::hours(10)));
            old.total_ms += 36_000_000;
            old.night_ms += 36_000_000;
        }
        let new = lived("new", 28, 0, now);
        assert_eq!(to_ask(&[old.clone(), new.clone()], Some("old"), now).0, Some(("new".into(), true)), "Is this your home now?");
        let mut said_no = new;
        said_no.not_home = true;
        assert_eq!(to_ask(&[old, said_no], Some("old"), now).0, None);
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
    fn a_week_there_reads_in_hours_and_nights() {
        let now = chrono_tz::UTC.with_ymd_and_hms(2026, 10, 7, 12, 0, 0).unwrap().with_timezone(&Utc);
        // Three weeks of a 3-hour morning every day, and ten nights.
        let work = lived("desk", 0, 21, now);
        assert_eq!(per_week(&work), (21.0, 0.0), "21 hours a week, no nights");
        let home = lived("bed", 10, 0, now);
        assert_eq!(per_week(&home).1, 7.0, "10 nights over 10 days: every night");
        assert_eq!(per_week(&lived("one", 1, 0, now)), (10.0, 1.0), "less than a week counts as one");
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
        Answer { kind: "name".into(), name: Some(name.into()), category: Some("dining".into()), lat: None, lon: None }
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
        answer(&pool, &id, &Answer { kind: "passing".into(), name: None, category: None, lat: None, lon: None }).await.unwrap();
        assert_eq!(row(&pool, &id).await.4.as_deref(), Some("true"));
        assert!(review(&pool).await.unwrap().questions.iter().all(|q| q.id != id));

        answer(&pool, &id, &named("Somewhere")).await.unwrap();
        answer(&pool, &id, &Answer { kind: "reopen".into(), name: None, category: None, lat: None, lon: None }).await.unwrap();
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

        answer(&pool, &id, &Answer { kind: "not_home".into(), name: None, category: None, lat: None, lon: None }).await.unwrap();
        assert_eq!(asked(&review(&pool).await.unwrap()), Some("place"), "a no is kept: asked what it was instead");

        answer(&pool, &id, &Answer { kind: "home".into(), name: None, category: None, lat: None, lon: None }).await.unwrap();
        let r = review(&pool).await.unwrap();
        assert!(r.confirmed.iter().any(|c| c.id == id && c.source == "home" && c.title == "Home"));
        assert_eq!(titles(&pool, &[id.clone()]).await.unwrap()[&id].title, "Home");

        answer(&pool, &id, &Answer { kind: "reopen".into(), name: None, category: None, lat: None, lon: None }).await.unwrap();
        assert_eq!(asked(&review(&pool).await.unwrap()), Some("home"), "reopened: Home is asked again");
    }

    #[sqlx::test]
    async fn no_and_a_tap_on_the_map_sets_home_there(pool: PgPool) {
        let id = sea_place(&pool).await;
        let tap = |kind: &str, at: Option<(f64, f64)>| Answer {
            kind: kind.into(),
            name: None,
            category: None,
            lat: at.map(|a| a.0),
            lon: at.map(|a| a.1),
        };
        assert!(answer(&pool, &id, &tap("home_here", None)).await.is_err(), "a tap needs where");
        let set = answer(&pool, &id, &tap("home_here", Some((0.0, -30.0)))).await.unwrap();
        let there = set.place_id.expect("a place at the tap");
        assert_ne!(there, id, "78 km from the sea place: a new place");
        assert_eq!(set_home(&pool).await.unwrap().as_deref(), Some(there.as_str()));
        let r = review(&pool).await.unwrap();
        assert!(r.questions.iter().find(|q| q.id == id).is_none_or(|q| q.ask == "place"), "said no to: never Home again");
        // A tap near a place with stays is that place.
        let again = answer(&pool, &there, &tap("work_here", Some((0.5005, -30.5)))).await.unwrap();
        assert_eq!(again.place_id.as_deref(), Some(id.as_str()), "55 m from it");
    }

    #[sqlx::test]
    async fn a_wrong_match_is_kept_and_an_answer_needs_a_name(pool: PgPool) {
        let id = sea_place(&pool).await;
        answer(&pool, &id, &Answer { kind: "not_this".into(), name: Some("Riverside".into()), category: None, lat: None, lon: None }).await.unwrap();
        let rejected: Option<String> = sqlx::query_scalar("SELECT metadata->>'rejected_match' FROM wiki_places WHERE id = $1")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rejected.as_deref(), Some("Riverside"));
        assert!(answer(&pool, &id, &Answer { kind: "name".into(), name: Some("  ".into()), category: None, lat: None, lon: None }).await.is_err());
        assert!(answer(&pool, &id, &Answer { kind: "shrug".into(), name: None, category: None, lat: None, lon: None }).await.is_err());
        assert!(answer(&pool, "place_none", &named("X")).await.is_err());
    }
}
