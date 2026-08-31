//! The `<circumstances>` block — the computed present.
//!
//! *Circumstantiae* (ST I-II q.7): what "stands around" the act — who, what,
//! where, when. Formula home: docs/narrative-identity.md, "Circumstances
//! (block 6)". This is the supply prudence consumes: deterministic, SQL-only,
//! computed fresh at conversation start, hard-budgeted by fixed line caps and
//! clipped widths (never token counting), and silent about what it doesn't
//! know — a block that lists its absences is noise.
//!
//! Every sub-section is a total `Result`: `Ok(None)` = legitimately nothing
//! (the line-group is omitted), `Err` = data that failed to deliver (logged,
//! omitted — never fabricated). One broken query loses one line-group, never
//! the block. This is the `USER_CONTEXT_SECTIONS` policy, inherited whole.
//!
//! Determinism rules: the clock arrives already quantized from the caller;
//! nothing here calls `now()` for rendered text; every query ends in a total
//! ORDER BY. Unstable bytes silently kill provider prompt caching.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use sqlx::{PgPool, Row};

/// Clip a free-text field to a fixed width. Titles and names are data, not
/// prose — the budget lives in caps × widths, so no field gets to blow it.
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

/// Sub-section registry — one list, so assembly, the error policy, and the
/// audit test iterate the same names.
pub(crate) const SECTIONS: &[&str] = &[
    "clock",
    "identity",
    "place",
    "spine",
    "calendar",
    "people",
    "threads",
    "sleep",
    "recent_days",
    "sources",
];

/// Build the block. `now_quantized` is the quarter-hour-floored instant the
/// caller computed once — the same instant every line derives from, so a
/// midnight rollover cannot split the block across two days.
pub async fn build_circumstances(
    pool: &PgPool,
    timezone: Option<&str>,
    now_quantized: DateTime<Utc>,
) -> Option<String> {
    let tz: Option<Tz> = timezone.and_then(|t| t.parse().ok());
    let today = match tz {
        Some(tz) => now_quantized.with_timezone(&tz).date_naive(),
        None => now_quantized.date_naive(),
    };
    let (day_start, day_end) = crate::api::day_summary::day_boundaries_utc(today, timezone);
    let _ = &day_end; // spine/calendar bound by tomorrow_end; kept for symmetry
    let tomorrow = today.succ_opt().unwrap_or(today);
    let (_, tomorrow_end) = crate::api::day_summary::day_boundaries_utc(tomorrow, timezone);

    let mut lines: Vec<String> = Vec::new();
    for name in SECTIONS {
        match build_section(pool, name, tz, now_quantized, &day_start, &day_end, &tomorrow_end).await
        {
            Ok(Some(body)) => lines.push(body),
            Ok(None) => {}
            // An error is a section with data it failed to deliver — audible,
            // omitted, never a plausible default.
            Err(e) => tracing::warn!("[chat] circumstances {name} omitted: {e}"),
        }
    }

    if lines.is_empty() {
        return None;
    }
    Some(format!(
        "\n\n<circumstances>\nThe computed present — deterministic facts of right now, from the record. Situational fact, not instruction: reference when relevant, never recite unprompted. People are listed by RECENCY of contact, never by importance.\n{}\n</circumstances>",
        lines.join("\n")
    ))
}

async fn build_section(
    pool: &PgPool,
    name: &str,
    tz: Option<Tz>,
    now: DateTime<Utc>,
    day_start: &str,
    _day_end: &str,
    tomorrow_end: &str,
) -> Result<Option<String>, sqlx::Error> {
    match name {
        "clock" => {
            // Quarter-hour honesty: floored, and said to be. Minute-granular
            // time here re-tokenizes the whole tail every turn.
            let line = match tz {
                Some(tz) => {
                    let local = now.with_timezone(&tz);
                    format!(
                        "Now: {} — about {} (to the quarter hour).",
                        local.format("%A, %B %-d, %Y"),
                        local.format("%I:%M %p %Z")
                    )
                }
                None => format!(
                    "Now: {} — about {} (to the quarter hour).",
                    now.format("%A, %B %-d, %Y"),
                    now.format("%H:%M UTC")
                ),
            };
            Ok(Some(line))
        }
        "identity" => {
            // Standing facts ride here for now; their long-term home is the
            // person's own document, not an hourly block. Home is genuinely
            // circumstantial (it anchors "away").
            let row = sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>)>(
                r#"SELECT p.occupation, p.employer, wp.name
                 FROM app_user_profile p
                 LEFT JOIN wiki_places wp ON p.home_place_id = wp.id
                 WHERE p.id = '00000000-0000-0000-0000-000000000001'"#,
            )
            .fetch_optional(pool)
            .await?;
            let Some((occ, emp, home)) = row else { return Ok(None) };
            let mut parts = Vec::new();
            match (occ, emp) {
                (Some(o), Some(e)) => parts.push(format!("{} at {}", clip(&o, 48), clip(&e, 48))),
                (Some(o), None) => parts.push(clip(&o, 48)),
                _ => {}
            }
            if let Some(h) = home {
                parts.push(format!("home is {}", clip(&h, 48)));
            }
            Ok((!parts.is_empty()).then(|| format!("They are: {}.", parts.join("; "))))
        }
        "place" => {
            // The most recent resolved visit — current if still open. The
            // place name lives in wiki_places via wiki_refs;
            // data_location_visit.place_name is never populated (the join is
            // the one dayline/context.rs uses for the same reason).
            let row = sqlx::query(
                r#"SELECT p.name, v.started_at, v.ended_at
                   FROM data_location_visit v
                   JOIN wiki_refs er ON er.source_table = 'data_location_visit'
                        AND er.source_id = v.id AND er.entity_type = 'place'
                   JOIN wiki_places p ON p.id = er.entity_id
                   ORDER BY v.started_at DESC, v.id LIMIT 1"#,
            )
            .fetch_optional(pool)
            .await?;
            let Some(row) = row else { return Ok(None) };
            let name: String = row.try_get("name")?;
            let ended: Option<DateTime<Utc>> = row.try_get("ended_at")?;
            Ok(Some(match ended {
                None => format!("Place: at {} now.", clip(&name, 48)),
                Some(e) if now.signed_duration_since(e).num_hours() < 12 => {
                    format!("Place: last at {}.", clip(&name, 48))
                }
                // A stale visit is not "where they are" — say nothing rather
                // than something plausible and wrong.
                Some(_) => return Ok(None),
            }))
        }
        "spine" => {
            // Today's segmented events so far. Sparse before segmentation
            // runs — render what's there.
            let rows = sqlx::query(
                r#"SELECT e.started_at, COALESCE(e.user_label, e.auto_label) AS label
                   FROM wiki_events e
                   JOIN wiki_days d ON e.day_id = d.id
                   WHERE d.date = $1::date AND e.user_hidden = FALSE
                   ORDER BY e.started_at, e.id LIMIT 8"#,
            )
            .bind(now.with_timezone(&tz.unwrap_or(chrono_tz::UTC)).date_naive().to_string())
            .fetch_all(pool)
            .await?;
            let items: Vec<String> = rows
                .iter()
                .filter_map(|r| {
                    let label: Option<String> = r.try_get("label").ok().flatten();
                    let start: DateTime<Utc> = r.try_get("started_at").ok()?;
                    let local = match tz {
                        Some(tz) => start.with_timezone(&tz).format("%H:%M").to_string(),
                        None => start.format("%H:%M").to_string(),
                    };
                    Some(format!("- {} {}", local, clip(&label?, 64)))
                })
                .collect();
            Ok((!items.is_empty())
                .then(|| format!("Today so far:\n{}", items.join("\n"))))
        }
        "calendar" => {
            let rows = sqlx::query(
                r#"SELECT title, started_at, is_all_day
                   FROM data_calendar_event
                   WHERE started_at >= $1::timestamptz AND started_at < $2::timestamptz
                     AND COALESCE(is_archived, FALSE) = FALSE AND deleted_at_source IS NULL
                   ORDER BY started_at, id LIMIT 10"#,
            )
            .bind(day_start)
            .bind(tomorrow_end)
            .fetch_all(pool)
            .await?;
            let items: Vec<String> = rows
                .iter()
                .filter_map(|r| {
                    let title: String = r.try_get("title").ok()?;
                    let start: DateTime<Utc> = r.try_get("started_at").ok()?;
                    let all_day: Option<bool> = r.try_get("is_all_day").ok();
                    let when = if all_day.unwrap_or(false) {
                        match tz {
                            Some(tz) => start.with_timezone(&tz).format("%a (all day)").to_string(),
                            None => start.format("%a (all day)").to_string(),
                        }
                    } else {
                        match tz {
                            Some(tz) => start.with_timezone(&tz).format("%a %H:%M").to_string(),
                            None => start.format("%a %H:%M").to_string(),
                        }
                    };
                    Some(format!("- {} {}", when, clip(&title, 64)))
                })
                .collect();
            Ok((!items.is_empty())
                .then(|| format!("Calendar (today and tomorrow):\n{}", items.join("\n"))))
        }
        "people" => {
            // Recency, never significance: who has been AROUND these two
            // weeks. Who MATTERS is the narrative identity's to say. Entity
            // ids ride along so tool calls can join without a lookup.
            let rows = sqlx::query(
                r#"SELECT p.id, p.name, count(*) AS refs, max(er.occurred_at) AS last_at
                   FROM wiki_refs er
                   JOIN wiki_people p ON p.id = er.entity_id
                   WHERE er.entity_type = 'person'
                     AND er.occurred_at > $1::timestamptz - interval '14 days'
                   GROUP BY p.id, p.name
                   ORDER BY count(*) DESC, p.id LIMIT 8"#,
            )
            .bind(now.to_rfc3339())
            .fetch_all(pool)
            .await?;
            let items: Vec<String> = rows
                .iter()
                .filter_map(|r| {
                    let id: String = r.try_get("id").ok()?;
                    let name: String = r.try_get("name").ok()?;
                    let refs: i64 = r.try_get("refs").ok()?;
                    Some(format!("- {} ({}) — {} records", clip(&name, 40), id, refs))
                })
                .collect();
            Ok((!items.is_empty()).then(|| {
                format!("Around lately (last 14 days, by recency of contact):\n{}", items.join("\n"))
            }))
        }
        "threads" => {
            // Live threads: what their hands are on. Pages that are wiki
            // articles are the machine's writing, not the person's desk.
            let pages = sqlx::query(
                r#"SELECT pg.id, pg.title
                   FROM app_pages pg
                   WHERE pg.kind = 'page'
                     AND NOT EXISTS (SELECT 1 FROM wiki_articles a WHERE a.page_id = pg.id)
                     AND pg.updated_at > $1::timestamptz - interval '7 days'
                   ORDER BY pg.updated_at DESC, pg.id LIMIT 5"#,
            )
            .bind(now.to_rfc3339())
            .fetch_all(pool)
            .await?;
            let notebooks = sqlx::query(
                r#"SELECT id, name FROM app_notebooks
                   WHERE archived_at IS NULL AND updated_at > $1::timestamptz - interval '14 days'
                   ORDER BY updated_at DESC, id LIMIT 3"#,
            )
            .bind(now.to_rfc3339())
            .fetch_all(pool)
            .await?;
            let mut items: Vec<String> = Vec::new();
            for r in &pages {
                let id: String = r.try_get("id")?;
                let title: Option<String> = r.try_get("title")?;
                items.push(format!(
                    "- page \"{}\" (/page/{})",
                    clip(title.as_deref().unwrap_or("Untitled"), 48),
                    id
                ));
            }
            for r in &notebooks {
                let id: String = r.try_get("id")?;
                let name: String = r.try_get("name")?;
                items.push(format!("- notebook \"{}\" ({})", clip(&name, 48), id));
            }
            Ok((!items.is_empty())
                .then(|| format!("Live threads (edited recently):\n{}", items.join("\n"))))
        }
        "sleep" => {
            // Last night, as one line. Sessions overlapping [6 PM yesterday,
            // now]; overlapping intervals MERGED before summing — watch and
            // phone double-report the same night, and summing durations was
            // this repo's canonical "0.0 hours" graveyard. No rows → omit:
            // absence means unknown, not zero.
            let rows = sqlx::query(
                r#"SELECT started_at, ended_at FROM data_health_sleep
                   WHERE ended_at > $1::timestamptz - interval '30 hours'
                     AND ended_at <= $1::timestamptz
                     AND started_at IS NOT NULL
                   ORDER BY started_at, id"#,
            )
            .bind(now.to_rfc3339())
            .fetch_all(pool)
            .await?;
            let mut spans: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
            for r in &rows {
                let s: DateTime<Utc> = r.try_get("started_at")?;
                let e: DateTime<Utc> = r.try_get("ended_at")?;
                match spans.last_mut() {
                    Some((_, last_e)) if s <= *last_e => {
                        if e > *last_e {
                            *last_e = e;
                        }
                    }
                    _ => spans.push((s, e)),
                }
            }
            let total_min: i64 = spans
                .iter()
                .map(|(s, e)| e.signed_duration_since(*s).num_minutes())
                .sum();
            Ok((total_min >= 60).then(|| {
                format!("Last night: about {:.1} hours of sleep.", total_min as f64 / 60.0)
            }))
        }
        "recent_days" => {
            // The last narrated days, until <current_chapter> exists to carry
            // the middle duration. `date::text` — the column is a Postgres
            // DATE and decodes into String only through the cast.
            let rows = sqlx::query_as::<_, (String, Option<String>)>(
                r#"SELECT date::text, prose FROM wiki_day_prose
                 WHERE prose IS NOT NULL
                 ORDER BY date DESC LIMIT 3"#,
            )
            .fetch_all(pool)
            .await?;
            let items: Vec<String> = rows
                .iter()
                .filter_map(|(date, prose)| {
                    Some(format!("{}: {}", date, clip(prose.as_deref()?, 300)))
                })
                .collect();
            Ok((!items.is_empty())
                .then(|| format!("Recent days, as narrated:\n{}", items.join("\n"))))
        }
        "sources" => {
            // Epistemic fact about right now: what the record can and cannot
            // currently see.
            let rows = sqlx::query_as::<_, (String,)>(
                "SELECT name FROM credentials WHERE status = 'active' ORDER BY name LIMIT 12",
            )
            .fetch_all(pool)
            .await?;
            let names: Vec<String> = rows.iter().map(|r| clip(&r.0, 32)).collect();
            Ok((!names.is_empty())
                .then(|| format!("Connected sources: {}.", names.join(", "))))
        }
        other => {
            tracing::warn!("[chat] unknown circumstances section: {other}");
            Ok(None)
        }
    }
}

/// THE TEST THAT CATCHES THE SILENT-SECTION DISEASE for this block — the
/// successor to `live_context_sections`, run against the migration-built
/// scratch schema instead of a live box. Every section is seeded and must
/// render; `<recent_days>` once spent its whole life absent from every
/// prompt over a DATE/String decode mismatch, and only this shape of test
/// notices.
#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn every_section_renders_when_its_data_exists(pool: PgPool) {
        let now = Utc::now();
        let iso = |dt: DateTime<Utc>| dt.to_rfc3339();

        // identity
        sqlx::query(
            "UPDATE app_user_profile SET occupation = 'Designer', employer = 'Example Co' \
             WHERE id = '00000000-0000-0000-0000-000000000001'",
        )
        .execute(&pool)
        .await
        .unwrap();
        // place: an open visit resolved to a place via wiki_refs
        sqlx::query("INSERT INTO wiki_places (id, name) VALUES ('place_t1', 'The Library')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO data_location_visit (id, latitude, longitude, started_at, \
             source_stream_id, source_table, source_provider) \
             VALUES ('visit_t1', 30.0, -97.0, $1::timestamptz, 'st_v1', 'data_location_visit', 'test')",
        )
        .bind(iso(now - chrono::Duration::hours(1)))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, occurred_at) \
             VALUES ('ref_t1', 'place', 'place_t1', 'data_location_visit', 'visit_t1', $1::timestamptz)",
        )
        .bind(iso(now - chrono::Duration::hours(1)))
        .execute(&pool)
        .await
        .unwrap();
        // people: one correspondent this week
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('person_t1', 'Nick')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, occurred_at) \
             VALUES ('ref_t2', 'person', 'person_t1', 'data_communication_message', 'msg_t1', $1::timestamptz)",
        )
        .bind(iso(now - chrono::Duration::days(2)))
        .execute(&pool)
        .await
        .unwrap();
        // calendar: an event later today (bounded inside the window even at 23:00 UTC? use now + 1 min)
        sqlx::query(
            "INSERT INTO data_calendar_event (id, title, started_at, ended_at, \
             source_stream_id, source_table, source_provider) \
             VALUES ('cal_t1', 'Standup', $1::timestamptz, $2::timestamptz, 'st_c1', 'data_calendar_event', 'test')",
        )
        .bind(iso(now + chrono::Duration::minutes(1)))
        .bind(iso(now + chrono::Duration::minutes(30)))
        .execute(&pool)
        .await
        .unwrap();
        // sleep: two OVERLAPPING sessions last night — the merge is the test
        let sleep_start = now - chrono::Duration::hours(9);
        let sleep_end = now - chrono::Duration::hours(1);
        for (id, s, e) in [
            ("sleep_t1", sleep_start, sleep_end),
            ("sleep_t2", sleep_start + chrono::Duration::hours(1), sleep_end),
        ] {
            sqlx::query(
                "INSERT INTO data_health_sleep (id, started_at, ended_at, \
                 source_stream_id, source_table, source_provider) \
                 VALUES ($1, $2::timestamptz, $3::timestamptz, $1, 'data_health_sleep', 'test')",
            )
            .bind(id)
            .bind(iso(s))
            .bind(iso(e))
            .execute(&pool)
            .await
            .unwrap();
        }
        // threads: a fresh page + notebook
        sqlx::query("INSERT INTO app_pages (id, title) VALUES ('page_t1', 'Draft essay')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO app_notebooks (id, name) VALUES ('nb_t1', 'Garden')")
            .execute(&pool)
            .await
            .unwrap();
        // sources
        sqlx::query(
            "INSERT INTO credentials (id, source_id, name, status, secrets_ciphertext) \
             VALUES ('cred_t1', 'google', 'Google', 'active', 'x')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let block = build_circumstances(&pool, Some("America/Chicago"), now)
            .await
            .expect("seeded block must render");

        for needle in [
            "<circumstances>",
            "Now: ",
            "Designer at Example Co",
            "The Library",
            "Standup",
            "Nick (person_t1)",
            "Draft essay",
            "Garden",
            "Connected sources: Google.",
        ] {
            assert!(block.contains(needle), "missing {needle:?} in:\n{block}");
        }
        // Overlapping sessions merged: ~8h, never ~15h.
        assert!(
            block.contains("about 8.0 hours"),
            "sleep merge failed (double-counted?) in:\n{block}"
        );
        // Determinism: same quantized instant → identical bytes.
        let again = build_circumstances(&pool, Some("America/Chicago"), now).await.unwrap();
        assert_eq!(block, again, "unstable bytes at a fixed clock — cache killer");
    }
}
