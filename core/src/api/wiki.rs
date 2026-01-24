//! Wiki API - Views of entities and narratives for wiki pages
//!
//! Wiki pages are not separate constructs - they are views of:
//! - Entities: Person, Place, Organization, Thing
//! - Narratives: Telos, Act, Chapter, Day

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::{Error, Result};
use crate::ids;


// ============================================================================
// Wiki Page Types - Entity Views
// ============================================================================

/// A person wiki page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPerson {
    pub id: String,
    pub slug: Option<String>,
    pub canonical_name: String,
    pub content: Option<String>,
    pub picture: Option<String>,
    pub cover_image: Option<String>,
    // vCard fields
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    pub birthday: Option<NaiveDate>,
    pub instagram: Option<String>,
    pub facebook: Option<String>,
    pub linkedin: Option<String>,
    pub x: Option<String>,
    // Metadata
    pub relationship_category: Option<String>,
    pub nickname: Option<String>,
    pub notes: Option<String>,
    pub first_interaction: Option<DateTime<Utc>>,
    pub last_interaction: Option<DateTime<Utc>>,
    pub interaction_count: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A place wiki page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPlace {
    pub id: String,
    pub slug: Option<String>,
    pub name: String,
    pub content: Option<String>,
    pub cover_image: Option<String>,
    pub category: Option<String>,
    pub address: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub visit_count: Option<i32>,
    pub first_visit: Option<DateTime<Utc>>,
    pub last_visit: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An organization wiki page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiOrganization {
    pub id: String,
    pub slug: Option<String>,
    pub canonical_name: String,
    pub content: Option<String>,
    pub cover_image: Option<String>,
    pub organization_type: Option<String>,
    pub relationship_type: Option<String>,
    pub role_title: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub interaction_count: Option<i32>,
    pub first_interaction: Option<DateTime<Utc>>,
    pub last_interaction: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A thing wiki page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiThing {
    pub id: String,
    pub slug: Option<String>,
    pub canonical_name: String,
    pub content: Option<String>,
    pub cover_image: Option<String>,
    pub thing_type: Option<String>,
    pub description: Option<String>,
    pub first_mentioned: Option<DateTime<Utc>>,
    pub last_mentioned: Option<DateTime<Utc>>,
    pub mention_count: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// Wiki Page Types - Narrative Views
// ============================================================================

/// A telos wiki page (life purpose/mission)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiTelos {
    pub id: String,
    pub slug: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub content: Option<String>,
    pub cover_image: Option<String>,
    pub is_active: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A narrative act wiki page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiAct {
    pub id: String,
    pub slug: Option<String>,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub content: Option<String>,
    pub cover_image: Option<String>,
    pub location: Option<String>,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub sort_order: i32,
    pub telos_id: Option<String>,
    pub themes: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A narrative chapter wiki page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiChapter {
    pub id: String,
    pub slug: Option<String>,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub content: Option<String>,
    pub cover_image: Option<String>,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub sort_order: i32,
    pub act_id: Option<String>,
    pub themes: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A day wiki page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiDay {
    pub id: String,
    pub date: NaiveDate,
    pub start_timezone: Option<String>,
    pub end_timezone: Option<String>,
    pub autobiography: Option<String>,
    pub autobiography_sections: Option<serde_json::Value>,
    pub last_edited_by: Option<String>,
    pub cover_image: Option<String>,
    pub act_id: Option<String>,
    pub chapter_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// List Item Types (lighter weight for lists)
// ============================================================================

/// A person list item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPersonListItem {
    pub id: String,
    pub slug: Option<String>,
    pub canonical_name: String,
    pub picture: Option<String>,
    pub relationship_category: Option<String>,
    pub last_interaction: Option<DateTime<Utc>>,
}

/// A place list item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPlaceListItem {
    pub id: String,
    pub slug: Option<String>,
    pub name: String,
    pub category: Option<String>,
    pub address: Option<String>,
    pub visit_count: Option<i32>,
}

/// An organization list item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiOrganizationListItem {
    pub id: String,
    pub slug: Option<String>,
    pub canonical_name: String,
    pub organization_type: Option<String>,
    pub relationship_type: Option<String>,
}

/// A thing list item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiThingListItem {
    pub id: String,
    pub slug: Option<String>,
    pub canonical_name: String,
    pub thing_type: Option<String>,
}

// ============================================================================
// Update Request Types
// ============================================================================

/// Request to update a person wiki page
#[derive(Debug, Deserialize)]
pub struct UpdateWikiPersonRequest {
    pub slug: Option<String>,
    pub canonical_name: Option<String>,
    pub content: Option<String>,
    pub picture: Option<String>,
    pub cover_image: Option<String>,
    pub emails: Option<Vec<String>>,
    pub phones: Option<Vec<String>>,
    pub birthday: Option<NaiveDate>,
    pub instagram: Option<String>,
    pub facebook: Option<String>,
    pub linkedin: Option<String>,
    pub x: Option<String>,
    pub relationship_category: Option<String>,
    pub nickname: Option<String>,
    pub notes: Option<String>,
}

/// Request to update a place wiki page
#[derive(Debug, Deserialize)]
pub struct UpdateWikiPlaceRequest {
    pub slug: Option<String>,
    pub name: Option<String>,
    pub content: Option<String>,
    pub cover_image: Option<String>,
    pub category: Option<String>,
    pub address: Option<String>,
}

/// Request to update an organization wiki page
#[derive(Debug, Deserialize)]
pub struct UpdateWikiOrganizationRequest {
    pub slug: Option<String>,
    pub canonical_name: Option<String>,
    pub content: Option<String>,
    pub cover_image: Option<String>,
    pub organization_type: Option<String>,
    pub relationship_type: Option<String>,
    pub role_title: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
}

/// Request to update a thing wiki page
#[derive(Debug, Deserialize)]
pub struct UpdateWikiThingRequest {
    pub slug: Option<String>,
    pub canonical_name: Option<String>,
    pub content: Option<String>,
    pub cover_image: Option<String>,
    pub thing_type: Option<String>,
    pub description: Option<String>,
}

/// Request to update a day wiki page
#[derive(Debug, Deserialize)]
pub struct UpdateWikiDayRequest {
    pub autobiography: Option<String>,
    pub autobiography_sections: Option<serde_json::Value>,
    pub last_edited_by: Option<String>,
    pub cover_image: Option<String>,
}

// ============================================================================
// Person CRUD Operations
// ============================================================================

/// Get a person by slug
pub async fn get_person_by_slug(pool: &SqlitePool, slug: &str) -> Result<WikiPerson> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, slug, canonical_name, content, picture, cover_image,
            emails, phones, birthday, instagram, facebook, linkedin, x,
            relationship_category, nickname, notes,
            first_interaction, last_interaction, interaction_count,
            created_at, updated_at
        FROM wiki_people
        WHERE slug = $1
        "#,
        slug
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get person: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Person not found: {}", slug)))?;

    // Get ID as string
    let id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing person ID".to_string()))?;

    Ok(WikiPerson {
        id,
        slug: row.slug.clone(),
        canonical_name: row.canonical_name.clone(),
        content: row.content.clone(),
        picture: row.picture.clone(),
        cover_image: row.cover_image.clone(),
        // Parse JSON arrays from TEXT columns
        emails: row
            .emails
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default(),
        phones: row
            .phones
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default(),
        birthday: row
            .birthday
            .as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        instagram: row.instagram.clone(),
        facebook: row.facebook.clone(),
        linkedin: row.linkedin.clone(),
        x: row.x.clone(),
        relationship_category: row.relationship_category.clone(),
        nickname: row.nickname.clone(),
        notes: row.notes.clone(),
        first_interaction: row
            .first_interaction
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        last_interaction: row
            .last_interaction
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        interaction_count: row.interaction_count.map(|v| v as i32),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// Get a person by ID
pub async fn get_person(pool: &SqlitePool, id: String) -> Result<WikiPerson> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, slug, canonical_name, content, picture, cover_image,
            emails, phones, birthday, instagram, facebook, linkedin, x,
            relationship_category, nickname, notes,
            first_interaction, last_interaction, interaction_count,
            created_at, updated_at
        FROM wiki_people
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get person: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Person not found: {}", id)))?;

    // Get ID as string
    let row_id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing person ID".to_string()))?;

    Ok(WikiPerson {
        id: row_id,
        slug: row.slug.clone(),
        canonical_name: row.canonical_name.clone(),
        content: row.content.clone(),
        picture: row.picture.clone(),
        cover_image: row.cover_image.clone(),
        emails: row
            .emails
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default(),
        phones: row
            .phones
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default(),
        birthday: row
            .birthday
            .as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        instagram: row.instagram.clone(),
        facebook: row.facebook.clone(),
        linkedin: row.linkedin.clone(),
        x: row.x.clone(),
        relationship_category: row.relationship_category.clone(),
        nickname: row.nickname.clone(),
        notes: row.notes.clone(),
        first_interaction: row
            .first_interaction
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        last_interaction: row
            .last_interaction
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        interaction_count: row.interaction_count.map(|v| v as i32),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// List all people
pub async fn list_people(pool: &SqlitePool) -> Result<Vec<WikiPersonListItem>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id, slug, canonical_name, picture, relationship_category, last_interaction
        FROM wiki_people
        ORDER BY canonical_name ASC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list people: {}", e)))?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let id = row.id.clone()?;
            Some(WikiPersonListItem {
                id,
                slug: row.slug.clone(),
                canonical_name: row.canonical_name.clone(),
                picture: row.picture.clone(),
                relationship_category: row.relationship_category.clone(),
                last_interaction: row
                    .last_interaction
                    .as_ref()
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
            })
        })
        .collect())
}

/// Update a person
pub async fn update_person(
    pool: &SqlitePool,
    id: String,
    req: UpdateWikiPersonRequest,
) -> Result<WikiPerson> {
    // Convert Vec<String> arrays to JSON strings for SQLite
    let emails_json = req
        .emails
        .as_ref()
        .map(|e| serde_json::to_string(e).unwrap_or_else(|_| "[]".to_string()));
    let phones_json = req
        .phones
        .as_ref()
        .map(|p| serde_json::to_string(p).unwrap_or_else(|_| "[]".to_string()));

    sqlx::query!(
        r#"
        UPDATE wiki_people
        SET
            slug = COALESCE($2, slug),
            canonical_name = COALESCE($3, canonical_name),
            content = COALESCE($4, content),
            picture = COALESCE($5, picture),
            cover_image = COALESCE($6, cover_image),
            emails = COALESCE($7, emails),
            phones = COALESCE($8, phones),
            birthday = COALESCE($9, birthday),
            instagram = COALESCE($10, instagram),
            facebook = COALESCE($11, facebook),
            linkedin = COALESCE($12, linkedin),
            x = COALESCE($13, x),
            relationship_category = COALESCE($14, relationship_category),
            nickname = COALESCE($15, nickname),
            notes = COALESCE($16, notes),
            updated_at = datetime('now')
        WHERE id = $1
        "#,
        id,
        req.slug,
        req.canonical_name,
        req.content,
        req.picture,
        req.cover_image,
        emails_json,
        phones_json,
        req.birthday,
        req.instagram,
        req.facebook,
        req.linkedin,
        req.x,
        req.relationship_category,
        req.nickname,
        req.notes
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update person: {}", e)))?;

    get_person(pool, id).await
}

// ============================================================================
// Place CRUD Operations
// ============================================================================

/// Get a place by slug
pub async fn get_place_by_slug(pool: &SqlitePool, slug: &str) -> Result<WikiPlace> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, slug, name, content, cover_image, category, address,
            latitude, longitude,
            visit_count, first_visit, last_visit,
            created_at, updated_at
        FROM wiki_places
        WHERE slug = $1
        "#,
        slug
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get place: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Place not found: {}", slug)))?;

    let id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing place ID".to_string()))?;

    Ok(WikiPlace {
        id,
        slug: row.slug.clone(),
        name: row.name.clone(),
        content: row.content.clone(),
        cover_image: row.cover_image.clone(),
        category: row.category.clone(),
        address: row.address.clone(),
        latitude: row.latitude,
        longitude: row.longitude,
        visit_count: row.visit_count.map(|v| v as i32),
        first_visit: row
            .first_visit
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        last_visit: row
            .last_visit
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// Get a place by ID (wiki view with content fields)
pub async fn get_wiki_place(pool: &SqlitePool, id: String) -> Result<WikiPlace> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, slug, name, content, cover_image, category, address,
            latitude, longitude,
            visit_count, first_visit, last_visit,
            created_at, updated_at
        FROM wiki_places
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get place: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Place not found: {}", id)))?;

    // Get ID as string
    let row_id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing place ID".to_string()))?;

    Ok(WikiPlace {
        id: row_id,
        slug: row.slug.clone(),
        name: row.name.clone(),
        content: row.content.clone(),
        cover_image: row.cover_image.clone(),
        category: row.category.clone(),
        address: row.address.clone(),
        latitude: row.latitude,
        longitude: row.longitude,
        visit_count: row.visit_count.map(|v| v as i32),
        first_visit: row
            .first_visit
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        last_visit: row
            .last_visit
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// List all places (wiki view with content fields)
pub async fn list_wiki_places(pool: &SqlitePool) -> Result<Vec<WikiPlaceListItem>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id, slug, name, category, address, visit_count
        FROM wiki_places
        ORDER BY name ASC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list places: {}", e)))?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let id = row.id.clone()?;
            Some(WikiPlaceListItem {
                id,
                slug: row.slug.clone(),
                name: row.name.clone(),
                category: row.category.clone(),
                address: row.address.clone(),
                visit_count: row.visit_count.map(|v| v as i32),
            })
        })
        .collect())
}

/// Update a place wiki content
pub async fn update_wiki_place(
    pool: &SqlitePool,
    id: String,
    req: UpdateWikiPlaceRequest,
) -> Result<WikiPlace> {
    sqlx::query!(
        r#"
        UPDATE wiki_places
        SET
            slug = COALESCE($2, slug),
            name = COALESCE($3, name),
            content = COALESCE($4, content),
            cover_image = COALESCE($5, cover_image),
            category = COALESCE($6, category),
            address = COALESCE($7, address),
            updated_at = datetime('now')
        WHERE id = $1
        "#,
        id,
        req.slug,
        req.name,
        req.content,
        req.cover_image,
        req.category,
        req.address
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update place: {}", e)))?;

    get_wiki_place(pool, id).await
}

// ============================================================================
// Organization CRUD Operations
// ============================================================================

/// Get an organization by slug
pub async fn get_organization_by_slug(pool: &SqlitePool, slug: &str) -> Result<WikiOrganization> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, slug, canonical_name, content, cover_image,
            organization_type, relationship_type, role_title,
            start_date, end_date, interaction_count,
            first_interaction, last_interaction,
            created_at, updated_at
        FROM wiki_orgs
        WHERE slug = $1
        "#,
        slug
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get organization: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Organization not found: {}", slug)))?;

    let id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing organization ID".to_string()))?;

    Ok(WikiOrganization {
        id,
        slug: row.slug.clone(),
        canonical_name: row.canonical_name.clone(),
        content: row.content.clone(),
        cover_image: row.cover_image.clone(),
        organization_type: row.organization_type.clone(),
        relationship_type: row.relationship_type.clone(),
        role_title: row.role_title.clone(),
        start_date: row
            .start_date
            .as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        end_date: row
            .end_date
            .as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        interaction_count: row.interaction_count.map(|v| v as i32),
        first_interaction: row
            .first_interaction
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        last_interaction: row
            .last_interaction
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// Get an organization by ID
pub async fn get_organization(pool: &SqlitePool, id: String) -> Result<WikiOrganization> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, slug, canonical_name, content, cover_image,
            organization_type, relationship_type, role_title,
            start_date, end_date, interaction_count,
            first_interaction, last_interaction,
            created_at, updated_at
        FROM wiki_orgs
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get organization: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Organization not found: {}", id)))?;

    // Get ID as string
    let row_id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing organization ID".to_string()))?;

    Ok(WikiOrganization {
        id: row_id,
        slug: row.slug.clone(),
        canonical_name: row.canonical_name.clone(),
        content: row.content.clone(),
        cover_image: row.cover_image.clone(),
        organization_type: row.organization_type.clone(),
        relationship_type: row.relationship_type.clone(),
        role_title: row.role_title.clone(),
        start_date: row
            .start_date
            .as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        end_date: row
            .end_date
            .as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        interaction_count: row.interaction_count.map(|v| v as i32),
        first_interaction: row
            .first_interaction
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        last_interaction: row
            .last_interaction
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// List all organizations
pub async fn list_organizations(pool: &SqlitePool) -> Result<Vec<WikiOrganizationListItem>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id, slug, canonical_name, organization_type, relationship_type
        FROM wiki_orgs
        ORDER BY canonical_name ASC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list organizations: {}", e)))?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let id = row.id.clone()?;
            Some(WikiOrganizationListItem {
                id,
                slug: row.slug.clone(),
                canonical_name: row.canonical_name.clone(),
                organization_type: row.organization_type.clone(),
                relationship_type: row.relationship_type.clone(),
            })
        })
        .collect())
}

/// Update an organization
pub async fn update_organization(
    pool: &SqlitePool,
    id: String,
    req: UpdateWikiOrganizationRequest,
) -> Result<WikiOrganization> {
    sqlx::query!(
        r#"
        UPDATE wiki_orgs
        SET
            slug = COALESCE($2, slug),
            canonical_name = COALESCE($3, canonical_name),
            content = COALESCE($4, content),
            cover_image = COALESCE($5, cover_image),
            organization_type = COALESCE($6, organization_type),
            relationship_type = COALESCE($7, relationship_type),
            role_title = COALESCE($8, role_title),
            start_date = COALESCE($9, start_date),
            end_date = COALESCE($10, end_date),
            updated_at = datetime('now')
        WHERE id = $1
        "#,
        id,
        req.slug,
        req.canonical_name,
        req.content,
        req.cover_image,
        req.organization_type,
        req.relationship_type,
        req.role_title,
        req.start_date,
        req.end_date
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update organization: {}", e)))?;

    get_organization(pool, id).await
}

// ============================================================================
// Thing CRUD Operations
// ============================================================================

/// Get a thing by slug
pub async fn get_thing_by_slug(pool: &SqlitePool, slug: &str) -> Result<WikiThing> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, slug, canonical_name, content, cover_image,
            thing_type, description,
            first_mentioned, last_mentioned, mention_count,
            created_at, updated_at
        FROM wiki_things
        WHERE slug = $1
        "#,
        slug
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get thing: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Thing not found: {}", slug)))?;

    let id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing thing ID".to_string()))?;

    Ok(WikiThing {
        id,
        slug: row.slug.clone(),
        canonical_name: row.canonical_name.clone(),
        content: row.content.clone(),
        cover_image: row.cover_image.clone(),
        thing_type: row.thing_type.clone(),
        description: row.description.clone(),
        first_mentioned: row
            .first_mentioned
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        last_mentioned: row
            .last_mentioned
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        mention_count: row.mention_count.map(|v| v as i32),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// Get a thing by ID
pub async fn get_thing(pool: &SqlitePool, id: String) -> Result<WikiThing> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, slug, canonical_name, content, cover_image,
            thing_type, description,
            first_mentioned, last_mentioned, mention_count,
            created_at, updated_at
        FROM wiki_things
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get thing: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Thing not found: {}", id)))?;

    // Get ID as string
    let row_id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing thing ID".to_string()))?;

    Ok(WikiThing {
        id: row_id,
        slug: row.slug.clone(),
        canonical_name: row.canonical_name.clone(),
        content: row.content.clone(),
        cover_image: row.cover_image.clone(),
        thing_type: row.thing_type.clone(),
        description: row.description.clone(),
        first_mentioned: row
            .first_mentioned
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        last_mentioned: row
            .last_mentioned
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        mention_count: row.mention_count.map(|v| v as i32),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// List all things
pub async fn list_things(pool: &SqlitePool) -> Result<Vec<WikiThingListItem>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id, slug, canonical_name, thing_type
        FROM wiki_things
        ORDER BY canonical_name ASC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list things: {}", e)))?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let id = row.id.clone()?;
            Some(WikiThingListItem {
                id,
                slug: row.slug.clone(),
                canonical_name: row.canonical_name.clone(),
                thing_type: row.thing_type.clone(),
            })
        })
        .collect())
}

/// Update a thing
pub async fn update_thing(
    pool: &SqlitePool,
    id: String,
    req: UpdateWikiThingRequest,
) -> Result<WikiThing> {
    sqlx::query!(
        r#"
        UPDATE wiki_things
        SET
            slug = COALESCE($2, slug),
            canonical_name = COALESCE($3, canonical_name),
            content = COALESCE($4, content),
            cover_image = COALESCE($5, cover_image),
            thing_type = COALESCE($6, thing_type),
            description = COALESCE($7, description),
            updated_at = datetime('now')
        WHERE id = $1
        "#,
        id,
        req.slug,
        req.canonical_name,
        req.content,
        req.cover_image,
        req.thing_type,
        req.description
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update thing: {}", e)))?;

    get_thing(pool, id).await
}

// ============================================================================
// Telos CRUD Operations
// ============================================================================

/// Get active telos
pub async fn get_active_telos(pool: &SqlitePool) -> Result<Option<WikiTelos>> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, slug, title, description, content, cover_image, is_active,
            created_at, updated_at
        FROM narrative_telos
        WHERE is_active = true
        "#
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get active telos: {}", e)))?;

    Ok(row.and_then(|r| {
        let id = r.id.clone()?;
        Some(WikiTelos {
            id,
            slug: r.slug.clone(),
            title: r.title.clone(),
            description: r.description.clone(),
            content: r.content.clone(),
            cover_image: r.cover_image.clone(),
            is_active: r.is_active.map(|v| v != 0),
            created_at: DateTime::parse_from_rfc3339(&r.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&r.updated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
    }))
}

/// Get a telos by slug
pub async fn get_telos_by_slug(pool: &SqlitePool, slug: &str) -> Result<WikiTelos> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, slug, title, description, content, cover_image, is_active,
            created_at, updated_at
        FROM narrative_telos
        WHERE slug = $1
        "#,
        slug
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get telos: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Telos not found: {}", slug)))?;

    let id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing telos ID".to_string()))?;

    Ok(WikiTelos {
        id,
        slug: row.slug.clone(),
        title: row.title.clone(),
        description: row.description.clone(),
        content: row.content.clone(),
        cover_image: row.cover_image.clone(),
        is_active: row.is_active.map(|v| v != 0),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

// ============================================================================
// Act CRUD Operations
// ============================================================================

/// Get an act by slug
pub async fn get_act_by_slug(pool: &SqlitePool, slug: &str) -> Result<WikiAct> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, slug, title, subtitle, description, content, cover_image, location,
            start_date, end_date, sort_order, telos_id, themes,
            created_at, updated_at
        FROM narrative_acts
        WHERE slug = $1
        "#,
        slug
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get act: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Act not found: {}", slug)))?;

    let id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing act ID".to_string()))?;

    Ok(WikiAct {
        id,
        slug: row.slug.clone(),
        title: row.title.clone(),
        subtitle: row.subtitle.clone(),
        description: row.description.clone(),
        content: row.content.clone(),
        cover_image: row.cover_image.clone(),
        location: row.location.clone(),
        start_date: NaiveDate::parse_from_str(&row.start_date, "%Y-%m-%d")
            .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()),
        end_date: row
            .end_date
            .as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        sort_order: row.sort_order as i32,
        telos_id: row.telos_id.clone(),
        themes: row
            .themes
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok()),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// Get an act by ID
pub async fn get_act(pool: &SqlitePool, id: String) -> Result<WikiAct> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, slug, title, subtitle, description, content, cover_image, location,
            start_date, end_date, sort_order, telos_id, themes,
            created_at, updated_at
        FROM narrative_acts
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get act: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Act not found: {}", id)))?;

    // Get ID as string
    let row_id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing act ID".to_string()))?;

    Ok(WikiAct {
        id: row_id,
        slug: row.slug.clone(),
        title: row.title.clone(),
        subtitle: row.subtitle.clone(),
        description: row.description.clone(),
        content: row.content.clone(),
        cover_image: row.cover_image.clone(),
        location: row.location.clone(),
        start_date: NaiveDate::parse_from_str(&row.start_date, "%Y-%m-%d")
            .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()),
        end_date: row
            .end_date
            .as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        sort_order: row.sort_order as i32,
        telos_id: row.telos_id.clone(),
        themes: row
            .themes
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok()),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// List all acts
pub async fn list_acts(pool: &SqlitePool) -> Result<Vec<WikiAct>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id, slug, title, subtitle, description, content, cover_image, location,
            start_date, end_date, sort_order, telos_id, themes,
            created_at, updated_at
        FROM narrative_acts
        ORDER BY sort_order ASC, start_date ASC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list acts: {}", e)))?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let id = row.id.clone()?;
            Some(WikiAct {
                id,
                slug: row.slug.clone(),
                title: row.title.clone(),
                subtitle: row.subtitle.clone(),
                description: row.description.clone(),
                content: row.content.clone(),
                cover_image: row.cover_image.clone(),
                location: row.location.clone(),
                start_date: NaiveDate::parse_from_str(&row.start_date, "%Y-%m-%d")
                    .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()),
                end_date: row
                    .end_date
                    .as_ref()
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
                sort_order: row.sort_order as i32,
                telos_id: row.telos_id.clone(),
                themes: row
                    .themes
                    .as_ref()
                    .and_then(|s| serde_json::from_str(s).ok()),
                created_at: DateTime::parse_from_rfc3339(&row.created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })
        .collect())
}

// ============================================================================
// Chapter CRUD Operations
// ============================================================================

/// Get a chapter by slug
pub async fn get_chapter_by_slug(pool: &SqlitePool, slug: &str) -> Result<WikiChapter> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, slug, title, subtitle, description, content, cover_image,
            start_date, end_date, sort_order, act_id, themes,
            created_at, updated_at
        FROM narrative_chapters
        WHERE slug = $1
        "#,
        slug
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get chapter: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Chapter not found: {}", slug)))?;

    let id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing chapter ID".to_string()))?;

    Ok(WikiChapter {
        id,
        slug: row.slug.clone(),
        title: row.title.clone(),
        subtitle: row.subtitle.clone(),
        description: row.description.clone(),
        content: row.content.clone(),
        cover_image: row.cover_image.clone(),
        start_date: NaiveDate::parse_from_str(&row.start_date, "%Y-%m-%d")
            .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()),
        end_date: row
            .end_date
            .as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        sort_order: row.sort_order as i32,
        act_id: row.act_id.clone(),
        themes: row
            .themes
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok()),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// Get a chapter by ID
pub async fn get_chapter(pool: &SqlitePool, id: String) -> Result<WikiChapter> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, slug, title, subtitle, description, content, cover_image,
            start_date, end_date, sort_order, act_id, themes,
            created_at, updated_at
        FROM narrative_chapters
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get chapter: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Chapter not found: {}", id)))?;

    // Get ID as string
    let row_id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing chapter ID".to_string()))?;

    Ok(WikiChapter {
        id: row_id,
        slug: row.slug.clone(),
        title: row.title.clone(),
        subtitle: row.subtitle.clone(),
        description: row.description.clone(),
        content: row.content.clone(),
        cover_image: row.cover_image.clone(),
        start_date: NaiveDate::parse_from_str(&row.start_date, "%Y-%m-%d")
            .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()),
        end_date: row
            .end_date
            .as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        sort_order: row.sort_order as i32,
        act_id: row.act_id.clone(),
        themes: row
            .themes
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok()),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// List chapters for an act
pub async fn list_chapters_for_act(pool: &SqlitePool, act_id: String) -> Result<Vec<WikiChapter>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id, slug, title, subtitle, description, content, cover_image,
            start_date, end_date, sort_order, act_id, themes,
            created_at, updated_at
        FROM narrative_chapters
        WHERE act_id = $1
        ORDER BY sort_order ASC, start_date ASC
        "#,
        act_id
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list chapters: {}", e)))?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let id = row.id.clone()?;
            Some(WikiChapter {
                id,
                slug: row.slug.clone(),
                title: row.title.clone(),
                subtitle: row.subtitle.clone(),
                description: row.description.clone(),
                content: row.content.clone(),
                cover_image: row.cover_image.clone(),
                start_date: NaiveDate::parse_from_str(&row.start_date, "%Y-%m-%d")
                    .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()),
                end_date: row
                    .end_date
                    .as_ref()
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
                sort_order: row.sort_order as i32,
                act_id: row.act_id.clone(),
                themes: row
                    .themes
                    .as_ref()
                    .and_then(|s| serde_json::from_str(s).ok()),
                created_at: DateTime::parse_from_rfc3339(&row.created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })
        .collect())
}

// ============================================================================
// Day CRUD Operations
// ============================================================================

/// Get a day by date (creates if not exists)
pub async fn get_or_create_day(pool: &SqlitePool, date: NaiveDate) -> Result<WikiDay> {
    let date_str = date.format("%Y-%m-%d").to_string();

    // Try to get existing day
    let existing = sqlx::query!(
        r#"
        SELECT
            id, date, start_timezone, end_timezone, autobiography, autobiography_sections,
            last_edited_by, cover_image, act_id, chapter_id,
            created_at, updated_at
        FROM wiki_days
        WHERE date = $1
        "#,
        date_str
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get day: {}", e)))?;

    if let Some(row) = existing {
        let id = row
            .id
            .clone()
            .ok_or_else(|| Error::Database("Missing day ID".to_string()))?;
        return Ok(WikiDay {
            id,
            date,
            start_timezone: row.start_timezone.clone(),
            end_timezone: row.end_timezone.clone(),
            autobiography: row.autobiography.clone(),
            autobiography_sections: row
                .autobiography_sections
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok()),
            last_edited_by: row.last_edited_by.clone(),
            cover_image: row.cover_image.clone(),
            act_id: row.act_id.clone(),
            chapter_id: row.chapter_id.clone(),
            created_at: DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        });
    }

    // Create new day
    let day_id = ids::generate_id(ids::WIKI_DAY_PREFIX, &[&date_str]);
    let row = sqlx::query!(
        r#"
        INSERT INTO wiki_days (id, date)
        VALUES ($1, $2)
        RETURNING
            id, date, start_timezone, end_timezone, autobiography, autobiography_sections,
            last_edited_by, cover_image, act_id, chapter_id,
            created_at, updated_at
        "#,
        day_id,
        date_str
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create day: {}", e)))?;

    let id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing day ID".to_string()))?;

    Ok(WikiDay {
        id,
        date,
        start_timezone: row.start_timezone.clone(),
        end_timezone: row.end_timezone.clone(),
        autobiography: row.autobiography.clone(),
        autobiography_sections: row
            .autobiography_sections
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok()),
        last_edited_by: Some(row.last_edited_by.clone()),
        cover_image: row.cover_image.clone(),
        act_id: row.act_id.clone(),
        chapter_id: row.chapter_id.clone(),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// Update a day
pub async fn update_day(
    pool: &SqlitePool,
    date: NaiveDate,
    req: UpdateWikiDayRequest,
) -> Result<WikiDay> {
    // Get or create the day first
    let day = get_or_create_day(pool, date).await?;
    let day_id_str = day.id.to_string();
    let autobiography_sections_json = req
        .autobiography_sections
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()));

    sqlx::query!(
        r#"
        UPDATE wiki_days
        SET
            autobiography = COALESCE($2, autobiography),
            autobiography_sections = COALESCE($3, autobiography_sections),
            last_edited_by = COALESCE($4, last_edited_by),
            cover_image = COALESCE($5, cover_image),
            updated_at = datetime('now')
        WHERE id = $1
        "#,
        day_id_str,
        req.autobiography,
        autobiography_sections_json,
        req.last_edited_by,
        req.cover_image
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update day: {}", e)))?;

    get_or_create_day(pool, date).await
}

/// List days in a date range
pub async fn list_days(
    pool: &SqlitePool,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Vec<WikiDay>> {
    let start_str = start_date.format("%Y-%m-%d").to_string();
    let end_str = end_date.format("%Y-%m-%d").to_string();

    let rows = sqlx::query!(
        r#"
        SELECT
            id, date, start_timezone, end_timezone, autobiography, autobiography_sections,
            last_edited_by, cover_image, act_id, chapter_id,
            created_at, updated_at
        FROM wiki_days
        WHERE date >= $1 AND date <= $2
        ORDER BY date DESC
        "#,
        start_str,
        end_str
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list days: {}", e)))?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let id = row.id.clone()?;
            let date = NaiveDate::parse_from_str(&row.date, "%Y-%m-%d").ok()?;
            Some(WikiDay {
                id,
                date,
                start_timezone: row.start_timezone.clone(),
                end_timezone: row.end_timezone.clone(),
                autobiography: row.autobiography.clone(),
                autobiography_sections: row
                    .autobiography_sections
                    .as_ref()
                    .and_then(|s| serde_json::from_str(s).ok()),
                last_edited_by: row.last_edited_by.clone(),
                cover_image: row.cover_image.clone(),
                act_id: row.act_id.clone(),
                chapter_id: row.chapter_id.clone(),
                created_at: DateTime::parse_from_rfc3339(&row.created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })
        .collect())
}

// ============================================================================
// Slug Resolution - Find entity type by slug
// ============================================================================

/// Result of resolving a slug to its entity type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlugResolution {
    pub entity_type: String,
    pub id: String,
}

/// Resolve a slug to find which entity type it belongs to
pub async fn resolve_slug(pool: &SqlitePool, slug: &str) -> Result<SlugResolution> {
    // Check if it's a date (day slug)
    if let Ok(date) = slug.parse::<NaiveDate>() {
        let day = get_or_create_day(pool, date).await?;
        return Ok(SlugResolution {
            entity_type: "day".to_string(),
            id: day.id,
        });
    }

    // Check person
    if let Ok(person) = get_person_by_slug(pool, slug).await {
        return Ok(SlugResolution {
            entity_type: "person".to_string(),
            id: person.id,
        });
    }

    // Check place
    if let Ok(place) = get_place_by_slug(pool, slug).await {
        return Ok(SlugResolution {
            entity_type: "place".to_string(),
            id: place.id,
        });
    }

    // Check organization
    if let Ok(org) = get_organization_by_slug(pool, slug).await {
        return Ok(SlugResolution {
            entity_type: "organization".to_string(),
            id: org.id,
        });
    }

    // Check thing
    if let Ok(thing) = get_thing_by_slug(pool, slug).await {
        return Ok(SlugResolution {
            entity_type: "thing".to_string(),
            id: thing.id,
        });
    }

    // Check telos
    if let Ok(telos) = get_telos_by_slug(pool, slug).await {
        return Ok(SlugResolution {
            entity_type: "telos".to_string(),
            id: telos.id,
        });
    }

    // Check act
    if let Ok(act) = get_act_by_slug(pool, slug).await {
        return Ok(SlugResolution {
            entity_type: "act".to_string(),
            id: act.id,
        });
    }

    // Check chapter
    if let Ok(chapter) = get_chapter_by_slug(pool, slug).await {
        return Ok(SlugResolution {
            entity_type: "chapter".to_string(),
            id: chapter.id,
        });
    }

    Err(Error::NotFound(format!(
        "No entity found with slug: {}",
        slug
    )))
}

// ============================================================================
// Citation Types
// ============================================================================

/// A citation linking wiki content to ontology data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub id: String,
    pub source_type: String,
    pub source_id: String,
    pub target_table: String,
    pub target_id: String,
    pub citation_index: i32,
    pub label: Option<String>,
    pub preview: Option<String>,
    pub is_hidden: Option<bool>,
    pub added_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to create a citation
#[derive(Debug, Deserialize)]
pub struct CreateCitationRequest {
    pub source_type: String,
    pub source_id: String,
    pub target_table: String,
    pub target_id: String,
    pub citation_index: i32,
    pub label: Option<String>,
    pub preview: Option<String>,
    pub is_hidden: Option<bool>,
    pub added_by: Option<String>,
}

/// Request to update a citation
#[derive(Debug, Deserialize)]
pub struct UpdateCitationRequest {
    pub label: Option<String>,
    pub preview: Option<String>,
    pub is_hidden: Option<bool>,
}

// ============================================================================
// Citation CRUD Operations
// ============================================================================

/// Get citations for a wiki page
pub async fn get_citations(
    pool: &SqlitePool,
    source_type: &str,
    source_id: String,
) -> Result<Vec<Citation>> {
    let source_id_str = source_id;

    let rows = sqlx::query!(
        r#"
        SELECT
            id, source_type, source_id, target_table, target_id,
            citation_index, label, preview, is_hidden, added_by,
            created_at, updated_at
        FROM wiki_citations
        WHERE source_type = $1 AND source_id = $2
        ORDER BY citation_index ASC
        "#,
        source_type,
        source_id_str
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get citations: {}", e)))?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let id = row.id.clone()?;
            let source_id = row.source_id.clone();
            let target_id = row.target_id.clone();
            Some(Citation {
                id,
                source_type: row.source_type.clone(),
                source_id,
                target_table: row.target_table.clone(),
                target_id,
                citation_index: row.citation_index as i32,
                label: row.label.clone(),
                preview: row.preview.clone(),
                is_hidden: row.is_hidden.map(|v| v != 0),
                added_by: row.added_by.clone(),
                created_at: DateTime::parse_from_rfc3339(&row.created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })
        .collect())
}

/// Create a citation
pub async fn create_citation(pool: &SqlitePool, req: CreateCitationRequest) -> Result<Citation> {
    let source_id_str = req.source_id.clone();
    let target_id_str = req.target_id.clone();
    let added_by = req.added_by.unwrap_or_else(|| "ai".to_string());

    let timestamp = Utc::now().to_rfc3339();
    let citation_id = ids::generate_id(ids::WIKI_CITATION_PREFIX, &[&source_id_str, &target_id_str, &timestamp]);
    let row = sqlx::query!(
        r#"
        INSERT INTO wiki_citations (
            id, source_type, source_id, target_table, target_id,
            citation_index, label, preview, is_hidden, added_by
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        RETURNING
            id, source_type, source_id, target_table, target_id,
            citation_index, label, preview, is_hidden, added_by,
            created_at, updated_at
        "#,
        citation_id,
        req.source_type,
        source_id_str,
        req.target_table,
        target_id_str,
        req.citation_index,
        req.label,
        req.preview,
        req.is_hidden,
        added_by
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create citation: {}", e)))?;

    let id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing citation ID".to_string()))?;

    Ok(Citation {
        id,
        source_type: row.source_type.clone(),
        source_id: req.source_id,
        target_table: row.target_table.clone(),
        target_id: req.target_id,
        citation_index: row.citation_index as i32,
        label: row.label.clone(),
        preview: row.preview.clone(),
        is_hidden: row.is_hidden.map(|v| v != 0),
        added_by: row.added_by.clone(),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// Update a citation
pub async fn update_citation(
    pool: &SqlitePool,
    id: String,
    req: UpdateCitationRequest,
) -> Result<Citation> {
    sqlx::query(
        r#"
        UPDATE wiki_citations
        SET
            label = COALESCE($2, label),
            preview = COALESCE($3, preview),
            is_hidden = COALESCE($4, is_hidden),
            updated_at = datetime('now')
        WHERE id = $1
        "#,
    )
    .bind(&id)
    .bind(&req.label)
    .bind(&req.preview)
    .bind(&req.is_hidden)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update citation: {}", e)))?;

    // Get updated citation
    let row = sqlx::query(
        r#"
        SELECT
            id, source_type, source_id, target_table, target_id,
            citation_index, label, preview, is_hidden, added_by,
            created_at, updated_at
        FROM wiki_citations
        WHERE id = $1
        "#,
    )
    .bind(&id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get updated citation: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Citation not found: {}", id)))?;

    use sqlx::Row;
    Ok(Citation {
        id: row.try_get("id").map_err(|e| Error::Database(e.to_string()))?,
        source_type: row.try_get("source_type").map_err(|e| Error::Database(e.to_string()))?,
        source_id: row.try_get("source_id").map_err(|e| Error::Database(e.to_string()))?,
        target_table: row.try_get("target_table").map_err(|e| Error::Database(e.to_string()))?,
        target_id: row.try_get("target_id").map_err(|e| Error::Database(e.to_string()))?,
        citation_index: row.try_get::<i32, _>("citation_index").map_err(|e| Error::Database(e.to_string()))?,
        label: row.try_get("label").ok(),
        preview: row.try_get("preview").ok(),
        is_hidden: row.try_get::<Option<bool>, _>("is_hidden").ok().flatten(),
        added_by: row.try_get("added_by").ok(),
        created_at: DateTime::parse_from_rfc3339(&row.try_get::<String, _>("created_at").map_err(|e| Error::Database(e.to_string()))?)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.try_get::<String, _>("updated_at").map_err(|e| Error::Database(e.to_string()))?)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// Delete a citation
pub async fn delete_citation(pool: &SqlitePool, id: String) -> Result<()> {
    let id_str = id.clone();

    let result = sqlx::query!("DELETE FROM wiki_citations WHERE id = $1", id_str)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete citation: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Citation not found: {}", id)));
    }

    Ok(())
}

// ============================================================================
// Temporal Event Types
// ============================================================================

/// A temporal event in a day timeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalEvent {
    pub id: String,
    pub day_id: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub auto_label: Option<String>,
    pub auto_location: Option<String>,
    pub user_label: Option<String>,
    pub user_location: Option<String>,
    pub user_notes: Option<String>,
    pub source_ontologies: Option<serde_json::Value>,
    pub is_unknown: Option<bool>,
    pub is_transit: Option<bool>,
    pub is_user_added: Option<bool>,
    pub is_user_edited: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to create a temporal event
#[derive(Debug, Deserialize)]
pub struct CreateTemporalEventRequest {
    pub day_id: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub auto_label: Option<String>,
    pub auto_location: Option<String>,
    pub user_label: Option<String>,
    pub user_location: Option<String>,
    pub user_notes: Option<String>,
    pub source_ontologies: Option<serde_json::Value>,
    pub is_unknown: Option<bool>,
    pub is_transit: Option<bool>,
    pub is_user_added: Option<bool>,
}

/// Request to update a temporal event
#[derive(Debug, Deserialize)]
pub struct UpdateTemporalEventRequest {
    pub user_label: Option<String>,
    pub user_location: Option<String>,
    pub user_notes: Option<String>,
}

// ============================================================================
// Temporal Event CRUD Operations
// ============================================================================

/// Get events for a day
pub async fn get_day_events(pool: &SqlitePool, day_id: String) -> Result<Vec<TemporalEvent>> {
    let day_id_str = day_id;

    let rows = sqlx::query!(
        r#"
        SELECT
            id, day_id, start_time, end_time,
            auto_label, auto_location, user_label, user_location, user_notes,
            source_ontologies, is_unknown, is_transit, is_user_added, is_user_edited,
            created_at, updated_at
        FROM wiki_events
        WHERE day_id = $1
        ORDER BY start_time ASC
        "#,
        day_id_str
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get day events: {}", e)))?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let id = row.id.clone()?;
            let day_id = row.day_id.clone();
            Some(TemporalEvent {
                id,
                day_id,
                start_time: DateTime::parse_from_rfc3339(&row.start_time)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                end_time: DateTime::parse_from_rfc3339(&row.end_time)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                auto_label: row.auto_label.clone(),
                auto_location: row.auto_location.clone(),
                user_label: row.user_label.clone(),
                user_location: row.user_location.clone(),
                user_notes: row.user_notes.clone(),
                source_ontologies: row
                    .source_ontologies
                    .as_ref()
                    .and_then(|s| serde_json::from_str(s).ok()),
                is_unknown: row.is_unknown.map(|v| v != 0),
                is_transit: row.is_transit.map(|v| v != 0),
                is_user_added: row.is_user_added.map(|v| v != 0),
                is_user_edited: row.is_user_edited.map(|v| v != 0),
                created_at: DateTime::parse_from_rfc3339(&row.created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })
        .collect())
}

/// Get events for a day by date
pub async fn get_events_by_date(pool: &SqlitePool, date: NaiveDate) -> Result<Vec<TemporalEvent>> {
    let day = get_or_create_day(pool, date).await?;
    get_day_events(pool, day.id).await
}

/// Create a temporal event
pub async fn create_temporal_event(
    pool: &SqlitePool,
    req: CreateTemporalEventRequest,
) -> Result<TemporalEvent> {
    let day_id_str = req.day_id.to_string();
    let start_time_str = req.start_time.to_rfc3339();
    let end_time_str = req.end_time.to_rfc3339();
    let source_ontologies_str = req
        .source_ontologies
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "null".to_string()));

    let event_id = ids::generate_id(ids::WIKI_EVENT_PREFIX, &[&req.day_id, &start_time_str, &end_time_str]);
    let row = sqlx::query!(
        r#"
        INSERT INTO wiki_events (
            id, day_id, start_time, end_time,
            auto_label, auto_location, user_label, user_location, user_notes,
            source_ontologies, is_unknown, is_transit, is_user_added
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        RETURNING
            id, day_id, start_time, end_time,
            auto_label, auto_location, user_label, user_location, user_notes,
            source_ontologies, is_unknown, is_transit, is_user_added, is_user_edited,
            created_at, updated_at
        "#,
        event_id,
        day_id_str,
        start_time_str,
        end_time_str,
        req.auto_label,
        req.auto_location,
        req.user_label,
        req.user_location,
        req.user_notes,
        source_ontologies_str,
        req.is_unknown,
        req.is_transit,
        req.is_user_added
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create temporal event: {}", e)))?;

    let id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing event ID".to_string()))?;

    Ok(TemporalEvent {
        id,
        day_id: req.day_id,
        start_time: req.start_time,
        end_time: req.end_time,
        auto_label: row.auto_label.clone(),
        auto_location: row.auto_location.clone(),
        user_label: row.user_label.clone(),
        user_location: row.user_location.clone(),
        user_notes: row.user_notes.clone(),
        source_ontologies: row
            .source_ontologies
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok()),
        is_unknown: row.is_unknown.map(|v| v != 0),
        is_transit: row.is_transit.map(|v| v != 0),
        is_user_added: row.is_user_added.map(|v| v != 0),
        is_user_edited: row.is_user_edited.map(|v| v != 0),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// Update a temporal event (user edits)
pub async fn update_temporal_event(
    pool: &SqlitePool,
    id: String,
    req: UpdateTemporalEventRequest,
) -> Result<TemporalEvent> {
    let id_str = id.clone();

    let row = sqlx::query!(
        r#"
        UPDATE wiki_events
        SET
            user_label = COALESCE($2, user_label),
            user_location = COALESCE($3, user_location),
            user_notes = COALESCE($4, user_notes),
            is_user_edited = true,
            updated_at = datetime('now')
        WHERE id = $1
        RETURNING
            id, day_id, start_time, end_time,
            auto_label, auto_location, user_label, user_location, user_notes,
            source_ontologies, is_unknown, is_transit, is_user_added, is_user_edited,
            created_at, updated_at
        "#,
        id_str,
        req.user_label,
        req.user_location,
        req.user_notes
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update temporal event: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Temporal event not found: {}", id)))?;

    let parsed_id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing event ID".to_string()))?;
    let day_id = row.day_id.clone();

    Ok(TemporalEvent {
        id: parsed_id,
        day_id,
        start_time: DateTime::parse_from_rfc3339(&row.start_time)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        end_time: DateTime::parse_from_rfc3339(&row.end_time)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        auto_label: row.auto_label.clone(),
        auto_location: row.auto_location.clone(),
        user_label: row.user_label.clone(),
        user_location: row.user_location.clone(),
        user_notes: row.user_notes.clone(),
        source_ontologies: row
            .source_ontologies
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok()),
        is_unknown: row.is_unknown.map(|v| v != 0),
        is_transit: row.is_transit.map(|v| v != 0),
        is_user_added: row.is_user_added.map(|v| v != 0),
        is_user_edited: row.is_user_edited.map(|v| v != 0),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// Delete a temporal event
pub async fn delete_temporal_event(pool: &SqlitePool, id: String) -> Result<()> {
    let id_str = id.clone();

    let result = sqlx::query!("DELETE FROM wiki_events WHERE id = $1", id_str)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete temporal event: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Temporal event not found: {}", id)));
    }

    Ok(())
}

/// Delete all auto-generated events for a day (for regeneration)
pub async fn delete_auto_events_for_day(pool: &SqlitePool, day_id: String) -> Result<u64> {
    let day_id_str = day_id;

    let result = sqlx::query!(
        r#"
        DELETE FROM wiki_events
        WHERE day_id = $1 AND is_user_added = false
        "#,
        day_id_str
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to delete auto events: {}", e)))?;

    Ok(result.rows_affected())
}

// ============================================================================
// Day Sources - Ontology records for a day
// ============================================================================

/// A data source record from an ontology table for a specific day
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaySource {
    pub source_type: String,
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub label: String,
    pub preview: Option<String>,
}

/// Get all ontology data sources for a specific date
pub async fn get_day_sources(pool: &SqlitePool, date: NaiveDate) -> Result<Vec<DaySource>> {
    // Calculate UTC bounds for the date
    // We expand the window to cover any timezone: from midnight UTC to noon next day UTC
    // This ensures we catch events in timezones up to UTC-12 and UTC+14
    // Example: For 2025-12-17, query from 2025-12-17 00:00 UTC to 2025-12-18 12:00 UTC
    let start_of_day = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let end_of_day = date
        .succ_opt()
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap()
        .and_utc();

    let start_str = start_of_day.to_rfc3339();
    let end_str = end_of_day.to_rfc3339();

    let mut sources: Vec<DaySource> = Vec::new();

    // Calendar events
    let calendar_rows = sqlx::query!(
        r#"
        SELECT id, start_time, title
        FROM data_calendar
        WHERE start_time >= $1 AND start_time <= $2
        ORDER BY start_time ASC
        "#,
        start_str,
        end_str
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get calendar sources: {}", e)))?;

    for row in calendar_rows {
        if let Some(id_str) = row.id.as_ref() {
            if let Ok(ts) = DateTime::parse_from_rfc3339(&row.start_time) {
                sources.push(DaySource {
                    source_type: "calendar".to_string(),
                    id: id_str.clone(),
                    timestamp: ts.with_timezone(&Utc),
                    label: row.title.clone(),
                    preview: None,
                });
            }
        }
    }

    // Emails
    let email_rows = sqlx::query!(
        r#"
        SELECT id, timestamp, subject, from_email, direction
        FROM data_social_email
        WHERE timestamp >= $1 AND timestamp <= $2
        ORDER BY timestamp ASC
        "#,
        start_str,
        end_str
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get email sources: {}", e)))?;

    for row in email_rows {
        if let Some(id_str) = row.id.as_ref() {
            if let Ok(ts) = DateTime::parse_from_rfc3339(&row.timestamp) {
                let direction_prefix = if row.direction == "sent" {
                    "To"
                } else {
                    "From"
                };
                let label = row
                    .subject
                    .clone()
                    .unwrap_or_else(|| "(no subject)".to_string());
                sources.push(DaySource {
                    source_type: "email".to_string(),
                    id: id_str.clone(),
                    timestamp: ts.with_timezone(&Utc),
                    label,
                    preview: Some(format!("{}: {}", direction_prefix, row.from_email)),
                });
            }
        }
    }

    // Location visits
    let visit_rows = sqlx::query!(
        r#"
        SELECT id, arrival_time, place_name, duration_minutes
        FROM data_location_visit
        WHERE arrival_time >= $1 AND arrival_time <= $2
        ORDER BY arrival_time ASC
        "#,
        start_str,
        end_str
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get location visit sources: {}", e)))?;

    for row in visit_rows {
        if let (Some(id_str), Ok(ts)) = (
            row.id.as_ref(),
            DateTime::parse_from_rfc3339(&row.arrival_time),
        ) {
            let label = row
                .place_name
                .clone()
                .unwrap_or_else(|| "Unknown location".to_string());
            let preview = row.duration_minutes.map(|d| format!("{} min", d));
            sources.push(DaySource {
                source_type: "location".to_string(),
                id: id_str.clone(),
                timestamp: ts.with_timezone(&Utc),
                label,
                preview,
            });
        }
    }

    // Workouts
    let workout_rows = sqlx::query!(
        r#"
        SELECT id, start_time, workout_type, duration_minutes
        FROM data_health_workout
        WHERE start_time >= $1 AND start_time <= $2
        ORDER BY start_time ASC
        "#,
        start_str,
        end_str
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get workout sources: {}", e)))?;

    for row in workout_rows {
        if let (Some(id_str), Ok(ts)) = (
            row.id.as_ref(),
            DateTime::parse_from_rfc3339(&row.start_time),
        ) {
            let preview = row.duration_minutes.map(|d| format!("{} min", d));
            sources.push(DaySource {
                source_type: "workout".to_string(),
                id: id_str.clone(),
                timestamp: ts.with_timezone(&Utc),
                label: row.workout_type.clone(),
                preview,
            });
        }
    }

    // Sleep (check if end_time falls on this date)
    let sleep_rows = sqlx::query!(
        r#"
        SELECT id, end_time, duration_minutes, sleep_quality_score
        FROM data_health_sleep
        WHERE end_time >= $1 AND end_time <= $2
        ORDER BY end_time ASC
        "#,
        start_str,
        end_str
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get sleep sources: {}", e)))?;

    for row in sleep_rows {
        if let (Some(id_str), Ok(ts)) =
            (row.id.as_ref(), DateTime::parse_from_rfc3339(&row.end_time))
        {
            let duration_str = row.duration_minutes.map(|d| {
                let hours = d / 60;
                let mins = d % 60;
                format!("{}h {}m", hours, mins)
            });
            let preview = match (duration_str, row.sleep_quality_score) {
                (Some(d), Some(q)) => Some(format!("{}, quality: {:.0}", d, q)),
                (Some(d), None) => Some(d),
                _ => None,
            };
            sources.push(DaySource {
                source_type: "sleep".to_string(),
                id: id_str.clone(),
                timestamp: ts.with_timezone(&Utc),
                label: "Sleep".to_string(),
                preview,
            });
        }
    }

    // Financial transactions
    let transaction_rows = sqlx::query!(
        r#"
        SELECT id, timestamp, merchant_name, amount, description
        FROM data_financial_transaction
        WHERE timestamp >= $1 AND timestamp <= $2
        ORDER BY timestamp ASC
        "#,
        start_str,
        end_str
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get transaction sources: {}", e)))?;

    for row in transaction_rows {
        if let (Some(id_str), Ok(ts)) = (
            row.id.as_ref(),
            DateTime::parse_from_rfc3339(&row.timestamp),
        ) {
            let label = row
                .merchant_name
                .clone()
                .or(row.description.clone())
                .unwrap_or_else(|| "Transaction".to_string());
            let preview = Some(format!("${:.2}", row.amount));
            sources.push(DaySource {
                source_type: "transaction".to_string(),
                id: id_str.clone(),
                timestamp: ts.with_timezone(&Utc),
                label,
                preview,
            });
        }
    }

    // Messages
    let message_rows = sqlx::query!(
        r#"
        SELECT id, timestamp, channel, from_name, body
        FROM data_social_message
        WHERE timestamp >= $1 AND timestamp <= $2
        ORDER BY timestamp ASC
        LIMIT 50
        "#,
        start_str,
        end_str
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get message sources: {}", e)))?;

    for row in message_rows {
        if let (Some(id_str), Ok(ts)) = (
            row.id.as_ref(),
            DateTime::parse_from_rfc3339(&row.timestamp),
        ) {
            let label = row
                .from_name
                .clone()
                .unwrap_or_else(|| "Message".to_string());
            let preview = row.body.as_ref().map(|c| {
                if c.len() > 50 {
                    format!("{}...", &c[..50])
                } else {
                    c.clone()
                }
            });
            sources.push(DaySource {
                source_type: format!("message:{}", row.channel),
                id: id_str.clone(),
                timestamp: ts.with_timezone(&Utc),
                label,
                preview,
            });
        }
    }

    // Sort all sources by timestamp
    sources.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    Ok(sources)
}

// ============================================================================
// Day Streams - Dynamic Ontology Queries
// ============================================================================

/// A single record from an ontology table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamRecord {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub end_timestamp: Option<DateTime<Utc>>,
    pub preview: serde_json::Value,
}

/// Data stream from a single ontology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayStream {
    pub ontology_name: String,
    pub display_name: String,
    pub domain: String,
    pub count: usize,
    pub records: Vec<StreamRecord>,
}

/// Response for GET /api/wiki/day/{date}/streams
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayStreamsResponse {
    pub date: NaiveDate,
    pub queried_at: DateTime<Utc>,
    pub streams: Vec<DayStream>,
    pub total_count: usize,
}

/// Get all ontology data streams for a specific date
///
/// Dynamically queries all registered ontology tables using their
/// timestamp_column metadata to filter records for the given day.
pub async fn get_day_streams(pool: &SqlitePool, date: NaiveDate) -> Result<DayStreamsResponse> {
    use crate::ontologies::registry::list_ontologies;
    use sqlx::Row;

    // Calculate UTC bounds for the date
    // Expand window to cover any timezone: UTC-12 to UTC+14
    let start = date
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .checked_sub_signed(chrono::Duration::hours(12))
        .unwrap();
    let end = date
        .succ_opt()
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .checked_add_signed(chrono::Duration::hours(14))
        .unwrap();

    let start_str = start.to_rfc3339();
    let end_str = end.to_rfc3339();

    let ontologies = list_ontologies();
    let mut streams = Vec::new();

    for ontology in ontologies {
        // Skip non-time-series ontologies
        if should_skip_ontology_for_streams(ontology.name) {
            continue;
        }

        let table = format!("data_{}", ontology.table_name);
        let ts_col = ontology.timestamp_column;

        // Build SELECT clause for end timestamp if present
        let end_select = ontology
            .end_timestamp_column
            .map(|c| format!(", {} as end_ts", c))
            .unwrap_or_default();

        // Build dynamic query - select id, timestamps, and all other columns as JSON
        let sql = format!(
            "SELECT id, {ts_col} as ts{end_select}, * FROM {table}
             WHERE {ts_col} >= ?1 AND {ts_col} < ?2
             ORDER BY {ts_col} ASC
             LIMIT 100",
            ts_col = ts_col,
            end_select = end_select,
            table = table,
        );

        // Execute query with dynamic SQL
        let rows = match sqlx::query(&sql)
            .bind(&start_str)
            .bind(&end_str)
            .fetch_all(pool)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!(
                    "Failed to query {} for day streams: {}",
                    ontology.name,
                    e
                );
                continue;
            }
        };

        if rows.is_empty() {
            continue;
        }

        let mut records = Vec::new();
        for row in &rows {
            // Get id
            let id: String = row.try_get("id").unwrap_or_default();
            if id.is_empty() {
                continue;
            }

            // Get timestamp
            let ts_str: String = row.try_get("ts").unwrap_or_default();
            let timestamp = match DateTime::parse_from_rfc3339(&ts_str) {
                Ok(ts) => ts.with_timezone(&Utc),
                Err(_) => continue,
            };

            // Get end timestamp if present
            let end_timestamp = if ontology.end_timestamp_column.is_some() {
                row.try_get::<String, _>("end_ts")
                    .ok()
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|ts| ts.with_timezone(&Utc))
            } else {
                None
            };

            // Build preview from key columns based on ontology type
            let preview = build_preview_for_ontology(ontology.name, &row);

            records.push(StreamRecord {
                id,
                timestamp,
                end_timestamp,
                preview,
            });
        }

        if !records.is_empty() {
            streams.push(DayStream {
                ontology_name: ontology.name.to_string(),
                display_name: ontology.display_name.to_string(),
                domain: ontology.domain.to_string(),
                count: records.len(),
                records,
            });
        }
    }

    // Sort streams by domain for consistent ordering
    streams.sort_by(|a, b| a.domain.cmp(&b.domain));

    let total_count = streams.iter().map(|s| s.count).sum();

    Ok(DayStreamsResponse {
        date,
        queried_at: Utc::now(),
        streams,
        total_count,
    })
}

/// Check if an ontology should be skipped for day streams
fn should_skip_ontology_for_streams(name: &str) -> bool {
    // Skip entity tables (not time-series events)
    name.starts_with("entities_")
        // Skip financial accounts (reference data, not events)
        || name == "financial_account"
        // Skip location points (use visits instead)
        || name == "location_point"
}

/// Build a preview JSON object for a specific ontology type
fn build_preview_for_ontology(ontology_name: &str, row: &sqlx::sqlite::SqliteRow) -> serde_json::Value {
    use sqlx::Row;

    match ontology_name {
        "praxis_calendar" => {
            serde_json::json!({
                "title": row.try_get::<String, _>("title").ok(),
                "location": row.try_get::<String, _>("location_name").ok(),
            })
        }
        "social_email" => {
            serde_json::json!({
                "subject": row.try_get::<String, _>("subject").ok(),
                "from": row.try_get::<String, _>("from_email").ok(),
                "direction": row.try_get::<String, _>("direction").ok(),
            })
        }
        "social_message" => {
            let content: Option<String> = row.try_get("content").ok();
            let preview = content.map(|c| {
                if c.len() > 100 {
                    format!("{}...", &c[..100])
                } else {
                    c
                }
            });
            serde_json::json!({
                "from": row.try_get::<String, _>("from_name").ok(),
                "platform": row.try_get::<String, _>("platform").ok(),
                "preview": preview,
            })
        }
        "location_visit" => {
            serde_json::json!({
                "place_name": row.try_get::<String, _>("place_name").ok(),
                "duration_minutes": row.try_get::<i32, _>("duration_minutes").ok(),
            })
        }
        "health_workout" => {
            serde_json::json!({
                "workout_type": row.try_get::<String, _>("workout_type").ok(),
                "duration_minutes": row.try_get::<i32, _>("duration_minutes").ok(),
                "calories": row.try_get::<i32, _>("calories_burned").ok(),
            })
        }
        "health_sleep" => {
            serde_json::json!({
                "duration_minutes": row.try_get::<i32, _>("duration_minutes").ok(),
                "quality_score": row.try_get::<f64, _>("sleep_quality_score").ok(),
            })
        }
        "health_heart_rate" => {
            serde_json::json!({
                "bpm": row.try_get::<i32, _>("bpm").ok(),
            })
        }
        "health_steps" => {
            serde_json::json!({
                "step_count": row.try_get::<i32, _>("step_count").ok(),
            })
        }
        "financial_transaction" => {
            serde_json::json!({
                "merchant": row.try_get::<String, _>("merchant_name").ok(),
                "amount": row.try_get::<f64, _>("amount").ok(),
                "category": row.try_get::<String, _>("merchant_category").ok(),
            })
        }
        "activity_app_usage" => {
            serde_json::json!({
                "app_name": row.try_get::<String, _>("app_name").ok(),
                "window_title": row.try_get::<String, _>("window_title").ok(),
            })
        }
        "activity_web_browsing" => {
            serde_json::json!({
                "domain": row.try_get::<String, _>("domain").ok(),
                "page_title": row.try_get::<String, _>("page_title").ok(),
            })
        }
        "knowledge_ai_conversation" => {
            let content: Option<String> = row.try_get("content").ok();
            let preview = content.map(|c| {
                if c.len() > 100 {
                    format!("{}...", &c[..100])
                } else {
                    c
                }
            });
            serde_json::json!({
                "role": row.try_get::<String, _>("role").ok(),
                "provider": row.try_get::<String, _>("provider").ok(),
                "preview": preview,
            })
        }
        "knowledge_document" => {
            serde_json::json!({
                "title": row.try_get::<String, _>("title").ok(),
                "document_type": row.try_get::<String, _>("document_type").ok(),
            })
        }
        "speech_transcription" => {
            let text: Option<String> = row.try_get("text").ok();
            let preview = text.map(|t| {
                if t.len() > 100 {
                    format!("{}...", &t[..100])
                } else {
                    t
                }
            });
            serde_json::json!({
                "duration_seconds": row.try_get::<f64, _>("duration_seconds").ok(),
                "preview": preview,
            })
        }
        _ => {
            // Generic fallback - just return empty object
            serde_json::json!({})
        }
    }
}
