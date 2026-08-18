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
    pub seen_count: Option<i32>,
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
            seen_count,
            metadata,
            created_at,
            updated_at
        FROM wiki_places
        WHERE (metadata->>'is_known_location')::boolean = true
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
            seen_count: Some(row.seen_count as i32),
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
            seen_count,
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
        seen_count: Some(row.seen_count as i32),
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
            metadata
        ) VALUES (
            $1, $2, $3, $4, $5, $6, 50.0, $7
        )
        "#,
        id_str,
        req.label,
        req.category,
        req.formatted_address,
        req.latitude,
        req.longitude,
        metadata,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create place: {}", e)))?;

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
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update place: {}", e)))?;

    // Fetch the updated place
    get_place(pool, id).await
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
/// free text in `app_pins` / `app_notebook_items` — the trap 0071 documented
/// when it dropped `wiki_things` and had to sweep `/thing/` urls in the same
/// migration. Columns orgs do not have (emails, phones, socials) are preserved
/// into `metadata` rather than dropped: reclassifying is a filing correction,
/// not a decision to forget anything.
pub async fn reclassify_person_as_organization(pool: &PgPool, person_id: String) -> Result<String> {
    let person = sqlx::query!(
        r#"
        SELECT name, emails, phones, handles, nickname,
               first_seen, last_seen, seen_count,
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
        INSERT INTO wiki_orgs (id, name, seen_count, first_seen,
                               last_seen, metadata, content, aliases)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
        &org_id,
        &person.name,
        person.seen_count,
        person.first_seen,
        person.last_seen,
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
    for table in ["app_pins", "app_notebook_items"] {
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
            "That person is you — reclassifying yourself as an organization is not what you meant"
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
/// notebook item kept a `/place/<id>` url that rendered as a row nothing could
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
    for table in ["app_pins", "app_notebook_items"] {
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
            "That person is you — deleting yourself from your own record is not what you meant"
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
            "data_content_conversation",
            "GAP: imported AI chat history — prose, and should be searchable",
        ),
        (
            "data_environment_weather",
            "GAP: ambient conditions; needs a decision on whether weather is a \
             lifeline lane or only day-page context",
        ),
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
