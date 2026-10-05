//! Entities API - Managing resolved entities (places, people, topics)
//!
//! This module provides CRUD operations for entity types:
//! - Places: Known locations (home, work, etc.)
//! - People: Contacts and relationships (future)
//! - Topics: Subjects and interests (future)

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::ids;

// ============================================================================
// Place Types
// ============================================================================

/// A place entity from the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Place {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub address: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub radius_m: Option<f64>,
    /// Counted from `wiki_refs`, never read off the row: the column of that
    /// name has no writer and reported 0 for a place visited weekly.
    pub seen_count: Option<i32>,
    /// The phone keeps no audio while the owner is inside this place. Read by
    /// the audio collector's place cache, set from the place's wiki page or
    /// the phone; never inferred.
    pub is_audio_muted: bool,
    pub metadata: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Request to create a new place
#[derive(Debug, Deserialize)]
pub struct CreatePlaceRequest {
    /// Display name/label for the place (e.g., "Home", "Work", "Gym")
    pub label: String,
    /// Full formatted address
    pub formatted_address: String,
    /// Latitude coordinate
    pub latitude: f64,
    /// Longitude coordinate
    pub longitude: f64,
    /// Google Place ID (optional, for linking to Google Places)
    pub google_place_id: Option<String>,
    /// Category (e.g., "home", "work", "gym")
    pub category: Option<String>,
    /// Whether to set this place as home (updates user_profile.home_place_id)
    pub set_as_home: Option<bool>,
    /// Mute the phone's audio collector inside this place ("Mute here").
    pub is_audio_muted: Option<bool>,
}

/// Request to update an existing place
#[derive(Debug, Deserialize)]
pub struct UpdatePlaceRequest {
    pub label: Option<String>,
    pub formatted_address: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub google_place_id: Option<String>,
    pub category: Option<String>,
    /// Mute the phone's audio collector inside this place.
    pub is_audio_muted: Option<bool>,
}

/// Response for created place
#[derive(Debug, Serialize)]
pub struct CreatePlaceResponse {
    pub id: String,
    pub name: String,
    pub is_home: bool,
}

// ============================================================================
// Place CRUD Operations
// ============================================================================

/// List all known places (places with is_known_location: true in metadata)
pub async fn list_places(pool: &PgPool) -> Result<Vec<Place>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id,
            name,
            category,
            address,
            latitude,
            longitude,
            radius_m,
            is_audio_muted,
            metadata,
            created_at,
            updated_at
        FROM wiki_places
        WHERE (metadata->>'is_known_location')::boolean = true
           OR is_audio_muted
        ORDER BY created_at ASC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list places: {}", e)))?;

    let places = rows
        .into_iter()
        .map(|row| Place {
            id: row.id,
            name: row.name,
            category: row.category,
            address: row.address,
            latitude: row.latitude,
            longitude: row.longitude,
            radius_m: Some(row.radius_m),
            seen_count: None,
            is_audio_muted: row.is_audio_muted,
            metadata: Some(row.metadata),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
        .collect();

    Ok(places)
}

/// Get a single place by ID
pub async fn get_place(pool: &PgPool, id: String) -> Result<Place> {
    let id_str = &id;
    let row = sqlx::query!(
        r#"
        SELECT
            id,
            name,
            category,
            address,
            latitude,
            longitude,
            radius_m,
            is_audio_muted,
            metadata,
            created_at,
            updated_at
        FROM wiki_places
        WHERE id = $1
        "#,
        id_str
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get place: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Place not found: {}", id)))?;

    Ok(Place {
        id: row.id,
        name: row.name,
        category: row.category,
        address: row.address,
        latitude: row.latitude,
        longitude: row.longitude,
        radius_m: Some(row.radius_m),
        seen_count: None,
        is_audio_muted: row.is_audio_muted,
        metadata: Some(row.metadata),
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Create a new place
pub async fn create_place(
    pool: &PgPool,
    req: CreatePlaceRequest,
) -> Result<CreatePlaceResponse> {
    let metadata = serde_json::json!({
        "google_place_id": req.google_place_id,
        "is_known_location": true,
        "source": "user"
    });

    // Generate ID with proper prefix (place_{hash16})
    let id = ids::generate_id(
        ids::WIKI_PLACE_PREFIX,
        &[&req.label, &req.latitude.to_string(), &req.longitude.to_string()],
    );
    let id_str = id.clone();

    sqlx::query!(
        r#"
        INSERT INTO wiki_places (
            id,
            name,
            category,
            address,
            latitude,
            longitude,
            radius_m,
            metadata,
            is_audio_muted
        ) VALUES (
            $1, $2, $3, $4, $5, $6, 50.0, $7, $8
        )
        "#,
        id_str,
        req.label,
        req.category,
        req.formatted_address,
        req.latitude,
        req.longitude,
        metadata,
        req.is_audio_muted.unwrap_or(false),
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create place: {}", e)))?;
    mark_place_named(pool, &id_str).await?;

    // Set as home if requested
    let is_home = req.set_as_home.unwrap_or(false);
    if is_home {
        set_home_place(pool, id.clone()).await?;
    }

    Ok(CreatePlaceResponse {
        id,
        name: req.label,
        is_home,
    })
}

/// Update an existing place
pub async fn update_place(pool: &PgPool, id: String, req: UpdatePlaceRequest) -> Result<Place> {
    // First get the existing place to preserve metadata
    let existing = get_place(pool, id.clone()).await?;
    let mut metadata = existing.metadata.unwrap_or_else(|| serde_json::json!({}));
 
    // Update metadata fields if provided (only google_place_id goes in metadata now)
    if let Some(ref gid) = req.google_place_id {
        metadata["google_place_id"] = serde_json::json!(gid);
    }

    let id_str = &id;

    sqlx::query!(
        r#"
        UPDATE wiki_places
        SET
            name = COALESCE($2, name),
            category = COALESCE($3, category),
            address = COALESCE($4, address),
            latitude = COALESCE($5, latitude),
            longitude = COALESCE($6, longitude),
            metadata = $7,
            is_audio_muted = COALESCE($8, is_audio_muted),
            updated_at = now()
        WHERE id = $1
        "#,
        id_str,
        req.label,
        req.category,
        req.formatted_address,
        req.latitude,
        req.longitude,
        metadata,
        req.is_audio_muted,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update place: {}", e)))?;
    if req.label.is_some() {
        mark_place_named(pool, &id).await?;
    }

    crate::api::wiki_articles::retitle_article(pool, "place", &id).await?;

    // Fetch the updated place
    get_place(pool, id).await
}

/// Record that the person named a place. The flag outlives the name: a place
/// they named keeps counting as named even if a later rename happens to read
/// like the resolver's "Location …".
pub async fn mark_place_named(pool: &PgPool, id: &str) -> Result<()> {
    sqlx::query("UPDATE wiki_places SET is_named = true WHERE id = $1 AND NOT is_named")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to mark place named: {}", e)))?;
    Ok(())
}

/// Fold `absorb` into `keep`: one place, seen as two.
///
/// Two places a few dozen metres apart are often one doorway that two stays'
/// centres drifted across, and once one is named the other sits beside it as a
/// second "Location …" that the same visits keep landing on. Merging moves
/// everything that pointed at `absorb` to `keep` - its visits and every other
/// ref, notes, rules, a chat's edit grant, pins, project items, home - then
/// deletes it. `keep` takes `absorb`'s name only when it has none of its own,
/// and keeps it as an alias otherwise, so a name the person gave either is
/// never lost.
pub async fn merge_places(pool: &PgPool, keep: &str, absorb: &str) -> Result<()> {
    if keep == absorb {
        return Err(Error::InvalidInput("That's the same place. Choose a different one to merge into.".into()));
    }
    let db = |e: sqlx::Error| Error::Database(format!("Failed to merge places: {e}"));
    let mut tx = pool.begin().await.map_err(db)?;
    let found: i64 =
        sqlx::query_scalar("SELECT count(*) FROM (SELECT 1 FROM wiki_places WHERE id = ANY($1) FOR UPDATE) p")
            .bind([keep, absorb])
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
    if found != 2 {
        return Err(Error::NotFound("Place not found".into()));
    }

    // Each statement first drops what would collide with a row `keep` already
    // has, then moves the rest.
    let steps: [&str; 13] = [
        "DELETE FROM wiki_refs a USING wiki_refs k \
         WHERE a.entity_id = $2 AND k.entity_id = $1 AND a.source_table = k.source_table \
           AND a.source_id = k.source_id AND a.role IS NOT DISTINCT FROM k.role",
        "UPDATE wiki_refs SET entity_id = $1 WHERE entity_id = $2",
        "UPDATE wiki_notes SET subject_id = $1 WHERE subject_type = 'place' AND subject_id = $2",
        "UPDATE wiki_rules SET subject_id = $1 WHERE subject_type = 'place' AND subject_id = $2",
        "DELETE FROM app_chat_edit_permissions a USING app_chat_edit_permissions k \
         WHERE a.entity_id = $2 AND k.entity_id = $1 AND a.chat_id = k.chat_id",
        "UPDATE app_chat_edit_permissions SET entity_id = $1 WHERE entity_id = $2",
        "DELETE FROM app_pins WHERE url = '/place/' || $2 \
           AND EXISTS (SELECT 1 FROM app_pins WHERE url = '/place/' || $1)",
        "UPDATE app_pins SET url = '/place/' || $1 WHERE url = '/place/' || $2",
        "DELETE FROM app_project_items a USING app_project_items k \
         WHERE a.url = '/place/' || $2 AND k.url = '/place/' || $1 AND a.project_id = k.project_id",
        "UPDATE app_project_items SET url = '/place/' || $1 WHERE url = '/place/' || $2",
        "UPDATE app_user_profile SET home_place_id = $1 WHERE home_place_id = $2",
        "UPDATE wiki_places k SET \
           name = CASE WHEN a.is_named AND NOT k.is_named THEN a.name ELSE k.name END, \
           aliases = CASE WHEN a.is_named AND k.is_named AND a.name <> k.name \
                          AND NOT COALESCE(k.aliases, '[]'::jsonb) ? a.name \
                     THEN COALESCE(k.aliases, '[]'::jsonb) || to_jsonb(a.name) \
                     ELSE k.aliases END, \
           is_named = k.is_named OR a.is_named, \
           is_audio_muted = COALESCE(k.is_audio_muted, false) OR COALESCE(a.is_audio_muted, false), \
           updated_at = now() \
         FROM wiki_places a WHERE k.id = $1 AND a.id = $2",
        "DELETE FROM wiki_places WHERE id = $2",
    ];
    for sql in steps {
        sqlx::query(sql).bind(keep).bind(absorb).execute(&mut *tx).await.map_err(db)?;
    }
    tx.commit().await.map_err(db)?;

    // The absorbed place's article, if one was written, describes a place that
    // is now part of another; `keep`'s article is retitled to the merged name.
    crate::api::wiki_articles::delete_article(pool, "place", absorb).await?;
    crate::api::wiki_articles::retitle_article(pool, "place", keep).await?;
    Ok(())
}

/// A named place near another, as a "Same as…?" choice.
#[derive(Debug, Serialize)]
pub struct NearbyPlace {
    pub id: String,
    pub name: String,
    pub meters: f64,
}

/// How close a named place must be to offer itself as the same place. Past
/// the 100 m a stay can drift, and short of the next building over.
pub const SAME_PLACE_METERS: f64 = 150.0;

/// The named places within `SAME_PLACE_METERS` of a place, nearest first.
pub async fn nearby_named_places(pool: &PgPool, id: &str) -> Result<Vec<NearbyPlace>> {
    let rows: Vec<(String, String, f64)> = sqlx::query_as(
        r#"
        WITH me AS (SELECT latitude, longitude FROM wiki_places WHERE id = $1)
        SELECT p.id, p.name,
               6371000 * 2 * asin(sqrt(
                   power(sin(radians(p.latitude - me.latitude) / 2), 2)
                   + cos(radians(me.latitude)) * cos(radians(p.latitude))
                     * power(sin(radians(p.longitude - me.longitude) / 2), 2))) AS meters
        FROM wiki_places p, me
        WHERE p.id <> $1 AND p.is_named
          AND abs(p.latitude - me.latitude) < 0.01 AND abs(p.longitude - me.longitude) < 0.01
        ORDER BY meters
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to find nearby places: {e}")))?;
    Ok(rows
        .into_iter()
        .filter(|(_, _, m)| *m <= SAME_PLACE_METERS)
        .map(|(id, name, meters)| NearbyPlace { id, name, meters: meters.round() })
        .collect())
}

/// Delete a place by ID
pub async fn delete_place(pool: &PgPool, id: String) -> Result<()> {
    // First, unset home_place_id if this place is currently set as home
    let profile_id_str = "00000000-0000-0000-0000-000000000001";
    let id_str = &id;

    sqlx::query!(
        r#"
        UPDATE app_user_profile
        SET home_place_id = NULL
        WHERE id = $1 AND home_place_id = $2
        "#,
        profile_id_str,
        id_str
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to unset home place: {}", e)))?;

    // Everything that pointed at this place: refs, article, notes, stored urls.
    // Without this the row goes and its edges stay, which is invisible from the
    // button and permanent in the data.
    purge_subject(pool, "place", "place", id_str).await?;

    let result = sqlx::query!(
        r#"
        DELETE FROM wiki_places
        WHERE id = $1
        "#,
        id_str
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to delete place: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Place not found: {}", id)));
    }

    Ok(())
}

/// Set a place as the user's home (updates user_profile.home_place_id)
pub async fn set_home_place(pool: &PgPool, place_id: String) -> Result<()> {
    let profile_id_str = "00000000-0000-0000-0000-000000000001";
    let place_id_str = &place_id;

    // Verify the place exists
    let exists = sqlx::query!(
        r#"SELECT id FROM wiki_places WHERE id = $1"#,
        place_id_str
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to verify place: {}", e)))?;

    if exists.is_none() {
        return Err(Error::NotFound(format!("Place not found: {}", place_id)));
    }

    // Update user profile
    sqlx::query!(
        r#"
        UPDATE app_user_profile
        SET home_place_id = $1
        WHERE id = $2
        "#,
        place_id_str,
        profile_id_str
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to set home place: {}", e)))?;

    Ok(())
}

// ============================================================================
// Reclassification
// ============================================================================

/// Move a person to the organizations table.
///
/// The People index is full of companies, and not by accident:
/// `extract_name_from_email()` mints a `wiki_people` row for any sender it has
/// not seen, with no test for whether a person is on the other end. On a real
/// box that produced `Gusto <automated@gusto.com>`, `Slack <no-reply@slack.com>`
/// and `The Plaid Team <info@email.plaid.com>` — filed as people, alongside the
/// user's actual contacts.
///
/// This is deliberately NOT merge, and the difference is what makes it safe.
/// Merge folds two rows into one *existing* row, so its refs can collide under
/// `idx_entity_refs_unique (entity_id, source_table, source_id, role)
/// NULLS NOT DISTINCT` — the same source row already referenced by the
/// survivor. Reclassify mints a **fresh** org id, so no `(new_id, source, …)`
/// tuple can already exist and the ref re-point cannot conflict. That is why
/// merge needs its own design pass and this does not.
///
/// Everything moves in one transaction: the entity refs (which carry the whole
/// interaction history), the aliases a human authored, and the routes stored as
/// free text in `app_pins` / `app_project_items` — the trap 0071 documented
/// when it dropped `wiki_things` and had to sweep `/thing/` urls in the same
/// migration. Columns orgs do not have (emails, phones, socials) are preserved
/// into `metadata` rather than dropped: reclassifying is a filing correction,
/// not a decision to forget anything.
pub async fn reclassify_person_as_organization(pool: &PgPool, person_id: String) -> Result<String> {
    let person = sqlx::query!(
        r#"
        SELECT name, emails, phones, handles, nickname,
               metadata, content, aliases
        FROM wiki_people WHERE id = $1
        "#,
        &person_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load person: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Person not found: {}", person_id)))?;

    // Salt the id with the source person so reclassifying twice is not a
    // silent no-op against an existing org of the same name.
    let org_id = ids::generate_id(
        ids::WIKI_ORG_PREFIX,
        &[&person.name, &person_id],
    );

    // What an org has no column for. Kept, not discarded.
    let mut metadata = match person.metadata {
        serde_json::Value::Object(m) => m,
        _ => serde_json::Map::new(),
    };
    metadata.insert("reclassified_from_person".into(), serde_json::json!(person_id));
    metadata.insert("emails".into(), person.emails);
    metadata.insert("phones".into(), person.phones);
    metadata.insert("handles".into(), person.handles);
    if let Some(n) = person.nickname {
        metadata.insert("nickname".into(), serde_json::json!(n));
    }

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to begin transaction: {}", e)))?;

    sqlx::query!(
        r#"
        INSERT INTO wiki_orgs (id, name, metadata, content, aliases)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        &org_id,
        &person.name,
        serde_json::Value::Object(metadata),
        person.content,
        person.aliases,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| Error::Database(format!("Failed to create organization: {}", e)))?;

    // The interaction history. A fresh org id cannot collide, so this is a
    // plain UPDATE rather than merge's upsert-then-delete.
    sqlx::query!(
        r#"
        UPDATE wiki_refs SET entity_id = $1, entity_type = 'organization'
        WHERE entity_id = $2 AND entity_type = 'person'
        "#,
        &org_id,
        &person_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| Error::Database(format!("Failed to move entity refs: {}", e)))?;

    // Routes stored as free text — a stale `/person/<id>` would render as a row
    // that looks openable and is not.
    let old_url = format!("/person/{}", person_id);
    let new_url = format!("/org/{}", org_id);
    for table in ["app_pins", "app_project_items"] {
        sqlx::query(&format!("UPDATE {table} SET url = $1 WHERE url = $2"))
            .bind(&new_url)
            .bind(&old_url)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Database(format!("Failed to move {table} url: {}", e)))?;
    }

    // The owner is a person by definition (migration 0080). If the row being
    // reclassified is the one the profile points at, someone has mis-clicked —
    // refuse rather than leave the profile pointing into the orgs table.
    let is_self = sqlx::query_scalar!(
        "SELECT EXISTS (SELECT 1 FROM app_user_profile WHERE self_person_id = $1)",
        &person_id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| Error::Database(format!("Failed to check self person: {}", e)))?
    .unwrap_or(false);
    if is_self {
        return Err(Error::InvalidInput(
            "That person is you - reclassifying yourself as an organization is not what you meant"
                .into(),
        ));
    }

    sqlx::query!("DELETE FROM wiki_people WHERE id = $1", &person_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete person: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to commit reclassification: {}", e)))?;

    Ok(org_id)
}

// ============================================================================
// People and organizations: create and delete
// ============================================================================

/// Create a person by hand.
///
/// Until now people only appeared by resolution — from a contact sync or an
/// email sender — so there was no way to write down someone the record had not
/// noticed yet. That is backwards for a personal wiki: the people who matter
/// most are often the ones you never email.
pub async fn create_person(pool: &PgPool, name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("A person needs a name".into()));
    }
    let id = ids::generate_id(ids::WIKI_PERSON_PREFIX, &[name, "manual"]);

    sqlx::query!(
        "INSERT INTO wiki_people (id, name) VALUES ($1, $2) \
         ON CONFLICT (id) DO NOTHING",
        &id,
        name
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create person: {}", e)))?;

    Ok(id)
}

/// Create an organization by hand.
pub async fn create_organization(pool: &PgPool, name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("An organization needs a name".into()));
    }
    let id = ids::generate_id(ids::WIKI_ORG_PREFIX, &[name, "manual"]);

    sqlx::query!(
        "INSERT INTO wiki_orgs (id, name) VALUES ($1, $2) \
         ON CONFLICT (id) DO NOTHING",
        &id,
        name
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create organization: {}", e)))?;

    Ok(id)
}

/// Everything that has to go when a subject is deleted.
///
/// Deleting an entity used to leave a trail: `delete_place` dropped the row and
/// nothing else, so its `wiki_refs` survived as edges pointing at a
/// vanished id, its article page stayed searchable and citable, and any pin or
/// project item kept a `/place/<id>` url that rendered as a row nothing could
/// open. None of that is visible from the delete button, which is exactly why
/// it lasted.
///
/// `subject_type` here is the SCHEMA word (`organization`); `route_prefix` is
/// the frontend's (`org`). They differ, and the one place that matters is this
/// function.
async fn purge_subject(
    pool: &PgPool,
    subject_type: &str,
    route_prefix: &str,
    id: &str,
) -> Result<()> {
    // The article, its page, and its index rows. Must happen before the entity
    // row goes, since the lookup keys on the subject.
    crate::api::wiki_articles::delete_article(pool, subject_type, id).await?;

    sqlx::query("DELETE FROM wiki_refs WHERE entity_id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to clear entity refs: {}", e)))?;

    sqlx::query("DELETE FROM wiki_notes WHERE subject_type = $1 AND subject_id = $2")
        .bind(subject_type)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to clear notes: {}", e)))?;

    // Routes stored as free text — the trap 0071 documented when it dropped
    // wiki_things and had to sweep `/thing/` urls in the same migration.
    let url = format!("/{route_prefix}/{id}");
    for table in ["app_pins", "app_project_items"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE url = $1"))
            .bind(&url)
            .execute(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to clear {table}: {}", e)))?;
    }
    Ok(())
}

/// Delete a person and everything that pointed at them.
pub async fn delete_person(pool: &PgPool, id: String) -> Result<()> {
    let is_self: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM app_user_profile WHERE self_person_id = $1)",
    )
    .bind(&id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to check self person: {}", e)))?;
    if is_self {
        return Err(Error::InvalidInput(
            "That person is you - deleting yourself from your own record is not what you meant"
                .into(),
        ));
    }

    purge_subject(pool, "person", "person", &id).await?;

    let n = sqlx::query("DELETE FROM wiki_people WHERE id = $1")
        .bind(&id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete person: {}", e)))?
        .rows_affected();
    if n == 0 {
        return Err(Error::NotFound(format!("Person not found: {}", id)));
    }
    Ok(())
}

/// Delete an organization and everything that pointed at it.
pub async fn delete_organization(pool: &PgPool, id: String) -> Result<()> {
    purge_subject(pool, "organization", "org", &id).await?;

    let n = sqlx::query("DELETE FROM wiki_orgs WHERE id = $1")
        .bind(&id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete organization: {}", e)))?
        .rows_affected();
    if n == 0 {
        return Err(Error::NotFound(format!("Organization not found: {}", id)));
    }
    Ok(())
}

#[cfg(test)]
mod entity_crud_tests {
    use super::*;

    /// Deleting an entity must not leave edges pointing at a vanished id. This
    /// is what `delete_place` did for its whole life: the row went, the refs
    /// stayed, and nothing surfaced it.
    #[sqlx::test]
    async fn deleting_a_person_takes_their_refs_and_pins(pool: PgPool) {
        let id = create_person(&pool, "Sarah").await.unwrap();

        sqlx::query(
            "INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id) \
             VALUES ('r_1', 'person', $1, 'data_communication_message', 'm_1')",
        )
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO app_pins (id, url, label) VALUES ('pin_1', $1, 'Sarah')")
            .bind(format!("/person/{id}"))
            .execute(&pool)
            .await
            .unwrap();

        delete_person(&pool, id.clone()).await.unwrap();

        let refs: i64 = sqlx::query_scalar("SELECT count(*) FROM wiki_refs WHERE entity_id = $1")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(refs, 0, "refs must not outlive the entity");

        let pins: i64 = sqlx::query_scalar("SELECT count(*) FROM app_pins WHERE url = $1")
            .bind(format!("/person/{id}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(pins, 0, "a pin to a deleted person opens nothing");
    }

    async fn insert_place(pool: &PgPool, id: &str, name: &str, named: bool, lat: f64) {
        sqlx::query(
            "INSERT INTO wiki_places (id, name, latitude, longitude, radius_m, is_named) \
             VALUES ($1, $2, $3, -97.0, 100, $4)",
        )
        .bind(id)
        .bind(name)
        .bind(lat)
        .bind(named)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Merging moves every visit and pin to the surviving place, never makes a
    /// duplicate ref, and keeps a name the person gave either place.
    #[sqlx::test]
    async fn merging_places_moves_what_pointed_at_the_absorbed_one(pool: PgPool) {
        insert_place(&pool, "place_keep", "Location 30.0000, -97.0000", false, 30.0).await;
        insert_place(&pool, "place_gone", "Corner bakery", true, 30.0004).await;
        sqlx::query(
            "INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, role) VALUES \
             ('r_1', 'place', 'place_keep', 'data_location_visit', 'v_1', 'location'), \
             ('r_2', 'place', 'place_gone', 'data_location_visit', 'v_2', 'location'), \
             ('r_3', 'place', 'place_gone', 'data_location_visit', 'v_1', 'location')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO app_pins (id, url) VALUES ('pin_1', '/place/place_gone')")
            .execute(&pool)
            .await
            .unwrap();

        merge_places(&pool, "place_keep", "place_gone").await.unwrap();

        let visits: Vec<String> =
            sqlx::query_scalar("SELECT source_id FROM wiki_refs WHERE entity_id = 'place_keep' ORDER BY source_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(visits, ["v_1", "v_2"], "both visits, v_1 once");
        let (name, named): (String, bool) =
            sqlx::query_as("SELECT name, is_named FROM wiki_places WHERE id = 'place_keep'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((name.as_str(), named), ("Corner bakery", true));
        let gone: i64 = sqlx::query_scalar("SELECT count(*) FROM wiki_places WHERE id = 'place_gone'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(gone, 0);
        let pin: String = sqlx::query_scalar("SELECT url FROM app_pins WHERE id = 'pin_1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(pin, "/place/place_keep");
    }

    /// Only named places close enough to be the same doorway are offered.
    #[sqlx::test]
    async fn nearby_offers_only_close_named_places(pool: PgPool) {
        insert_place(&pool, "place_me", "Location 30.0000, -97.0000", false, 30.0).await;
        insert_place(&pool, "place_close", "Corner bakery", true, 30.0009).await; // ~100 m
        insert_place(&pool, "place_far", "Library", true, 30.0030).await; // ~330 m
        insert_place(&pool, "place_unnamed", "Location 30.0002, -97.0000", false, 30.0002).await;
        let near = nearby_named_places(&pool, "place_me").await.unwrap();
        assert_eq!(near.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["place_close"]);
    }

    /// Renaming a place marks it named, which keeps it out of the naming queue
    /// and out of the empty-place sweep.
    #[sqlx::test]
    async fn renaming_a_place_marks_it_named(pool: PgPool) {
        insert_place(&pool, "place_x", "Location 30.0000, -97.0000", false, 30.0).await;
        crate::api::wiki::update_wiki_place(
            &pool,
            "place_x".into(),
            serde_json::from_value(serde_json::json!({ "name": "Studio" })).unwrap(),
        )
        .await
        .unwrap();
        let named: bool = sqlx::query_scalar("SELECT is_named FROM wiki_places WHERE id = 'place_x'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(named);
    }

    /// The owner is a person by definition (migration 0080). Deleting yourself
    /// from your own record is never what someone meant to click.
    #[sqlx::test]
    async fn the_self_person_cannot_be_deleted(pool: PgPool) {
        let id = create_person(&pool, "Adam").await.unwrap();
        sqlx::query("UPDATE app_user_profile SET self_person_id = $1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();

        assert!(delete_person(&pool, id).await.is_err());
    }
/// Every `data_*` table participates in the pipeline, or is exempted by name.
///
/// ## What this guards
///
/// A `data_*` table is one KIND OF OBSERVATION about the owner's life: one row
/// is one thing observed at one time, with provenance back to the stream that
/// delivered it. It is never derived from another table (that is `wiki_*`) and
/// never product state (that is `app_*`).
///
/// An `OntologyDescriptor` is that observation type's PARTICIPATION CONTRACT —
/// how to read its time, whether to embed it, whether it carries prose worth
/// extracting entities from, how it reaches a day page, whether it counts as the
/// owner doing something. (The word "ontology" is doing the work of "record
/// type" here; there is no concept hierarchy and no inference. See the note at
/// the top of `crates/virtues-registry/src/ontologies.rs`.)
///
/// `search/indexer.rs` decides what is searchable by iterating
/// `registered_ontologies()`. So a table with no descriptor is written and then
/// invisible — not searchable, absent from the lifeline, the dayline, and day
/// summaries. Nothing anywhere reports this: the collector succeeds, the rows
/// are there, and the data simply never appears.
///
/// It had happened SEVEN times before this test existed, including
/// `data_content_conversation` — imported AI chat history, which is prose, and
/// was unsearchable.
///
/// ## Why exemptions are named rather than inferred
///
/// A few tables genuinely should not be indexed, and the only way to tell them
/// from an oversight is for a person to say so. Adding a table without a
/// descriptor therefore costs one deliberate line here, which is the point: the
/// omission becomes a decision instead of an accident.
#[sqlx::test]
async fn every_data_table_participates_or_is_exempted(pool: sqlx::PgPool) {
    /// Tables deliberately without a participation contract, and why.
    ///
    /// KNOWN GAPS, not decisions — each needs a product call and then either a
    /// descriptor or a reason to move up into the exempt list above it:
    const EXEMPT: &[(&str, &str)] = &[
        (
            "data_audio_recording",
            "the audio blob itself; its WORDS are searchable through \
             data_communication_transcription, which shares its source_stream_id",
        ),
        // ── Known gaps below. Each is collected today and invisible. ─────────
        (
            "data_financial_asset",
            "GAP: holdings are collected by plaid_investments_sync and unreachable",
        ),
        (
            "data_financial_liability",
            "GAP: debts are collected by plaid_liabilities_sync and unreachable",
        ),
        (
            "data_health_active_energy",
            "GAP: written by ios_ingest, no lane, no measure",
        ),
        (
            "data_health_distance",
            "GAP: written by ios_ingest, no lane, no measure",
        ),
    ];

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_tables \
         WHERE schemaname = 'public' AND tablename LIKE 'data\\_%' ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .expect("list data_* tables");

    let described: std::collections::HashSet<&str> =
        virtues_registry::ontologies::registered_ontologies()
            .iter()
            .map(|o| o.table_name)
            .collect();
    let exempt: std::collections::HashSet<&str> = EXEMPT.iter().map(|(t, _)| *t).collect();

    // 1. No table is silently unreachable.
    let orphans: Vec<&String> = tables
        .iter()
        .filter(|t| !described.contains(t.as_str()) && !exempt.contains(t.as_str()))
        .collect();
    assert!(
        orphans.is_empty(),
        "these data_* tables have no OntologyDescriptor, so they are collected and \
         then invisible to search, the lifeline, the dayline and day summaries.\n\
         Give each one a descriptor, or add it to EXEMPT in this test with the \
         reason:\n  {orphans:?}"
    );

    // 2. And no descriptor points at a table that no longer exists — the same
    //    failure from the other side, which a rename produces.
    let real: std::collections::HashSet<&str> = tables.iter().map(String::as_str).collect();
    let dangling: Vec<&str> = described
        .iter()
        .copied()
        .filter(|t| t.starts_with("data_") && !real.contains(t))
        .collect();
    assert!(
        dangling.is_empty(),
        "these descriptors name a data_* table that does not exist: {dangling:?}"
    );

    // 3. An exemption for a table that is gone is stale bookkeeping; an
    //    exemption for a table that HAS a descriptor is a contradiction.
    for (t, _why) in EXEMPT {
        assert!(
            real.contains(t),
            "EXEMPT names {t}, which is not a table — remove the entry"
        );
        assert!(
            !described.contains(t),
            "{t} is both EXEMPT and described — delete the EXEMPT entry"
        );
    }
}

}
