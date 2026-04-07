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

/// A thing wiki page (catchall entity: pets, projects, concepts, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiThing {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub content: Option<String>,
    pub cover_image: Option<String>,
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
    pub epigraph: Option<String>,
    /// True if this day has a generated illustration BLOB. The BLOB itself
    /// is served separately via GET /api/wiki/day/:date/illustration.
    pub has_illustration: bool,
    pub last_edited_by: Option<String>,
    pub cover_image: Option<String>,
    pub act_id: Option<String>,
    pub chapter_id: Option<String>,
    pub morning_baseline: Option<f64>,
    pub battery_curve: Option<String>,
    pub data_quality: Option<serde_json::Value>,
    pub snapshot: Option<String>,
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
    pub canonical_name: String,
    pub picture: Option<String>,
    pub relationship_category: Option<String>,
    pub last_interaction: Option<DateTime<Utc>>,
}

/// A place list item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPlaceListItem {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub address: Option<String>,
    pub visit_count: Option<i32>,
}

/// An organization list item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiOrganizationListItem {
    pub id: String,
    pub canonical_name: String,
    pub organization_type: Option<String>,
    pub relationship_type: Option<String>,
}

/// A thing list item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiThingListItem {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
}

// ============================================================================
// Update Request Types
// ============================================================================

/// Request to update a person wiki page
#[derive(Debug, Deserialize)]
pub struct UpdateWikiPersonRequest {
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
    pub name: Option<String>,
    pub content: Option<String>,
    pub cover_image: Option<String>,
    pub category: Option<String>,
    pub address: Option<String>,
}

/// Request to update an organization wiki page
#[derive(Debug, Deserialize)]
pub struct UpdateWikiOrganizationRequest {
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
    pub name: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub content: Option<String>,
    pub cover_image: Option<String>,
}

/// Request to update a day wiki page
#[derive(Debug, Deserialize)]
pub struct UpdateWikiDayRequest {
    pub autobiography: Option<String>,
    pub autobiography_sections: Option<serde_json::Value>,
    pub epigraph: Option<String>,
    pub last_edited_by: Option<String>,
    pub cover_image: Option<String>,
    pub start_timezone: Option<String>,
    pub data_quality: Option<String>,
    pub snapshot: Option<String>,
}

// ============================================================================
// Person CRUD Operations
// ============================================================================

/// Get a person by ID
pub async fn get_person(pool: &SqlitePool, id: String) -> Result<WikiPerson> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, canonical_name, content, picture, cover_image,
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
            id, canonical_name, picture, relationship_category, last_interaction
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
            canonical_name = COALESCE($2, canonical_name),
            content = COALESCE($3, content),
            picture = COALESCE($4, picture),
            cover_image = COALESCE($5, cover_image),
            emails = COALESCE($6, emails),
            phones = COALESCE($7, phones),
            birthday = COALESCE($8, birthday),
            instagram = COALESCE($9, instagram),
            facebook = COALESCE($10, facebook),
            linkedin = COALESCE($11, linkedin),
            x = COALESCE($12, x),
            relationship_category = COALESCE($13, relationship_category),
            nickname = COALESCE($14, nickname),
            notes = COALESCE($15, notes),
            updated_at = datetime('now')
        WHERE id = $1
        "#,
        id,
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

/// Get a place by ID (wiki view with content fields)
pub async fn get_wiki_place(pool: &SqlitePool, id: String) -> Result<WikiPlace> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, name, content, cover_image, category, address,
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
            id, name, category, address, visit_count
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
            name = COALESCE($2, name),
            content = COALESCE($3, content),
            cover_image = COALESCE($4, cover_image),
            category = COALESCE($5, category),
            address = COALESCE($6, address),
            updated_at = datetime('now')
        WHERE id = $1
        "#,
        id,
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

/// Get an organization by ID
pub async fn get_organization(pool: &SqlitePool, id: String) -> Result<WikiOrganization> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, canonical_name, content, cover_image,
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
            id, canonical_name, organization_type, relationship_type
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
            canonical_name = COALESCE($2, canonical_name),
            content = COALESCE($3, content),
            cover_image = COALESCE($4, cover_image),
            organization_type = COALESCE($5, organization_type),
            relationship_type = COALESCE($6, relationship_type),
            role_title = COALESCE($7, role_title),
            start_date = COALESCE($8, start_date),
            end_date = COALESCE($9, end_date),
            updated_at = datetime('now')
        WHERE id = $1
        "#,
        id,
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

/// Get a thing by ID
pub async fn get_thing(pool: &SqlitePool, id: String) -> Result<WikiThing> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, name, category, description, content, cover_image,
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

    let row_id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing thing ID".to_string()))?;

    Ok(WikiThing {
        id: row_id,
        name: row.name.clone(),
        category: row.category.clone(),
        description: row.description.clone(),
        content: row.content.clone(),
        cover_image: row.cover_image.clone(),
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
        SELECT id, name, category, description
        FROM wiki_things
        ORDER BY name ASC
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
                name: row.name.clone(),
                category: row.category.clone(),
                description: row.description.clone(),
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
            name = COALESCE($2, name),
            category = COALESCE($3, category),
            description = COALESCE($4, description),
            content = COALESCE($5, content),
            cover_image = COALESCE($6, cover_image),
            updated_at = datetime('now')
        WHERE id = $1
        "#,
        id,
        req.name,
        req.category,
        req.description,
        req.content,
        req.cover_image
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update thing: {}", e)))?;

    get_thing(pool, id).await
}

// ============================================================================
// Narrative Identity
// ============================================================================

/// The user's narrative identity — a present-orientation self-portrait.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeIdentity {
    pub id: String,
    pub content: String,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Get the narrative identity (singleton row, always exists).
pub async fn get_narrative_identity(pool: &SqlitePool) -> Result<NarrativeIdentity> {
    let row = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT id, content, updated_at, created_at FROM wiki_narrative_identity LIMIT 1"
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get narrative identity: {}", e)))?;

    Ok(NarrativeIdentity {
        id: row.0,
        content: row.1,
        updated_at: DateTime::parse_from_rfc3339(&row.2)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        created_at: DateTime::parse_from_rfc3339(&row.3)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// Update request for narrative identity
#[derive(Debug, Deserialize)]
pub struct UpdateNarrativeIdentityRequest {
    pub content: String,
}

/// Update the narrative identity content.
pub async fn update_narrative_identity(
    pool: &SqlitePool,
    request: UpdateNarrativeIdentityRequest,
) -> Result<NarrativeIdentity> {
    sqlx::query("UPDATE wiki_narrative_identity SET content = ? WHERE id = 'nar_identity_001'")
        .bind(&request.content)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to update narrative identity: {}", e)))?;

    get_narrative_identity(pool).await
}

// ============================================================================
// Telos CRUD Operations
// ============================================================================

/// Get active telos
pub async fn get_active_telos(pool: &SqlitePool) -> Result<Option<WikiTelos>> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, title, description, content, cover_image, is_active,
            created_at, updated_at
        FROM wiki_telos
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

/// Get a telos by ID
pub async fn get_telos(pool: &SqlitePool, id: &str) -> Result<WikiTelos> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, title, description, content, cover_image, is_active,
            created_at, updated_at
        FROM wiki_telos
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get telos: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Telos not found: {}", id)))?;

    let row_id = row
        .id
        .clone()
        .ok_or_else(|| Error::Database("Missing telos ID".to_string()))?;

    Ok(WikiTelos {
        id: row_id,
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

/// Get an act by ID
pub async fn get_act(pool: &SqlitePool, id: String) -> Result<WikiAct> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, title, subtitle, description, content, cover_image, location,
            start_date, end_date, sort_order, telos_id, themes,
            created_at, updated_at
        FROM wiki_acts
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
            id, title, subtitle, description, content, cover_image, location,
            start_date, end_date, sort_order, telos_id, themes,
            created_at, updated_at
        FROM wiki_acts
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

/// Get a chapter by ID
pub async fn get_chapter(pool: &SqlitePool, id: String) -> Result<WikiChapter> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, title, subtitle, description, content, cover_image,
            start_date, end_date, sort_order, act_id, themes,
            created_at, updated_at
        FROM wiki_chapters
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
            id, title, subtitle, description, content, cover_image,
            start_date, end_date, sort_order, act_id, themes,
            created_at, updated_at
        FROM wiki_chapters
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
    let existing: Option<sqlx::sqlite::SqliteRow> = sqlx::query(
        r#"
        SELECT
            id, date, start_timezone, end_timezone, autobiography, autobiography_sections,
            epigraph, (illustration IS NOT NULL) as has_illustration,
            last_edited_by, cover_image, act_id, chapter_id, morning_baseline, battery_curve,
            data_quality, snapshot, created_at, updated_at
        FROM wiki_days
        WHERE date = $1
        "#,
    )
    .bind(&date_str)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get day: {}", e)))?;

    if let Some(row) = existing {
        return wiki_day_from_row(&row, date);
    }

    // Create new day
    let day_id = ids::generate_id(ids::WIKI_DAY_PREFIX, &[&date_str]);
    let row: sqlx::sqlite::SqliteRow = sqlx::query(
        r#"
        INSERT INTO wiki_days (id, date)
        VALUES ($1, $2)
        RETURNING
            id, date, start_timezone, end_timezone, autobiography, autobiography_sections,
            epigraph, (illustration IS NOT NULL) as has_illustration,
            last_edited_by, cover_image, act_id, chapter_id, morning_baseline, battery_curve,
            data_quality, snapshot, created_at, updated_at
        "#,
    )
    .bind(&day_id)
    .bind(&date_str)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create day: {}", e)))?;

    wiki_day_from_row(&row, date)
}

/// Parse a WikiDay from a raw SqliteRow
fn wiki_day_from_row(row: &sqlx::sqlite::SqliteRow, date: NaiveDate) -> Result<WikiDay> {
    use sqlx::Row;

    let id: String = row
        .try_get::<Option<String>, _>("id")
        .map_err(|e| Error::Database(format!("Missing day ID: {e}")))?
        .ok_or_else(|| Error::Database("Missing day ID".to_string()))?;
    let autobiography_sections_str: Option<String> = row
        .try_get("autobiography_sections")
        .ok()
        .flatten();
    let created_at_str: String = row.try_get("created_at").unwrap_or_default();
    let updated_at_str: String = row.try_get("updated_at").unwrap_or_default();
    Ok(WikiDay {
        id,
        date,
        start_timezone: row.try_get("start_timezone").ok().flatten(),
        end_timezone: row.try_get("end_timezone").ok().flatten(),
        autobiography: row.try_get("autobiography").ok().flatten(),
        autobiography_sections: autobiography_sections_str
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok()),
        epigraph: row.try_get("epigraph").ok().flatten(),
        has_illustration: row.try_get::<bool, _>("has_illustration").unwrap_or(false),
        last_edited_by: row.try_get("last_edited_by").ok().flatten(),
        cover_image: row.try_get("cover_image").ok().flatten(),
        act_id: row.try_get("act_id").ok().flatten(),
        chapter_id: row.try_get("chapter_id").ok().flatten(),
        morning_baseline: row.try_get("morning_baseline").ok().flatten(),
        battery_curve: row.try_get("battery_curve").ok().flatten(),
        data_quality: row
            .try_get::<Option<String>, _>("data_quality")
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok()),
        snapshot: row.try_get("snapshot").ok().flatten(),
        created_at: DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
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

    sqlx::query(
        r#"
        UPDATE wiki_days
        SET
            autobiography = COALESCE($2, autobiography),
            autobiography_sections = COALESCE($3, autobiography_sections),
            epigraph = COALESCE($4, epigraph),
            last_edited_by = COALESCE($5, last_edited_by),
            cover_image = COALESCE($6, cover_image),
            start_timezone = COALESCE($7, start_timezone),
            data_quality = COALESCE($8, data_quality),
            snapshot = COALESCE($9, snapshot),
            updated_at = datetime('now')
        WHERE id = $1
        "#,
    )
    .bind(&day_id_str)
    .bind(&req.autobiography)
    .bind(&autobiography_sections_json)
    .bind(&req.epigraph)
    .bind(&req.last_edited_by)
    .bind(&req.cover_image)
    .bind(&req.start_timezone)
    .bind(&req.data_quality)
    .bind(&req.snapshot)
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
    use sqlx::Row;

    let start_str = start_date.format("%Y-%m-%d").to_string();
    let end_str = end_date.format("%Y-%m-%d").to_string();

    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        r#"
        SELECT
            id, date, start_timezone, end_timezone, autobiography, autobiography_sections,
            epigraph, (illustration IS NOT NULL) as has_illustration,
            last_edited_by, cover_image, act_id, chapter_id, morning_baseline, battery_curve,
            data_quality, snapshot, created_at, updated_at
        FROM wiki_days
        WHERE date >= $1 AND date <= $2
        ORDER BY date DESC
        "#,
    )
    .bind(&start_str)
    .bind(&end_str)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list days: {}", e)))?;

    Ok(rows
        .iter()
        .filter_map(|row| {
            let date_str: String = row.try_get("date").ok()?;
            let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").ok()?;
            wiki_day_from_row(row, date).ok()
        })
        .collect())
}

/// Fetch the raw illustration PNG bytes for a day. Returns None if no illustration.
pub async fn get_day_illustration(pool: &SqlitePool, date: NaiveDate) -> Result<Option<Vec<u8>>> {
    let date_str = date.format("%Y-%m-%d").to_string();
    let row: Option<(Vec<u8>,)> = sqlx::query_as(
        "SELECT illustration FROM wiki_days WHERE date = ? AND illustration IS NOT NULL",
    )
    .bind(&date_str)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get illustration: {e}")))?;

    Ok(row.map(|(blob,)| blob))
}

/// Save illustration PNG bytes to a day's BLOB column.
pub async fn save_day_illustration(pool: &SqlitePool, date: NaiveDate, png_bytes: &[u8]) -> Result<()> {
    let date_str = date.format("%Y-%m-%d").to_string();
    sqlx::query("UPDATE wiki_days SET illustration = ?, updated_at = datetime('now') WHERE date = ?")
        .bind(png_bytes)
        .bind(&date_str)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to save illustration: {e}")))?;
    Ok(())
}

// ============================================================================
// ID Resolution - Parse entity type from ID
// ============================================================================

/// Result of resolving an ID to its entity type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdResolution {
    pub entity_type: String,
    pub id: String,
}

/// Resolve an entity ID to find its type
/// IDs follow the format: {type}_{hash} (e.g., person_abc123, place_xyz789)
/// For days, the ID format is: day_{YYYY-MM-DD}
pub fn resolve_id(id: &str) -> Result<IdResolution> {
    // Parse the prefix from the ID
    let parts: Vec<&str> = id.splitn(2, '_').collect();
    if parts.len() != 2 {
        return Err(Error::NotFound(format!(
            "Invalid entity ID format: {}",
            id
        )));
    }

    let entity_type = parts[0];

    // Validate known entity types
    let valid_types = ["person", "place", "org", "thing", "day", "telos", "act", "chapter", "page", "chat", "year", "source"];
    if !valid_types.contains(&entity_type) {
        return Err(Error::NotFound(format!(
            "Unknown entity type in ID: {}",
            id
        )));
    }

    Ok(IdResolution {
        entity_type: entity_type.to_string(),
        id: id.to_string(),
    })
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
    // Dayline fields
    pub novelty_z: Option<f64>,
    pub avg_hr: Option<f64>,
    pub autonomic_z: Option<f64>,
    pub hr_z: Option<f64>,
    pub hrv_z: Option<f64>,
    pub topics: Option<serde_json::Value>,
    pub event_summary: Option<String>,
    pub agent_action: Option<String>,
    pub is_sleep: Option<bool>,
    pub user_hidden: Option<bool>,
    pub user_created: Option<bool>,
    // Entity/topic novelty
    pub entities: Option<serde_json::Value>,
    pub topic_novelty: Option<serde_json::Value>,
    pub entity_novelty: Option<serde_json::Value>,
    /// Map of entity_id → ISO8601 timestamp of earliest ref within event window.
    /// Sourced from wiki_entity_refs. Used to position entity dots at their actual
    /// moment (not event center).
    pub entity_timestamps: Option<serde_json::Value>,
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
    use sqlx::Row;
    use std::collections::HashMap;

    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        r#"
        SELECT
            id, day_id, start_time, end_time,
            auto_label, auto_location, user_label, user_location, user_notes,
            source_ontologies, is_unknown, is_transit, is_user_added, is_user_edited,
            novelty_z, avg_hr, autonomic_z, hr_z, hrv_z,
            topics, event_summary, agent_action,
            is_sleep, user_hidden, user_created,
            entities, topic_novelty, entity_novelty,
            created_at, updated_at
        FROM wiki_events
        WHERE day_id = $1
        ORDER BY start_time ASC
        "#,
    )
    .bind(&day_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get day events: {}", e)))?;

    // Fetch entity timestamps for the day: for each event's window, the earliest
    // timestamp each entity appears in wiki_entity_refs.
    let event_windows: Vec<(String, String, String)> = rows
        .iter()
        .filter_map(|row| {
            let id: String = row.try_get("id").ok()?;
            let start: String = row.try_get("start_time").ok()?;
            let end: String = row.try_get("end_time").ok()?;
            Some((id, start, end))
        })
        .collect();

    let mut entity_ts_by_event: HashMap<String, serde_json::Value> = HashMap::new();
    for (event_id, start, end) in &event_windows {
        let ref_rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT entity_id, MIN(timestamp) as earliest
            FROM wiki_entity_refs
            WHERE timestamp IS NOT NULL
              AND timestamp >= $1
              AND timestamp < $2
            GROUP BY entity_id
            "#,
        )
        .bind(start)
        .bind(end)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        if !ref_rows.is_empty() {
            let map: serde_json::Map<String, serde_json::Value> = ref_rows
                .into_iter()
                .map(|(id, ts)| (id, serde_json::Value::String(ts)))
                .collect();
            entity_ts_by_event.insert(event_id.clone(), serde_json::Value::Object(map));
        }
    }

    Ok(rows
        .iter()
        .filter_map(|row| {
            let id: String = row.try_get("id").ok()?;
            let day_id: String = row.try_get("day_id").ok()?;
            let start_time: String = row.try_get("start_time").ok()?;
            let end_time: String = row.try_get("end_time").ok()?;
            let created_at: String = row.try_get("created_at").ok()?;
            let updated_at: String = row.try_get("updated_at").ok()?;
            let entity_timestamps = entity_ts_by_event.get(&id).cloned();

            Some(TemporalEvent {
                id,
                day_id,
                start_time: DateTime::parse_from_rfc3339(&start_time)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                end_time: DateTime::parse_from_rfc3339(&end_time)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                auto_label: row.try_get::<Option<String>, _>("auto_label").ok().flatten(),
                auto_location: row.try_get::<Option<String>, _>("auto_location").ok().flatten(),
                user_label: row.try_get::<Option<String>, _>("user_label").ok().flatten(),
                user_location: row.try_get::<Option<String>, _>("user_location").ok().flatten(),
                user_notes: row.try_get::<Option<String>, _>("user_notes").ok().flatten(),
                source_ontologies: row
                    .try_get::<Option<String>, _>("source_ontologies")
                    .ok()
                    .flatten()
                    .and_then(|s| serde_json::from_str(&s).ok()),
                is_unknown: row.try_get::<Option<i32>, _>("is_unknown").ok().flatten().map(|v| v != 0),
                is_transit: row.try_get::<Option<i32>, _>("is_transit").ok().flatten().map(|v| v != 0),
                is_user_added: row.try_get::<Option<i32>, _>("is_user_added").ok().flatten().map(|v| v != 0),
                is_user_edited: row.try_get::<Option<i32>, _>("is_user_edited").ok().flatten().map(|v| v != 0),
                novelty_z: row.try_get::<Option<f64>, _>("novelty_z").ok().flatten(),
                avg_hr: row.try_get::<Option<f64>, _>("avg_hr").ok().flatten(),
                autonomic_z: row.try_get::<Option<f64>, _>("autonomic_z").ok().flatten(),
                hr_z: row.try_get::<Option<f64>, _>("hr_z").ok().flatten(),
                hrv_z: row.try_get::<Option<f64>, _>("hrv_z").ok().flatten(),
                topics: row.try_get::<Option<String>, _>("topics")
                    .ok()
                    .flatten()
                    .and_then(|s| serde_json::from_str(&s).ok()),
                event_summary: row.try_get::<Option<String>, _>("event_summary").ok().flatten(),
                agent_action: row.try_get::<Option<String>, _>("agent_action").ok().flatten(),
                is_sleep: row.try_get::<Option<i32>, _>("is_sleep").ok().flatten().map(|v| v != 0),
                user_hidden: row.try_get::<Option<i32>, _>("user_hidden").ok().flatten().map(|v| v != 0),
                user_created: row.try_get::<Option<i32>, _>("user_created").ok().flatten().map(|v| v != 0),
                entities: row.try_get::<Option<String>, _>("entities")
                    .ok()
                    .flatten()
                    .and_then(|s| serde_json::from_str(&s).ok()),
                topic_novelty: row.try_get::<Option<String>, _>("topic_novelty")
                    .ok()
                    .flatten()
                    .and_then(|s| serde_json::from_str(&s).ok()),
                entity_novelty: row.try_get::<Option<String>, _>("entity_novelty")
                    .ok()
                    .flatten()
                    .and_then(|s| serde_json::from_str(&s).ok()),
                entity_timestamps,
                created_at: DateTime::parse_from_rfc3339(&created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&updated_at)
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
        novelty_z: None,
        avg_hr: None,
        autonomic_z: None,
        hr_z: None,
        hrv_z: None,
        topics: None,
        event_summary: None,
        agent_action: None,
        is_sleep: Some(false),
        user_hidden: Some(false),
        user_created: Some(false),
        entities: None,
        topic_novelty: None,
        entity_novelty: None,
        entity_timestamps: None,
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
        novelty_z: None,
        avg_hr: None,
        autonomic_z: None,
        hr_z: None,
        hrv_z: None,
        topics: None,
        event_summary: None,
        agent_action: None,
        is_sleep: Some(false),
        user_hidden: Some(false),
        user_created: Some(false),
        entities: None,
        topic_novelty: None,
        entity_novelty: None,
        entity_timestamps: None,
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

/// Get all ontology data sources for a specific date (registry-driven).
///
/// Iterates over all registered ontologies that have a `DaySourceConfig` and builds
/// dynamic SQL queries from the config. No arbitrary LIMITs — all data included
/// with a sanity check for overflow.
pub async fn get_day_sources(pool: &SqlitePool, date: NaiveDate) -> Result<Vec<DaySource>> {
    use sqlx::Row;
    use virtues_registry::ontologies::registered_ontologies;

    // UTC bounds: midnight to noon next day (covers all timezones)
    let start_of_day = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let end_of_day = date
        .succ_opt()
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap()
        .and_utc();
    let start_str = start_of_day.to_rfc3339();
    let end_str = end_of_day.to_rfc3339();
    let date_str = date.format("%Y-%m-%d").to_string();

    let mut sources: Vec<DaySource> = Vec::new();

    for ont in registered_ontologies() {
        let cfg = match &ont.day_source {
            Some(c) => c,
            None => continue,
        };

        // Build SELECT columns
        let source_type_col = cfg
            .source_type_sql
            .map(|sql| format!("{} as source_type_dyn", sql))
            .unwrap_or_else(|| format!("'{}' as source_type_dyn", cfg.source_type));

        let query = if cfg.use_date_filter {
            format!(
                "SELECT {id} as src_id, {ts} as src_ts, {label} as src_label, {preview} as src_preview, {st} \
                 FROM {table} t \
                 WHERE date(t.{ts_col}) = $1 \
                 {extra} \
                 ORDER BY t.{ts_col} ASC",
                id = cfg.id_sql,
                ts = ont.timestamp_column,
                label = cfg.label_sql,
                preview = cfg.preview_sql,
                st = source_type_col,
                table = ont.table_name,
                ts_col = ont.timestamp_column,
                extra = cfg.extra_where.unwrap_or(""),
            )
        } else {
            format!(
                "SELECT {id} as src_id, {ts} as src_ts, {label} as src_label, {preview} as src_preview, {st} \
                 FROM {table} t \
                 WHERE t.{ts_col} >= $1 AND t.{ts_col} <= $2 \
                 {extra} \
                 ORDER BY t.{ts_col} ASC",
                id = cfg.id_sql,
                ts = ont.timestamp_column,
                label = cfg.label_sql,
                preview = cfg.preview_sql,
                st = source_type_col,
                table = ont.table_name,
                ts_col = ont.timestamp_column,
                extra = cfg.extra_where.unwrap_or(""),
            )
        };

        let rows = if cfg.use_date_filter {
            sqlx::query(&query)
                .bind(&date_str)
                .fetch_all(pool)
                .await
        } else {
            sqlx::query(&query)
                .bind(&start_str)
                .bind(&end_str)
                .fetch_all(pool)
                .await
        };

        let rows = match rows {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(ontology = ont.name, error = %e, "Failed to query day sources");
                continue;
            }
        };

        // Sanity check
        if rows.len() > 5000 {
            tracing::warn!(
                ontology = ont.name,
                count = rows.len(),
                "Unusually large source count for single day"
            );
        }

        for row in &rows {
            let id: String = match row.try_get("src_id") {
                Ok(v) => v,
                Err(_) => continue,
            };
            let ts_str: String = match row.try_get("src_ts") {
                Ok(v) => v,
                Err(_) => continue,
            };
            let label: String = row.try_get("src_label").unwrap_or_else(|_| ont.display_name.to_string());
            let preview: Option<String> = row.try_get("src_preview").ok().flatten();
            let source_type: String = row.try_get("source_type_dyn").unwrap_or_else(|_| cfg.source_type.to_string());

            // Parse timestamp: try RFC3339 first, fall back to "YYYY-MM-DD HH:MM:SS"
            let ts = if let Ok(parsed) = DateTime::parse_from_rfc3339(&ts_str) {
                parsed.with_timezone(&Utc)
            } else if let Ok(naive) =
                chrono::NaiveDateTime::parse_from_str(&ts_str, "%Y-%m-%d %H:%M:%S")
            {
                naive.and_utc()
            } else {
                continue;
            };

            sources.push(DaySource {
                source_type,
                id,
                timestamp: ts,
                label,
                preview,
            });
        }
    }

    sources.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    Ok(sources)
}

// ============================================================================
// Timeline Day - Location chunks for movement map
// ============================================================================

/// A location chunk for the timeline day view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineChunk {
    #[serde(rename = "type")]
    pub chunk_type: String,
    pub start_time: String,
    pub end_time: String,
    pub place_name: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
}

/// Timeline day view response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineDayView {
    pub date: String,
    pub chunks: Vec<TimelineChunk>,
}

/// Get location points for a day, returned as timeline chunks
pub async fn get_timeline_day(pool: &SqlitePool, date: NaiveDate) -> Result<TimelineDayView> {
    let start_of_day = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let end_of_day = date
        .succ_opt()
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap()
        .and_utc();

    let start_str = start_of_day.to_rfc3339();
    let end_str = end_of_day.to_rfc3339();

    // Query location points for the day
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        r#"
        SELECT latitude, longitude, timestamp
        FROM data_location_point
        WHERE timestamp >= $1 AND timestamp <= $2
        ORDER BY timestamp ASC
        "#,
    )
    .bind(&start_str)
    .bind(&end_str)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get location points: {}", e)))?;

    use sqlx::Row;
    let chunks: Vec<TimelineChunk> = rows
        .iter()
        .filter_map(|row| {
            let lat: Option<f64> = row.try_get("latitude").ok();
            let lng: Option<f64> = row.try_get("longitude").ok();
            let ts: Option<String> = row.try_get("timestamp").ok();
            match (lat, lng, ts) {
                (Some(lat), Some(lng), Some(ts)) => Some(TimelineChunk {
                    chunk_type: "location".to_string(),
                    start_time: ts.clone(),
                    end_time: ts,
                    place_name: None,
                    latitude: lat,
                    longitude: lng,
                }),
                _ => None,
            }
        })
        .collect();

    Ok(TimelineDayView {
        date: date.to_string(),
        chunks,
    })
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
    use virtues_registry::ontologies::registered_ontologies;
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

    let ontologies = registered_ontologies();
    let mut streams = Vec::new();

    for ontology in &ontologies {
        // Skip non-time-series ontologies
        if should_skip_ontology_for_streams(ontology.name) {
            continue;
        }

        let table = ontology.table_name;
        let ts_col = ontology.timestamp_column;

        // Build SELECT clause for end timestamp if present
        let end_select = ontology
            .end_timestamp_column
            .map(|c| format!(", {} as end_ts", c))
            .unwrap_or_default();

        // Use hex(id) for tables with blob IDs (location_visit), plain id otherwise
        let id_select = if ontology.name == "location_visit" {
            "hex(id) as id"
        } else {
            "id"
        };

        // Build dynamic query - select id, timestamps, and all other columns as JSON
        let sql = format!(
            "SELECT {id_select}, {ts_col} as ts{end_select}, * FROM {table}
             WHERE {ts_col} >= ?1 AND {ts_col} < ?2
             ORDER BY {ts_col} ASC
             LIMIT 100",
            id_select = id_select,
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
        "calendar_event" => {
            serde_json::json!({
                "title": row.try_get::<String, _>("title").ok(),
                "location": row.try_get::<String, _>("location_name").ok(),
            })
        }
        "communication_email" => {
            serde_json::json!({
                "subject": row.try_get::<String, _>("subject").ok(),
                "from": row.try_get::<String, _>("from_email").ok(),
                "direction": row.try_get::<String, _>("direction").ok(),
            })
        }
        "communication_message" => {
            let body: Option<String> = row.try_get("body").ok();
            let preview = body.map(|c| {
                let truncated: String = c.chars().take(100).collect();
                if truncated.len() < c.len() {
                    format!("{truncated}...")
                } else {
                    truncated
                }
            });
            serde_json::json!({
                "from": row.try_get::<String, _>("from_name").ok(),
                "channel": row.try_get::<String, _>("channel").ok(),
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
            let amount_cents: Option<i64> = row.try_get("amount").ok();
            serde_json::json!({
                "merchant": row.try_get::<String, _>("merchant_name").ok(),
                "amount": amount_cents.map(|c| c as f64 / 100.0),
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
        "content_conversation" => {
            let content: Option<String> = row.try_get("content").ok();
            let preview = content.map(|c| {
                let truncated: String = c.chars().take(100).collect();
                if truncated.len() < c.len() {
                    format!("{truncated}...")
                } else {
                    truncated
                }
            });
            serde_json::json!({
                "role": row.try_get::<String, _>("role").ok(),
                "provider": row.try_get::<String, _>("provider").ok(),
                "preview": preview,
            })
        }
        "content_document" => {
            serde_json::json!({
                "title": row.try_get::<String, _>("title").ok(),
                "document_type": row.try_get::<String, _>("document_type").ok(),
            })
        }
        "communication_transcription" => {
            let text: Option<String> = row.try_get("text").ok();
            let preview = text.map(|t| {
                let truncated: String = t.chars().take(100).collect();
                if truncated.len() < t.len() {
                    format!("{truncated}...")
                } else {
                    truncated
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
