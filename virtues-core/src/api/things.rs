//! Wiki Things API.
//!
//! A "thing" is a long-running named anchor in the user's life that the
//! standard NER pipeline can't auto-classify into person/place/org. Common
//! categories: `project`, `pet`, `goal`, `condition`, `vehicle`, `topic`,
//! `account`. The category field is freeform — users (and LLMs) can type
//! whatever fits.
//!
//! Things accumulate references (pinned URLs to pages, chats, days, people,
//! external links) and grow a "catch-up memo" — an AI-written 2–4 sentence
//! status read at the top of the detail page that re-orients the user when
//! they return after a gap. The memo is regenerated nightly + on-open if
//! stale, unless the user has hand-edited it (then `current_status_edited_by`
//! flips to 'human' and AI stops overwriting).
//!
//! This module replaces the old `app_projects` API. Projects are now things
//! with `category = 'project'`. Same shape, generalized.

use crate::error::{Error, Result};
use crate::ids::{generate_id, THING_PREFIX};
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Thing {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    /// Freeform notes/body (the "notes" section on the detail page).
    pub content: Option<String>,
    pub cover_image: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThingSummary {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub cover_image: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThingListResponse {
    pub things: Vec<ThingSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateThingRequest {
    pub name: String,
    pub category: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateThingRequest {
    pub name: Option<String>,
    pub category: Option<Option<String>>,
    pub icon: Option<Option<String>>,
    pub description: Option<Option<String>>,
    pub content: Option<Option<String>>,
    pub cover_image: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListThingsParams {
    pub category: Option<String>,
}

// ============================================================================
// Thing CRUD
// ============================================================================

/// List things, optionally filtered by category. Sorted by most-recently-updated.
pub async fn list_things(
    pool: &PgPool,
    params: ListThingsParams,
) -> Result<ThingListResponse> {
    let things = if let Some(cat) = params.category {
        sqlx::query_as::<_, ThingSummary>(
            r#"
            SELECT
                t.id, t.name, t.category, t.icon, t.description, t.cover_image,
                t.created_at, t.updated_at
            FROM wiki_things t
            WHERE t.category = $1
            ORDER BY t.updated_at DESC
            "#,
        )
        .bind(cat)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, ThingSummary>(
            r#"
            SELECT
                t.id, t.name, t.category, t.icon, t.description, t.cover_image,
                t.created_at, t.updated_at
            FROM wiki_things t
            ORDER BY t.updated_at DESC
            "#,
        )
        .fetch_all(pool)
        .await
    }
    .map_err(|e| Error::Database(format!("Failed to list things: {}", e)))?;

    Ok(ThingListResponse { things })
}

/// Get a single thing.
pub async fn get_thing(pool: &PgPool, id: &str) -> Result<Thing> {
    let thing = sqlx::query_as::<_, Thing>(
        r#"
        SELECT id, name, category, icon, description, content, cover_image,
               created_at, updated_at
        FROM wiki_things
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get thing: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Thing not found: {}", id)))?;

    Ok(thing)
}

/// Create a new thing.
pub async fn create_thing(pool: &PgPool, req: CreateThingRequest) -> Result<Thing> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("Thing name cannot be empty".into()));
    }

    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(THING_PREFIX, &[name, &timestamp]);

    let thing = sqlx::query_as::<_, Thing>(
        r#"
        INSERT INTO wiki_things (id, name, category, icon, description)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, name, category, icon, description, content, cover_image,
                  created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(&req.category)
    .bind(&req.icon)
    .bind(&req.description)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create thing: {}", e)))?;

    Ok(thing)
}

/// Update a thing. Only provided fields change.
pub async fn update_thing(
    pool: &PgPool,
    id: &str,
    req: UpdateThingRequest,
) -> Result<Thing> {
    let existing = sqlx::query_as::<_, Thing>(
        r#"
        SELECT id, name, category, icon, description, content, cover_image,
               created_at, updated_at
        FROM wiki_things WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get thing: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Thing not found: {}", id)))?;

    let name = req.name.as_deref().unwrap_or(&existing.name).trim().to_string();
    if name.is_empty() {
        return Err(Error::InvalidInput("Thing name cannot be empty".into()));
    }

    let category = match req.category {
        Some(val) => val,
        None => existing.category,
    };
    let icon = match req.icon {
        Some(val) => val,
        None => existing.icon,
    };
    let description = match req.description {
        Some(val) => val,
        None => existing.description,
    };
    let content = match req.content {
        Some(val) => val,
        None => existing.content,
    };
    let cover_image = match req.cover_image {
        Some(val) => val,
        None => existing.cover_image,
    };

    let thing = sqlx::query_as::<_, Thing>(
        r#"
        UPDATE wiki_things
        SET name = $2, category = $3, icon = $4, description = $5,
            content = $6, cover_image = $7
        WHERE id = $1
        RETURNING id, name, category, icon, description, content, cover_image,
                  created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(&name)
    .bind(&category)
    .bind(&icon)
    .bind(&description)
    .bind(&content)
    .bind(&cover_image)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update thing: {}", e)))?;

    Ok(thing)
}

/// Delete a thing. Cascades to pins via FK.
pub async fn delete_thing(pool: &PgPool, id: &str) -> Result<()> {
    let result = sqlx::query(r#"DELETE FROM wiki_things WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete thing: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Thing not found: {}", id)));
    }
    Ok(())
}

