//! The chat sky: the dots above the composer on an empty new chat.
//!
//! Each dot is a moment of one local day: a stay at a place, a calendar
//! entry, a workout, or the night the day woke from. Today's `wiki_events`
//! are only cut after the day ends (`day_summary::segment_day_events`), so a
//! day is assembled here from what is recorded live: the visits the entity
//! resolver writes every 15 minutes, the calendar, workouts and sleep.
//!
//! Labels are written by code from place titles, calendar titles and workout
//! types. Nothing here calls a model. A calendar entry that a stay mostly
//! covers becomes one moment ("Lunch with David Okafor, Café Rosa"), and so
//! does a workout at a place.
//!
//! Positions are hours from the day's local midnight in its zone, so the
//! client never converts time zones; the night a day woke from starts at a
//! negative hour.
//!
//! A moment is mentioned in the composer by its record (`url`), and carries
//! up to four `links` to mention instead: its place, the people on its
//! calendar entry, and the largest purchase made during it.
//!
//! A moment at a place also carries how unusual that place is for its hour
//! and weekday (`novelty`), which the sky draws as height: routine sits low,
//! the out of the ordinary rises. It is the weighted share of your past
//! visits at about that time that were at that place, with the recency and
//! time-of-week weighting `dayline::novelty` uses.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use futures::{StreamExt, TryStreamExt};
use serde::Serialize;
use sqlx::{PgPool, Row};

use super::place_review::PlaceTitle;
use super::timeline::CalendarEvent;
use crate::dayline::sleep::Night;
use crate::error::{Error, Result};

/// A stay shorter than this, once the night is cut off it, is not drawn.
const MIN_STAY_MINUTES: i64 = 10;
/// Two stays at one place this close together are one stay.
const SAME_PLACE_GAP_MINUTES: i64 = 20;
/// How many calendar items still to come the sky shows.
const PLANNED_MAX: usize = 3;
/// A day with more moments than this drops its shortest plain stays.
const MOMENTS_MAX: usize = 28;
/// How far back the place routine reads.
const ROUTINE_LOOKBACK_DAYS: i64 = 60;
/// How many days of a week are assembled at once, so a week does not take
/// the database pool from chat and ingest while it loads.
const WEEK_DAYS_AT_ONCE: usize = 2;

/// A local day and its wall clock. The sky's hours are wall-clock hours from
/// the day's local midnight, so 3:00 PM is 15 on a daylight-saving day too,
/// and an hour turns back into the instant it names.
#[derive(Debug, Clone, Copy)]
pub struct DayClock {
    pub date: NaiveDate,
    pub tz: Tz,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl DayClock {
    pub fn new(date: NaiveDate, zone: &str) -> Self {
        let (start, end) = super::day_summary::day_bounds(date, Some(zone));
        // absent-ok: a zone that does not parse is read as UTC, as `day_bounds` reads it.
        let tz = zone.parse::<Tz>().unwrap_or(chrono_tz::UTC);
        Self { date, tz, start, end }
    }

    /// Hours on the day's wall clock: before midnight is below 0, the next
    /// morning above 24.
    pub fn hours(&self, t: DateTime<Utc>) -> f64 {
        let l = t.with_timezone(&self.tz);
        let days = (l.date_naive() - self.date).num_days() as f64;
        let h = days * 24.0 + l.hour() as f64 + l.minute() as f64 / 60.0 + l.second() as f64 / 3600.0;
        (h * 1000.0).round() / 1000.0
    }

    /// The instant a wall-clock hour of the day names: the first of the two
    /// when the clocks went back, and the minute they reached when they went
    /// forward past it.
    pub fn instant(&self, h: f64) -> DateTime<Utc> {
        let day = h.div_euclid(24.0);
        let seconds = ((h - day * 24.0) * 3600.0).round() as i64;
        let naive = (self.date + Duration::days(day as i64)).and_time(NaiveTime::MIN) + Duration::seconds(seconds);
        match self.tz.from_local_datetime(&naive) {
            LocalResult::Single(t) | LocalResult::Ambiguous(t, _) => t.with_timezone(&Utc),
            LocalResult::None => match self.tz.from_local_datetime(&(naive + Duration::hours(1))) {
                LocalResult::Single(t) | LocalResult::Ambiguous(t, _) => t.with_timezone(&Utc),
                LocalResult::None => self.start + Duration::seconds((h * 3600.0).round() as i64),
            },
        }
    }
}
/// Distinct past days of visits needed before any moment is scored, as
/// `dayline::novelty` asks of its baseline.
const ROUTINE_MIN_DAYS: usize = 3;
/// How many links a moment opens to, and how many of them are people.
const LINKS_MAX: usize = 4;
const PEOPLE_MAX: usize = 2;

#[derive(Debug, Serialize)]
pub struct SkyDay {
    pub date: NaiveDate,
    /// The zone the day is read in (`timezone::day_timezone`).
    pub zone: String,
    /// Hours since local midnight, when the day is today.
    pub now_hour: Option<f64>,
    pub moments: Vec<Moment>,
    /// The weather now, when the day is today and a reading is fresh.
    pub weather: Option<SkyWeather>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Moment {
    /// Stable across reads: the source and its row, e.g. `visit:vis_…`.
    pub id: String,
    /// `stay`, `event`, `workout` or `sleep`.
    pub kind: &'static str,
    pub start_hour: f64,
    pub end_hour: f64,
    pub label: String,
    pub place_id: Option<String>,
    /// A calendar item that has not started yet.
    pub planned: bool,
    /// The record the moment is mentioned by (`/record/calendar_event/…`),
    /// or none when it has no single row.
    pub url: Option<String>,
    /// What clicking the dot offers to mention, filled by `get_day`.
    pub links: Vec<SkyLink>,
    /// How unusual its place is for its hour and weekday, from 0 (where you
    /// always are then) to 1 (never), filled by `get_day`. None without a
    /// place, or before there are `ROUTINE_MIN_DAYS` of visits to compare.
    pub novelty: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SkyLink {
    pub url: String,
    pub label: String,
    /// `place`, `person` or `purchase`.
    pub kind: &'static str,
}

/// Seven days as the sky's Week level draws them.
#[derive(Debug, Serialize)]
pub struct SkyWeek {
    /// The Monday the week starts on, as the Timeline's week does.
    pub start: NaiveDate,
    pub days: Vec<SkyDay>,
    /// What the week's moments belong to, each moment as `date|id`.
    pub threads: Vec<Thread>,
}

/// A purchase as the links need it: when it was made, and what it says.
#[derive(Debug, Clone)]
pub struct PurchaseIn {
    pub id: String,
    pub at: DateTime<Utc>,
    pub cents: i64,
    pub currency: Option<String>,
    pub merchant: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SkyWeather {
    pub temperature_c: Option<f64>,
    pub condition: String,
}

/// A stay as the assembler needs it: its place already titled.
#[derive(Debug, Clone)]
pub struct StayIn {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub place_id: Option<String>,
    pub place: Option<PlaceTitle>,
}

#[derive(Debug, Clone)]
pub struct WorkoutIn {
    pub id: String,
    pub workout_type: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub distance_km: Option<f64>,
}

pub async fn get_day(pool: &PgPool, date: NaiveDate) -> Result<SkyDay> {
    day_with(pool, date, None).await
}

/// A day, scored against `past` visits when a week has read them already.
async fn day_with(pool: &PgPool, date: NaiveDate, past: Option<&[PastVisit]>) -> Result<SkyDay> {
    let zone = crate::timezone::day_timezone(pool, date).await?;
    let clock = DayClock::new(date, &zone);
    let (start, end) = (clock.start, clock.end);

    let stays = super::timeline::stays_between(pool, start, end).await?;
    let mut place_ids: Vec<String> = stays.iter().filter_map(|s| s.place_id.clone()).collect();
    place_ids.sort();
    place_ids.dedup();
    let titles = super::place_review::titles(pool, &place_ids).await?;
    let stays = stays
        .into_iter()
        .map(|s| StayIn {
            place: s.place_id.as_ref().and_then(|p| titles.get(p).cloned()),
            id: s.id,
            started_at: s.started_at,
            ended_at: s.ended_at,
            place_id: s.place_id,
        })
        .collect::<Vec<_>>();

    let calendar = super::timeline::calendar_between(pool, start, end).await?;

    let workouts = sqlx::query(
        "SELECT id, workout_type, started_at, ended_at, distance_km FROM data_health_workout \
         WHERE started_at < $2 AND ended_at > $1 \
           AND deleted_at_source IS NULL AND NOT is_archived \
         ORDER BY started_at",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| {
        Ok(WorkoutIn {
            id: r.try_get("id")?,
            workout_type: r.try_get("workout_type")?,
            started_at: r.try_get("started_at")?,
            ended_at: r.try_get("ended_at")?,
            distance_km: r.try_get("distance_km")?,
        })
    })
    .collect::<std::result::Result<Vec<_>, sqlx::Error>>()?;

    let night = crate::dayline::sleep::night_for_day(pool, date).await?;

    let now = Utc::now();
    let is_today = start <= now && now < end;
    let weather = if is_today {
        super::home::get_current_weather(pool)
            .await?
            .filter(|w| is_fresh(&w.occurred_at, now))
            .map(|w| SkyWeather { temperature_c: w.temperature_c, condition: w.condition })
    } else {
        None
    };

    let mut moments = assemble(&clock, now, &stays, &calendar, &workouts, night.as_ref());

    // A night can be several sleep rows; the longest one stands for it.
    if let (Some(n), Some(m)) = (night.as_ref(), moments.iter_mut().find(|m| m.kind == "sleep")) {
        let id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM data_health_sleep \
             WHERE started_at >= $1 AND ended_at <= $2 AND deleted_at_source IS NULL \
             ORDER BY ended_at - started_at DESC LIMIT 1",
        )
        .bind(n.start)
        .bind(n.end)
        .fetch_optional(pool)
        .await?;
        m.url = id.map(|id| format!("/record/health_sleep/{id}"));
    }

    let read;
    let past = match past {
        Some(p) => p,
        None => {
            read = past_visits(pool, start - Duration::days(ROUTINE_LOOKBACK_DAYS), start, &zone).await?;
            &read[..]
        }
    };
    score_routine(&clock, &mut moments, past);

    let event_ids: Vec<String> =
        moments.iter().filter_map(|m| m.id.strip_prefix("event:").map(str::to_string)).collect();
    let people = event_people(pool, &event_ids).await?;
    let purchases = purchases_between(pool, start, end).await?;
    link_moments(&clock, &mut moments, &titles, &people, &purchases);


    Ok(SkyDay { date, zone, now_hour: is_today.then(|| clock.hours(now)), moments, weather })
}

/// The Monday-to-Sunday week holding `date`, each day assembled as `get_day`
/// does, the seven at once, and the threads that run through it.
pub async fn get_week(pool: &PgPool, date: NaiveDate) -> Result<SkyWeek> {
    let start = week_monday(date);
    let zone = crate::timezone::day_timezone(pool, start).await?;
    let first = DayClock::new(start, &zone);
    let past = past_visits(pool, first.start - Duration::days(ROUTINE_LOOKBACK_DAYS), first.start + Duration::days(7), &zone).await?;
    let days: Vec<SkyDay> = futures::stream::iter((0..7).map(|i| day_with(pool, start + Duration::days(i), Some(&past))))
        .buffered(WEEK_DAYS_AT_ONCE)
        .try_collect()
        .await?;
    let threads = threads_for(pool, &days, |d, m| format!("{}|{}", d.date, m.id)).await?;
    Ok(SkyWeek { start, days, threads: rank_threads(threads) })
}

fn week_monday(date: NaiveDate) -> NaiveDate {
    date - Duration::days(date.weekday().num_days_from_monday() as i64)
}

/// A past visit as the routine needs it: where, at what hour and weekday,
/// and how long ago.
#[derive(Debug, Clone)]
pub struct PastVisit {
    pub place_id: String,
    pub phase: (f64, f64),
    /// The middle of the visit, and its local date.
    pub at: DateTime<Utc>,
    pub date: NaiveDate,
}

/// The visits between `from` and `to` that resolved to a place.
async fn past_visits(pool: &PgPool, from: DateTime<Utc>, to: DateTime<Utc>, zone: &str) -> Result<Vec<PastVisit>> {
    let rows = sqlx::query(
        r#"SELECT v.started_at, v.ended_at, er.entity_id AS place_id
           FROM data_location_visit v
           JOIN LATERAL (
               SELECT r.entity_id FROM wiki_refs r
               WHERE r.source_table = 'data_location_visit' AND r.source_id = v.id AND r.entity_type = 'place'
               ORDER BY r.created_at DESC LIMIT 1
           ) er ON TRUE
           WHERE v.started_at >= $1 AND v.started_at < $2 AND v.ended_at IS NOT NULL
             AND v.deleted_at_source IS NULL AND NOT v.is_archived"#,
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;
    // absent-ok: a zone that does not parse counts days by UTC date, which only moves a visit near midnight.
    let tz: Tz = zone.parse().unwrap_or(chrono_tz::UTC);
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let began: DateTime<Utc> = r.try_get("started_at")?;
        let ended: DateTime<Utc> = r.try_get("ended_at")?;
        let mid = began + (ended - began) / 2;
        out.push(PastVisit {
            place_id: r.try_get("place_id")?,
            phase: crate::dayline::novelty::local_phase(mid, Some(zone)),
            at: mid,
            date: mid.with_timezone(&tz).date_naive(),
        });
    }
    Ok(out)
}

/// Score each moment at a place by how rarely you are there at its hour and
/// weekday: one minus the square root of the weighted share of past visits
/// at about that time that were at that place. The root spreads the
/// occasional places out of the top, so a café you visit monthly sits
/// higher than your desk but below somewhere new.
pub fn score_routine(clock: &DayClock, moments: &mut [Moment], past: &[PastVisit]) {
    use crate::dayline::novelty::{local_phase, phase_weight, time_weight};
    let since = clock.start - Duration::days(ROUTINE_LOOKBACK_DAYS);
    let past: Vec<&PastVisit> = past.iter().filter(|v| v.at < clock.start && v.at >= since).collect();
    let days: std::collections::HashSet<NaiveDate> = past.iter().map(|v| v.date).collect();
    if days.len() < ROUTINE_MIN_DAYS {
        return;
    }
    for m in moments.iter_mut().filter(|m| m.kind != "sleep") {
        let Some(place) = m.place_id.as_deref() else { continue };
        let phase = local_phase(clock.instant((m.start_hour + m.end_hour) / 2.0), Some(clock.tz.name()));
        let (mut here, mut all) = (0.0, 0.0);
        for v in &past {
            let days_ago = (clock.start - v.at).num_seconds() as f64 / 86_400.0;
            let w = time_weight(days_ago) * phase_weight(phase, v.phase);
            all += w;
            if v.place_id == place {
                here += w;
            }
        }
        if all > 0.0 {
            m.novelty = Some(((1.0 - (here / all).sqrt()) * 100.0).round() / 100.0);
        }
    }
}

/// The people on each calendar entry, the owner left out, by event id.
async fn event_people(pool: &PgPool, event_ids: &[String]) -> Result<HashMap<String, Vec<(String, String)>>> {
    if event_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(
        "SELECT r.source_id, p.id, p.name FROM wiki_refs r \
         JOIN wiki_people p ON p.id = r.entity_id \
         WHERE r.entity_type = 'person' AND r.source_table = 'data_calendar_event' \
           AND r.source_id = ANY($1) AND COALESCE(p.name, '') <> '' \
           AND p.id IS DISTINCT FROM (SELECT self_person_id FROM app_user_profile LIMIT 1) \
         ORDER BY r.source_id, p.name",
    )
    .bind(event_ids)
    .fetch_all(pool)
    .await?;
    let mut out: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for r in rows {
        let entry = out.entry(r.try_get("source_id")?).or_default();
        let id: String = r.try_get("id")?;
        if !entry.iter().any(|(p, _)| *p == id) {
            entry.push((id, r.try_get("name")?));
        }
    }
    Ok(out)
}

/// The day's purchases made in person, by the registry's own rule for what
/// counts as spending. A row whose time is exactly midnight UTC carries only
/// a date, which says nothing about where you were, so it is left out.
async fn purchases_between(pool: &PgPool, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<PurchaseIn>> {
    let spend = virtues_registry::ontologies::lane_measures()
        .iter()
        .find(|m| m.id == "spend")
        .and_then(|m| m.filter)
        .ok_or_else(|| Error::Other("the registry has no spend measure".into()))?;
    let rows = sqlx::query(&format!(
        "SELECT id, amount, currency, COALESCE(NULLIF(merchant_name, ''), NULLIF(description, '')) AS merchant, \
                COALESCE(authorized_timestamp, occurred_at) AS at \
         FROM data_financial_transaction \
         WHERE COALESCE(authorized_timestamp, occurred_at) >= $1 AND COALESCE(authorized_timestamp, occurred_at) < $2 \
           AND deleted_at_source IS NULL AND NOT is_archived \
           AND COALESCE(payment_channel, '') <> 'online' \
           AND {spend}"
    ))
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for r in rows {
        let at: DateTime<Utc> = r.try_get("at")?;
        if at.time() == NaiveTime::MIN {
            continue;
        }
        out.push(PurchaseIn {
            id: r.try_get("id")?,
            at,
            cents: r.try_get("amount")?,
            currency: r.try_get("currency")?,
            merchant: r.try_get("merchant")?,
        });
    }
    Ok(out)
}

/// Fill each moment's links: its place, up to two people from its calendar
/// entry, and the largest purchase made during it away from home.
pub fn link_moments(
    clock: &DayClock,
    moments: &mut [Moment],
    places: &HashMap<String, PlaceTitle>,
    people: &HashMap<String, Vec<(String, String)>>,
    purchases: &[PurchaseIn],
) {
    let at_hour = |h: f64| clock.instant(h);
    for m in moments.iter_mut() {
        let mut links = Vec::new();
        let place = m.place_id.as_ref().and_then(|id| places.get(id).map(|t| (id, t)));
        if let Some((id, t)) = place.filter(|(_, t)| t.source != "none") {
            links.push(SkyLink { url: format!("/place/{id}"), label: t.title.clone(), kind: "place" });
        }
        if let Some(ps) = m.id.strip_prefix("event:").and_then(|id| people.get(id)) {
            for (id, name) in ps.iter().take(PEOPLE_MAX) {
                links.push(SkyLink { url: format!("/person/{id}"), label: name.clone(), kind: "person" });
            }
        }
        let away = place.is_some_and(|(_, t)| t.source != "home");
        if away && !m.planned && matches!(m.kind, "stay" | "event") {
            let (from, to) = (at_hour(m.start_hour), at_hour(m.end_hour));
            if let Some(p) = purchases.iter().filter(|p| p.at >= from && p.at < to).max_by_key(|p| p.cents) {
                links.push(SkyLink {
                    url: format!("/record/financial_transaction/{}", p.id),
                    label: purchase_label(p),
                    kind: "purchase",
                });
            }
        }
        links.truncate(LINKS_MAX);
        m.links = links;
    }
}

/// "$42.10 at Café Rosa"; another currency is named, never shown as dollars.
fn purchase_label(p: &PurchaseIn) -> String {
    let amount = p.cents.unsigned_abs() as f64 / 100.0;
    let amount = match p.currency.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        None | Some("USD") => format!("${amount:.2}"),
        Some(c) => format!("{amount:.2} {c}"),
    };
    match p.merchant.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        Some(m) => format!("{amount} at {m}"),
        None => format!("{amount} spent"),
    }
}

/// A reading older than three hours says nothing about now.
fn is_fresh(occurred_at: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(occurred_at)
        .map(|t| now - t.with_timezone(&Utc) < Duration::hours(3))
        // absent-ok: an unparseable reading time is not shown, rather than shown as now.
        .unwrap_or(false)
}


fn minutes(a: DateTime<Utc>, b: DateTime<Utc>) -> i64 {
    (b - a).num_minutes()
}

fn overlap_minutes(a0: DateTime<Utc>, a1: DateTime<Utc>, b0: DateTime<Utc>, b1: DateTime<Utc>) -> i64 {
    (a1.min(b1) - a0.max(b0)).num_minutes().max(0)
}

/// The day's moments, in time order.
pub fn assemble(
    clock: &DayClock,
    now: DateTime<Utc>,
    stays: &[StayIn],
    calendar: &[CalendarEvent],
    workouts: &[WorkoutIn],
    night: Option<&Night>,
) -> Vec<Moment> {
    let (start, end) = (clock.start, clock.end);
    let mut moments = Vec::new();

    // The night the day woke from, and the stays it covers cut away.
    if let Some(n) = night.filter(|n| n.end > start && n.end <= end) {
        moments.push(Moment {
            id: format!("night:{}", n.end.format("%Y-%m-%d")),
            kind: "sleep",
            start_hour: clock.hours(n.start),
            end_hour: clock.hours(n.end),
            label: format!("Sleep, {}", hours_minutes(n.asleep_minutes)),
            place_id: None,
            planned: false,
            url: None,
            links: Vec::new(),
            novelty: None,
        });
    }
    let awake_from = night.filter(|n| n.end > start && n.end <= end).map(|n| n.end).unwrap_or(start);

    // Stays inside the day, after waking, with neighbors at one place joined.
    let mut kept: Vec<StayIn> = Vec::new();
    for s in stays {
        let s0 = s.started_at.max(start).max(awake_from);
        let s1 = s.ended_at.min(end);
        if minutes(s0, s1) < MIN_STAY_MINUTES {
            continue;
        }
        if let Some(prev) = kept.last_mut() {
            if prev.place_id.is_some()
                && prev.place_id == s.place_id
                && minutes(prev.ended_at, s0) <= SAME_PLACE_GAP_MINUTES
            {
                prev.ended_at = prev.ended_at.max(s1);
                continue;
            }
        }
        kept.push(StayIn { started_at: s0, ended_at: s1, ..s.clone() });
    }

    // Calendar entries that happened or will: not all-day, cancelled or declined.
    let events: Vec<&CalendarEvent> = calendar
        .iter()
        .filter(|e| !e.is_all_day && e.ended_at > e.started_at)
        .filter(|e| !e.status.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("cancelled")))
        .filter(|e| !e.response_status.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("declined")))
        .filter(|e| e.started_at < end && e.ended_at > start)
        .collect();

    // A stay that is mostly one calendar entry, or one workout, is that moment.
    let mut stay_used = vec![false; kept.len()];
    let mut event_place: HashMap<&str, usize> = HashMap::new();
    for e in &events {
        let e_min = minutes(e.started_at, e.ended_at).max(1);
        let best = kept
            .iter()
            .enumerate()
            .filter(|(i, _)| !stay_used[*i])
            .map(|(i, s)| (i, overlap_minutes(s.started_at, s.ended_at, e.started_at, e.ended_at), s))
            .filter(|(_, o, s)| *o * 2 >= e_min && minutes(s.started_at, s.ended_at) * 4 <= e_min * 7)
            .max_by_key(|(_, o, _)| *o);
        if let Some((i, _, _)) = best {
            stay_used[i] = true;
            event_place.insert(e.id.as_str(), i);
        }
    }
    let mut workout_place: HashMap<&str, usize> = HashMap::new();
    for w in workouts {
        let w_min = minutes(w.started_at, w.ended_at).max(1);
        let best = kept
            .iter()
            .enumerate()
            .filter(|(i, _)| !stay_used[*i])
            .map(|(i, s)| (i, overlap_minutes(s.started_at, s.ended_at, w.started_at, w.ended_at), s))
            .filter(|(_, o, s)| *o * 10 >= w_min * 7 && minutes(s.started_at, s.ended_at) * 2 <= w_min * 3)
            .max_by_key(|(_, o, _)| *o);
        if let Some((i, _, _)) = best {
            stay_used[i] = true;
            workout_place.insert(w.id.as_str(), i);
        }
    }

    for (i, s) in kept.iter().enumerate() {
        if stay_used[i] {
            continue;
        }
        moments.push(Moment {
            id: format!("visit:{}", s.id),
            kind: "stay",
            start_hour: clock.hours(s.started_at),
            end_hour: clock.hours(s.ended_at),
            label: place_label(s.place.as_ref()),
            place_id: s.place_id.clone(),
            planned: false,
            url: Some(format!("/record/location_visit/{}", s.id)),
            links: Vec::new(),
            novelty: None,
        });
    }

    let mut planned = 0;
    for e in &events {
        let is_planned = e.started_at > now;
        if is_planned {
            if planned >= PLANNED_MAX {
                continue;
            }
            planned += 1;
        }
        let stay = event_place.get(e.id.as_str()).map(|i| &kept[*i]);
        moments.push(Moment {
            id: format!("event:{}", e.id),
            kind: "event",
            start_hour: clock.hours(e.started_at.max(start)),
            end_hour: clock.hours(e.ended_at.min(end)),
            label: event_label(&e.title, stay.and_then(|s| s.place.as_ref())),
            place_id: stay.and_then(|s| s.place_id.clone()),
            planned: is_planned,
            url: Some(format!("/record/calendar_event/{}", e.id)),
            links: Vec::new(),
            novelty: None,
        });
    }

    for w in workouts.iter().filter(|w| w.ended_at > start && w.started_at < end) {
        let stay = workout_place.get(w.id.as_str()).map(|i| &kept[*i]);
        let mut label = workout_name(&w.workout_type);
        if let Some(km) = w.distance_km.filter(|k| *k >= 0.1) {
            label = format!("{label}, {km:.1} km");
        }
        if let Some(p) = stay.and_then(|s| s.place.as_ref()).filter(|p| p.source != "none") {
            label = format!("{label}, {}", p.title);
        }
        moments.push(Moment {
            id: format!("workout:{}", w.id),
            kind: "workout",
            start_hour: clock.hours(w.started_at.max(start)),
            end_hour: clock.hours(w.ended_at.min(end)),
            label,
            place_id: stay.and_then(|s| s.place_id.clone()),
            planned: false,
            url: Some(format!("/record/health_workout/{}", w.id)),
            links: Vec::new(),
            novelty: None,
        });
    }

    // Too many dots: the shortest plain stays go first.
    while moments.len() > MOMENTS_MAX {
        let shortest = moments
            .iter()
            .enumerate()
            .filter(|(_, m)| m.kind == "stay")
            .min_by(|a, b| (a.1.end_hour - a.1.start_hour).total_cmp(&(b.1.end_hour - b.1.start_hour)))
            .map(|(i, _)| i);
        match shortest {
            Some(i) => {
                moments.remove(i);
            }
            None => break,
        }
    }

    moments.sort_by(|a, b| a.start_hour.total_cmp(&b.start_hour));
    moments
}

fn hours_minutes(total: i64) -> String {
    format!("{} h {:02} m", total / 60, total % 60)
}

fn place_label(place: Option<&PlaceTitle>) -> String {
    match place {
        Some(p) if p.source != "none" && !p.title.trim().is_empty() => p.title.clone(),
        _ => "An unnamed place".to_string(),
    }
}

/// "Lunch with David Okafor, Café Rosa"; "Dinner with Nick at home"; the
/// title alone when it already names the place or the place has no name.
fn event_label(title: &str, place: Option<&PlaceTitle>) -> String {
    let title = title.trim();
    match place {
        Some(p) if p.source == "home" => format!("{title} at home"),
        Some(p) if p.source != "none" && !title.to_lowercase().contains(&p.title.to_lowercase()) => {
            format!("{title}, {}", p.title)
        }
        _ => title.to_string(),
    }
}

/// A readable name for a workout type, from HealthKit (`running`,
/// `HKWorkoutActivityTypeRunning`) or Strava (`Run`, `TrailRun`).
pub fn workout_name(raw: &str) -> String {
    let t = raw.trim();
    let t = t.strip_prefix("HKWorkoutActivityType").unwrap_or(t);
    let lower = t.to_lowercase();
    let known = [
        ("run", "Run"),
        ("walk", "Walk"),
        ("hik", "Hike"),
        ("cycl", "Ride"),
        ("ride", "Ride"),
        ("bik", "Ride"),
        ("swim", "Swim"),
        ("yoga", "Yoga"),
        ("strength", "Strength training"),
        ("weight", "Strength training"),
        ("row", "Row"),
    ];
    if let Some((_, name)) = known.iter().find(|(k, _)| lower.contains(k)) {
        return name.to_string();
    }
    if lower.is_empty() || lower == "unknown" || lower == "other" {
        return "Workout".to_string();
    }
    // `HighIntensityIntervalTraining`, `mixed_cardio` → "High intensity interval training", "Mixed cardio".
    let mut words = String::new();
    for (i, c) in t.chars().enumerate() {
        if c == '_' || c == '-' {
            words.push(' ');
        } else if c.is_uppercase() && i > 0 && !words.ends_with(' ') {
            words.push(' ');
            words.extend(c.to_lowercase());
        } else {
            words.extend(c.to_lowercase());
        }
    }
    let mut chars = words.trim().chars();
    match chars.next() {
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Workout".to_string(),
    }
}

/// What happened inside one moment, for the sky's Moment level.
#[derive(Debug, Serialize)]
pub struct SkyMomentDetail {
    pub date: NaiveDate,
    pub now_hour: Option<f64>,
    pub moment: Moment,
    pub pieces: Vec<Piece>,
}

/// One thing inside a moment, placed by its hour on the moment's band.
#[derive(Debug, Serialize, PartialEq)]
pub struct Piece {
    pub id: String,
    /// `arrive`, `leave`, `message`, `voice` or `purchase`.
    pub kind: &'static str,
    /// Hours from the day's local midnight, like the moment's own.
    pub hour: f64,
    pub label: String,
    /// Who it was from, under the label.
    pub detail: Option<String>,
    /// The record it is mentioned by.
    pub url: Option<String>,
}

/// How many messages and voice notes a moment opens to.
const MESSAGES_MAX: i64 = 8;
const VOICE_MAX: i64 = 6;
/// How much of a message or a transcript a piece shows.
const SNIPPET_CHARS: usize = 80;
/// How long after a stay's last recorded minute it is still going on: the
/// entity resolver rewrites the live visit every 15 minutes.
const ONGOING_SLACK_MINUTES: f64 = 20.0;

/// One moment of a day and what happened inside it: arriving and leaving,
/// the messages (with its people, when it has any), voice notes and
/// purchases in its window.
pub async fn get_moment(pool: &PgPool, date: NaiveDate, id: &str) -> Result<SkyMomentDetail> {
    let day = get_day(pool, date).await?;
    let clock = DayClock::new(date, &day.zone);
    let moment = day
        .moments
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| Error::NotFound(format!("No moment {id} on {date}")))?;
    let (from, to) = (clock.instant(moment.start_hour), clock.instant(moment.end_hour));
    let mut pieces = edges(&moment, day.now_hour);
    if moment.planned {
        return Ok(SkyMomentDetail { date, now_hour: day.now_hour, moment, pieces });
    }

    let people: Vec<String> = moment
        .links
        .iter()
        .filter_map(|l| l.url.strip_prefix("/person/").map(str::to_string))
        .collect();
    let messages = sqlx::query(
        "SELECT m.id, m.body, m.from_name, m.occurred_at, \
                COALESCE(m.metadata->>'is_from_me', 'false') = 'true' AS from_me \
         FROM data_communication_message m \
         WHERE m.occurred_at >= $1 AND m.occurred_at < $2 \
           AND m.deleted_at_source IS NULL AND NOT m.is_archived \
           AND (cardinality($3::text[]) = 0 OR EXISTS ( \
               SELECT 1 FROM wiki_refs r WHERE r.source_table = 'data_communication_message' \
                 AND r.source_id = m.id AND r.entity_type = 'person' AND r.entity_id = ANY($3))) \
         ORDER BY m.occurred_at LIMIT $4",
    )
    .bind(from)
    .bind(to)
    .bind(&people)
    .bind(MESSAGES_MAX)
    .fetch_all(pool)
    .await?;
    for r in messages {
        let id: String = r.try_get("id")?;
        let body: Option<String> = r.try_get("body")?;
        let from_me: bool = r.try_get("from_me")?;
        let sender: Option<String> = r.try_get("from_name")?;
        pieces.push(Piece {
            kind: "message",
            hour: clock.hours(r.try_get("occurred_at")?),
            label: body.as_deref().map(snippet).filter(|s| !s.is_empty()).unwrap_or_else(|| "A message".into()),
            detail: if from_me { Some("You".into()) } else { sender.filter(|s| !s.trim().is_empty()) },
            url: Some(format!("/record/communication_message/{id}")),
            id: format!("message:{id}"),
        });
    }

    let voice = sqlx::query(
        "SELECT id, title, summary, text, started_at FROM data_communication_transcription \
         WHERE started_at < $2 AND COALESCE(ended_at, started_at) >= $1 \
           AND deleted_at_source IS NULL AND NOT is_archived \
         ORDER BY started_at LIMIT $3",
    )
    .bind(from)
    .bind(to)
    .bind(VOICE_MAX)
    .fetch_all(pool)
    .await?;
    for r in voice {
        let id: String = r.try_get("id")?;
        let said = [r.try_get::<Option<String>, _>("title")?, r.try_get("summary")?, r.try_get("text")?]
            .into_iter()
            .flatten()
            .map(|s| snippet(&s))
            .find(|s| !s.is_empty());
        let began: DateTime<Utc> = r.try_get("started_at")?;
        pieces.push(Piece {
            kind: "voice",
            hour: clock.hours(began.max(from)),
            label: said.unwrap_or_else(|| "A voice note".into()),
            detail: None,
            url: Some(format!("/record/communication_transcription/{id}")),
            id: format!("voice:{id}"),
        });
    }

    for p in purchases_between(pool, from, to).await? {
        pieces.push(Piece {
            kind: "purchase",
            hour: clock.hours(p.at),
            label: purchase_label(&p),
            detail: None,
            url: Some(format!("/record/financial_transaction/{}", p.id)),
            id: format!("purchase:{}", p.id),
        });
    }

    pieces.sort_by(|a, b| a.hour.total_cmp(&b.hour));
    Ok(SkyMomentDetail { date, now_hour: day.now_hour, moment, pieces })
}

/// The moment's two ends: falling asleep and waking for the night, arriving
/// and leaving for anything at a place. A moment still going on has not
/// been left.
fn edges(m: &Moment, now: Option<f64>) -> Vec<Piece> {
    let edge = |kind: &'static str, hour: f64, label: String| Piece {
        id: format!("{kind}:{}", m.id),
        kind,
        hour,
        label,
        detail: None,
        url: None,
    };
    if m.kind == "sleep" {
        return vec![edge("arrive", m.start_hour, "Asleep".into()), edge("leave", m.end_hour, "Up".into())];
    }
    let Some(place) = m.links.iter().find(|l| l.kind == "place") else {
        return Vec::new();
    };
    if m.planned {
        return Vec::new();
    }
    let ongoing = now.is_some_and(|n| m.start_hour <= n && n <= m.end_hour + ONGOING_SLACK_MINUTES / 60.0);
    let mut out = vec![edge("arrive", m.start_hour, format!("Arrived at {}", place.label))];
    if !ongoing {
        out.push(edge("leave", m.end_hour, format!("Left {}", place.label)));
    }
    out
}

/// A message or transcript cut to a line: whitespace collapsed, at most
/// `SNIPPET_CHARS` characters.
fn snippet(s: &str) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= SNIPPET_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(SNIPPET_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

/// The projects, pages and chats a day's moments belong to, for the sky's
/// Context level.
#[derive(Debug, Serialize)]
pub struct SkyContext {
    pub date: NaiveDate,
    pub threads: Vec<Thread>,
}

/// A project, page or chat, and the moments of the day it links to.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Thread {
    pub url: String,
    pub label: String,
    /// `project`, `page` or `chat`.
    pub kind: &'static str,
    /// The ids of the moments it links to.
    pub moments: Vec<String>,
}

/// How many threads the Context level labels.
const THREADS_MAX: usize = 6;
/// How far back a chat that mentions a moment's person or place counts.
const CHAT_LOOKBACK_DAYS: i64 = 7;

/// What a day's moments belong to, cheapest evidence first: projects that
/// hold a moment's record, place or person; chats you wrote in during a
/// moment; and pages and recent chats that mention its person or place.
/// Home is left out: every home stay would link to everything about it.
pub async fn get_context(pool: &PgPool, date: NaiveDate) -> Result<SkyContext> {
    let day = get_day(pool, date).await?;
    let threads = threads_for(pool, std::slice::from_ref(&day), |_, m| m.id.clone()).await?;
    Ok(SkyContext { date, threads: rank_threads(threads) })
}

/// Every thread the moments of `days` belong to, unranked, each moment
/// named by `key`. One pass serves a day or a week: a person or place that
/// recurs across the days is looked up once.
async fn threads_for(pool: &PgPool, days: &[SkyDay], key: impl Fn(&SkyDay, &Moment) -> String) -> Result<Vec<Thread>> {
    let clocks: Vec<DayClock> = days.iter().map(|d| DayClock::new(d.date, &d.zone)).collect();
    let (Some(first), Some(last)) = (clocks.first(), clocks.last()) else { return Ok(Vec::new()) };
    let (start, end) = (first.start, last.end);
    let moments = || days.iter().zip(&clocks).flat_map(|(d, c)| d.moments.iter().map(move |m| (d, c, m)));
    let home: Option<String> = sqlx::query_scalar::<_, Option<String>>("SELECT home_place_id FROM app_user_profile LIMIT 1")
        .fetch_optional(pool)
        .await?
        .flatten();
    let home_url = home.as_deref().map(|h| format!("/place/{h}"));
    let mut found: Vec<Thread> = Vec::new();

    // Projects holding a moment's record, its place or one of its people.
    let mut by_url: HashMap<String, Vec<String>> = HashMap::new();
    for (d, _, m) in moments() {
        let urls = m.url.iter().chain(m.links.iter().filter(|l| l.kind != "purchase").map(|l| &l.url));
        for u in urls.filter(|u| Some(*u) != home_url.as_ref()) {
            by_url.entry(u.clone()).or_default().push(key(d, m));
        }
    }
    if !by_url.is_empty() {
        let urls: Vec<&String> = by_url.keys().collect();
        let rows = sqlx::query(
            "SELECT i.url, p.id, p.name FROM app_thread_items i JOIN app_threads p ON p.id = i.thread_id \
             WHERE i.url = ANY($1) AND p.deleted_at IS NULL AND p.archived_at IS NULL",
        )
        .bind(&urls)
        .fetch_all(pool)
        .await?;
        for r in rows {
            let url: String = r.try_get("url")?;
            let id: String = r.try_get("id")?;
            for mid in by_url.get(&url).into_iter().flatten() {
                add_thread(&mut found, format!("/thread/{id}"), r.try_get("name")?, "thread", mid);
            }
        }
    }

    // Chats you wrote in while a moment was happening.
    let written = sqlx::query(
        "SELECT c.id, COALESCE(NULLIF(c.title, ''), 'Untitled chat') AS title, x.created_at \
         FROM app_chat_messages x JOIN app_chats c ON c.id = x.chat_id \
         WHERE x.role = 'user' AND x.created_at >= $1 AND x.created_at < $2 AND c.deleted_at IS NULL",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;
    for r in written {
        let when: DateTime<Utc> = r.try_get("created_at")?;
        let id: String = r.try_get("id")?;
        let title: String = r.try_get("title")?;
        for (d, c, m) in moments().filter(|(_, _, m)| !m.planned && m.kind != "sleep") {
            if c.instant(m.start_hour) <= when && when < c.instant(m.end_hour) {
                add_thread(&mut found, format!("/chat/{id}"), title.clone(), "chat", &key(d, m));
            }
        }
    }

    // Pages, and chats in the past week, that mention a moment's person or place.
    let mut subjects: HashMap<(&'static str, String), Vec<String>> = HashMap::new();
    for (d, _, m) in moments() {
        for l in m.links.iter().filter(|l| Some(&l.url) != home_url.as_ref()) {
            let subject = match l.kind {
                "person" => l.url.strip_prefix("/person/").map(|id| ("person", id.to_string())),
                "place" => l.url.strip_prefix("/place/").map(|id| ("place", id.to_string())),
                _ => None,
            };
            if let Some(s) = subject {
                subjects.entry(s).or_default().push(key(d, m));
            }
        }
    }
    for ((kind, id), mids) in &subjects {
        for b in super::wiki_articles::get_subject_backlinks(pool, kind, id).await? {
            for mid in mids {
                add_thread(&mut found, b.route.clone(), b.title.clone(), "page", mid);
            }
        }
        // The id is matched literally: `_` and `%` in it are not wildcards.
        let literal = id.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        let chats = sqlx::query(
            "SELECT c.id, COALESCE(NULLIF(c.title, ''), 'Untitled chat') AS title \
             FROM app_chat_messages x JOIN app_chats c ON c.id = x.chat_id \
             WHERE x.content LIKE $1 AND x.created_at >= $2 AND x.created_at < $3 AND c.deleted_at IS NULL \
             GROUP BY c.id, c.title ORDER BY max(x.created_at) DESC LIMIT 5",
        )
        .bind(format!("%/{kind}/{literal})%"))
        .bind(start - Duration::days(CHAT_LOOKBACK_DAYS))
        .bind(end)
        .fetch_all(pool)
        .await?;
        for r in chats {
            let cid: String = r.try_get("id")?;
            let title: String = r.try_get("title")?;
            for mid in mids {
                add_thread(&mut found, format!("/chat/{cid}"), title.clone(), "chat", mid);
            }
        }
    }

    Ok(found)
}

fn add_thread(found: &mut Vec<Thread>, url: String, label: String, kind: &'static str, moment: &str) {
    match found.iter_mut().find(|t| t.url == url) {
        Some(t) if !t.moments.iter().any(|m| m == moment) => t.moments.push(moment.to_string()),
        Some(_) => {}
        None => found.push(Thread { url, label, kind, moments: vec![moment.to_string()] }),
    }
}

/// The threads that hold the most of the day first, a project before a page
/// before a chat when they tie, and at most `THREADS_MAX`.
fn rank_threads(mut threads: Vec<Thread>) -> Vec<Thread> {
    let order = |k: &str| match k {
        "thread" => 0,
        "page" => 1,
        _ => 2,
    };
    threads.sort_by(|a, b| {
        b.moments.len().cmp(&a.moments.len()).then(order(a.kind).cmp(&order(b.kind))).then(a.label.cmp(&b.label))
    });
    threads.truncate(THREADS_MAX);
    threads
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// An invented Thursday, read in UTC so the hours are plain to check.
    fn day() -> (DateTime<Utc>, DateTime<Utc>) {
        let s = Utc.with_ymd_and_hms(2031, 3, 13, 0, 0, 0).unwrap();
        (s, s + Duration::days(1))
    }

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        day().0 + Duration::hours(h as i64) + Duration::minutes(m as i64)
    }

    /// The invented day's clock, in UTC, where wall-clock and elapsed hours agree.
    fn clock(s: DateTime<Utc>, e: DateTime<Utc>) -> DayClock {
        DayClock { date: s.date_naive(), tz: chrono_tz::UTC, start: s, end: e }
    }

    fn title(t: &str, source: &'static str) -> Option<PlaceTitle> {
        Some(PlaceTitle { title: t.into(), source })
    }

    fn stay(id: &str, from: DateTime<Utc>, to: DateTime<Utc>, place: &str, t: Option<PlaceTitle>) -> StayIn {
        StayIn { id: id.into(), started_at: from, ended_at: to, place_id: Some(place.into()), place: t }
    }

    fn event(id: &str, title: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> CalendarEvent {
        CalendarEvent {
            id: id.into(),
            title: title.into(),
            started_at: from,
            ended_at: to,
            is_all_day: false,
            calendar_name: None,
            location_name: None,
            status: None,
            response_status: None,
        }
    }

    fn labels(ms: &[Moment]) -> Vec<&str> {
        ms.iter().map(|m| m.label.as_str()).collect()
    }

    #[test]
    fn a_calendar_lunch_and_its_stay_are_one_moment() {
        let (s, e) = day();
        let stays = [stay("v1", at(12, 4), at(12, 58), "pl_rosa", title("Café Rosa", "named"))];
        let cal = [event("c1", "Lunch with David Okafor", at(12, 0), at(13, 0))];
        let ms = assemble(&clock(s, e), at(16, 0), &stays, &cal, &[], None);
        assert_eq!(labels(&ms), ["Lunch with David Okafor, Café Rosa"]);
        assert_eq!(ms[0].kind, "event");
        assert_eq!(ms[0].place_id.as_deref(), Some("pl_rosa"));
    }

    #[test]
    fn a_short_meeting_inside_a_long_stay_stays_its_own_dot() {
        let (s, e) = day();
        let stays = [stay("v1", at(9, 0), at(12, 0), "pl_studio", title("Studio North", "named"))];
        let cal = [event("c1", "Standup", at(9, 0), at(9, 30))];
        let ms = assemble(&clock(s, e), at(16, 0), &stays, &cal, &[], None);
        assert_eq!(labels(&ms), ["Studio North", "Standup"]);
    }

    #[test]
    fn dinner_at_home_reads_at_home_and_the_title_is_not_repeated() {
        let (s, e) = day();
        let stays = [
            stay("v1", at(18, 25), at(20, 5), "pl_home", title("Home", "home")),
            stay("v2", at(21, 0), at(22, 0), "pl_lib", title("Elm Street Library", "named")),
        ];
        let cal = [
            event("c1", "Dinner with Nick", at(18, 30), at(20, 0)),
            event("c2", "Book club, Elm Street Library", at(21, 0), at(22, 0)),
        ];
        let ms = assemble(&clock(s, e), at(23, 0), &stays, &cal, &[], None);
        assert_eq!(labels(&ms), ["Dinner with Nick at home", "Book club, Elm Street Library"]);
    }

    #[test]
    fn the_night_is_one_moment_and_cuts_the_home_stay_it_covers() {
        let (s, e) = day();
        let night = Night { start: s - Duration::minutes(65), end: at(6, 35), asleep_minutes: 460, stages: Vec::new() };
        let stays = [stay("v1", s - Duration::hours(3), at(7, 40), "pl_home", title("Home", "home"))];
        let ms = assemble(&clock(s, e), at(16, 0), &stays, &[], &[], Some(&night));
        assert_eq!(labels(&ms), ["Sleep, 7 h 40 m", "Home"]);
        assert!(ms[0].start_hour < 0.0, "the night starts before midnight");
        assert!((ms[1].start_hour - 6.583).abs() < 0.01, "the home stay starts at waking");
    }

    #[test]
    fn three_planned_items_at_most_and_none_declined_cancelled_or_all_day() {
        let (s, e) = day();
        let mut declined = event("c2", "Optional sync", at(16, 30), at(17, 0));
        declined.response_status = Some("declined".into());
        let mut cancelled = event("c3", "Coffee", at(17, 0), at(17, 30));
        cancelled.status = Some("cancelled".into());
        let mut all_day = event("c4", "Holiday", s, e);
        all_day.is_all_day = true;
        let cal = [
            event("c1", "Design review prep", at(16, 0), at(17, 0)),
            declined,
            cancelled,
            all_day,
            event("c5", "Dinner with Nick", at(18, 30), at(20, 0)),
            event("c6", "Book club", at(19, 45), at(20, 45)),
            event("c7", "Call with Sam", at(21, 0), at(21, 30)),
        ];
        let ms = assemble(&clock(s, e), at(15, 20), &[], &cal, &[], None);
        assert_eq!(labels(&ms), ["Design review prep", "Dinner with Nick", "Book club"]);
        assert!(ms.iter().all(|m| m.planned));
    }

    #[test]
    fn a_workout_names_its_distance_and_takes_its_place_from_the_stay() {
        let (s, e) = day();
        let stays = [stay("v1", at(18, 0), at(19, 10), "pl_gym", title("Ironside Gym", "named"))];
        let workouts = [
            WorkoutIn { id: "w1".into(), workout_type: "running".into(), started_at: at(6, 40), ended_at: at(7, 25), distance_km: Some(5.12) },
            WorkoutIn { id: "w2".into(), workout_type: "TraditionalStrengthTraining".into(), started_at: at(18, 5), ended_at: at(19, 5), distance_km: None },
        ];
        let ms = assemble(&clock(s, e), at(22, 0), &stays, &[], &workouts, None);
        assert_eq!(labels(&ms), ["Run, 5.1 km", "Strength training, Ironside Gym"]);
    }

    #[test]
    fn two_stays_at_one_place_with_a_short_gap_are_one() {
        let (s, e) = day();
        let stays = [
            stay("v1", at(9, 0), at(11, 30), "pl_studio", title("Studio North", "named")),
            stay("v2", at(11, 45), at(12, 0), "pl_studio", title("Studio North", "named")),
            stay("v3", at(13, 10), at(14, 0), "pl_studio", title("Studio North", "named")),
        ];
        let ms = assemble(&clock(s, e), at(16, 0), &stays, &[], &[], None);
        assert_eq!(labels(&ms), ["Studio North", "Studio North"]);
        assert!((ms[0].end_hour - 12.0).abs() < 0.01);
    }

    #[test]
    fn a_crowded_day_drops_its_shortest_plain_stays() {
        let (s, e) = day();
        let stays: Vec<StayIn> = (0..40)
            .map(|i| {
                let from = at(6, 0) + Duration::minutes(i * 25);
                let len = 11 + (i % 7);
                stay(&format!("v{i}"), from, from + Duration::minutes(len), &format!("pl_{i}"), title("Somewhere", "map"))
            })
            .collect();
        let cal = [event("c1", "Design review", at(16, 0), at(17, 0))];
        let ms = assemble(&clock(s, e), at(23, 0), &stays, &cal, &[], None);
        assert_eq!(ms.len(), MOMENTS_MAX);
        assert!(ms.iter().any(|m| m.label == "Design review"), "calendar items are never dropped for room");
    }

    fn purchase(id: &str, at: DateTime<Utc>, cents: i64, merchant: &str) -> PurchaseIn {
        PurchaseIn { id: id.into(), at, cents, currency: Some("USD".into()), merchant: Some(merchant.into()) }
    }

    fn places(list: &[(&str, &str, &'static str)]) -> HashMap<String, PlaceTitle> {
        list.iter().map(|(id, t, source)| (id.to_string(), PlaceTitle { title: t.to_string(), source })).collect()
    }

    #[test]
    fn each_moment_is_mentioned_by_its_record() {
        let (s, e) = day();
        let stays = [stay("v1", at(9, 0), at(12, 0), "pl_studio", title("Studio North", "named"))];
        let cal = [event("c1", "Standup", at(9, 0), at(9, 15))];
        let workouts = [WorkoutIn { id: "w1".into(), workout_type: "running".into(), started_at: at(6, 40), ended_at: at(7, 25), distance_km: None }];
        let ms = assemble(&clock(s, e), at(16, 0), &stays, &cal, &workouts, None);
        let urls: Vec<_> = ms.iter().map(|m| m.url.as_deref().unwrap_or("")).collect();
        assert_eq!(urls, ["/record/health_workout/w1", "/record/location_visit/v1", "/record/calendar_event/c1"]);
    }

    #[test]
    fn a_lunch_links_its_place_its_people_and_the_largest_purchase_during_it() {
        let (s, e) = day();
        let stays = [stay("v1", at(12, 4), at(12, 58), "pl_rosa", title("Café Rosa", "named"))];
        let cal = [event("c1", "Lunch with David Okafor", at(12, 0), at(13, 0))];
        let mut ms = assemble(&clock(s, e), at(16, 0), &stays, &cal, &[], None);
        let people = HashMap::from([(
            "c1".to_string(),
            vec![("person_d".into(), "David Okafor".into()), ("person_n".into(), "Nick".into()), ("person_s".into(), "Sam".into())],
        )]);
        let purchases = [
            purchase("t1", at(12, 40), 4210, "Café Rosa"),
            purchase("t2", at(12, 45), 350, "Café Rosa"),
            purchase("t3", at(15, 0), 8000, "Corner Market"),
        ];
        link_moments(&clock(s, e), &mut ms, &places(&[("pl_rosa", "Café Rosa", "named")]), &people, &purchases);
        let links: Vec<_> = ms[0].links.iter().map(|l| (l.kind, l.label.as_str(), l.url.as_str())).collect();
        assert_eq!(
            links,
            [
                ("place", "Café Rosa", "/place/pl_rosa"),
                ("person", "David Okafor", "/person/person_d"),
                ("person", "Nick", "/person/person_n"),
                ("purchase", "$42.10 at Café Rosa", "/record/financial_transaction/t1"),
            ]
        );
    }

    #[test]
    fn home_and_what_is_still_ahead_link_no_purchase() {
        let (s, e) = day();
        let stays = [stay("v1", at(16, 0), at(17, 30), "pl_home", title("Home", "home"))];
        let cal = [event("c1", "Dinner with Nick", at(18, 30), at(20, 0))];
        let mut ms = assemble(&clock(s, e), at(17, 0), &stays, &cal, &[], None);
        let people = HashMap::from([("c1".to_string(), vec![("person_n".into(), "Nick".into())])]);
        let purchases = [purchase("t1", at(16, 30), 2599, "Grocer"), purchase("t2", at(19, 0), 6400, "Trattoria")];
        link_moments(&clock(s, e), &mut ms, &places(&[("pl_home", "Home", "home")]), &people, &purchases);
        let kinds: Vec<Vec<_>> = ms.iter().map(|m| m.links.iter().map(|l| l.kind).collect()).collect();
        assert_eq!(kinds, [vec!["place"], vec!["person"]]);
    }

    /// The links' queries against a migrated database: the attendee, the
    /// purchase made in person (not the online one), and the night's row.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_day_read_from_the_database_carries_its_refs_and_links(pool: PgPool) {
        let date = NaiveDate::from_ymd_opt(2024, 3, 14).unwrap();
        let zone = crate::timezone::day_timezone(&pool, date).await.unwrap();
        let (s, _) = crate::api::day_summary::day_bounds(date, Some(&zone));
        let t = |h: i64, m: i64| s + Duration::hours(h) + Duration::minutes(m);
        let run = |sql: &'static str| sqlx::query(sql);
        run("INSERT INTO wiki_places (id, name, latitude, longitude, is_named) VALUES ('pl_rosa', 'Café Rosa', 0, 0, true)")
            .execute(&pool)
            .await
            .unwrap();
        run("INSERT INTO wiki_people (id, name) VALUES ('person_d', 'David Okafor')").execute(&pool).await.unwrap();
        run("INSERT INTO data_location_visit (id, latitude, longitude, started_at, ended_at, source_stream_id, source_table, source_provider) \
             VALUES ('v1', 0, 0, $1, $2, 'v1', 'test', 'test')")
        .bind(t(12, 4))
        .bind(t(12, 58))
        .execute(&pool)
        .await
        .unwrap();
        run("INSERT INTO data_calendar_event (id, title, started_at, ended_at, source_stream_id, source_table, source_provider) \
             VALUES ('c1', 'Lunch with David Okafor', $1, $2, 'c1', 'test', 'test')")
        .bind(t(12, 0))
        .bind(t(13, 0))
        .execute(&pool)
        .await
        .unwrap();
        run("INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, role) VALUES \
             ('r1', 'place', 'pl_rosa', 'data_location_visit', 'v1', NULL), \
             ('r2', 'person', 'person_d', 'data_calendar_event', 'c1', 'attendee')")
        .execute(&pool)
        .await
        .unwrap();
        run("INSERT INTO data_financial_account (id, account_name, account_type, source_stream_id, source_table, source_provider) \
             VALUES ('a1', 'Checking', 'depository', 'a1', 'test', 'plaid')")
        .execute(&pool)
        .await
        .unwrap();
        run("INSERT INTO data_financial_transaction \
             (id, account_id, transaction_id, amount, currency, merchant_name, merchant_category, payment_channel, occurred_at, source_stream_id, source_table, source_provider) VALUES \
             ('t1', 'a1', 't1', 4210, 'USD', 'Café Rosa', 'FOOD_AND_DRINK', 'in store', $1, 't1', 'test', 'plaid'), \
             ('t2', 'a1', 't2', 9900, 'USD', 'Bookshop', 'GENERAL_MERCHANDISE', 'online', $2, 't2', 'test', 'plaid')")
        .bind(t(12, 40))
        .bind(t(12, 30))
        .execute(&pool)
        .await
        .unwrap();
        run("INSERT INTO data_health_sleep (id, started_at, ended_at, source_stream_id, source_table, source_provider) \
             VALUES ('s1', $1, $2, 's1', 'test', 'test')")
        .bind(t(-1, 0))
        .bind(t(6, 30))
        .execute(&pool)
        .await
        .unwrap();

        let day = get_day(&pool, date).await.unwrap();
        let sleep = day.moments.iter().find(|m| m.kind == "sleep").expect("the night");
        assert_eq!(sleep.url.as_deref(), Some("/record/health_sleep/s1"));
        let lunch = day.moments.iter().find(|m| m.kind == "event").expect("the lunch");
        assert_eq!(lunch.url.as_deref(), Some("/record/calendar_event/c1"));
        let links: Vec<_> = lunch.links.iter().map(|l| l.url.as_str()).collect();
        assert_eq!(links, ["/place/pl_rosa", "/person/person_d", "/record/financial_transaction/t1"]);
    }

    #[test]
    fn a_purchase_in_another_currency_is_not_shown_as_dollars() {
        let mut p = purchase("t1", at(12, 0), 1250, "Bäckerei");
        p.currency = Some("EUR".into());
        assert_eq!(purchase_label(&p), "12.50 EUR at Bäckerei");
        p.merchant = None;
        assert_eq!(purchase_label(&p), "12.50 EUR spent");
    }

    #[test]
    fn a_snippet_is_one_line_and_cut_short() {
        assert_eq!(snippet("  see you\n at  noon "), "see you at noon");
        let long = "word ".repeat(40);
        let cut = snippet(&long);
        assert!(cut.chars().count() <= SNIPPET_CHARS);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn threads_that_hold_more_of_the_day_come_first() {
        let t = |url: &str, kind: &'static str, n: usize| Thread {
            url: url.into(),
            label: url.into(),
            kind,
            moments: (0..n).map(|i| format!("m{i}")).collect(),
        };
        let ranked = rank_threads(vec![t("/chat/c", "chat", 2), t("/page/p", "page", 2), t("/thread/a", "thread", 1), t("/thread/b", "thread", 3)]);
        let urls: Vec<_> = ranked.iter().map(|t| t.url.as_str()).collect();
        assert_eq!(urls, ["/thread/b", "/page/p", "/chat/c", "/thread/a"]);
    }

    #[test]
    fn a_moment_still_going_on_has_not_been_left() {
        let (s, e) = day();
        let stays = [stay("v1", at(9, 0), at(11, 30), "pl_studio", title("Studio North", "named"))];
        let mut ms = assemble(&clock(s, e), at(11, 40), &stays, &[], &[], None);
        link_moments(&clock(s, e), &mut ms, &places(&[("pl_studio", "Studio North", "named")]), &HashMap::new(), &[]);
        let labels = |now: f64| edges(&ms[0], Some(now)).into_iter().map(|p| p.label).collect::<Vec<_>>();
        assert_eq!(labels(11.6), ["Arrived at Studio North"]);
        assert_eq!(labels(15.0), ["Arrived at Studio North", "Left Studio North"]);
    }

    /// The Moment and Context levels against a migrated database: a message
    /// with the lunch's guest inside the lunch, another outside it, a project
    /// holding the guest, and a chat written during the lunch.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_moment_opens_to_what_happened_in_it_and_the_day_to_what_it_belongs_to(pool: PgPool) {
        let date = NaiveDate::from_ymd_opt(2024, 3, 14).unwrap();
        let zone = crate::timezone::day_timezone(&pool, date).await.unwrap();
        let (s, _) = crate::api::day_summary::day_bounds(date, Some(&zone));
        let t = |h: i64, m: i64| s + Duration::hours(h) + Duration::minutes(m);
        let run = |sql: &'static str| sqlx::query(sql);
        run("INSERT INTO wiki_places (id, name, latitude, longitude, is_named) VALUES ('pl_rosa', 'Café Rosa', 0, 0, true)")
            .execute(&pool)
            .await
            .unwrap();
        run("INSERT INTO wiki_people (id, name) VALUES ('person_d', 'David Okafor')").execute(&pool).await.unwrap();
        run("INSERT INTO data_location_visit (id, latitude, longitude, started_at, ended_at, source_stream_id, source_table, source_provider) \
             VALUES ('v1', 0, 0, $1, $2, 'v1', 'test', 'test')")
        .bind(t(12, 4))
        .bind(t(12, 58))
        .execute(&pool)
        .await
        .unwrap();
        run("INSERT INTO data_calendar_event (id, title, started_at, ended_at, source_stream_id, source_table, source_provider) \
             VALUES ('c1', 'Lunch with David Okafor', $1, $2, 'c1', 'test', 'test')")
        .bind(t(12, 0))
        .bind(t(13, 0))
        .execute(&pool)
        .await
        .unwrap();
        run("INSERT INTO data_communication_message \
             (id, message_id, channel, from_identifier, body, from_name, occurred_at, source_stream_id, source_table, source_provider) VALUES \
             ('msg_in', 'msg_in', 'imessage', '+15125550101', 'Running five minutes late', 'David Okafor', $1, 'msg_in', 'test', 'test'), \
             ('msg_out', 'msg_out', 'imessage', '+15125550102', 'Unrelated, and before lunch', 'Nick', $2, 'msg_out', 'test', 'test')")
        .bind(t(12, 10))
        .bind(t(10, 0))
        .execute(&pool)
        .await
        .unwrap();
        run("INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, role) VALUES \
             ('r1', 'place', 'pl_rosa', 'data_location_visit', 'v1', NULL), \
             ('r2', 'person', 'person_d', 'data_calendar_event', 'c1', 'attendee'), \
             ('r3', 'person', 'person_d', 'data_communication_message', 'msg_in', 'sender')")
        .execute(&pool)
        .await
        .unwrap();
        run("INSERT INTO app_threads (id, name) VALUES ('proj_r', 'Friday review')").execute(&pool).await.unwrap();
        run("INSERT INTO app_thread_items (thread_id, url, sort_order, role) VALUES ('proj_r', '/person/person_d', 0, 'library')")
            .execute(&pool)
            .await
            .unwrap();
        run("INSERT INTO app_chats (id, title) VALUES ('chat_l', 'Menu ideas')").execute(&pool).await.unwrap();
        run("INSERT INTO app_chat_messages (id, chat_id, role, content, sequence_num, created_at) \
             VALUES ('cm1', 'chat_l', 'user', 'what should I order', 1, $1)")
        .bind(t(12, 20))
        .execute(&pool)
        .await
        .unwrap();

        let detail = get_moment(&pool, date, "event:c1").await.unwrap();
        let kinds: Vec<_> = detail.pieces.iter().map(|p| (p.kind, p.label.as_str())).collect();
        assert_eq!(
            kinds,
            [("arrive", "Arrived at Café Rosa"), ("message", "Running five minutes late"), ("leave", "Left Café Rosa")]
        );

        let context = get_context(&pool, date).await.unwrap();
        let threads: Vec<_> = context.threads.iter().map(|t| (t.kind, t.label.as_str(), t.moments.clone())).collect();
        assert_eq!(
            threads,
            [
                ("thread", "Friday review", vec!["event:c1".to_string()]),
                ("chat", "Menu ideas", vec!["event:c1".to_string()]),
            ]
        );

        let week = get_week(&pool, date).await.unwrap();
        assert_eq!(week.days.len(), 7);
        let review = week.threads.iter().find(|t| t.label == "Friday review").expect("the project runs through the week");
        assert_eq!(review.moments, ["2024-03-14|event:c1"]);
    }

    #[test]
    fn a_place_you_are_always_at_then_sits_low_and_a_new_one_high() {
        let (s, e) = day();
        let stays = [
            stay("v1", at(9, 0), at(12, 0), "pl_studio", title("Studio North", "named")),
            stay("v2", at(13, 0), at(14, 0), "pl_new", title("Somewhere new", "named")),
        ];
        // Eight past Thursdays at the studio mid-morning, and at home in the evening.
        let visit = |place: &str, weeks: i64, h: f64| {
            let at = s - Duration::weeks(weeks) + Duration::minutes((h * 60.0) as i64);
            PastVisit { place_id: place.into(), phase: (h, 4.0), at, date: at.date_naive() }
        };
        let past: Vec<PastVisit> = (1..=8).flat_map(|w| [visit("pl_studio", w, 10.5), visit("pl_home", w, 19.0)]).collect();
        let mut ms = assemble(&clock(s, e), at(20, 0), &stays, &[], &[], None);
        score_routine(&clock(s, e), &mut ms, &past);
        let novelty = |label: &str| ms.iter().find(|m| m.label == label).and_then(|m| m.novelty).unwrap();
        assert!(novelty("Studio North") < 0.3, "the desk at its usual hour is routine");
        assert_eq!(novelty("Somewhere new"), 1.0);

        let mut early = assemble(&clock(s, e), at(20, 0), &stays, &[], &[], None);
        let few: Vec<PastVisit> = past.iter().filter(|v| v.at > s - Duration::weeks(ROUTINE_MIN_DAYS as i64 - 1)).cloned().collect();
        score_routine(&clock(s, e), &mut early, &few);
        assert!(early.iter().all(|m| m.novelty.is_none()), "too few past days to say");
    }

    #[test]
    fn a_daylight_saving_day_reads_on_the_wall_clock() {
        // The second Sunday of March 2031, when New York's clocks go forward.
        let c = DayClock::new(NaiveDate::from_ymd_opt(2031, 3, 9).unwrap(), "America/New_York");
        let three_pm = c.instant(15.0);
        assert_eq!(three_pm, Utc.with_ymd_and_hms(2031, 3, 9, 19, 0, 0).unwrap(), "3:00 PM is EDT, UTC-4");
        assert_eq!(c.hours(three_pm), 15.0);
        assert_eq!(c.hours(c.start), 0.0);
        assert_eq!(c.hours(c.instant(-1.5)), -1.5, "the evening before reads below midnight");
        assert_eq!(c.hours(c.instant(2.5)), 3.5, "2:30 AM was skipped; it reads as the clocks reached it");
    }

    #[test]
    fn a_week_starts_on_its_monday() {
        let monday = NaiveDate::from_ymd_opt(2031, 3, 10).unwrap();
        for offset in 0..7 {
            assert_eq!(week_monday(monday + Duration::days(offset)), monday);
        }
    }

    #[test]
    fn workout_types_read_as_words() {
        assert_eq!(workout_name("HKWorkoutActivityTypeRunning"), "Run");
        assert_eq!(workout_name("TrailRun"), "Run");
        assert_eq!(workout_name("cycling"), "Ride");
        assert_eq!(workout_name("HighIntensityIntervalTraining"), "High intensity interval training");
        assert_eq!(workout_name("mixed_cardio"), "Mixed cardio");
        assert_eq!(workout_name("Unknown"), "Workout");
    }
}
