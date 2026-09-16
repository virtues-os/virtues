//! wiki_editor: the record's contributor.
//!
//! Article resolution's scheduler half. See
//! `agents/record/article-resolution.md`.
//!
//! This subprocess does not write prose. It answers one question — which
//! article, if any, should be revised on this run — and hands that to the
//! AGENT phase, which holds the live document layer and does the writing. The
//! split is not stylistic: an applet subprocess has a bare pool and no
//! `YjsState`, so an article edit made here would be silently discarded by the
//! next CRDT save. That is why `refresh_due_entity_articles` could never be
//! implemented where it lived, and it is the reason this binary stops short.
//!
//! The gate it owns is DRIFT. `due_articles` answers "has this rested long
//! enough", which is cheap; whether the article's inputs actually moved needs
//! the dossier built, which costs a query per candidate. An article whose
//! evidence has not changed is not revised — it records its fingerprint and
//! waits, which is what keeps an hourly schedule from being an hourly model
//! call.

use anyhow::Result;
use virtues_helpers::{output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let input = read_input()?;
    let pool = virtues_helpers::connect_from_env("virtues-action-wiki_editor").await?;

    let due = virtues::api::wiki_editor::due_articles(
        &pool,
        virtues::api::wiki_editor::MAX_PER_RUN,
    )
    .await?;

    for article in due {
        // Only kinds with a brief. A subject whose brief does not exist yet is
        // not handed the generic door — the day is the case that matters, and
        // its narrator is released and tuned.
        if virtues::api::wiki_editor::brief_for(&article.subject_type).is_none() {
            continue;
        }

        let fingerprint =
            virtues::api::wiki_editor::evidence_fingerprint(&pool, &article).await?;
        if article.input_fingerprint.as_deref() == Some(fingerprint.as_str()) {
            // Rested long enough, but nothing beneath it moved. Record the pass
            // so the interval restarts, and spend nothing.
            virtues::api::wiki_editor::record_pass(&pool, &article.id, &fingerprint, None)
                .await?;
            continue;
        }

        // The fingerprint is stored BEFORE the agent runs, not after. If the
        // revision fails — a refused edit, a model error, a restart — this
        // article must not come back next hour to fail the same way and burn a
        // call an hour until its interval elapses. The agent's own write path
        // records the edition when it succeeds.
        virtues::api::wiki_editor::record_pass(&pool, &article.id, &fingerprint, None).await?;

        // The summary IS the agent phase's brief for this run: it names the
        // one article to revise, in the vocabulary the revise_article tool
        // takes, so the agent never has to guess what it was woken for.
        // The current text goes in the hand-over, not just the ids. The agent
        // has no page id and no way to guess one, and the first real run spent
        // its opening turns hunting for the page before giving up and editing
        // it through the wrong door.
        let current: String = sqlx::query_scalar(
            "SELECT coalesce(content, '') FROM app_pages WHERE id = $1",
        )
        .bind(&article.page_id)
        .fetch_one(&pool)
        .await?;

        // WHAT CHANGED, not just that something did. The first year revision
        // spent its whole budget in ten tool calls rediscovering the days it
        // could have been handed, and wrote nothing. Research should be the
        // agent's option, not its only way to find out why it was woken.
        let changed = changed_since(&pool, &article).await?;

        // A FIRST WRITE is not a revision, and the difference decides the run.
        // `machine_text` is NULL when the record has never written this article
        // — a story seeded with the owner's one sentence, say. Told to
        // "revise", the editor compares the evidence against that sentence,
        // finds it already said, and correctly declines: the restraint rule
        // firing on a page that has never been written at all.
        let never_written = article.machine_text.is_none();
        let task = if never_written {
            "WRITE this article for the first time. The record has never written it: \
             what follows is the owner's own seeding text, which is theirs to keep \
             and not an article. Research the subject and write the piece."
        } else {
            "REVISE this article. Change what the new evidence actually changes and \
             leave every sentence that is still true exactly as it is."
        };

        output(
            &format!(
                "{task}\n\nsubject_type={} subject_id={}\n\n\
                 WHAT THE RECORD HAS:\n{changed}\n\n\
                 THE PAGE AS IT STANDS — pass the whole finished text to \
                 `revise_article`:\n\n{current}",
                article.subject_type, article.subject_id
            ),
            &input.config,
        )?;
        return Ok(());
    }

    output("No article is due for revision.", &input.config)?;
    Ok(())
}

/// What is new beneath a subject since its article was last written.
///
/// Each rung rests on a different thing, so each is handed a different thing.
/// A year rests on its days, and the days written up ARE the change, at one
/// query. A chapter rests on the years inside it, not their days. An entity
/// rests on records that reference it, and those are better searched than
/// dumped, so it gets a count and goes looking if it wants detail. A story
/// rests on nothing at all, which is the rung's whole point.
async fn changed_since(
    pool: &sqlx::PgPool,
    article: &virtues::api::wiki_editor::DueArticle,
) -> Result<String> {
    if article.subject_type == "story" {
        // A story has nothing beneath it to hand over — that is the rung's whole
        // point. Saying "new records reference this" would be a lie: nothing
        // references a story.
        return Ok("Nothing is gathered for a story. Its material is wherever the \
                   record happens to keep it, so searching IS the work here."
            .to_string());
    }
    if article.subject_type == "chapter" {
        return chapter_handover(pool, &article.subject_id).await;
    }
    if article.subject_type != "year" {
        return Ok("New records reference this subject. Search for them.".to_string());
    }
    let Some(year) = article
        .subject_id
        .strip_prefix("year_")
        .and_then(|y| y.parse::<i32>().ok())
    else {
        return Ok(String::new());
    };

    let days = virtues::api::years::days_of(pool, year).await?;
    let lines: Vec<String> = days
        .iter()
        .filter(|d| d.narrated && d.lede.is_some())
        .map(|d| format!("- {} — {}", d.date, d.lede.clone().unwrap_or_default()))
        .collect();
    if lines.is_empty() {
        return Ok("Nothing is written up for this year yet.".to_string());
    }
    Ok(format!(
        "Every written-up day of {year}, with its opening line. Compare them \
         against the article: what is here and not there is what you are for.\n{}",
        lines.join("\n")
    ))
}

/// A chapter's era, handed over one year at a time.
///
/// **Years, not days.** A chapter can span a decade, and its days would be
/// four thousand lines of hand-over for a page whose whole job is to say what
/// a year cannot. The level directly beneath a chapter is the year, so that is
/// the level it is given.
///
/// Their own three fields ride along. The title, summary and changepoint are
/// the spine of a chapter article, and an agent that has to go looking for
/// them spends its budget re-finding what the caller already had — the same
/// mistake the first year revision made.
async fn chapter_handover(pool: &sqlx::PgPool, chapter_id: &str) -> Result<String> {
    use sqlx::Row;

    let ch = sqlx::query(
        "SELECT title, summary, changepoint, started_at, ended_at, kind \
         FROM wiki_chapters WHERE id = $1",
    )
    .bind(chapter_id)
    .fetch_optional(pool)
    .await?;
    let Some(ch) = ch else {
        return Ok(String::new());
    };

    let title: Option<String> = ch.try_get("title")?;
    let summary: Option<String> = ch.try_get("summary")?;
    let changepoint: Option<String> = ch.try_get("changepoint")?;
    let started_at: chrono::NaiveDate = ch.try_get("started_at")?;
    let ended_at: Option<chrono::NaiveDate> = ch.try_get("ended_at")?;
    let kind: String = ch.try_get("kind")?;

    let mut out = String::new();
    out.push_str("THEIRS — the era as they drew it. These are not yours to revise:\n");
    match (&title, kind.as_str()) {
        (Some(t), _) => out.push_str(&format!("- they call it: {t}\n")),
        (None, "unknown") => out.push_str(
            "- they left this stretch unnamed, and that is an answer. Do not \
             supply the name they withheld.\n",
        ),
        _ => {}
    }
    out.push_str(&match ended_at {
        Some(e) => format!("- {started_at} to {e}\n"),
        None => format!("- {started_at} to now; this era has not ended\n"),
    });
    if let Some(s) = summary.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!("- their sentence about it: {}\n", s.trim()));
    }
    if let Some(c) = changepoint.as_deref().filter(|c| !c.trim().is_empty()) {
        out.push_str(&format!("- what ended it, in their words: {}\n", c.trim()));
    }

    // One row per year of the era: how much of it is written up, and the year
    // article's own opening line where one exists. `ended_at IS NULL` is the
    // running chapter and means "through today".
    let rows = sqlx::query(&format!(
        r#"
        WITH span AS (SELECT started_at, ended_at FROM wiki_chapters WHERE id = $1),
             yrs AS (
               SELECT EXTRACT(YEAR FROM d.date)::int AS y,
                      count(*) FILTER (WHERE d.narrated_at IS NOT NULL) AS narrated
               FROM wiki_days d, span s
               WHERE d.date >= s.started_at
                 AND (s.ended_at IS NULL OR d.date < s.ended_at)
               GROUP BY 1
             )
        SELECT yrs.y, yrs.narrated, wy.title AS year_title, {lede} AS lede
        FROM yrs
        LEFT JOIN wiki_years wy ON wy.id = 'year_' || yrs.y
        LEFT JOIN wiki_articles a
               ON a.subject_type = 'year' AND a.subject_id = 'year_' || yrs.y
        LEFT JOIN app_pages p ON p.id = a.page_id
        ORDER BY yrs.y
        "#,
        lede = virtues::api::wiki::lede_sql("p.content")
    ))
    .bind(chapter_id)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        out.push_str(
            "\nTHE RECORD HAS NOTHING INSIDE THIS ERA. It happened before the box \
             existed, or nothing was collected. Their fields above are the whole \
             article; do not invent the rest of it.",
        );
        return Ok(out);
    }

    out.push_str(
        "\nTHE YEARS INSIDE IT, each with what is written up and the year \
         article's opening line. Start here, then go looking for what runs \
         ACROSS them — a thread confined to one of these years belongs on that \
         year's page, not this one:\n",
    );
    for r in &rows {
        let y: i32 = r.try_get("y")?;
        let narrated: i64 = r.try_get("narrated")?;
        let year_title: Option<String> = r.try_get("year_title")?;
        let lede: Option<String> = r.try_get("lede")?;
        out.push_str(&format!("- {y}"));
        if let Some(t) = year_title.as_deref().filter(|t| !t.trim().is_empty()) {
            out.push_str(&format!(" \"{}\"", t.trim()));
        }
        out.push_str(&format!(" — {narrated} days written up"));
        match lede.as_deref().filter(|l| !l.trim().is_empty()) {
            Some(l) => out.push_str(&format!("; the year's article opens: {}\n", l.trim())),
            None => out.push_str("; no year article yet\n"),
        }
    }
    Ok(out)
}
