//! A story: a subject the person names.
//!
//! "Piano & Composition". "Books Written". "My relationship with animals &
//! pets". "How I learned to pray." None of those is a stretch of time, which
//! is what separates a story from a chapter and from a year. A story MAY carry
//! dates and usually will not.
//!
//! **This is the rung that proves the editor has to be agentic.** A story has
//! nothing beneath it — no day list, no ref set, no partition — so there is
//! nothing to fold. The only way to write "Piano & Composition" is to go and
//! search the record for it, which is the whole argument of
//! `agents/record/article-resolution.md`, "Draft is one-shot; revision is agentic".
//!
//! **Created by the person, never by the machine.** The constitution forbids
//! the editor from starting a subject; the most it may do is leave a note
//! suggesting one. A story is a claim about what mattered, and that is not the
//! record's claim to make.

use crate::error::{Error, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Story {
    pub id: String,
    pub title: String,
    /// Theirs, verbatim — what this story is, in one sentence.
    pub summary: Option<String>,
    /// Optional and usually absent. A story is not a span.
    pub started_at: Option<NaiveDate>,
    pub ended_at: Option<NaiveDate>,
    pub started_precision: Option<String>,
    pub ended_precision: Option<String>,
    #[sqlx(default)]
    pub has_article: bool,
}

#[derive(Debug, Deserialize)]
pub struct StoryFields {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub started_at: Option<NaiveDate>,
    pub ended_at: Option<NaiveDate>,
    pub started_precision: Option<String>,
    pub ended_precision: Option<String>,
}

const SELECT: &str = "SELECT s.id, s.title, s.summary, s.started_at, s.ended_at, \
     s.started_precision, s.ended_precision, \
     EXISTS (SELECT 1 FROM wiki_articles a \
              WHERE a.subject_type = 'story' AND a.subject_id = s.id) AS has_article \
     FROM wiki_stories s";

pub async fn list_stories(pool: &PgPool) -> Result<Vec<Story>> {
    sqlx::query_as::<_, Story>(&format!("{SELECT} ORDER BY s.created_at DESC"))
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list the stories: {e}")))
}

pub async fn get_story(pool: &PgPool, id: &str) -> Result<Story> {
    sqlx::query_as::<_, Story>(&format!("{SELECT} WHERE s.id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to read the story: {e}")))?
        .ok_or_else(|| Error::NotFound(format!("No story: {id}")))
}

/// Start a story. A title is all it takes, and all it needs.
pub async fn create_story(pool: &PgPool, title: &str) -> Result<Story> {
    let title = title.trim();
    if title.is_empty() {
        return Err(Error::InvalidInput("a story needs a name".into()));
    }
    let id = crate::ids::generate_id("story", &[title, &chrono::Utc::now().to_rfc3339()]);
    sqlx::query("INSERT INTO wiki_stories (id, title) VALUES ($1, $2)")
        .bind(&id)
        .bind(title)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to start the story: {e}")))?;
    get_story(pool, &id).await
}

pub async fn update_story(pool: &PgPool, id: &str, f: &StoryFields) -> Result<Story> {
    sqlx::query(
        "UPDATE wiki_stories SET \
             title = COALESCE($2, title), \
             summary = COALESCE($3, summary), \
             started_at = COALESCE($4, started_at), \
             ended_at = COALESCE($5, ended_at), \
             started_precision = COALESCE($6, started_precision), \
             ended_precision = COALESCE($7, ended_precision), \
             updated_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .bind(f.title.as_deref().map(str::trim).filter(|t| !t.is_empty()))
    .bind(f.summary.as_deref())
    .bind(f.started_at)
    .bind(f.ended_at)
    .bind(f.started_precision.as_deref())
    .bind(f.ended_precision.as_deref())
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update the story: {e}")))?;
    get_story(pool, id).await
}

/// Remove a story, and the article with it.
///
/// A story is the one subject a person invented, so it is the one they may
/// take back. Deleting the article row cascades to its page.
pub async fn delete_story(pool: &PgPool, id: &str) -> Result<()> {
    let page: Option<String> = sqlx::query_scalar(
        "SELECT page_id FROM wiki_articles WHERE subject_type = 'story' AND subject_id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to find the article: {e}")))?;

    sqlx::query("DELETE FROM wiki_articles WHERE subject_type = 'story' AND subject_id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove the article: {e}")))?;
    if let Some(p) = page {
        let _ = sqlx::query("DELETE FROM app_pages WHERE id = $1")
            .bind(&p)
            .execute(pool)
            .await;
    }
    sqlx::query("DELETE FROM wiki_stories WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove the story: {e}")))?;
    Ok(())
}

/// Seed a story's article so the editor has something to revise.
///
/// Deliberately NOT a first draft written by a model. A story has nothing
/// beneath it to draft FROM — that is the point of the rung — so the first
/// version is the person's own words and a blank page under them, and the
/// editor fills it by searching the record on its next pass.
///
/// This also keeps the paradigm's promise where it is easiest to break: the
/// machine never invents the first thing said about a subject the person just
/// named.
pub async fn start_article(pool: &PgPool, id: &str) -> Result<String> {
    let story = get_story(pool, id).await?;
    if story.has_article {
        return Err(Error::InvalidInput(
            "this story already has a page".to_string(),
        ));
    }
    let opening = story
        .summary
        .clone()
        .unwrap_or_else(|| format!("{} — nothing written yet.", story.title));
    let created =
        crate::api::wiki_articles::create_article(pool, "story", id, &story.title, &opening)
            .await?;
    // `machine_text` stays NULL on purpose: the editor wrote none of this, and
    // claiming it would let the next pass treat the person's own summary as
    // its own prose to rewrite.
    //
    // Their seed sentence is recorded as THEIRS at the same time, and that
    // second half matters as much as the first. The editor's first write
    // returns the whole page — their sentence included, because it is supposed
    // to keep it — and that text becomes `machine_text`. From the next pass on,
    // the editor would see no difference between its own output and the live
    // page, conclude it had written every word, and be free to rewrite the one
    // sentence on the page that was never its own. Recording it here is what
    // makes the invariant protect a seeded article at all.
    let theirs: Vec<String> = story
        .summary
        .as_deref()
        .map(crate::api::wiki_editor::sentences)
        .unwrap_or_default();

    sqlx::query(
        "UPDATE wiki_articles SET update_requested_at = now(), theirs = $2 WHERE id = $1",
    )
    .bind(&created.id)
    .bind(serde_json::json!(theirs))
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to queue the story: {e}")))?;
    Ok(created.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn a_story_is_a_title_and_nothing_else_is_required(pool: PgPool) {
        let s = create_story(&pool, "Piano & Composition").await.unwrap();
        assert_eq!(s.title, "Piano & Composition");
        assert!(
            s.started_at.is_none() && s.ended_at.is_none(),
            "a story is not a span — dates are optional and usually absent"
        );
        assert!(!s.has_article);
        assert!(create_story(&pool, "   ").await.is_err(), "a story needs a name");
    }

    #[sqlx::test]
    async fn dates_are_optional_and_may_be_vague(pool: PgPool) {
        let s = create_story(&pool, "How I learned to pray").await.unwrap();
        let updated = update_story(
            &pool,
            &s.id,
            &StoryFields {
                title: None,
                summary: Some("It took about a decade and I am still at it.".into()),
                started_at: Some(NaiveDate::from_ymd_opt(1998, 1, 1).unwrap()),
                ended_at: None,
                started_precision: Some("year".into()),
                ended_precision: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.started_precision.as_deref(), Some("year"));
        assert!(
            updated.ended_at.is_none(),
            "a story that has not ended is the normal case"
        );
    }

    #[sqlx::test]
    async fn the_first_version_is_theirs_and_the_editor_owns_none_of_it(pool: PgPool) {
        let s = create_story(&pool, "Books Written").await.unwrap();
        update_story(
            &pool,
            &s.id,
            &StoryFields {
                title: None,
                summary: Some("Two finished, one abandoned.".into()),
                started_at: None,
                ended_at: None,
                started_precision: None,
                ended_precision: None,
            },
        )
        .await
        .unwrap();

        let article_id = start_article(&pool, &s.id).await.unwrap();
        let (machine, requested): (Option<String>, Option<chrono::DateTime<chrono::Utc>>) =
            sqlx::query_as(
                "SELECT machine_text, update_requested_at FROM wiki_articles WHERE id = $1",
            )
            .bind(&article_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            machine.is_none(),
            "the editor wrote none of this, and must not be told it did — otherwise \\
             the next pass treats their own sentence as its prose to rewrite"
        );
        assert!(requested.is_some(), "and it is queued for the editor to fill");

        // Their seed sentence is theirs from the start. The editor's first
        // write returns the whole page, their sentence included, and that
        // becomes `machine_text` — so without this the next pass would see no
        // difference from its own output, conclude it wrote every word, and be
        // free to rewrite the one sentence that was never its own.
        let theirs: serde_json::Value =
            sqlx::query_scalar("SELECT theirs FROM wiki_articles WHERE id = $1")
                .bind(&article_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(theirs[0], "Two finished, one abandoned.");

        assert!(
            start_article(&pool, &s.id).await.is_err(),
            "a second page for one story is not a thing"
        );
    }

    #[sqlx::test]
    async fn deleting_a_story_takes_its_page_with_it(pool: PgPool) {
        let s = create_story(&pool, "Studying Abroad").await.unwrap();
        start_article(&pool, &s.id).await.unwrap();
        delete_story(&pool, &s.id).await.unwrap();
        assert!(get_story(&pool, &s.id).await.is_err());
        let orphans: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM wiki_articles WHERE subject_type = 'story'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(orphans, 0);
    }
}
