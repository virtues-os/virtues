//! A thing: a noun in the person's life that is not a person, a place or an
//! organization.
//!
//! The dog. The car. The boat. The house. Something happening is a thread
//! (`crate::api::threads`), not a thing. The person names a thing and it gets
//! an article. Threads and tasks can sit under it (`app_threads.thing_id`,
//! `app_tasks.thing_id`), and its page lists them.
//!
//! A thing MAY carry dates and usually will not: `started_at` is when it came
//! into their life, and `ended_at` when it left.
//!
//! **Created by the person, never by the server.** The editor's constitution
//! forbids it from starting a subject; the most it may do is leave a note
//! suggesting one. Naming a thing says what is part of a life, and that is
//! the person's to say.
//!
//! **No record points at a thing.** A person has the messages that mention
//! them and a year has its days, but nothing resolves a record to "the car". So
//! the editor writes a thing's article by searching for it, which is the
//! argument of `agents/record/article-resolution.md`, "Draft is one-shot;
//! revision is agentic".

use crate::error::{Error, Result};
use crate::ids::{generate_id, THING_PREFIX};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Thing {
    pub id: String,
    pub title: String,
    /// Theirs, verbatim: what this thing is, in one sentence.
    pub summary: Option<String>,
    /// Optional and usually absent. A thing is not a span.
    pub started_at: Option<NaiveDate>,
    pub ended_at: Option<NaiveDate>,
    pub started_precision: Option<String>,
    pub ended_precision: Option<String>,
    #[sqlx(default)]
    pub has_article: bool,
}

/// A thing as its page reads it: the row, and its article's prose when it
/// has one.
#[derive(Debug, Clone, Serialize)]
pub struct ThingDetail {
    #[serde(flatten)]
    pub thing: Thing,
    pub article: Option<String>,
    pub article_updated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Whether the wiki editor keeps the article up to date.
    pub article_maintained: bool,
}

#[derive(Debug, Deserialize)]
pub struct ThingFields {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub started_at: Option<NaiveDate>,
    pub ended_at: Option<NaiveDate>,
    pub started_precision: Option<String>,
    pub ended_precision: Option<String>,
}

const SELECT: &str = "SELECT t.id, t.title, t.summary, t.started_at, t.ended_at, \
     t.started_precision, t.ended_precision, \
     EXISTS (SELECT 1 FROM wiki_articles a \
              WHERE a.subject_type = 'thing' AND a.subject_id = t.id) AS has_article \
     FROM wiki_things t";

pub async fn list_things(pool: &PgPool) -> Result<Vec<Thing>> {
    sqlx::query_as::<_, Thing>(&format!("{SELECT} ORDER BY t.created_at DESC"))
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list the things: {e}")))
}

pub async fn get_thing(pool: &PgPool, id: &str) -> Result<Thing> {
    sqlx::query_as::<_, Thing>(&format!("{SELECT} WHERE t.id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to read the thing: {e}")))?
        .ok_or_else(|| Error::NotFound(format!("No thing: {id}")))
}

/// A thing with its article's prose, for its page.
pub async fn get_thing_detail(pool: &PgPool, id: &str) -> Result<ThingDetail> {
    let thing = get_thing(pool, id).await?;
    let prose = crate::api::wiki_articles::get_article_prose(pool, "thing", id).await?;
    Ok(ThingDetail {
        thing,
        article_updated_at: prose.as_ref().map(|p| p.updated_at),
        article_maintained: prose.as_ref().is_some_and(|p| p.maintained),
        article: prose.map(|p| p.content),
    })
}

/// Name a thing. A title is all it takes.
pub async fn create_thing(pool: &PgPool, title: &str) -> Result<Thing> {
    let title = title.trim();
    if title.is_empty() {
        return Err(Error::InvalidInput("a thing needs a name".into()));
    }
    let id = generate_id(THING_PREFIX, &[title, &chrono::Utc::now().to_rfc3339()]);
    sqlx::query("INSERT INTO wiki_things (id, title) VALUES ($1, $2)")
        .bind(&id)
        .bind(title)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to name the thing: {e}")))?;
    get_thing(pool, &id).await
}

pub async fn update_thing(pool: &PgPool, id: &str, f: &ThingFields) -> Result<Thing> {
    sqlx::query(
        "UPDATE wiki_things SET \
             title = COALESCE($2, title), \
             summary = CASE WHEN $3::text IS NULL THEN summary \
                            ELSE NULLIF(btrim($3), '') END, \
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
    .map_err(|e| Error::Database(format!("Failed to update the thing: {e}")))?;
    crate::api::wiki_articles::retitle_article(pool, "thing", id).await?;
    get_thing(pool, id).await
}

/// Remove a thing, and its article and page with it.
///
/// A thing is a subject the person named, so they may take it back. The
/// projects and tasks under it stay and lose the link (`ON DELETE SET NULL`).
/// One transaction, so a failure leaves the thing and its page as they were.
pub async fn delete_thing(pool: &PgPool, id: &str) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to remove the thing: {e}")))?;
    let page: Option<String> = sqlx::query_scalar(
        "DELETE FROM wiki_articles WHERE subject_type = 'thing' AND subject_id = $1 \
         RETURNING page_id",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| Error::Database(format!("Failed to remove the article: {e}")))?;
    if let Some(page) = page {
        sqlx::query("DELETE FROM app_pages WHERE id = $1")
            .bind(&page)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Database(format!("Failed to remove the page: {e}")))?;
    }
    sqlx::query("DELETE FROM wiki_things WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove the thing: {e}")))?;
    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to remove the thing: {e}")))
}

/// Seed a thing's article so the editor has something to revise.
///
/// The first version is the person's own sentence and a blank page under it,
/// and the editor fills it by searching the record on its next pass. A model
/// does not write the first draft: no record points at a thing, so there is
/// nothing to draft from, and the first words about a subject the person just
/// named stay theirs.
pub async fn start_article(pool: &PgPool, id: &str) -> Result<String> {
    let thing = get_thing(pool, id).await?;
    if thing.has_article {
        return Err(Error::InvalidInput("this thing already has a page".to_string()));
    }
    let opening = thing
        .summary
        .clone()
        .unwrap_or_else(|| "Nothing written yet.".to_string());
    let created =
        crate::api::wiki_articles::create_article(pool, "thing", id, &thing.title, &opening)
            .await?;
    // `machine_text` stays NULL: the editor wrote none of this, and claiming
    // it would let the next pass treat the person's own summary as its own
    // prose to rewrite.
    //
    // Their seed sentence is recorded as THEIRS at the same time. The editor's
    // first write returns the whole page, their sentence included, and that
    // text becomes `machine_text`. From the next pass on the editor would see
    // no difference between its own output and the live page, conclude it had
    // written every word, and be free to rewrite the one sentence on the page
    // that was never its own. Recording it here is what protects a seeded
    // article.
    let theirs: Vec<String> = thing
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
    .map_err(|e| Error::Database(format!("Failed to queue the thing's article: {e}")))?;
    Ok(created.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn a_thing_is_a_title_and_nothing_else_is_required(pool: PgPool) {
        let t = create_thing(&pool, "Sailing").await.unwrap();
        assert_eq!(t.title, "Sailing");
        assert!(t.id.starts_with("thing_"), "the id's prefix is the subject kind: {}", t.id);
        assert!(
            t.started_at.is_none() && t.ended_at.is_none(),
            "a thing is not a span: dates are optional and usually absent"
        );
        assert!(!t.has_article);
        assert!(create_thing(&pool, "   ").await.is_err(), "a thing needs a name");
    }

    #[sqlx::test]
    async fn dates_are_optional_and_may_be_vague(pool: PgPool) {
        let t = create_thing(&pool, "Guitar").await.unwrap();
        let updated = update_thing(
            &pool,
            &t.id,
            &ThingFields {
                title: None,
                summary: Some("I took lessons as a kid and came back to it.".into()),
                started_at: Some(NaiveDate::from_ymd_opt(1998, 1, 1).unwrap()),
                ended_at: None,
                started_precision: Some("year".into()),
                ended_precision: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.started_precision.as_deref(), Some("year"));
        assert!(updated.ended_at.is_none(), "a thing goes on, so no end is the normal case");

        // An edit that says nothing about the sentence keeps it, and a blank
        // one takes it away.
        let blank = |summary: Option<&str>| ThingFields {
            title: None,
            summary: summary.map(str::to_string),
            started_at: None,
            ended_at: None,
            started_precision: None,
            ended_precision: None,
        };
        let kept = update_thing(&pool, &t.id, &blank(None)).await.unwrap();
        assert_eq!(kept.summary.as_deref(), Some("I took lessons as a kid and came back to it."));
        let cleared = update_thing(&pool, &t.id, &blank(Some("  "))).await.unwrap();
        assert!(cleared.summary.is_none());
    }

    #[sqlx::test]
    async fn the_first_version_is_theirs_and_the_editor_owns_none_of_it(pool: PgPool) {
        let t = create_thing(&pool, "The garden").await.unwrap();
        update_thing(
            &pool,
            &t.id,
            &ThingFields {
                title: None,
                summary: Some("Two beds planted, one left wild.".into()),
                started_at: None,
                ended_at: None,
                started_precision: None,
                ended_precision: None,
            },
        )
        .await
        .unwrap();

        let article_id = start_article(&pool, &t.id).await.unwrap();
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
            "the editor wrote none of this, and must not be told it did, or the \
             next pass treats their own sentence as its prose to rewrite"
        );
        assert!(requested.is_some(), "and it is queued for the editor to fill");

        // Their seed sentence is theirs from the start. The editor's first
        // write returns the whole page, their sentence included, and that
        // becomes `machine_text`. Without this the next pass would see no
        // difference from its own output and be free to rewrite it.
        let theirs: serde_json::Value =
            sqlx::query_scalar("SELECT theirs FROM wiki_articles WHERE id = $1")
                .bind(&article_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(theirs[0], "Two beds planted, one left wild.");

        assert!(
            start_article(&pool, &t.id).await.is_err(),
            "a thing has one page"
        );

        let detail = get_thing_detail(&pool, &t.id).await.unwrap();
        assert_eq!(detail.article.as_deref().map(str::trim), Some("Two beds planted, one left wild."));
        assert!(detail.article_maintained && detail.thing.has_article);
    }

    #[sqlx::test]
    async fn deleting_a_thing_takes_its_page_with_it(pool: PgPool) {
        let t = create_thing(&pool, "The cat").await.unwrap();
        let article_id = start_article(&pool, &t.id).await.unwrap();
        let page_id: String =
            sqlx::query_scalar("SELECT page_id FROM wiki_articles WHERE id = $1")
                .bind(&article_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        delete_thing(&pool, &t.id).await.unwrap();
        assert!(get_thing(&pool, &t.id).await.is_err());
        let articles: i64 =
            sqlx::query_scalar("SELECT count(*) FROM wiki_articles WHERE subject_type = 'thing'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(articles, 0);
        let pages: i64 = sqlx::query_scalar("SELECT count(*) FROM app_pages WHERE id = $1")
            .bind(&page_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(pages, 0, "the article's page goes with it");
    }

    /// A project and a task under a thing outlive it and lose the link.
    #[sqlx::test]
    async fn what_sat_under_a_removed_thing_stays(pool: PgPool) {
        let t = create_thing(&pool, "The house").await.unwrap();
        sqlx::query("INSERT INTO app_threads (id, name, thing_id) VALUES ('nb_t', 'Repaint the hallway', $1)")
            .bind(&t.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO app_tasks (id, title, origin, thing_id) VALUES ('task_t', 'Change the furnace filter', 'owner', $1)",
        )
        .bind(&t.id)
        .execute(&pool)
        .await
        .unwrap();
        delete_thing(&pool, &t.id).await.unwrap();
        let left: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT (SELECT thing_id FROM app_threads WHERE id = 'nb_t'), \
                    (SELECT thing_id FROM app_tasks WHERE id = 'task_t')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(left, (None, None));
    }
}
