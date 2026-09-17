//! The owner's own page.
//!
//! The one place in the wiki where the record is not the author. Everything
//! else here is written from evidence and maintained by the editor; this is
//! written by the person, in the first person, and the editor may not touch it
//! — its only channel is a note.
//!
//! **The self's person row and the narrative-identity article are one thing.**
//! A person page and an identity document were two names for the same subject,
//! and keeping them apart meant the owner was the only human in their own wiki
//! without a page. So this endpoint serves both: the article's prose as the
//! body, the person's row as the identity, and the apparatus around it drawn
//! live from the record.
//!
//! The apparatus is deliberately NOT injected into any prompt. What the
//! assistant carries is the prose, byte for byte, and nothing else — see
//! `api::chat::build_narrative_identity`.

use crate::error::{Error, Result};
use serde::Serialize;
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize)]
pub struct MePage {
    /// The `wiki_people` row that is the owner. Created on first read if the
    /// profile has not named one — see `ensure_self_person`.
    pub person_id: Option<String>,
    /// What to put at the top of the page. Their name, never "Narrative
    /// identity": a person's page is titled with their name.
    pub name: Option<String>,
    /// The year partition starts here, which is why it is editable on this
    /// page rather than buried in settings.
    pub birth_date: Option<chrono::NaiveDate>,
    /// The document itself, first person, theirs.
    pub article: Option<String>,
    pub article_updated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Where the document is edited. Empty until the interview is written up.
    pub page_id: Option<String>,
    /// Their own partition of their life, for the lifeline and the list.
    pub chapters: Vec<crate::api::narrative_draft::ChapterRow>,
    /// The years the record covers, newest first.
    pub years: Vec<i32>,
}

/// The name the box knows the owner by, in the order it should be trusted.
async fn owner_name(pool: &PgPool) -> Result<Option<String>> {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT COALESCE(preferred_name, full_name) FROM app_user_profile LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map(Option::flatten)
    .map_err(|e| Error::Database(format!("Failed to read the profile: {e}")))
}

/// Make sure the owner has a row among the people of their own wiki.
///
/// Nothing has ever created one. `self_person_id` is optional on the profile
/// and the one endpoint that writes it only STORES an id it is handed, so on
/// most boxes the owner is the single human their wiki has no page for — and
/// every feature keyed to "the self" silently does nothing.
///
/// Refuses to invent a name. A row called "Unknown" is worse than no row: it
/// would show up in the people index as a stranger, and the person would have
/// to work out that it was them.
pub async fn ensure_self_person(pool: &PgPool) -> Result<Option<String>> {
    let existing: Option<String> =
        sqlx::query_scalar("SELECT self_person_id FROM app_user_profile LIMIT 1")
            .fetch_optional(pool)
            .await
            .map(Option::flatten)
            .map_err(|e| Error::Database(format!("Failed to read the profile: {e}")))?;
    if let Some(id) = existing {
        return Ok(Some(id));
    }

    let Some(name) = owner_name(pool).await?.filter(|n| !n.trim().is_empty()) else {
        return Ok(None);
    };

    let id = crate::api::entities::create_person(pool, &name).await?;
    sqlx::query("UPDATE app_user_profile SET self_person_id = $1 WHERE self_person_id IS NULL")
        .bind(&id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to name the self: {e}")))?;
    Ok(Some(id))
}

/// Everything the owner's page shows.
pub async fn get_me(pool: &PgPool) -> Result<MePage> {
    let person_id = ensure_self_person(pool).await?;
    let name = owner_name(pool).await?;

    let birth_date: Option<chrono::NaiveDate> =
        sqlx::query_scalar("SELECT birth_date FROM app_user_profile LIMIT 1")
            .fetch_optional(pool)
            .await
            .map(Option::flatten)
            .map_err(|e| Error::Database(format!("Failed to read the profile: {e}")))?;

    // The document. Still keyed by the narrative-identity subject rather than
    // by the person: chat reads it there on every turn, and re-pointing it is a
    // migration with a fallback, not a rename.
    // `get_narrative_identity` returns the empty string before the interview
    // has been written up, which is a real state rather than an error.
    let identity = crate::api::wiki::get_narrative_identity(pool).await.ok();
    let article = identity
        .as_ref()
        .map(|i| i.content.clone())
        .filter(|c| !c.trim().is_empty());

    let chapters = crate::api::narrative_draft::list_chapters(pool).await?;

    let years: Vec<i32> = sqlx::query_scalar(
        "SELECT DISTINCT EXTRACT(YEAR FROM date)::int AS y FROM wiki_days ORDER BY y DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to read the years on record: {e}")))?;

    Ok(MePage {
        person_id,
        name,
        birth_date,
        article_updated_at: identity.as_ref().map(|i| i.updated_at),
        page_id: identity
            .as_ref()
            .map(|i| i.page_id.clone())
            .filter(|p| !p.is_empty()),
        article,
        chapters,
        years,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn the_owner_gets_a_row_once_the_box_knows_their_name(pool: PgPool) {
        // `app_user_profile` is a singleton the migrations already seed, so the
        // tests update it rather than inserting a second row.
        sqlx::query("UPDATE app_user_profile SET preferred_name = NULL, full_name = NULL")
            .execute(&pool)
            .await
            .unwrap();

        assert!(
            ensure_self_person(&pool).await.unwrap().is_none(),
            "no name yet — a row called 'Unknown' would appear in the people \
             index as a stranger they have to decode"
        );

        sqlx::query("UPDATE app_user_profile SET preferred_name = 'Nick'")
            .execute(&pool)
            .await
            .unwrap();
        let id = ensure_self_person(&pool).await.unwrap().expect("a row now");

        let again = ensure_self_person(&pool).await.unwrap();
        assert_eq!(again, Some(id.clone()), "and only ever one");

        let stored: Option<String> =
            sqlx::query_scalar("SELECT self_person_id FROM app_user_profile LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored, Some(id));
    }

    #[sqlx::test]
    async fn the_page_is_their_name_and_their_document(pool: PgPool) {
        sqlx::query("UPDATE app_user_profile SET preferred_name = 'Nick', birth_date = '1997-04-02'")
            .execute(&pool)
            .await
            .unwrap();

        let me = get_me(&pool).await.unwrap();
        assert_eq!(me.name.as_deref(), Some("Nick"));
        assert!(me.person_id.is_some());
        assert_eq!(
            me.birth_date,
            Some(chrono::NaiveDate::from_ymd_opt(1997, 4, 2).unwrap()),
            "the year partition starts here, which is why it is editable on \
             this page rather than buried in settings"
        );
        assert!(me.article.is_none(), "nothing written yet is the normal state");
    }
}
