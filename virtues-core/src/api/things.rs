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
use crate::ids::{generate_id, THING_PIN_PREFIX, THING_PREFIX};
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
    pub cover_image: Option<String>,
    pub current_status: Option<String>,
    pub current_status_at: Option<Timestamp>,
    pub current_status_edited_by: String,
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
    pub current_status: Option<String>,
    pub current_status_at: Option<Timestamp>,
    pub pin_count: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThingPin {
    pub id: String,
    pub thing_id: String,
    pub url: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub sort_order: i32,
    pub added_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThingDetail {
    #[serde(flatten)]
    pub thing: Thing,
    pub pins: Vec<ThingPin>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddThingPinRequest {
    pub url: String,
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderThingPinsRequest {
    pub urls: Vec<String>,
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
                t.current_status, t.current_status_at,
                COALESCE((SELECT COUNT(*) FROM wiki_thing_pins WHERE thing_id = t.id), 0) AS pin_count,
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
                t.current_status, t.current_status_at,
                COALESCE((SELECT COUNT(*) FROM wiki_thing_pins WHERE thing_id = t.id), 0) AS pin_count,
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

/// Get a single thing with its pins (ordered).
pub async fn get_thing(pool: &PgPool, id: &str) -> Result<ThingDetail> {
    let thing = sqlx::query_as::<_, Thing>(
        r#"
        SELECT id, name, category, icon, description, cover_image,
               current_status, current_status_at, current_status_edited_by,
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

    let pins = sqlx::query_as::<_, ThingPin>(
        r#"
        SELECT id, thing_id, url, name, description, sort_order, added_at
        FROM wiki_thing_pins
        WHERE thing_id = $1
        ORDER BY sort_order ASC, added_at ASC
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get thing pins: {}", e)))?;

    Ok(ThingDetail { thing, pins })
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
        RETURNING id, name, category, icon, description, cover_image,
                  current_status, current_status_at, current_status_edited_by,
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
        SELECT id, name, category, icon, description, cover_image,
               current_status, current_status_at, current_status_edited_by,
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

    let thing = sqlx::query_as::<_, Thing>(
        r#"
        UPDATE wiki_things
        SET name = $2, category = $3, icon = $4, description = $5
        WHERE id = $1
        RETURNING id, name, category, icon, description, cover_image,
                  current_status, current_status_at, current_status_edited_by,
                  created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(&name)
    .bind(&category)
    .bind(&icon)
    .bind(&description)
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

// ============================================================================
// Thing Pins
// ============================================================================

/// Add a pin to a thing. Idempotent on (thing_id, url) — pinning twice
/// returns the existing row.
pub async fn add_thing_pin(
    pool: &PgPool,
    thing_id: &str,
    req: AddThingPinRequest,
) -> Result<ThingPin> {
    let url = req.url.trim();
    if url.is_empty() {
        return Err(Error::InvalidInput("Pin url cannot be empty".into()));
    }

    let thing_exists: Option<String> =
        sqlx::query_scalar(r#"SELECT id FROM wiki_things WHERE id = $1"#)
            .bind(thing_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to verify thing: {}", e)))?;

    if thing_exists.is_none() {
        return Err(Error::NotFound(format!("Thing not found: {}", thing_id)));
    }

    if let Some(existing) = sqlx::query_as::<_, ThingPin>(
        r#"
        SELECT id, thing_id, url, name, description, sort_order, added_at
        FROM wiki_thing_pins
        WHERE thing_id = $1 AND url = $2
        "#,
    )
    .bind(thing_id)
    .bind(url)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to check existing pin: {}", e)))?
    {
        return Ok(existing);
    }

    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(THING_PIN_PREFIX, &[thing_id, url, &timestamp]);

    let next_sort: i64 = sqlx::query_scalar(
        r#"SELECT COALESCE(MAX(sort_order), -1) + 1 FROM wiki_thing_pins WHERE thing_id = $1"#,
    )
    .bind(thing_id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to compute sort_order: {}", e)))?;

    let pin = sqlx::query_as::<_, ThingPin>(
        r#"
        INSERT INTO wiki_thing_pins (id, thing_id, url, name, description, sort_order)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, thing_id, url, name, description, sort_order, added_at
        "#,
    )
    .bind(&id)
    .bind(thing_id)
    .bind(url)
    .bind(&req.name)
    .bind(&req.description)
    .bind(next_sort)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to add thing pin: {}", e)))?;

    sqlx::query(r#"UPDATE wiki_things SET updated_at = now() WHERE id = $1"#)
        .bind(thing_id)
        .execute(pool)
        .await
        .ok();

    Ok(pin)
}

/// Remove a pin from a thing (by URL).
pub async fn remove_thing_pin(pool: &PgPool, thing_id: &str, url: &str) -> Result<()> {
    let result = sqlx::query(
        r#"DELETE FROM wiki_thing_pins WHERE thing_id = $1 AND url = $2"#,
    )
    .bind(thing_id)
    .bind(url)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to remove thing pin: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!(
            "Pin not found on thing: {} / {}",
            thing_id, url
        )));
    }

    sqlx::query(r#"UPDATE wiki_things SET updated_at = now() WHERE id = $1"#)
        .bind(thing_id)
        .execute(pool)
        .await
        .ok();

    Ok(())
}

/// Reorder pins on a thing. Unknown URLs are ignored.
pub async fn reorder_thing_pins(
    pool: &PgPool,
    thing_id: &str,
    req: ReorderThingPinsRequest,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;

    for (idx, url) in req.urls.iter().enumerate() {
        sqlx::query(
            r#"UPDATE wiki_thing_pins SET sort_order = $1 WHERE thing_id = $2 AND url = $3"#,
        )
        .bind(idx as i64)
        .bind(thing_id)
        .bind(url)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to reorder thing pins: {}", e)))?;
    }

    sqlx::query(r#"UPDATE wiki_things SET updated_at = now() WHERE id = $1"#)
        .bind(thing_id)
        .execute(&mut *tx)
        .await
        .ok();

    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to commit reorder: {}", e)))?;

    Ok(())
}
