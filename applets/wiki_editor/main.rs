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
/// A year rests on its days, so the days written up since the last edition ARE
/// the change, and handing them over costs one query. An entity rests on
/// records that reference it, and those are better searched than dumped, so it
/// gets a count and goes looking if it wants detail.
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
