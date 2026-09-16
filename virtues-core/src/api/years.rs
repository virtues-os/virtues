//! The year as a subject.
//!
//! A year is a narrative article, in the sense a wikipedia gives "2026" — not
//! a folder of days. See `agents/record/article-resolution.md`.
//!
//! **Materialized lazily.** Every year of a life has a page from the reader's
//! side, but the rows are not written ahead of time. A forty-five-year-old
//! would otherwise get forty-five empty `app_pages` on first boot, each one an
//! article the search index would happily return, and every one of them
//! written without being asked for. So the partition is computed by range —
//! the same trick the day rung uses for its holes — and a row appears the first
//! time somebody opens, edits, or drafts that year.

use crate::error::{Error, Result};
use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use sqlx::PgPool;

/// One year in the life, as the index and the page need it.
#[derive(Debug, Clone, Serialize)]
pub struct YearSummary {
    pub id: String,
    pub year: i32,
    /// Theirs, optional — "the year of the shop".
    pub title: Option<String>,
    /// Theirs, verbatim. Same idiom as `wiki_chapters.summary`.
    pub summary: Option<String>,
    pub days_recorded: i64,
    pub days_narrated: i64,
    pub has_article: bool,
    /// Which chapters this year falls inside, in their words.
    pub chapters: Vec<String>,
}

/// A day of the year, with the one line the year reads.
#[derive(Debug, Clone, Serialize)]
pub struct YearDay {
    pub date: NaiveDate,
    pub narrated: bool,
    pub event_count: i64,
    /// The day article's opening paragraph.
    pub lede: Option<String>,
}

/// The page.
#[derive(Debug, Clone, Serialize)]
pub struct YearPage {
    #[serde(flatten)]
    pub summary: YearSummary,
    pub days: Vec<YearDay>,
    /// The article prose, when one has been written.
    pub article: Option<String>,
    /// Which of the three states this page is in, so the client does not have
    /// to re-derive it: `before_record`, `thin`, or `dense`.
    pub state: &'static str,
}

pub fn year_id(year: i32) -> String {
    format!("year_{year}")
}

/// The span of years a life covers.
///
/// Birth to now, when a birth date is known — that is the whole life, and a
/// year before the record is still a year of it. Without one, the span starts
/// at the earliest thing the box knows about (a chapter the person named, or a
/// day it recorded) rather than guessing, and widens on its own if a birth
/// date arrives later.
pub async fn life_span(pool: &PgPool) -> Result<(i32, i32)> {
    let now = chrono::Utc::now().year();
    let earliest: Option<NaiveDate> = sqlx::query_scalar(
        "SELECT LEAST( \
             (SELECT min(birth_date) FROM app_user_profile), \
             (SELECT min(started_at) FROM wiki_chapters), \
             (SELECT min(date) FROM wiki_days) \
         )",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to read the life's span: {e}")))?;
    Ok((earliest.map(|d| d.year()).unwrap_or(now), now))
}

/// Make sure a year's row exists, and return its id.
///
/// The lazy half of the materialization. Called by anything that opens, edits
/// or drafts a year — never by a sweep.
pub async fn ensure_year(pool: &PgPool, year: i32) -> Result<String> {
    let id = year_id(year);
    sqlx::query("INSERT INTO wiki_years (id, year) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING")
        .bind(&id)
        .bind(year)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to create the year: {e}")))?;
    Ok(id)
}

/// The chapters a year falls inside, in the person's words.
///
/// A range lookup, never a stored key: chapters are authored and their
/// boundaries move, and a foreign key would need a backfill on every edit.
async fn chapters_for(pool: &PgPool, year: i32) -> Result<Vec<String>> {
    let titles: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT title FROM wiki_chapters \
         WHERE started_at <= make_date($1, 12, 31) \
           AND (ended_at IS NULL OR ended_at >= make_date($1, 1, 1)) \
         ORDER BY started_at",
    )
    .bind(year)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to read the chapters: {e}")))?;
    Ok(titles.into_iter().flatten().collect())
}

/// Every year of the life, newest first. Derived — nothing is written here.
pub async fn list_years(pool: &PgPool) -> Result<Vec<YearSummary>> {
    let (from, to) = life_span(pool).await?;
    let mut out = Vec::new();
    for year in (from..=to).rev() {
        out.push(read_summary(pool, year).await?);
    }
    Ok(out)
}

async fn read_summary(pool: &PgPool, year: i32) -> Result<YearSummary> {
    let id = year_id(year);
    let row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT title, summary FROM wiki_years WHERE id = $1")
            .bind(&id)
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to read the year: {e}")))?;
    let (title, summary) = row.unwrap_or((None, None));

    let (recorded, narrated): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE narrated_at IS NOT NULL) \
         FROM wiki_days WHERE EXTRACT(YEAR FROM date) = $1",
    )
    .bind(year)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to count the days: {e}")))?;

    let has_article: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM wiki_articles WHERE subject_type = 'year' AND subject_id = $1)",
    )
    .bind(&id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to look for the article: {e}")))?;

    Ok(YearSummary {
        id,
        year,
        title,
        summary,
        days_recorded: recorded,
        days_narrated: narrated,
        has_article,
        chapters: chapters_for(pool, year).await?,
    })
}

/// The days of a year, each with its lede.
pub async fn days_of(pool: &PgPool, year: i32) -> Result<Vec<YearDay>> {
    use sqlx::Row;
    let rows = sqlx::query(&format!(
        r#"
        SELECT d.date,
               (d.narrated_at IS NOT NULL) AS narrated,
               (SELECT count(*) FROM wiki_events e
                 WHERE e.day_id = d.id AND e.user_hidden = false) AS event_count,
               {lede} AS lede
        FROM wiki_days d
        LEFT JOIN wiki_day_prose dp ON dp.day_id = d.id
        WHERE EXTRACT(YEAR FROM d.date) = $1
        ORDER BY d.date
        "#,
        lede = crate::api::wiki_editor::lede_sql("dp.prose")
    ))
    .bind(year)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to read the days: {e}")))?;

    rows.into_iter()
        .map(|r| {
            Ok(YearDay {
                date: r
                    .try_get("date")
                    .map_err(|e| Error::Database(format!("Failed to decode a date: {e}")))?,
                narrated: r
                    .try_get("narrated")
                    .map_err(|e| Error::Database(format!("Failed to decode narrated: {e}")))?,
                event_count: r
                    .try_get("event_count")
                    .map_err(|e| Error::Database(format!("Failed to decode a count: {e}")))?,
                lede: r
                    .try_get("lede")
                    .map_err(|e| Error::Database(format!("Failed to decode a lede: {e}")))?,
            })
        })
        .collect()
}

/// One year's page. Creates the row on the way, which is the lazy half.
pub async fn get_year(pool: &PgPool, year: i32) -> Result<YearPage> {
    let (from, to) = life_span(pool).await?;
    if year < from || year > to {
        return Err(Error::NotFound(format!(
            "{year} is outside the years this record covers ({from}–{to})"
        )));
    }
    ensure_year(pool, year).await?;
    let summary = read_summary(pool, year).await?;
    let days = days_of(pool, year).await?;
    let article = crate::api::wiki_articles::get_article_prose(pool, "year", &summary.id)
        .await
        .ok()
        .flatten()
        .map(|p| p.content);

    // The three states, decided here rather than in the client, because what
    // the page offers to DO depends on which one it is in — a year from before
    // the record is not offered a draft, because an article invented from an
    // empty record is the worst thing that page could hold.
    let state = if summary.days_recorded == 0 {
        "before_record"
    } else if summary.days_narrated == 0 || article.is_none() {
        "thin"
    } else {
        "dense"
    };

    Ok(YearPage {
        summary,
        days,
        article,
        state,
    })
}

/// Set what only the person can say about a year.
pub async fn update_year(
    pool: &PgPool,
    year: i32,
    title: Option<&str>,
    summary: Option<&str>,
) -> Result<()> {
    let id = ensure_year(pool, year).await?;
    sqlx::query(
        "UPDATE wiki_years \
         SET title = COALESCE($2, title), summary = COALESCE($3, summary), updated_at = now() \
         WHERE id = $1",
    )
    .bind(&id)
    .bind(title)
    .bind(summary)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update the year: {e}")))?;
    Ok(())
}

/// Write a year's first article.
///
/// One call with everything in the prompt, the same split the day uses: a
/// first draft has no existing text to respect and nothing to go looking for.
/// Revision is the agent's job, and it is a different shape entirely.
///
/// A year with no narrated day is refused rather than drafted. There is
/// nothing to write from, and prose invented over an empty record is worse on
/// this page than silence — it would look exactly like a year the box knew
/// something about.
pub async fn write_year_article(pool: &PgPool, year: i32) -> Result<String> {
    let page = get_year(pool, year).await?;
    if page.summary.days_narrated == 0 {
        return Err(Error::InvalidInput(format!(
            "{year} has no narrated day to write from"
        )));
    }
    if page.article.is_some() {
        return Err(Error::InvalidInput(format!(
            "{year} already has an article — it is maintained by editing now"
        )));
    }

    let mut p = String::new();
    p.push_str(&format!("YEAR {year}\n"));
    if !page.summary.chapters.is_empty() {
        p.push_str(&format!(
            "The chapters of the owner's life this year falls inside, named by \
             them: {}\n",
            page.summary.chapters.join("; ")
        ));
    }

    // Their words first and marked, because the constitution turns on the
    // distinction: what is here is placed, never paraphrased.
    let has_authored = page.summary.title.is_some() || page.summary.summary.is_some();
    if has_authored {
        // The owner is "you" in the article. This block describes them to
        // you in the third person because it is a briefing — say so, or the
        // briefing's voice becomes the article's.
        p.push_str(
            "\n## THE OWNER'S OWN WORDS — place these, never reword them\n\
             (The article addresses the owner as \"you\", as every page does.)\n",
        );
        if let Some(t) = &page.summary.title {
            p.push_str(&format!("- The name they gave this year: {t}\n"));
        }
        if let Some(sm) = &page.summary.summary {
            p.push_str(&format!("- What they say the year was: {sm}\n"));
        }
    }

    p.push_str("\n## THE DAYS — each one's opening line\n");
    let mut linkable = Vec::new();
    for d in page.days.iter().filter(|d| d.narrated) {
        let label = d.date.format("%-d %B").to_string();
        if let Some(lede) = &d.lede {
            p.push_str(&format!("- {} — {lede}\n", d.date));
            linkable.push(format!("- [{label}](/day/day_{})", d.date));
        }
    }

    if !linkable.is_empty() {
        p.push_str(
            "\n## DAYS YOU MAY LINK (copy the exact markdown link, once each)\n",
        );
        p.push_str(&linkable.join("\n"));
        p.push('\n');
    }

    let system = crate::api::wiki_editor::system_prompt("year", &load_rules(pool).await)?;
    // The Chat slot, as the day's narration uses: this is prose a person reads
    // on their own wiki, not a background summary.
    let article = crate::virtues_api::completion::system_completion(
        pool,
        virtues_registry::models::ModelSlot::Chat,
        "year_article",
        &system,
        &p,
        crate::virtues_api::request::Thinking::Off,
        0.4,
    )
    .await?;

    let id = ensure_year(pool, year).await?;
    let title = page
        .summary
        .title
        .clone()
        .unwrap_or_else(|| year.to_string());
    let created =
        crate::api::wiki_articles::create_article(pool, "year", &id, &title, &article).await?;
    crate::api::wiki_editor::record_edition(pool, &created.id, &article).await?;
    Ok(article)
}

/// The person's standing rules, which ride in every editor prompt.
async fn load_rules(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar::<_, String>("SELECT rule FROM wiki_rules WHERE active")
        .fetch_all(pool)
        .await
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn the_span_starts_at_the_earliest_thing_known_and_widens_for_a_birth_date(
        pool: PgPool,
    ) {
        let now = chrono::Utc::now().year();
        // Nothing known: the span is this year alone, rather than a guess.
        assert_eq!(life_span(&pool).await.unwrap(), (now, now));

        sqlx::query("INSERT INTO wiki_days (id, date) VALUES ('day_2024-03-03', '2024-03-03')")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(life_span(&pool).await.unwrap().0, 2024);

        // A chapter they named reaches further back than the record does.
        sqlx::query(
            "INSERT INTO wiki_chapters (id, title, started_at) VALUES ('chapter_1', 'School', '2009-06-10')",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(life_span(&pool).await.unwrap().0, 2009);
    }

    #[sqlx::test]
    async fn a_year_is_created_on_first_read_and_not_before(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_days (id, date) VALUES ('day_2026-01-02', '2026-01-02')")
            .execute(&pool)
            .await
            .unwrap();
        let before: i64 = sqlx::query_scalar("SELECT count(*) FROM wiki_years")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(before, 0, "listing a life must not write forty rows");

        let _ = list_years(&pool).await.unwrap();
        let after_list: i64 = sqlx::query_scalar("SELECT count(*) FROM wiki_years")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after_list, 0, "and neither must reading the index");

        let page = get_year(&pool, 2026).await.unwrap();
        assert_eq!(page.summary.year, 2026);
        let after_open: i64 = sqlx::query_scalar("SELECT count(*) FROM wiki_years")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after_open, 1, "opening one year creates one row");
    }

    #[sqlx::test]
    async fn a_year_before_the_record_is_never_offered_a_draft(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_days (id, date) VALUES ('day_2026-01-02', '2026-01-02')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO wiki_chapters (id, title, started_at) VALUES ('chapter_1', 'Before', '2019-01-01')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let empty = get_year(&pool, 2020).await.unwrap();
        assert_eq!(
            empty.state, "before_record",
            "no days at all — an article invented from nothing is the worst thing \
             this page could hold"
        );
        assert!(empty.days.is_empty());
        assert_eq!(empty.summary.chapters, vec!["Before".to_string()]);

        let thin = get_year(&pool, 2026).await.unwrap();
        assert_eq!(thin.state, "thin", "a day on record, but nothing written");
        assert_eq!(thin.days.len(), 1);
    }

    #[sqlx::test]
    async fn a_year_outside_the_life_is_not_a_page(pool: PgPool) {
        let err = get_year(&pool, 1850).await.unwrap_err().to_string();
        assert!(err.contains("outside the years"), "{err}");
    }
}
