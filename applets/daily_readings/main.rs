//! daily_readings — the day's lectionary readings, cached on the box.
//!
//! Fetches from the USCCB daily-readings pages and stores them in
//! `applet_daily_readings.readings`. Runs ahead rather than on demand: the
//! lectionary is deterministic, so a fortnight is knowable in advance, and
//! fetching in a batch means the examen never waits on someone else's server
//! and a bad morning for bible.usccb.org is not a bad morning for the applet.
//!
//! It is deliberately its OWN applet rather than a phase of the examen. A
//! subprocess that fails takes the whole run down with it, so folding the
//! fetch into the examen would mean an outage at USCCB costs you the
//! reflection too — and the reflection needs no network at all.
//!
//! The source is HTML, not an API. That is a real fragility and it is handled
//! by degrading rather than failing: a day that will not parse is skipped and
//! logged, the rest of the batch still lands, and a citation without a body is
//! stored as a partial rather than thrown away.

use anyhow::Result;
use chrono::{Datelike, Duration, NaiveDate, Utc};
use sqlx::PgPool;

use virtues_applets::http_client;
use virtues_helpers::{connect_from_env, output, read_input};

/// How far ahead to keep the cache stocked. Two weeks is enough that a week of
/// outages never reaches the examen, and short enough that a lectionary
/// correction upstream is picked up in reasonable time.
const HORIZON_DAYS: i64 = 14;

/// How many pages to fetch in one run, and how long to wait between them.
///
/// Learned the hard way: asking for all fourteen back-to-back got this box's
/// IP 403'd within a second, and the block outlasted the run — plain curl was
/// still refused afterwards. A burst reads as scraping because it is one. So
/// the horizon fills over several mornings instead, a few days at a time, and
/// then costs one or two requests a day to maintain. Nothing is waiting on it.
const MAX_PER_RUN: usize = 3;
const PAUSE_BETWEEN: std::time::Duration = std::time::Duration::from_secs(3);

/// Identify the client honestly. An unattributed burst is indistinguishable
/// from a scraper; a named one can at least be asked to stop.
const USER_AGENT: &str =
    "virtues-box/0.3 (personal appliance; daily readings for one household)";

/// Per-day page. The path is MMDDYY.
const BASE: &str = "https://bible.usccb.org/bible/readings";

/// One reading slot as the page presents it.
struct Reading {
    slot: String,
    order: i32,
    citation: Option<String>,
    body: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();
    let input = read_input()?;
    let pool = connect_from_env("virtues-applet-daily_readings").await?;

    let today = Utc::now().date_naive();
    let mut fetched = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    for offset in 0..HORIZON_DAYS {
        if fetched + failed >= MAX_PER_RUN {
            break;
        }
        let day = today + Duration::days(offset);
        if have_day(&pool, day).await? {
            skipped += 1;
            continue;
        }
        // Space the requests out. The first fetch of a run goes immediately;
        // the rest wait, because the burst is what gets you blocked.
        if fetched + failed > 0 {
            tokio::time::sleep(PAUSE_BETWEEN).await;
        }
        match fetch_day(day).await {
            Ok(readings) if !readings.is_empty() => {
                store_day(&pool, day, &readings).await?;
                fetched += 1;
            }
            // A day that returns nothing parseable is not an error worth
            // failing the run over — the page may be a feast with a layout we
            // do not read, and tomorrow's fetch will try again.
            Ok(_) => {
                tracing::warn!(%day, "no readings parsed; leaving the day uncached");
                failed += 1;
            }
            Err(e) => {
                tracing::warn!(%day, error = %e, "fetch failed; leaving the day uncached");
                failed += 1;
            }
        }
    }

    // Old days are dropped: this is a cache of what is coming, not an archive.
    // Anything worth keeping was copied into a journal page on the day.
    let pruned = sqlx::query("DELETE FROM applet_daily_readings.readings WHERE day < $1")
        .bind(today - Duration::days(30))
        .execute(&pool)
        .await?
        .rows_affected();

    output(
        &format!(
            "daily_readings: {fetched} fetched, {skipped} already cached, \
             {failed} unavailable, {pruned} pruned (up to {MAX_PER_RUN} a run, \
             so the fortnight fills over several days)"
        ),
        &input.config,
    )
}

async fn have_day(pool: &PgPool, day: NaiveDate) -> Result<bool> {
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM applet_daily_readings.readings WHERE day = $1 AND body IS NOT NULL",
    )
    .bind(day)
    .fetch_one(pool)
    .await?;
    Ok(n > 0)
}

async fn store_day(pool: &PgPool, day: NaiveDate, readings: &[Reading]) -> Result<()> {
    let lectionary: Option<String> = None;
    for r in readings {
        sqlx::query(
            "INSERT INTO applet_daily_readings.readings \
                 (day, slot, slot_order, citation, body, lectionary) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (day, slot) DO UPDATE SET \
                 citation = EXCLUDED.citation, \
                 body = COALESCE(EXCLUDED.body, applet_daily_readings.readings.body), \
                 slot_order = EXCLUDED.slot_order, \
                 fetched_at = now()",
        )
        .bind(day)
        .bind(&r.slot)
        .bind(r.order)
        .bind(r.citation.as_deref())
        .bind(r.body.as_deref())
        .bind(lectionary.as_deref())
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn fetch_day(day: NaiveDate) -> Result<Vec<Reading>> {
    let url = format!(
        "{BASE}/{:02}{:02}{:02}.cfm",
        day.month(),
        day.day(),
        day.year() % 100
    );
    let html = http_client()
        .get(&url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(parse(&html))
}

/// Pull the reading slots out of the page.
///
/// Hand-rolled rather than pulling in an HTML parser: the shape needed is one
/// repeated `<h3>slot</h3> … <div class="address">citation</div> …
/// <div class="content-body">body</div>`, and a scraper crate would be a large
/// dependency for one page whose structure we would still have to know.
fn parse(html: &str) -> Vec<Reading> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    let mut order = 0i32;

    while let Some(rel) = html[cursor..].find("<h3") {
        let h3 = cursor + rel;
        let Some(slot) = tag_text(html, h3, "h3") else {
            cursor = h3 + 3;
            continue;
        };
        // The next block after this heading, but not past the following one —
        // otherwise a heading with no readings swallows the next slot's text.
        let next_h3 = html[h3 + 3..]
            .find("<h3")
            .map(|r| h3 + 3 + r)
            .unwrap_or(html.len());
        let window = &html[h3..next_h3];

        let citation = class_text(window, "address");
        let body = class_text(window, "content-body");

        if citation.is_some() || body.is_some() {
            order += 1;
            out.push(Reading {
                slot,
                order,
                citation,
                body,
            });
        }
        cursor = next_h3;
    }
    out
}

/// Text content of the first `<tag …>…</tag>` at or after `from`.
fn tag_text(html: &str, from: usize, tag: &str) -> Option<String> {
    let open_end = html[from..].find('>')? + from + 1;
    let close = html[open_end..].find(&format!("</{tag}"))? + open_end;
    let text = strip_tags(&html[open_end..close]);
    (!text.is_empty()).then_some(text)
}

/// Text content of the first `class="…<name>…"` div in `window`.
fn class_text(window: &str, name: &str) -> Option<String> {
    let at = window.find(&format!("class=\"{name}\""))?;
    let open_end = window[at..].find('>')? + at + 1;
    // Divs nest, so count them rather than taking the first `</div>`.
    let mut depth = 1i32;
    let mut i = open_end;
    let bytes = window.as_bytes();
    while i < window.len() {
        if bytes[i] == b'<' {
            if window[i..].starts_with("</div") {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            } else if window[i..].starts_with("<div") {
                depth += 1;
            }
        }
        i += 1;
    }
    let text = strip_tags(&window[open_end..i.min(window.len())]);
    (!text.is_empty()).then_some(text)
}

/// Drop tags, decode the handful of entities this page actually uses, and
/// collapse whitespace into readable paragraphs.
fn strip_tags(fragment: &str) -> String {
    let mut out = String::with_capacity(fragment.len());
    let mut in_tag = false;
    for c in fragment.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&#160;", " ")
        .replace("&rsquo;", "\u{2019}")
        .replace("&lsquo;", "\u{2018}")
        .replace("&rdquo;", "\u{201D}")
        .replace("&ldquo;", "\u{201C}")
        .replace("&mdash;", "\u{2014}")
        .replace("&ndash;", "\u{2013}")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r##"
        <div><h2>Ignore me</h2></div>
        <h3>Reading 1</h3>
        <div class="address"><a href="#">Nahum 2:1, 3; 3:1-3, 6-7</a></div>
        <div class="content-body"><p>Thus says the Lord&rsquo;s prophet.</p></div>
        <h3>Responsorial Psalm</h3>
        <div class="address">Deuteronomy 32:35cd-36ab</div>
        <div class="content-body"><p>R. <div>It is I who deal death</div> and give life.</p></div>
        <h3>Gospel</h3>
        <div class="address">Matthew 16:24-28</div>
        <div class="content-body"><p>Whoever wishes to come after me.</p></div>
    "##;

    #[test]
    fn every_slot_comes_back_with_its_citation_and_body() {
        let r = parse(PAGE);
        assert_eq!(r.len(), 3, "three slots on this page");
        assert_eq!(r[0].slot, "Reading 1");
        assert_eq!(r[0].citation.as_deref(), Some("Nahum 2:1, 3; 3:1-3, 6-7"));
        assert_eq!(r[2].slot, "Gospel");
        assert_eq!(r[2].citation.as_deref(), Some("Matthew 16:24-28"));
        assert_eq!(r[2].order, 3, "order is the page's order");
    }

    /// Entities have to be decoded, or the reading arrives full of `&rsquo;`.
    #[test]
    fn entities_are_decoded() {
        let r = parse(PAGE);
        assert_eq!(
            r[0].body.as_deref(),
            Some("Thus says the Lord\u{2019}s prophet.")
        );
    }

    /// Divs nest inside the psalm response. Taking the first `</div>` would
    /// truncate the reading mid-sentence, which is the kind of thing nobody
    /// notices until they read it in the morning.
    #[test]
    fn a_nested_div_does_not_truncate_the_body(){
        let r = parse(PAGE);
        let psalm = r[1].body.as_deref().unwrap();
        assert!(psalm.contains("give life"), "body was cut short: {psalm}");
    }

    /// A heading with nothing under it must not swallow the next slot.
    #[test]
    fn an_empty_heading_is_skipped_not_merged() {
        let page = "<h3>Nothing Here</h3><h3>Gospel</h3>\
                    <div class=\"address\">Mark 1:1</div>";
        let r = parse(page);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].slot, "Gospel");
        assert_eq!(r[0].citation.as_deref(), Some("Mark 1:1"));
    }
}
