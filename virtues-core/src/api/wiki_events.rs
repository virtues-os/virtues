//! The day's EVENT SPINE — a gapless, ordered sequence of what happened, and
//! the reads and writes over it.
//!
//! Event resolution builds this nightly out of incomplete, out-of-order and
//! sometimes contradictory evidence; the design record is
//! `agents/record/event-timeline.md`. What lives here is the CRUD around it:
//! reading a day's events, and the person's own corrections, which are the
//! rows that must never be overwritten by the next nightly pass.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::ids;
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
    pub topics: Option<serde_json::Value>,
    pub event_summary: Option<String>,
    pub agent_action: Option<String>,
    pub is_sleep: Option<bool>,
    pub user_hidden: Option<bool>,
    // Entity/topic novelty
    pub entities: Option<serde_json::Value>,
    pub topic_novelty: Option<serde_json::Value>,
    pub entity_novelty: Option<serde_json::Value>,
    /// `{entity_id: name}` for every subject this event references.
    ///
    /// The ids alone travelled to the client for the life of the project and
    /// nothing could draw them, because nothing on that side can turn
    /// `person_a1b2c3d4` into a person. The chart tried, by stripping
    /// `person_demo_` off the front with a regex — which reads correctly on
    /// the seeded demo box and renders a hash fragment on every real one.
    pub entity_names: Option<serde_json::Value>,
    /// Map of entity_id → ISO8601 timestamp of earliest ref within event window.
    /// Sourced from wiki_refs. Used to position entity dots at their actual
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
    /// 1-3 sentence factual description of the event. Renders in the day page
    /// timeline as the expandable detail under the label. Optional.
    pub event_summary: Option<String>,
    /// Topical tags emitted by the segmenting LLM. Written on INSERT rather
    /// than a follow-up UPDATE, because this row is about to be read by
    /// `topic_entity_novelty` — which, until topics were emitted at all, scored
    /// an empty array on every cron-generated event.
    pub topics: Option<serde_json::Value>,
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
pub async fn get_day_events(pool: &PgPool, day_id: String) -> Result<Vec<TemporalEvent>> {
    use sqlx::Row;
    use std::collections::HashMap;

    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT
            id, day_id, started_at, ended_at,
            auto_label, auto_location, user_label, user_location, user_notes,
            source_ontologies, is_unknown, is_transit, is_user_added, is_user_edited,
            novelty_z, avg_hr, autonomic_z, hr_z,
            topics, event_summary, agent_action,
            is_sleep, user_hidden,
            entities, topic_novelty, entity_novelty,
            created_at, updated_at
        FROM wiki_events
        WHERE day_id = $1
        ORDER BY started_at ASC
        "#,
    )
    .bind(&day_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get day events: {}", e)))?;

    // Fetch entity timestamps for the day: for each event's window, the earliest
    // timestamp each entity appears in wiki_refs.
    let event_windows: Vec<(String, DateTime<Utc>, DateTime<Utc>)> = rows
        .iter()
        .filter_map(|row| {
            let id: String = row.try_get("id").ok()?;
            let start: DateTime<Utc> = row.try_get("started_at").ok()?;
            let end: DateTime<Utc> = row.try_get("ended_at").ok()?;
            Some((id, start, end))
        })
        .collect();

    let mut entity_ts_by_event: HashMap<String, serde_json::Value> = HashMap::new();
    for (event_id, start, end) in &event_windows {
        let ref_rows: Vec<(String, DateTime<Utc>)> = sqlx::query_as(
            r#"
            SELECT entity_id, MIN(occurred_at) as earliest
            FROM wiki_refs
            WHERE occurred_at IS NOT NULL
              AND occurred_at >= $1
              AND occurred_at < $2
            GROUP BY entity_id
            "#,
        )
        .bind(*start)
        .bind(*end)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to load entity refs for day events: {}", e)))?;

        if !ref_rows.is_empty() {
            let map: serde_json::Map<String, serde_json::Value> = ref_rows
                .into_iter()
                .map(|(id, ts)| (id, serde_json::Value::String(ts.to_rfc3339())))
                .collect();
            entity_ts_by_event.insert(event_id.clone(), serde_json::Value::Object(map));
        }
    }

    // Every subject id the day's events point at, from both places one can
    // appear: the event's own `entities` list and the keys of its novelty map.
    // One query for the day rather than one per event — a busy day has fifteen
    // events and perhaps a dozen distinct subjects between them.
    let mut wanted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for row in &rows {
        if let Ok(Some(serde_json::Value::Array(ids))) =
            row.try_get::<Option<serde_json::Value>, _>("entities")
        {
            wanted.extend(ids.iter().filter_map(|v| v.as_str().map(str::to_string)));
        }
        if let Ok(Some(serde_json::Value::Object(map))) =
            row.try_get::<Option<serde_json::Value>, _>("entity_novelty")
        {
            wanted.extend(map.keys().cloned());
        }
    }

    let mut names: HashMap<String, String> = HashMap::new();
    if !wanted.is_empty() {
        let ids: Vec<String> = wanted.into_iter().collect();
        let found: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, name FROM wiki_people WHERE id = ANY($1) \
             UNION ALL SELECT id, name FROM wiki_places WHERE id = ANY($1) \
             UNION ALL SELECT id, name FROM wiki_orgs   WHERE id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to name the day's subjects: {e}")))?;
        names.extend(found);
    }

    Ok(rows
        .iter()
        .filter_map(|row| {
            let id: String = row.try_get("id").ok()?;
            let day_id: String = row.try_get("day_id").ok()?;
            // TIMESTAMPTZ columns decode directly into DateTime<Utc>; the prior
            // `try_get::<String>` + parse_from_rfc3339 failed at the decode step
            // (timestamptz is not text), so `.ok()?` dropped every row and the
            // timeline came back empty. JSONB decodes into serde_json::Value and
            // BOOLEAN into bool — no string round-trip, no `!= 0`.
            let start_time: DateTime<Utc> = row.try_get("started_at").ok()?;
            let end_time: DateTime<Utc> = row.try_get("ended_at").ok()?;
            let created_at: DateTime<Utc> = row.try_get("created_at").ok()?;
            let updated_at: DateTime<Utc> = row.try_get("updated_at").ok()?;
            let entity_timestamps = entity_ts_by_event.get(&id).cloned();

            // Only the names this event refers to. A resolved id is one an
            // entity table still holds; an unresolved one is omitted rather
            // than guessed at, so a deleted subject reads as absent instead of
            // as its own id.
            let entity_names = {
                let mut m = serde_json::Map::new();
                let mut take = |k: &str| {
                    if let Some(n) = names.get(k) {
                        m.insert(k.to_string(), serde_json::Value::String(n.clone()));
                    }
                };
                if let Ok(Some(serde_json::Value::Array(ids))) =
                    row.try_get::<Option<serde_json::Value>, _>("entities")
                {
                    for v in ids.iter().filter_map(|v| v.as_str()) {
                        take(v);
                    }
                }
                if let Ok(Some(serde_json::Value::Object(nov))) =
                    row.try_get::<Option<serde_json::Value>, _>("entity_novelty")
                {
                    for k in nov.keys() {
                        take(k);
                    }
                }
                (!m.is_empty()).then(|| serde_json::Value::Object(m))
            };

            Some(TemporalEvent {
                id,
                day_id,
                start_time,
                end_time,
                auto_label: row.try_get::<Option<String>, _>("auto_label").ok().flatten(),
                auto_location: row.try_get::<Option<String>, _>("auto_location").ok().flatten(),
                user_label: row.try_get::<Option<String>, _>("user_label").ok().flatten(),
                user_location: row.try_get::<Option<String>, _>("user_location").ok().flatten(),
                user_notes: row.try_get::<Option<String>, _>("user_notes").ok().flatten(),
                source_ontologies: row.try_get::<Option<serde_json::Value>, _>("source_ontologies").ok().flatten(),
                is_unknown: row.try_get::<Option<bool>, _>("is_unknown").ok().flatten(),
                is_transit: row.try_get::<Option<bool>, _>("is_transit").ok().flatten(),
                is_user_added: row.try_get::<Option<bool>, _>("is_user_added").ok().flatten(),
                is_user_edited: row.try_get::<Option<bool>, _>("is_user_edited").ok().flatten(),
                novelty_z: row.try_get::<Option<f64>, _>("novelty_z").ok().flatten(),
                avg_hr: row.try_get::<Option<f64>, _>("avg_hr").ok().flatten(),
                autonomic_z: row.try_get::<Option<f64>, _>("autonomic_z").ok().flatten(),
                hr_z: row.try_get::<Option<f64>, _>("hr_z").ok().flatten(),
                topics: row.try_get::<Option<serde_json::Value>, _>("topics").ok().flatten(),
                event_summary: row.try_get::<Option<String>, _>("event_summary").ok().flatten(),
                agent_action: row.try_get::<Option<String>, _>("agent_action").ok().flatten(),
                is_sleep: row.try_get::<Option<bool>, _>("is_sleep").ok().flatten(),
                user_hidden: row.try_get::<Option<bool>, _>("user_hidden").ok().flatten(),
                entities: row.try_get::<Option<serde_json::Value>, _>("entities").ok().flatten(),
                topic_novelty: row.try_get::<Option<serde_json::Value>, _>("topic_novelty").ok().flatten(),
                entity_novelty: row.try_get::<Option<serde_json::Value>, _>("entity_novelty").ok().flatten(),
                entity_names,
                entity_timestamps,
                created_at,
                updated_at,
            })
        })
        .collect())
}

/// Get events for a day by date
pub async fn get_events_by_date(pool: &PgPool, date: NaiveDate) -> Result<Vec<TemporalEvent>> {
    let day = crate::api::wiki_days::get_or_create_day(pool, date).await?;
    get_day_events(pool, day.id).await
}

/// Create a temporal event
///
/// Takes any executor so the segmenter can run every insert of a re-cut inside
/// ONE transaction with the delete that precedes them (see
/// `day_summary::store_structured_events`). The id is content-addressed from the
/// boundaries, so a fresh cut can land on exactly the span of an event the user
/// edited, hid, or added — the delete deliberately spares those rows. In that
/// case the insert is a no-op (`ON CONFLICT DO NOTHING`) rather than a unique
/// violation: a violation would abort the transaction and throw away the whole
/// cut, and the user's judgement outranks the model's anyway. It surfaces as
/// `Error::InvalidInput` so the caller can tell "already there" from a real
/// failure; the API caller treats it as the duplicate it is.
pub async fn create_temporal_event<'e>(
    exec: impl sqlx::PgExecutor<'e>,
    req: CreateTemporalEventRequest,
) -> Result<TemporalEvent> {
    use sqlx::Row;

    let day_id_str = req.day_id.to_string();
    let start_time_str = req.start_time.to_rfc3339();
    let end_time_str = req.end_time.to_rfc3339();
    // `source_ontologies` is NOT NULL with a `'[]'` default, and the segmentation
    // path deliberately passes None — it is stamped afterwards by `annotate`. But
    // naming the column in the INSERT and binding None sends SQL NULL, which
    // OVERRIDES the default and violates the constraint, so EVERY event insert on
    // that path failed and no day could be segmented. Default None to an empty
    // array, which is what the column would have used had we omitted it.
    let source_ontologies_str = Some(
        req.source_ontologies
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string()))
            .unwrap_or_else(|| "[]".to_string()),
    );

    let event_id = ids::generate_id(ids::WIKI_EVENT_PREFIX, &[&req.day_id, &start_time_str, &end_time_str]);

    // `kind` is the source of truth; the is_unknown/is_transit booleans are generated
    // from it, so we set kind here rather than the (unwritable) generated columns.
    // create_temporal_event never mints sleep — that is `dayline::sleep`'s job.
    let kind = if req.is_unknown == Some(true) {
        "unknown"
    } else if req.is_transit == Some(true) {
        "transit"
    } else {
        "stay"
    };

    // Runtime query (not the macro) so we can include `event_summary` without
    // regenerating the sqlx offline cache.
    let row = sqlx::query(
        r#"
        INSERT INTO wiki_events (
            id, day_id, started_at, ended_at,
            auto_label, auto_location, user_label, user_location, user_notes,
            source_ontologies, kind, is_user_added, event_summary,
            topics
        ) VALUES ($1, $2, $3::timestamptz, $4::timestamptz, $5, $6, $7, $8, $9, $10::jsonb, $11, $12, $13, $14::jsonb)
        ON CONFLICT (id) DO NOTHING
        RETURNING
            id, is_user_edited, created_at, updated_at
        "#,
    )
    .bind(&event_id)
    .bind(&day_id_str)
    .bind(&start_time_str)
    .bind(&end_time_str)
    .bind(&req.auto_label)
    .bind(&req.auto_location)
    .bind(&req.user_label)
    .bind(&req.user_location)
    .bind(&req.user_notes)
    .bind(&source_ontologies_str)
    .bind(kind)
    .bind(req.is_user_added)
    .bind(&req.event_summary)
    .bind(req.topics.clone().unwrap_or_else(|| serde_json::json!([])))
    .fetch_optional(exec)
    .await
    .map_err(|e| Error::Database(format!("Failed to create temporal event: {}", e)))?;
    let Some(row) = row else {
        return Err(Error::InvalidInput(format!(
            "an event already spans {start_time_str}–{end_time_str} on this day ({event_id})"
        )));
    };

    let id: String = row
        .try_get("id")
        .map_err(|e| Error::Database(format!("Missing event ID: {}", e)))?;
    let is_user_edited: Option<bool> = row.try_get("is_user_edited").ok().flatten();
    let created_at: DateTime<Utc> = row
        .try_get("created_at")
        .map_err(|e| Error::Database(format!("Missing created_at: {}", e)))?;
    let updated_at: DateTime<Utc> = row
        .try_get("updated_at")
        .map_err(|e| Error::Database(format!("Missing updated_at: {}", e)))?;

    Ok(TemporalEvent {
        id,
        day_id: req.day_id,
        start_time: req.start_time,
        end_time: req.end_time,
        auto_label: req.auto_label,
        auto_location: req.auto_location,
        user_label: req.user_label,
        user_location: req.user_location,
        user_notes: req.user_notes,
        source_ontologies: req.source_ontologies,
        is_unknown: req.is_unknown,
        is_transit: req.is_transit,
        is_user_added: req.is_user_added,
        is_user_edited,
        novelty_z: None,
        avg_hr: None,
        autonomic_z: None,
        hr_z: None,
        topics: None,
        event_summary: req.event_summary,
        agent_action: None,
        is_sleep: Some(false),
        user_hidden: Some(false),
        entities: None,
        topic_novelty: None,
        entity_novelty: None,
        // A write path returns what it just stored; names are a read-time
        // enrichment and the caller re-reads the day to get them.
        entity_names: None,
        entity_timestamps: None,
        created_at,
        updated_at,
    })
}

/// Update a temporal event (user edits)
pub async fn update_temporal_event(
    pool: &PgPool,
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
            updated_at = now()
        WHERE id = $1
        RETURNING
            id, day_id, started_at, ended_at,
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

    Ok(TemporalEvent {
        id: row.id,
        day_id: row.day_id,
        start_time: row.started_at,
        end_time: row.ended_at,
        auto_label: row.auto_label,
        auto_location: row.auto_location,
        user_label: row.user_label,
        user_location: row.user_location,
        user_notes: row.user_notes,
        source_ontologies: serde_json::from_value(row.source_ontologies).ok(),
        is_unknown: Some(row.is_unknown),
        is_transit: Some(row.is_transit),
        is_user_added: Some(row.is_user_added),
        is_user_edited: Some(row.is_user_edited),
        novelty_z: None,
        avg_hr: None,
        autonomic_z: None,
        hr_z: None,
        topics: None,
        event_summary: None,
        agent_action: None,
        is_sleep: Some(false),
        user_hidden: Some(false),
        entities: None,
        topic_novelty: None,
        entity_novelty: None,
        // A write path returns what it just stored; names are a read-time
        // enrichment and the caller re-reads the day to get them.
        entity_names: None,
        entity_timestamps: None,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Delete a temporal event
pub async fn delete_temporal_event(pool: &PgPool, id: String) -> Result<()> {
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

/// Delete all auto-generated events for a day (for regeneration).
///
/// "Auto-generated" means UNTOUCHED BY THE USER, and that is three flags, not
/// one. `is_user_added` alone was the guard, and it is only set by
/// `create_temporal_event` — relabelling an event the machine cut sets
/// `is_user_edited` (see `update_temporal_event`) and leaves `is_user_added`
/// false, so every re-cut silently deleted the user's own label. That fires
/// whenever late audio or a backfilled message changes the day's
/// `sources_fingerprint`, which is exactly when a day gets re-cut in practice.
/// `user_hidden` is the same class: a hide the re-cut eats comes back visible.
///
/// Preserved events can now overlap the fresh cut — but that was already true
/// of `is_user_added` events, so this widens an accepted condition rather than
/// introducing one. A user's judgement outranks a gapless timeline.
///
/// Any executor: the segmenter runs this inside the same transaction as the
/// inserts that replace the deleted rows, so a failed re-cut leaves the old
/// events standing instead of an empty day.
pub async fn delete_auto_events_for_day<'e>(
    exec: impl sqlx::PgExecutor<'e>,
    day_id: String,
) -> Result<u64> {
    let day_id_str = day_id;

    let result = sqlx::query!(
        r#"
        DELETE FROM wiki_events
        WHERE day_id = $1
          AND is_user_added = false
          AND is_user_edited = false
          AND user_hidden = false
        "#,
        day_id_str
    )
    .execute(exec)
    .await
    .map_err(|e| Error::Database(format!("Failed to delete auto events: {}", e)))?;

    Ok(result.rows_affected())
}
