//! wiki_editor: the record's contributor.
//!
//! Article resolution's scheduler half. See
//! `agents/plan/article-resolution-plan.md`.
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

        output(
            &format!(
                "Revise the article for subject_type={} subject_id={}. Its evidence has \
                 changed since the last edition.\n\n\
                 THE ARTICLE AS IT STANDS — revise THIS text and pass the whole result \
                 to `revise_article`:\n\n{current}",
                article.subject_type, article.subject_id
            ),
            &input.config,
        )?;
        return Ok(());
    }

    output("No article is due for revision.", &input.config)?;
    Ok(())
}
