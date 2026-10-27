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

use std::collections::HashMap;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row};

use super::place_review::PlaceTitle;
use super::timeline::CalendarEvent;
use crate::dayline::sleep::Night;
use crate::error::Result;

/// A stay shorter than this, once the night is cut off it, is not drawn.
const MIN_STAY_MINUTES: i64 = 10;
/// Two stays at one place this close together are one stay.
const SAME_PLACE_GAP_MINUTES: i64 = 20;
/// How many calendar items still to come the sky shows.
const PLANNED_MAX: usize = 3;
/// A day with more moments than this drops its shortest plain stays.
const MOMENTS_MAX: usize = 28;

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
    let zone = crate::timezone::day_timezone(pool, date).await?;
    let (start, end) = super::day_summary::day_bounds(date, Some(&zone));

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

    Ok(SkyDay {
        date,
        zone,
        now_hour: is_today.then(|| hours(start, now)),
        moments: assemble(start, end, now, &stays, &calendar, &workouts, night.as_ref()),
        weather,
    })
}

/// A reading older than three hours says nothing about now.
fn is_fresh(occurred_at: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(occurred_at)
        .map(|t| now - t.with_timezone(&Utc) < Duration::hours(3))
        // absent-ok: an unparseable reading time is not shown, rather than shown as now.
        .unwrap_or(false)
}

fn hours(start: DateTime<Utc>, t: DateTime<Utc>) -> f64 {
    let h = (t - start).num_seconds() as f64 / 3600.0;
    (h * 1000.0).round() / 1000.0
}

fn minutes(a: DateTime<Utc>, b: DateTime<Utc>) -> i64 {
    (b - a).num_minutes()
}

fn overlap_minutes(a0: DateTime<Utc>, a1: DateTime<Utc>, b0: DateTime<Utc>, b1: DateTime<Utc>) -> i64 {
    (a1.min(b1) - a0.max(b0)).num_minutes().max(0)
}

/// The day's moments, in time order.
pub fn assemble(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    now: DateTime<Utc>,
    stays: &[StayIn],
    calendar: &[CalendarEvent],
    workouts: &[WorkoutIn],
    night: Option<&Night>,
) -> Vec<Moment> {
    let mut moments = Vec::new();

    // The night the day woke from, and the stays it covers cut away.
    if let Some(n) = night.filter(|n| n.end > start && n.end <= end) {
        moments.push(Moment {
            id: format!("night:{}", n.end.format("%Y-%m-%d")),
            kind: "sleep",
            start_hour: hours(start, n.start),
            end_hour: hours(start, n.end),
            label: format!("Sleep, {}", hours_minutes(n.asleep_minutes)),
            place_id: None,
            planned: false,
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
            start_hour: hours(start, s.started_at),
            end_hour: hours(start, s.ended_at),
            label: place_label(s.place.as_ref()),
            place_id: s.place_id.clone(),
            planned: false,
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
            start_hour: hours(start, e.started_at.max(start)),
            end_hour: hours(start, e.ended_at.min(end)),
            label: event_label(&e.title, stay.and_then(|s| s.place.as_ref())),
            place_id: stay.and_then(|s| s.place_id.clone()),
            planned: is_planned,
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
            start_hour: hours(start, w.started_at.max(start)),
            end_hour: hours(start, w.ended_at.min(end)),
            label,
            place_id: stay.and_then(|s| s.place_id.clone()),
            planned: false,
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
        let ms = assemble(s, e, at(16, 0), &stays, &cal, &[], None);
        assert_eq!(labels(&ms), ["Lunch with David Okafor, Café Rosa"]);
        assert_eq!(ms[0].kind, "event");
        assert_eq!(ms[0].place_id.as_deref(), Some("pl_rosa"));
    }

    #[test]
    fn a_short_meeting_inside_a_long_stay_stays_its_own_dot() {
        let (s, e) = day();
        let stays = [stay("v1", at(9, 0), at(12, 0), "pl_studio", title("Studio North", "named"))];
        let cal = [event("c1", "Standup", at(9, 0), at(9, 30))];
        let ms = assemble(s, e, at(16, 0), &stays, &cal, &[], None);
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
        let ms = assemble(s, e, at(23, 0), &stays, &cal, &[], None);
        assert_eq!(labels(&ms), ["Dinner with Nick at home", "Book club, Elm Street Library"]);
    }

    #[test]
    fn the_night_is_one_moment_and_cuts_the_home_stay_it_covers() {
        let (s, e) = day();
        let night = Night { start: s - Duration::minutes(65), end: at(6, 35), asleep_minutes: 460, stages: Vec::new() };
        let stays = [stay("v1", s - Duration::hours(3), at(7, 40), "pl_home", title("Home", "home"))];
        let ms = assemble(s, e, at(16, 0), &stays, &[], &[], Some(&night));
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
        let ms = assemble(s, e, at(15, 20), &[], &cal, &[], None);
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
        let ms = assemble(s, e, at(22, 0), &stays, &[], &workouts, None);
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
        let ms = assemble(s, e, at(16, 0), &stays, &[], &[], None);
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
        let ms = assemble(s, e, at(23, 0), &stays, &cal, &[], None);
        assert_eq!(ms.len(), MOMENTS_MAX);
        assert!(ms.iter().any(|m| m.label == "Design review"), "calendar items are never dropped for room");
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
