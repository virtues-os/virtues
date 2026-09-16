//! The wiki's SUBJECTS: people, places, organizations, and the records that
//! reference them.
//!
//! A wiki page is not a separate construct — it is a view of a subject plus
//! whatever prose the record has written about it (`wiki_articles`).
//!
//! This file used to be everything the wiki room touched, at three thousand
//! lines, of which barely a third was about subjects at all. The day, its
//! events and its raw streams have their own modules now:
//!
//! | module | what it answers |
//! |---|---|
//! | this one | who and what the record knows about |
//! | [`crate::api::wiki_days`] | what a day was — the day row, its activity, its timeline |
//! | [`crate::api::wiki_events`] | the day's event spine, and editing it |
//! | [`crate::api::wiki_streams`] | the raw records beneath a day, unsynthesised |

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{Error, Result};

// ============================================================================
// Wiki Page Types - Entity Views
// ============================================================================

/// A person wiki page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPerson {
    pub id: String,
    pub name: String,
    pub content: Option<String>,
    /// Machine-written wikipedia-style record (entity_article applet). Never
    /// user-edited — `content`/`notes` carry the user's own writing.
    pub article: Option<String>,
    pub article_updated_at: Option<DateTime<Utc>>,
    /// Is this article being kept up to date? Off unless the user asked.
    #[serde(default)]
    pub article_maintained: bool,
    pub picture: Option<String>,
    pub cover_image: Option<String>,
    // vCard fields
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    pub birthday: Option<NaiveDate>,
    /// When they died, if they have — with the precision the person gave
    /// ("sometime in 2019" is a real answer; see migration 0016).
    pub died_on: Option<NaiveDate>,
    pub died_precision: Option<String>,
    pub instagram: Option<String>,
    pub facebook: Option<String>,
    pub linkedin: Option<String>,
    pub x: Option<String>,
    // Metadata
    pub relationship_category: Option<String>,
    pub nickname: Option<String>,
    /// The AUTHORED line about what this person means — one sentence, the
    /// owner's verbatim, never inferred. Recency says who is around; only
    /// this says who matters. Rendered above every observed stat.
    pub bond: Option<String>,
    // `notes` retired to wiki_notes (migration 0082). The column still exists —
    // drops trail by a release — but nothing reads or writes it from here, which
    // is what lets the next migration drop it safely.
    /// Surfaces this entity also answers to (0037). Read alongside write, or an
    /// editor cannot show what is already there.
    pub aliases: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A place wiki page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPlace {
    pub id: String,
    pub name: String,
    pub content: Option<String>,
    pub article: Option<String>,
    pub article_updated_at: Option<DateTime<Utc>>,
    /// Is this article being kept up to date? Off unless the user asked.
    #[serde(default)]
    pub article_maintained: bool,
    pub cover_image: Option<String>,
    pub category: Option<String>,
    pub address: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// Visits, COUNTED FROM `wiki_refs` rather than read off the row. The
    /// columns of these names had no writer, so the page reported "Total
    /// visits: 0" for somewhere the person goes weekly; they are dropped in
    /// 0025 and these fields now carry the real count and its two dates.
    pub seen_count: Option<i32>,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    /// The phone keeps no audio while the owner is inside this place.
    #[serde(default)]
    pub is_audio_muted: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An organization wiki page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiOrganization {
    pub id: String,
    pub name: String,
    pub content: Option<String>,
    pub article: Option<String>,
    pub article_updated_at: Option<DateTime<Utc>>,
    /// Is this article being kept up to date? Off unless the user asked.
    #[serde(default)]
    pub article_maintained: bool,
    pub cover_image: Option<String>,
    pub organization_type: Option<String>,
    pub relationship_type: Option<String>,
    pub role_title: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    /// Surfaces this entity also answers to (0037).
    pub aliases: Vec<String>,
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
    pub name: String,
    pub picture: Option<String>,
    pub relationship_category: Option<String>,
    /// How many records mention this entity — see `REF_COUNT` in this module.
    pub ref_count: i64,
}

/// A place list item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPlaceListItem {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub address: Option<String>,
    /// How many records mention this entity — see `REF_COUNT` in this module.
    pub ref_count: i64,
}

/// An organization list item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiOrganizationListItem {
    pub id: String,
    pub name: String,
    pub organization_type: Option<String>,
    pub relationship_type: Option<String>,
    /// How many records mention this entity — see `REF_COUNT` in this module.
    pub ref_count: i64,
}

// ============================================================================
// Update Request Types
// ============================================================================

/// Request to update a person wiki page
#[derive(Debug, Deserialize)]
pub struct UpdateWikiPersonRequest {
    pub name: Option<String>,
    pub content: Option<String>,
    pub picture: Option<String>,
    pub cover_image: Option<String>,
    pub emails: Option<Vec<String>>,
    pub phones: Option<Vec<String>>,
    pub birthday: Option<NaiveDate>,
    pub died_on: Option<NaiveDate>,
    pub died_precision: Option<String>,
    pub instagram: Option<String>,
    pub facebook: Option<String>,
    pub linkedin: Option<String>,
    pub x: Option<String>,
    pub relationship_category: Option<String>,
    pub nickname: Option<String>,
    /// One sentence, theirs verbatim — see WikiPerson::bond.
    pub bond: Option<String>,
    // `notes` retired to wiki_notes (migration 0082). The column still exists —
    // drops trail by a release — but nothing reads or writes it from here, which
    // is what lets the next migration drop it safely.
    /// Surfaces this entity also answers to. 0037 calls an alias "the record of
    /// a human decision" and built the column for exactly this — then nothing
    /// ever wrote it: 3 of 573 people on a real box have one. Stored
    /// lowercased; the resolver lowercases the surface before matching, so a
    /// name linked once resolves every past and future mention of it.
    pub aliases: Option<Vec<String>>,
}

/// Request to update a place wiki page
#[derive(Debug, Deserialize)]
pub struct UpdateWikiPlaceRequest {
    pub name: Option<String>,
    pub content: Option<String>,
    pub cover_image: Option<String>,
    pub category: Option<String>,
    pub address: Option<String>,
    /// Mute the phone's audio collector inside this place.
    pub is_audio_muted: Option<bool>,
}

/// Request to update an organization wiki page
#[derive(Debug, Deserialize)]
pub struct UpdateWikiOrganizationRequest {
    pub name: Option<String>,
    pub content: Option<String>,
    pub cover_image: Option<String>,
    pub organization_type: Option<String>,
    pub relationship_type: Option<String>,
    pub role_title: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    /// Surfaces this entity also answers to. 0037 calls an alias "the record of
    /// a human decision" and built the column for exactly this — then nothing
    /// ever wrote it: 3 of 573 people on a real box have one. Stored
    /// lowercased; the resolver lowercases the surface before matching, so a
    /// name linked once resolves every past and future mention of it.
    pub aliases: Option<Vec<String>>,
}

/// Request to update a day wiki page

// ============================================================================
// Person CRUD Operations
// ============================================================================

/// Get a person by ID
pub async fn get_person(pool: &PgPool, id: String) -> Result<WikiPerson> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, name, content, picture, cover_image,
            emails, phones, birthday, died_on, died_precision, instagram,
            facebook, linkedin, x,
            relationship_category, nickname, bond, aliases,
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

    let (article, article_updated_at, maintained) =
        overlay_article(pool, "person", &row.id).await;

    Ok(WikiPerson {
        id: row.id,
        name: row.name,
        content: row.content,
        article,
        article_updated_at,
        article_maintained: maintained,
        picture: row.picture,
        cover_image: row.cover_image,
        emails: serde_json::from_value(row.emails).unwrap_or_default(),
        phones: serde_json::from_value(row.phones).unwrap_or_default(),
        aliases: serde_json::from_value(row.aliases).unwrap_or_default(),
        birthday: row.birthday,
        died_on: row.died_on,
        died_precision: row.died_precision,
        instagram: row.instagram,
        facebook: row.facebook,
        linkedin: row.linkedin,
        x: row.x,
        relationship_category: row.relationship_category,
        nickname: row.nickname,
        bond: row.bond,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// List all people

/// Overlay a subject's article from `wiki_articles` onto the legacy column.
///
/// Prose moved to `app_pages` (migration 0081), but the per-entity `article`
/// columns from 0072 are still there — drops trail their phase by a release, so
/// a box in the middle can hold prose in either place. New articles live on the
/// page; anything written before the move still lives in the column. Read the
/// page first and fall back, so neither is lost while both exist.
async fn overlay_article(
    pool: &PgPool,
    subject_type: &str,
    subject_id: &str,
) -> (Option<String>, Option<DateTime<Utc>>, bool) {
    match crate::api::wiki_articles::get_article_prose(pool, subject_type, subject_id).await {
        Ok(Some(a)) => (Some(a.content), Some(a.updated_at), a.maintained),
        // No article is the ordinary case, not an error: they are opt-in.
        Ok(None) => (None, None, false),
        // A read failure must not take the whole entity page down with it — the
        // records below the article are the more important half.
        //
        // This used to fall back to the entity's own `article` column, which
        // had no writer on any box, so the fallback could only ever return the
        // same None it now returns directly.
        Err(e) => {
            tracing::warn!(subject_id, error = %e, "article read failed");
            (None, None, false)
        }
    }
}

/// Aliases are stored lowercased, trimmed, deduped, and never empty.
///
/// 0037 stores them lowercased and matches with `aliases ? lower(surface)`, so
/// a mixed-case alias is simply invisible to the resolver — it would look
/// saved and never resolve anything. Normalizing on the way in is the only
/// place that can be enforced once for every caller.
fn normalize_aliases(input: Option<&Vec<String>>) -> Option<serde_json::Value> {
    let list = input?;
    let mut seen: Vec<String> = Vec::with_capacity(list.len());
    for raw in list {
        let a = raw.trim().to_lowercase();
        if !a.is_empty() && !seen.contains(&a) {
            seen.push(a);
        }
    }
    Some(serde_json::json!(seen))
}

/// Entity indexes sort by how many records mention the entity, not by name.
///
/// The People index had no order at all: it sorted by `name`, and the
/// column that was supposed to carry importance — `seen_count` — is 0 on
/// every row on a real box, because nothing has ever written it. So an address
/// book of 573 contacts arrived alphabetically, with `no-reply@slack.com` sitting
/// level with the people you actually talk to.
///
/// The signal was always there: `wiki_refs` holds 130k message refs
/// across 314 people. Counting them sorts the wall on its own — people you
/// message rise, contacts with no traffic sink, transactional senders land at
/// the bottom with two email refs each. No classifier, no model, no deletion:
/// the noise does not need removing, it needs ordering.
///
/// **Computed per query, not materialized.** The obvious move is a counter
/// column, but that needs a refresh path and can drift, and this is a sort key
/// rather than a fact. Measured on the real corpus (131k refs, 573 people) the
/// aggregate runs in 11 ms, which is cheaper than being wrong. Materialize it
/// when the index gets slow, and not before.
///
/// Deliberately NOT `seen_count` or `wiki_places.seen_count`: those are
/// two different quantities on two tables (and visits are not refs), so reusing
/// either would make "the default sort" mean something different per index.
/// Both are legacy; this is the one uniform measure.
pub async fn list_people(pool: &PgPool) -> Result<Vec<WikiPersonListItem>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            p.id, p.name, p.picture, p.relationship_category,
            COALESCE(r.n, 0) AS "ref_count!"
        FROM wiki_people p
        LEFT JOIN (
            SELECT entity_id, count(*) AS n
            FROM wiki_refs WHERE entity_type = 'person' GROUP BY entity_id
        ) r ON r.entity_id = p.id
        ORDER BY COALESCE(r.n, 0) DESC, p.name ASC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list people: {}", e)))?;

    Ok(rows
        .into_iter()
        .map(|row| WikiPersonListItem {
            id: row.id,
            name: row.name,
            picture: row.picture,
            relationship_category: row.relationship_category,
            ref_count: row.ref_count,
        })
        .collect())
}

/// Update a person
pub async fn update_person(
    pool: &PgPool,
    id: String,
    req: UpdateWikiPersonRequest,
) -> Result<WikiPerson> {
    let emails_json: Option<serde_json::Value> = req.emails.as_ref().map(|e| serde_json::json!(e));
    let phones_json: Option<serde_json::Value> = req.phones.as_ref().map(|p| serde_json::json!(p));
    let aliases_json = normalize_aliases(req.aliases.as_ref());

    sqlx::query!(
        r#"
        UPDATE wiki_people
        SET
            name = COALESCE($2, name),
            content = COALESCE($3, content),
            picture = COALESCE($4, picture),
            cover_image = COALESCE($5, cover_image),
            emails = COALESCE($6, emails),
            phones = COALESCE($7, phones),
            birthday = COALESCE($8, birthday),
            died_on = COALESCE($16, died_on),
            died_precision = COALESCE($17, died_precision),
            bond = COALESCE($18, bond),
            instagram = COALESCE($9, instagram),
            facebook = COALESCE($10, facebook),
            linkedin = COALESCE($11, linkedin),
            x = COALESCE($12, x),
            relationship_category = COALESCE($13, relationship_category),
            nickname = COALESCE($14, nickname),
            aliases = COALESCE($15, aliases),
            updated_at = now()
        WHERE id = $1
        "#,
        id,
        req.name,
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
        aliases_json,
        req.died_on,
        req.died_precision,
        req.bond
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
pub async fn get_wiki_place(pool: &PgPool, id: String) -> Result<WikiPlace> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, name, content, cover_image, category, address,
            latitude, longitude, is_audio_muted,
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

    let (article, article_updated_at, maintained) =
        overlay_article(pool, "place", &row.id).await;

    let visits: (i64, Option<DateTime<Utc>>, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT count(*), min(occurred_at), max(occurred_at) \
         FROM wiki_refs WHERE entity_id = $1 AND role = 'location'",
    )
    .bind(&row.id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to count visits: {e}")))?;

    Ok(WikiPlace {
        id: row.id,
        name: row.name.clone(),
        content: row.content.clone(),
        article,
        article_updated_at,
        article_maintained: maintained,
        cover_image: row.cover_image.clone(),
        category: row.category.clone(),
        address: row.address.clone(),
        latitude: row.latitude,
        longitude: row.longitude,
        // Counted from the refs, not read off the row. `seen_count`,
        // `first_seen` and `last_seen` have no writer anywhere, so the place
        // page showed "Total visits: 0" and no dates for somewhere the person
        // had been fifty times — the same zero the article prompt was being
        // fed. The refs are where the visits actually are.
        seen_count: Some(visits.0 as i32),
        first_seen: visits.1,
        last_seen: visits.2,
        is_audio_muted: row.is_audio_muted,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// List all places (wiki view with content fields)
pub async fn list_wiki_places(pool: &PgPool) -> Result<Vec<WikiPlaceListItem>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            p.id, p.name, p.category, p.address,
            COALESCE(r.n, 0) AS "ref_count!"
        FROM wiki_places p
        LEFT JOIN (
            SELECT entity_id, count(*) AS n
            FROM wiki_refs WHERE entity_type = 'place' GROUP BY entity_id
        ) r ON r.entity_id = p.id
        ORDER BY COALESCE(r.n, 0) DESC, p.name ASC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list places: {}", e)))?;

    Ok(rows
        .into_iter()
        .map(|row| WikiPlaceListItem {
            id: row.id,
            name: row.name,
            category: row.category,
            address: row.address,
            ref_count: row.ref_count,
        })
        .collect())
}

/// Update a place wiki content
pub async fn update_wiki_place(
    pool: &PgPool,
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
            is_audio_muted = COALESCE($7, is_audio_muted),
            updated_at = now()
        WHERE id = $1
        "#,
        id,
        req.name,
        req.content,
        req.cover_image,
        req.category,
        req.address,
        req.is_audio_muted
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
pub async fn get_organization(pool: &PgPool, id: String) -> Result<WikiOrganization> {
    let row = sqlx::query!(
        r#"
        SELECT
            id, name, content, cover_image,
            organization_type, relationship_type, role_title, aliases,
            start_date, end_date,
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

    let (article, article_updated_at, maintained) =
        overlay_article(pool, "organization", &row.id).await;

    Ok(WikiOrganization {
        id: row.id,
        name: row.name,
        content: row.content,
        article,
        article_updated_at,
        article_maintained: maintained,
        cover_image: row.cover_image,
        organization_type: row.organization_type,
        relationship_type: row.relationship_type,
        role_title: row.role_title,
        start_date: row.start_date,
        end_date: row.end_date,
        aliases: serde_json::from_value(row.aliases).unwrap_or_default(),
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// List all organizations
pub async fn list_organizations(pool: &PgPool) -> Result<Vec<WikiOrganizationListItem>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            o.id, o.name, o.organization_type, o.relationship_type,
            COALESCE(r.n, 0) AS "ref_count!"
        FROM wiki_orgs o
        LEFT JOIN (
            SELECT entity_id, count(*) AS n
            FROM wiki_refs WHERE entity_type = 'organization' GROUP BY entity_id
        ) r ON r.entity_id = o.id
        ORDER BY COALESCE(r.n, 0) DESC, o.name ASC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list organizations: {}", e)))?;

    Ok(rows
        .into_iter()
        .map(|row| WikiOrganizationListItem {
            id: row.id,
            name: row.name,
            organization_type: row.organization_type,
            relationship_type: row.relationship_type,
            ref_count: row.ref_count,
        })
        .collect())
}

/// Update an organization
pub async fn update_organization(
    pool: &PgPool,
    id: String,
    req: UpdateWikiOrganizationRequest,
) -> Result<WikiOrganization> {
    sqlx::query!(
        r#"
        UPDATE wiki_orgs
        SET
            name = COALESCE($2, name),
            content = COALESCE($3, content),
            cover_image = COALESCE($4, cover_image),
            organization_type = COALESCE($5, organization_type),
            relationship_type = COALESCE($6, relationship_type),
            role_title = COALESCE($7, role_title),
            start_date = COALESCE($8, start_date),
            end_date = COALESCE($9, end_date),
            aliases = COALESCE($10, aliases),
            updated_at = now()
        WHERE id = $1
        "#,
        id,
        req.name,
        req.content,
        req.cover_image,
        req.organization_type,
        req.relationship_type,
        req.role_title,
        req.start_date,
        req.end_date,
        normalize_aliases(req.aliases.as_ref())
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update organization: {}", e)))?;

    get_organization(pool, id).await
}

// ============================================================================
// Narrative Identity
// ============================================================================

/// The user's narrative identity — the "In your own words" DOCUMENT, read
/// straight from its wiki article's page. There is no separate stored copy
/// (the abridged capsule was retired 2026-09-01): editing happens on the page
/// itself, which is why this view is read-only and carries the page id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeIdentity {
    pub content: String,
    pub updated_at: DateTime<Utc>,
    /// The document's page — where editing happens. Empty until the interview
    /// has been written up.
    pub page_id: String,
}

/// Get the narrative identity. Before the interview is written up there is no
/// article yet, so we return an empty placeholder (content = "") rather than
/// 500ing — the clients treat empty content as "not authored yet".
pub async fn get_narrative_identity(pool: &PgPool) -> Result<NarrativeIdentity> {
    let article = crate::api::wiki_articles::get_article(
        pool,
        "narrative_identity",
        crate::api::narrative_draft::NAR_IDENTITY_ID,
    )
    .await?;
    let Some(article) = article else {
        return Ok(NarrativeIdentity {
            content: String::new(),
            updated_at: Utc::now(),
            page_id: String::new(),
        });
    };
    let prose = crate::api::wiki_articles::get_article_prose(
        pool,
        "narrative_identity",
        crate::api::narrative_draft::NAR_IDENTITY_ID,
    )
    .await?;
    Ok(match prose {
        Some(p) => NarrativeIdentity {
            content: p.content,
            updated_at: p.updated_at,
            page_id: article.page_id,
        },
        None => NarrativeIdentity {
            content: String::new(),
            updated_at: Utc::now(),
            page_id: article.page_id,
        },
    })
}

/// One raw record linked to an entity via `wiki_refs` — the entity
/// page's CRM-style evidence feed. Same shape as `DaySource` plus the ref's
/// `role` (sender, attendee, merchant, location, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRecord {
    pub source_type: String,
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub label: String,
    pub preview: Option<String>,
    pub role: Option<String>,
    pub continuous: bool,
}

/// One page of an entity's records, plus the true total for the query — the
/// server half of the grid's server-side pagination.
#[derive(Debug, Serialize)]
pub struct EntityRecordsPage {
    pub items: Vec<EntityRecord>,
    pub total: i64,
}

/// Per-raw-source_type counts across ALL of an entity's records, for the chip
/// rail. Computed server-side because the grid only ever holds one page —
/// chips counted from loaded rows would lie.
#[derive(Debug, Serialize)]
pub struct EntityRecordFacet {
    pub source_type: String,
    pub count: i64,
    pub continuous: bool,
}

/// Requests can't ask for unbounded pages.
const ENTITY_RECORDS_MAX_LIMIT: i64 = 100;

/// Build the UNION ALL body over every source table holding refs for this
/// entity, each subquery rendered with its ontology's `DaySourceConfig` SQL
/// (same labels/previews as the day page). Roles are merged per record inside
/// each subquery, so pagination and totals count records, not refs. Returns
/// `None` when the entity has no refs in any renderable table.
///
/// All subqueries bind the entity id as `$1`; callers add outer binds from $2.
async fn entity_records_union(pool: &PgPool, entity_id: &str) -> Result<Option<String>> {
    use virtues_registry::ontologies::registered_ontologies;

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT source_table FROM wiki_refs WHERE entity_id = $1",
    )
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list entity ref tables: {}", e)))?;

    let mut subqueries: Vec<String> = Vec::new();
    for table in &tables {
        let Some(ont) = registered_ontologies()
            .into_iter()
            .find(|o| o.table_name == table.as_str() && o.day_source.is_some())
        else {
            // Refs exist but no ontology knows how to render them. Unlike the
            // day pipeline (where a hole feeds an LLM), this only starves a
            // reading surface — so skip loudly instead of failing the page,
            // and treat the log line as the bug report it is.
            tracing::warn!(
                source_table = %table,
                entity_id = %entity_id,
                "entity refs point at a table with no day-source config; its records are invisible"
            );
            continue;
        };
        let cfg = ont.day_source.as_ref().unwrap();

        let source_type_col = cfg
            .source_type_sql
            .map(|sql| format!("{} as source_type_dyn", sql))
            .unwrap_or_else(|| format!("'{}' as source_type_dyn", cfg.source_type));

        // Notes on the shape:
        //  - `WHERE true` so `extra_where` (which carries its own leading AND)
        //    splices the same way it does in the day-source template.
        //  - The refs JOIN introduces a second `timestamp` column, so the
        //    ontology's timestamp must be `t.`-qualified or Postgres calls it
        //    ambiguous.
        //  - The refs unique key includes `role`, so one record can join once
        //    per role (sender AND recipient): the GROUP BY collapses those to
        //    one row with the roles aggregated. Positional GROUP BY, because
        //    the grouped expressions are registry-supplied SQL.
        //  - `src_cont` is a bare literal, which Postgres exempts from
        //    GROUP BY.
        subqueries.push(format!(
            "SELECT {id} as src_id, t.{ts} as src_ts, {label} as src_label, \
                    {preview} as src_preview, {st}, \
                    string_agg(DISTINCT er.role, ', ') as src_role, \
                    {cont} as src_cont \
             FROM {table} t \
             JOIN wiki_refs er \
               ON er.source_table = '{table}' AND er.source_id = {id} AND er.entity_id = $1 \
             WHERE true \
             {extra} \
             GROUP BY 1, 2, 3, 4, 5",
            id = cfg.id_sql,
            ts = ont.timestamp_column,
            label = cfg.label_sql,
            preview = cfg.preview_sql,
            st = source_type_col,
            cont = if ont.temporal_type == virtues_registry::ontologies::TemporalType::Continuous {
                "TRUE"
            } else {
                "FALSE"
            },
            table = ont.table_name,
            extra = cfg.extra_where.unwrap_or(""),
        ));
    }

    Ok(if subqueries.is_empty() {
        None
    } else {
        Some(subqueries.join(" UNION ALL "))
    })
}

/// Shared narrowing clause for the union: $2 = search text ('' = all),
/// $3 = raw source_type allowlist (empty array = all).
const ENTITY_RECORDS_WHERE: &str = "($2 = '' \
       OR u.src_label ILIKE '%' || $2 || '%' \
       OR COALESCE(u.src_preview, '') ILIKE '%' || $2 || '%') \
   AND (cardinality($3::text[]) = 0 OR u.source_type_dyn = ANY($3::text[]))";

fn entity_record_from_row(row: &sqlx::postgres::PgRow) -> Option<EntityRecord> {
    use sqlx::Row;
    Some(EntityRecord {
        id: row.try_get("src_id").ok()?,
        // TIMESTAMPTZ — decode directly, never via String (see get_day_sources).
        timestamp: row.try_get("src_ts").ok()?,
        label: row.try_get("src_label").unwrap_or_default(),
        preview: row.try_get("src_preview").ok().flatten(),
        source_type: row.try_get("source_type_dyn").unwrap_or_default(),
        role: row.try_get("src_role").ok().flatten(),
        continuous: row.try_get("src_cont").unwrap_or(false),
    })
}

/// One page of the records linked to an entity (registry-driven, refs-driven),
/// with search and source_type narrowing applied server-side.
pub async fn get_entity_records_page(
    pool: &PgPool,
    entity_id: &str,
    offset: i64,
    limit: i64,
    search: &str,
    types: &[String],
    newest_first: bool,
) -> Result<EntityRecordsPage> {
    let limit = limit.clamp(1, ENTITY_RECORDS_MAX_LIMIT);
    let offset = offset.max(0);

    let Some(union) = entity_records_union(pool, entity_id).await? else {
        return Ok(EntityRecordsPage { items: Vec::new(), total: 0 });
    };

    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM ({union}) u WHERE {ENTITY_RECORDS_WHERE}"
    ))
    .bind(entity_id)
    .bind(search)
    .bind(types)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("entity records count failed: {e}")))?;

    let dir = if newest_first { "DESC" } else { "ASC" };
    let rows = sqlx::query(&format!(
        "SELECT * FROM ({union}) u WHERE {ENTITY_RECORDS_WHERE} \
         ORDER BY u.src_ts {dir} LIMIT $4 OFFSET $5"
    ))
    .bind(entity_id)
    .bind(search)
    .bind(types)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("entity records page failed: {e}")))?;

    Ok(EntityRecordsPage {
        items: rows.iter().filter_map(entity_record_from_row).collect(),
        total,
    })
}

/// Facet counts over ALL of an entity's records (unnarrowed), for the chips.
pub async fn get_entity_record_facets(
    pool: &PgPool,
    entity_id: &str,
) -> Result<Vec<EntityRecordFacet>> {
    use sqlx::Row;

    let Some(union) = entity_records_union(pool, entity_id).await? else {
        return Ok(Vec::new());
    };

    let rows = sqlx::query(&format!(
        "SELECT u.source_type_dyn as st, count(*) as n, bool_and(u.src_cont) as cont \
         FROM ({union}) u GROUP BY 1 ORDER BY 1"
    ))
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("entity record facets failed: {e}")))?;

    Ok(rows
        .iter()
        .filter_map(|row| {
            Some(EntityRecordFacet {
                source_type: row.try_get("st").ok()?,
                count: row.try_get("n").ok()?,
                continuous: row.try_get("cont").unwrap_or(false),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of normalizing is that 0037 matches with
    /// `aliases ? lower(surface)`. An alias stored with capitals or padding is
    /// not "slightly wrong" — it is invisible to the resolver, so it looks
    /// saved and resolves nothing. These cases are the ones a human actually
    /// types.
    #[test]
    fn aliases_are_lowercased_trimmed_and_deduped() {
        let input = vec![
            "  Sarah ".to_string(),
            "SARAH".to_string(), // same surface, different case
            "sarah".to_string(), // exact duplicate
            "Mum".to_string(),
            "   ".to_string(), // whitespace only
            "".to_string(),
        ];
        let out = normalize_aliases(Some(&input)).expect("some input");
        assert_eq!(out, serde_json::json!(["sarah", "mum"]));
    }

    /// `None` must stay `None`: the update statement is
    /// `aliases = COALESCE($n, aliases)`, so a null leaves the column alone.
    /// Returning an empty array instead would silently erase every alias on
    /// any request that simply did not mention them.
    #[test]
    fn absent_aliases_do_not_clear_the_column() {
        assert!(normalize_aliases(None).is_none());
    }

    /// Clearing has to remain possible, and is distinct from "not mentioned".
    #[test]
    fn an_explicit_empty_list_clears() {
        let empty: Vec<String> = vec![];
        assert_eq!(
            normalize_aliases(Some(&empty)).expect("some"),
            serde_json::json!([])
        );
    }
}
