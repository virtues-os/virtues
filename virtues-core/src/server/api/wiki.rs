//! The wiki: entities, memories, notes, the lifeline, articles, days and
//! events, and the home-page loops.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json,
    Router,
};
use serde::Deserialize;

use super::{api_response, error_response, success_message};
use crate::error::Error;
use crate::server::AppState;

/// This area's authenticated routes. Merged into the protected router, whose
/// `route_layer` requires a resolved `AuthUser`.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Timeline day (location chunks for movement map)
        .route("/api/timeline/day/:date", get(timeline_get_day_handler))
        // The Timeline's derived stays, drives, nights and moments over a window
        .route("/api/timeline/derived", get(timeline_derived_handler))
        // One local day's bounds, in the zone the day woke up in
        .route("/api/timeline/day-window/:date", get(timeline_day_window_handler))
        // The transcription windows over a window: the rail's conversations and the mic's coverage
        .route("/api/timeline/voice", get(timeline_voice_handler))
        // Today streams — location/calendar/audio spans, pre-synthesis (homepage)
        .route("/api/today/:date/streams", get(today_streams_handler))
        // Home-page loops — weather · upcoming calendar · unnamed-place backlog
        .route("/api/weather/current", get(weather_now_handler))
        .route("/api/calendar/upcoming", get(calendar_upcoming_handler))
        .route("/api/places/unnamed", get(unnamed_places_handler))
        // Entities API - Places
        .route(
            "/api/entities/places",
            get(list_places_handler).post(create_place_handler),
        )
        .route(
            "/api/entities/places/:id",
            get(get_place_handler)
                .put(update_place_handler)
                .delete(delete_place_handler),
        )
        .route(
            "/api/assistant/memories",
            get(list_assistant_memories_handler),
        )
        .route(
            "/api/assistant/memories/:id",
            put(edit_assistant_memory_handler)
                .delete(retire_assistant_memory_handler),
        )
        .route(
            "/api/wiki/notes/:subject_type/:subject_id",
            get(list_notes_handler).post(create_note_handler),
        )
        .route(
            "/api/wiki/notes/:id/resolve",
            put(resolve_note_handler),
        )
        .route(
            "/api/wiki/notes-open-count",
            get(open_notes_count_handler),
        )
        .route(
            "/api/wiki/lifeline",
            get(lifeline_handler),
        )
        .route(
            "/api/wiki/lifeline/ground",
            get(lifeline_ground_handler),
        )
        .route(
            "/api/wiki/lifeline/clock",
            get(lifeline_clock_handler),
        )
        .route(
            "/api/wiki/lifeline/feed",
            get(lifeline_feed_handler),
        )
        .route(
            "/api/wiki/lifeline/processed",
            get(lifeline_processed_handler),
        )
        .route(
            "/api/wiki/history",
            get(history_feed_handler),
        )
        .route(
            "/api/wiki/articles/:subject_type/:subject_id/history",
            get(article_history_handler),
        )
        .route(
            "/api/wiki/subjects/:subject_type/:subject_id/backlinks",
            get(subject_backlinks_handler),
        )
        .route(
            "/api/wiki/articles/:subject_type/:subject_id",
            get(get_article_handler).post(write_article_handler),
        )
        .route(
            "/api/wiki/articles/:subject_type/:subject_id/maintenance",
            put(set_article_maintenance_handler),
        )
        .route(
            "/api/wiki/articles/:subject_type/:subject_id/revert",
            post(revert_article_handler),
        )
        .route(
            "/api/entities/people",
            post(create_person_handler),
        )
        .route(
            "/api/entities/people/:id",
            delete(delete_person_handler),
        )
        .route(
            "/api/entities/orgs/:id",
            delete(delete_org_handler),
        )
        .route(
            "/api/entities/people/:id/reclassify-as-org",
            post(reclassify_person_handler),
        )
        // ── Wiki API ────────────────────────────────────────────────────
        //
        // TWO ADDRESSING SHAPES, and both are right. Don't unify them.
        //
        //   generic   /api/wiki/articles/:subject_type/:subject_id
        //             /api/wiki/notes/:subject_type/:subject_id
        //             /api/wiki/subjects/:subject_type/:subject_id/backlinks
        //   per-kind  /api/wiki/person/:id, /place/:id, /organization/:id
        //
        // The test is whether the PAYLOAD varies by kind. An article is the
        // same row whatever it is about, so its route takes the subject as a
        // parameter and one handler serves every rung. An entity's own fields
        // are not: a person has a relationship, a place has coordinates, an
        // organization has a type. A generic entity route would return a union
        // the client has to discriminate anyway — the per-kind route has
        // already done that, in the one place it costs nothing.
        //
        // What was genuinely wrong here was duplicate SPELLINGS of one route,
        // not the shape: organizations had four routes for two handlers.
        //
        // Wiki - Person
        // Mention review queue (entity resolution HITL)
        .route("/api/wiki/people", get(wiki_list_people_handler))
        .route(
            "/api/wiki/person/:id",
            get(wiki_get_person_handler).put(wiki_update_person_handler),
        )
        // Wiki - Place
        .route("/api/wiki/places", get(wiki_list_places_handler))
        .route(
            "/api/wiki/place/:id",
            get(wiki_get_place_handler).put(wiki_update_place_handler),
        )
        // Wiki - Organization. The table is `wiki_orgs` and the id prefix is
        // `org_`, but the ROUTE spells it out, matching `subject_type =
        // 'organization'` everywhere else. `/orgs` and `/org/:id` also existed,
        // pointed at these same handlers, and no client has ever called either.
        .route(
            "/api/wiki/organizations",
            get(wiki_list_organizations_handler),
        )
        .route(
            "/api/wiki/organization/:id",
            get(wiki_get_organization_handler).put(wiki_update_organization_handler),
        )
        // Wiki - Narrative Identity. Read-only: the document is edited on its
        // page, and the retired abridged copy took its PUT with it.
        .route(
            "/api/wiki/narrative-identity",
            get(wiki_get_narrative_identity_handler),
        )
        // Wiki - Chapter (the life's partition, written by the interview or
        // drawn on the Getting started timeline; PUT replaces the whole list)
        .route(
            "/api/wiki/chapters",
            get(crate::api::narrative_draft::chapters_handler)
                .put(wiki_replace_chapters_handler),
        )
        // Wiki - Day
        .route("/api/wiki/days", get(wiki_list_days_handler))
        .route("/api/wiki/activity", get(wiki_day_activity_handler))
        .route("/api/wiki/on-this-day", get(wiki_on_this_day_handler))
        .route(
            "/api/wiki/entity/:id/records",
            get(wiki_entity_records_handler),
        )
        .route(
            "/api/wiki/entity/:id/records/facets",
            get(wiki_entity_record_facets_handler),
        )
        .route("/api/wiki/day/:date", get(wiki_get_day_handler))
        .route(
            "/api/wiki/stories",
            get(wiki_list_stories_handler).post(wiki_create_story_handler),
        )
        .route(
            "/api/wiki/story/:id",
            get(wiki_get_story_handler)
                .put(wiki_update_story_handler)
                .delete(wiki_delete_story_handler),
        )
        .route(
            "/api/wiki/story/:id/article",
            post(wiki_start_story_article_handler),
        )
        .route(
            "/api/wiki/chapter/:id",
            put(wiki_update_chapter_handler)
                .delete(wiki_delete_chapter_handler),
        )
        .route("/api/wiki/me", get(wiki_me_handler))
        .route("/api/wiki/years", get(wiki_list_years_handler))
        .route(
            "/api/wiki/year/:year",
            get(wiki_get_year_handler).put(wiki_update_year_handler),
        )
        .route(
            "/api/wiki/year/:year/article",
            post(wiki_write_year_article_handler),
        )
        // Wiki - Temporal Events
        .route(
            "/api/wiki/day/:date/events",
            get(wiki_get_day_events_handler),
        )
        .route("/api/wiki/events", post(wiki_create_event_handler))
        .route(
            "/api/wiki/events/:id",
            put(wiki_update_event_handler).delete(wiki_delete_event_handler),
        )
        .route(
            "/api/wiki/day/:day_id/auto-events",
            delete(wiki_delete_auto_events_handler),
        )
        // Wiki - Day Sources (ontology data)
        .route(
            "/api/wiki/day/:date/sources",
            get(wiki_get_day_sources_handler),
        )
        // Wiki - Day Chats (in-app + external AI conversations)
        .route(
            "/api/wiki/day/:date/chats",
            get(wiki_get_day_chats_handler),
        )
        // Wiki - Day facts (the strip under the day's Abstract)
        .route(
            "/api/wiki/day/:date/facts",
            get(wiki_get_day_facts_handler),
        )
        // Wiki - Days whose pages read like this one's
        .route(
            "/api/wiki/day/:date/similar",
            get(wiki_get_similar_days_handler),
        )
        // Wiki - Day Streams (dynamic ontology queries)
        .route(
            "/api/wiki/day/:date/streams",
            get(wiki_get_day_streams_handler),
        )
        // Wiki - Day heart rate (the Autonomic chart)
        .route(
            "/api/wiki/day/:date/heart-rate",
            get(day_heart_rate_handler),
        )
}


// =============================================================================
// Entities API - Places
// =============================================================================

/// List all known places
pub async fn list_places_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::entities::list_places(state.db.pool()).await)
}

/// Create a place by hand — the phone's "Mute here" and its Google-places door.
pub async fn create_place_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::entities::CreatePlaceRequest>,
) -> Response {
    api_response(crate::api::entities::create_place(state.db.pool(), request).await)
}

/// Get a specific place by ID
pub async fn get_place_handler(
    State(state): State<AppState>,
    Path(place_id): Path<String>,
) -> Response {
    api_response(crate::api::entities::get_place(state.db.pool(), place_id).await)
}

/// Update an existing place
pub async fn update_place_handler(
    State(state): State<AppState>,
    Path(place_id): Path<String>,
    Json(request): Json<crate::api::entities::UpdatePlaceRequest>,
) -> Response {
    api_response(crate::api::entities::update_place(state.db.pool(), place_id, request).await)
}

/// Delete a place
pub async fn delete_place_handler(
    State(state): State<AppState>,
    Path(place_id): Path<String>,
) -> Response {
    match crate::api::entities::delete_place(state.db.pool(), place_id).await {
        Ok(_) => success_message("Place deleted successfully"),
        Err(e) => error_response(e),
    }
}

#[derive(serde::Deserialize)]
pub struct CreateEntityBody {
    pub name: String,
}

/// Create a person by hand.
pub async fn create_person_handler(
    State(state): State<AppState>,
    Json(b): Json<CreateEntityBody>,
) -> Response {
    match crate::api::entities::create_person(state.db.pool(), &b.name).await {
        Ok(id) => api_response(Ok::<_, crate::error::Error>(
            serde_json::json!({ "id": id, "route": format!("/person/{id}") }),
        )),
        Err(e) => error_response(e),
    }
}

/// Delete a person, and everything that pointed at them.
pub async fn delete_person_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::entities::delete_person(state.db.pool(), id).await {
        Ok(()) => success_message("Person deleted"),
        Err(e) => error_response(e),
    }
}

/// Delete an organization, and everything that pointed at it.
pub async fn delete_org_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::entities::delete_organization(state.db.pool(), id).await {
        Ok(()) => success_message("Organization deleted"),
        Err(e) => error_response(e),
    }
}

#[derive(serde::Deserialize)]
pub struct LifelineQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub buckets: Option<i32>,
    /// Comma-separated lane ids; absent = every lane.
    pub lanes: Option<String>,
    /// Comma-separated lane ids to split into their member tables.
    pub expand: Option<String>,
    /// Comma-separated `lane:measure_id` pairs — what each lane should plot
    /// instead of a row count. Unknown pairs fall back to the default.
    pub measures: Option<String>,
    /// Feed paging.
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// IANA zone the day-clock's hours are read in. One zone for the whole
    /// raster, so travel shows as a dislocation rather than being normalised
    /// away. Unknown names fall back to UTC.
    pub tz: Option<String>,
}

/// Per-lane density over a window — the lifeline's only data source.
pub async fn lifeline_handler(
    State(state): State<AppState>,
    Query(q): Query<LifelineQuery>,
) -> Response {
    // No window given = the whole life. Computed from the data, because a
    // lifeline that defaults to the last 365 days is not a lifeline.
    let (span_from, span_to) = match crate::api::lifeline::corpus_span(state.db.pool()).await {
        Ok(s) => s,
        Err(e) => return error_response(e),
    };
    let to = q
        .to
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or(span_to);
    let from = q
        .from
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or(span_from);
    let csv = |v: Option<String>| {
        v.map(|s| {
            s.split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
        })
    };
    let lanes = csv(q.lanes);
    let expand = csv(q.expand);
    let measures = csv(q.measures);

    api_response(
        crate::api::lifeline::get_lifeline(
            state.db.pool(),
            from,
            to,
            q.buckets.unwrap_or(365),
            lanes,
            expand,
            measures,
        )
        .await,
    )
}

/// Where a window was spent — the location lane's second view.
pub async fn lifeline_ground_handler(
    State(state): State<AppState>,
    Query(q): Query<LifelineQuery>,
) -> Response {
    let (span_from, span_to) = match crate::api::lifeline::corpus_span(state.db.pool()).await {
        Ok(s) => s,
        Err(e) => return error_response(e),
    };
    let parse = |s: Option<String>, fallback: chrono::DateTime<chrono::Utc>| {
        s.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or(fallback)
    };
    api_response(
        crate::api::lifeline::get_ground(
            state.db.pool(),
            parse(q.from, span_from),
            parse(q.to, span_to),
        )
        .await,
    )
}

/// Time-of-day against date — the lifeline's primary band.
pub async fn lifeline_clock_handler(
    State(state): State<AppState>,
    Query(q): Query<LifelineQuery>,
) -> Response {
    let (span_from, span_to) = match crate::api::lifeline::corpus_span(state.db.pool()).await {
        Ok(s) => s,
        Err(e) => return error_response(e),
    };
    let parse = |s: Option<String>, fallback: chrono::DateTime<chrono::Utc>| {
        s.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or(fallback)
    };
    api_response(
        crate::api::lifeline::get_clock(
            state.db.pool(),
            parse(q.from, span_from),
            parse(q.to, span_to),
            q.buckets.unwrap_or(720),
            q.tz.as_deref().unwrap_or("UTC"),
        )
        .await,
    )
}

/// The records inside a window — what a selection actually contains.
pub async fn lifeline_feed_handler(
    State(state): State<AppState>,
    Query(q): Query<LifelineQuery>,
) -> Response {
    let (span_from, span_to) = match crate::api::lifeline::corpus_span(state.db.pool()).await {
        Ok(s) => s,
        Err(e) => return error_response(e),
    };
    let parse = |s: Option<String>, fallback: chrono::DateTime<chrono::Utc>| {
        s.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or(fallback)
    };
    let lanes = q.lanes.map(|s| {
        s.split(',')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>()
    });
    api_response(
        crate::api::lifeline::get_feed(
            state.db.pool(),
            parse(q.from, span_from),
            parse(q.to, span_to),
            lanes,
            q.limit.unwrap_or(50),
            q.offset.unwrap_or(0),
        )
        .await,
    )
}

/// What Virtues has interpreted inside a window — days and events.
pub async fn lifeline_processed_handler(
    State(state): State<AppState>,
    Query(q): Query<LifelineQuery>,
) -> Response {
    let (span_from, span_to) = match crate::api::lifeline::corpus_span(state.db.pool()).await {
        Ok(s) => s,
        Err(e) => return error_response(e),
    };
    let parse = |s: Option<String>, fallback: chrono::DateTime<chrono::Utc>| {
        s.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or(fallback)
    };
    api_response(
        crate::api::lifeline::get_processed(
            state.db.pool(),
            parse(q.from, span_from),
            parse(q.to, span_to),
            q.limit.unwrap_or(80),
        )
        .await,
    )
}

/// Notes on a subject.
/// GET /api/assistant/memories — the live memories, lane-grouped.
pub async fn list_assistant_memories_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::assistant_memories::list_memories(state.db.pool()).await)
}

/// PUT /api/assistant/memories/:id — the person rewrites one in their words.
pub async fn edit_assistant_memory_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<crate::api::assistant_memories::EditMemoryRequest>,
) -> Response {
    api_response(crate::api::assistant_memories::edit_memory(state.db.pool(), id, &req.body).await)
}

/// DELETE /api/assistant/memories/:id — soft-retire with provenance.
pub async fn retire_assistant_memory_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Response {
    api_response(
        crate::api::assistant_memories::retire_memory(state.db.pool(), id)
            .await
            .map(|_| serde_json::json!({ "retired": true })),
    )
}

pub async fn list_notes_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
    Query(q): Query<NotesQuery>,
) -> Response {
    api_response(
        crate::api::wiki_notes::list_notes(
            state.db.pool(),
            &subject_type,
            &subject_id,
            q.include_resolved.unwrap_or(false),
        )
        .await,
    )
}

#[derive(serde::Deserialize)]
pub struct NotesQuery {
    pub include_resolved: Option<bool>,
}

/// Open notes across the whole record — the Overview's what-changed count.
pub async fn open_notes_count_handler(State(state): State<AppState>) -> Response {
    api_response(
        crate::api::wiki_notes::count_open_total(state.db.pool())
            .await
            .map(|n| serde_json::json!({ "open": n })),
    )
}

#[derive(serde::Deserialize)]
pub struct CreateNoteBody {
    pub body: String,
    pub kind: Option<String>,
}

/// Leave a note on a subject.
pub async fn create_note_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
    Json(b): Json<CreateNoteBody>,
) -> Response {
    api_response(
        crate::api::wiki_notes::create_note(
            state.db.pool(),
            &subject_type,
            &subject_id,
            b.kind.as_deref().unwrap_or("memo"),
            &b.body,
        )
        .await,
    )
}

#[derive(serde::Deserialize)]
pub struct ResolveNoteBody {
    pub resolution: String,
}

/// Accept or dismiss a note.
pub async fn resolve_note_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<ResolveNoteBody>,
) -> Response {
    match crate::api::wiki_notes::resolve_note(state.db.pool(), id, &b.resolution).await {
        Ok(()) => success_message("Note closed"),
        Err(e) => error_response(e),
    }
}

/// One article's edit history, with diffs.
pub async fn article_history_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
) -> Response {
    api_response(
        crate::api::wiki_articles::get_article_history(
            state.db.pool(),
            &subject_type,
            &subject_id,
        )
        .await,
    )
}

/// The wiki's History room: every recent edit to any article.
pub async fn history_feed_handler(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Response {
    api_response(
        crate::api::wiki_articles::get_history_feed(state.db.pool(), q.limit.unwrap_or(50)).await,
    )
}

/// Everything that mentions this subject.
pub async fn subject_backlinks_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
) -> Response {
    api_response(
        crate::api::wiki_articles::get_subject_backlinks(
            state.db.pool(),
            &subject_type,
            &subject_id,
        )
        .await,
    )
}

/// Write a subject's article, now, because someone asked for it.
///
/// A plain handler rather than a trip through the applet runner. The applet
/// path looked available — `entity_article` declares a `manual` trigger — but
/// it ships `default_enabled = false` and `prepare_run` refuses disabled
/// applets, so the button would 404 on a fresh box; its singleton concurrency
/// gate turns a second click into a `skipped` run, which is wrong for
/// per-subject work; and its entry point takes no target, so there is no way to
/// say *this one*. The applet stays the cron host; this is the door.
///
/// Synchronous on purpose: it is one model call the user is waiting for, and
/// returning 202 would mean polling `app_applet_runs` to find out whether your
/// own click worked.
/// GET one subject's article row — the join, not the prose. The frontend
/// uses `page_id` to open the article in the page editor.
pub async fn get_article_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
) -> Response {
    match crate::api::wiki_articles::get_article(state.db.pool(), &subject_type, &subject_id).await
    {
        Ok(Some(a)) => Json(a).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => error_response(e),
    }
}

pub async fn write_article_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
) -> Response {
    api_response(
        crate::api::entity_article_gen::write_entity_article_now(
            state.db.pool(),
            &subject_type,
            &subject_id,
        )
        .await,
    )
}

/// The owner's own page: their name, their document, and the apparatus.
///
/// Also the one place that ensures they have a row among the people of their
/// own wiki — nothing else ever created one.
pub async fn wiki_me_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::me::get_me(state.db.pool()).await)
}

/// The stories: subjects the person named because they mattered.
pub async fn wiki_list_stories_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::stories::list_stories(state.db.pool()).await)
}

pub async fn wiki_get_story_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::stories::get_story(state.db.pool(), &id).await)
}

/// Start a story. Only the person may: the editor's constitution forbids it
/// from creating a subject, because naming one is a claim about what mattered.
pub async fn wiki_create_story_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("");
    api_response(crate::api::stories::create_story(state.db.pool(), title).await)
}

pub async fn wiki_update_story_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(fields): Json<crate::api::stories::StoryFields>,
) -> Response {
    api_response(crate::api::stories::update_story(state.db.pool(), &id, &fields).await)
}

pub async fn wiki_delete_story_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::stories::delete_story(state.db.pool(), &id).await {
        Ok(()) => success_message("Removed"),
        Err(e) => error_response(e),
    }
}

/// Give a story a page. Seeded with THEIR words, not a machine draft — a story
/// has nothing beneath it to draft from, and the editor fills it by searching.
pub async fn wiki_start_story_article_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::stories::start_article(state.db.pool(), &id).await)
}

/// Edit a chapter. There has never been a way to change one.
pub async fn wiki_update_chapter_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(edit): Json<crate::api::narrative_draft::ChapterEdit>,
) -> Response {
    api_response(crate::api::narrative_draft::update_chapter(state.db.pool(), &id, &edit).await)
}

/// Replace every chapter with the timeline the person drew in Getting started.
/// Refused once the chapters have pages of their own; see `replace_chapters`.
pub async fn wiki_replace_chapters_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::api::narrative_draft::ReplaceChapters>,
) -> Response {
    api_response(
        crate::api::narrative_draft::replace_chapters(state.db.pool(), &req.chapters)
            .await
            .map(|chapters| serde_json::json!({ "chapters": chapters })),
    )
}

/// Unname a chapter, leaving the years it covered as an unnamed stretch.
pub async fn wiki_delete_chapter_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::narrative_draft::delete_chapter(state.db.pool(), &id).await)
}

/// Every year of the life, newest first. Derived — opening the index writes
/// nothing.
pub async fn wiki_list_years_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::years::list_years(state.db.pool()).await)
}

/// One year's page. Creates its row on the way, which is the lazy half of the
/// partition: every year is there to read, and none is written until asked for.
pub async fn wiki_get_year_handler(
    State(state): State<AppState>,
    Path(year): Path<i32>,
) -> Response {
    api_response(crate::api::years::get_year(state.db.pool(), year).await)
}

/// Set what only the person can say about a year.
pub async fn wiki_update_year_handler(
    State(state): State<AppState>,
    Path(year): Path<i32>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let title = body.get("title").and_then(|v| v.as_str());
    let summary = body.get("summary").and_then(|v| v.as_str());
    match crate::api::years::update_year(state.db.pool(), year, title, summary).await {
        Ok(()) => success_message("Saved"),
        Err(e) => error_response(e),
    }
}

/// Write a year's first article.
pub async fn wiki_write_year_article_handler(
    State(state): State<AppState>,
    Path(year): Path<i32>,
) -> Response {
    api_response(crate::api::years::write_year_article(state.db.pool(), year).await)
}

/// Put a named version of an article back.
///
/// Rule 4 of the wiki's paradigm — every edit is a revision you can read AND
/// revert — has been half true since the history feed shipped: the diff was
/// readable and there was no way to undo it.
pub async fn revert_article_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let Some(version) = body.get("version_number").and_then(|v| v.as_i64()) else {
        return error_response(Error::InvalidInput(
            "version_number is required".to_string(),
        ));
    };
    match crate::api::wiki_editor::revert_article(
        state.db.pool(),
        &state.yjs_state,
        &subject_type,
        &subject_id,
        version,
    )
    .await
    {
        Ok(change) => success_message(&format!("Reverted to v{version} — {change}")),
        Err(e) => error_response(e),
    }
}

/// Set how an article is maintained: always, auto, or never.
pub async fn set_article_maintenance_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let mode = body.get("maintenance").and_then(|v| v.as_str()).unwrap_or("");
    match crate::api::wiki_articles::set_maintenance(
        state.db.pool(),
        &subject_type,
        &subject_id,
        mode,
    )
    .await
    {
        Ok(()) => success_message(match mode {
            "always" => "The record will revisit this whenever anything changes",
            "never" => "The record will leave this article alone",
            _ => "The record will keep this up to date",
        }),
        Err(e) => error_response(e),
    }
}


/// Reclassify a person as an organization.
///
/// Returns the new org id so the caller can navigate to it — the person route
/// it came from stops resolving the moment this succeeds.
pub async fn reclassify_person_handler(
    State(state): State<AppState>,
    Path(person_id): Path<String>,
) -> Response {
    match crate::api::entities::reclassify_person_as_organization(state.db.pool(), person_id).await {
        Ok(org_id) => api_response(Ok::<_, crate::error::Error>(
            serde_json::json!({ "id": org_id, "route": format!("/org/{org_id}") }),
        )),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Wiki API Handlers
// ============================================================================

// --- Person ---

/// Get a person by ID
pub async fn wiki_get_person_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::wiki::get_person(state.db.pool(), id).await)
}

/// List all people
pub async fn wiki_list_people_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::wiki::list_people(state.db.pool()).await)
}

/// Update a person by ID
pub async fn wiki_update_person_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::wiki::UpdateWikiPersonRequest>,
) -> Response {
    api_response(crate::api::wiki::update_person(state.db.pool(), id, request).await)
}

// --- Place ---

/// Get a place by ID
pub async fn wiki_get_place_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::wiki::get_wiki_place(state.db.pool(), id).await)
}

/// List all places (wiki view)
pub async fn wiki_list_places_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::wiki::list_wiki_places(state.db.pool()).await)
}

/// Update a place by ID (wiki fields)
pub async fn wiki_update_place_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::wiki::UpdateWikiPlaceRequest>,
) -> Response {
    api_response(crate::api::wiki::update_wiki_place(state.db.pool(), id, request).await)
}

// --- Organization ---

/// Get an organization by ID
pub async fn wiki_get_organization_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::wiki::get_organization(state.db.pool(), id).await)
}

/// List all organizations
pub async fn wiki_list_organizations_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::wiki::list_organizations(state.db.pool()).await)
}

/// Update an organization by ID
pub async fn wiki_update_organization_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::wiki::UpdateWikiOrganizationRequest>,
) -> Response {
    api_response(crate::api::wiki::update_organization(state.db.pool(), id, request).await)
}

// --- Narrative Identity ---

/// Get narrative identity
pub async fn wiki_get_narrative_identity_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::wiki::get_narrative_identity(state.db.pool()).await)
}

// --- Day ---

#[derive(Deserialize)]
pub struct WikiDayQuery {
    pub start_date: Option<chrono::NaiveDate>,
    pub end_date: Option<chrono::NaiveDate>,
}

/// Get a day by date
pub async fn wiki_get_day_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::wiki_days::get_or_create_day(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}



/// List days in a date range
pub async fn wiki_list_days_handler(
    State(state): State<AppState>,
    Query(query): Query<WikiDayQuery>,
) -> Response {
    let today = chrono::Utc::now().date_naive();
    let start_date = query
        .start_date
        .unwrap_or(today - chrono::Duration::days(30));
    let end_date = query.end_date.unwrap_or(today);
    api_response(crate::api::wiki_days::list_days(state.db.pool(), start_date, end_date).await)
}

/// Per-day activity counts for the wiki calendar heatmap
pub async fn wiki_day_activity_handler(
    State(state): State<AppState>,
    Query(query): Query<WikiDayQuery>,
) -> Response {
    let today = chrono::Utc::now().date_naive();
    let start_date = query
        .start_date
        .unwrap_or(today - chrono::Duration::days(365));
    let end_date = query.end_date.unwrap_or(today);
    api_response(crate::api::wiki_days::day_activity(state.db.pool(), start_date, end_date).await)
}

#[derive(Deserialize)]
pub struct OnThisDayQuery {
    pub date: Option<chrono::NaiveDate>,
}

/// Past years' entries for the same calendar date
pub async fn wiki_on_this_day_handler(
    State(state): State<AppState>,
    Query(query): Query<OnThisDayQuery>,
) -> Response {
    let date = query.date.unwrap_or_else(|| chrono::Utc::now().date_naive());
    api_response(crate::api::wiki_days::on_this_day(state.db.pool(), date).await)
}

#[derive(Deserialize)]
pub struct EntityRecordsQuery {
    pub offset: Option<i64>,
    pub limit: Option<i64>,
    pub search: Option<String>,
    /// Comma-separated raw source_types to include (empty/absent = all).
    pub types: Option<String>,
    /// "asc" for oldest-first; anything else is newest-first.
    pub dir: Option<String>,
}

/// One page of the records linked to an entity (the entity page's evidence feed)
pub async fn wiki_entity_records_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<EntityRecordsQuery>,
) -> Response {
    let types: Vec<String> = q
        .types
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    api_response(
        crate::api::wiki::get_entity_records_page(
            state.db.pool(),
            &id,
            q.offset.unwrap_or(0),
            q.limit.unwrap_or(10),
            q.search.as_deref().unwrap_or(""),
            &types,
            q.dir.as_deref() != Some("asc"),
        )
        .await,
    )
}

/// Facet counts over all of an entity's records, for the chip rail
pub async fn wiki_entity_record_facets_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::wiki::get_entity_record_facets(state.db.pool(), &id).await)
}

// =============================================================================
// Wiki Temporal Events API
// =============================================================================

/// Get events for a day by date
pub async fn wiki_get_day_events_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::wiki_events::get_events_by_date(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// Create a temporal event
pub async fn wiki_create_event_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::wiki_events::CreateTemporalEventRequest>,
) -> Response {
    match crate::api::wiki_events::create_temporal_event(state.db.pool(), request).await {
        Ok(event) => (StatusCode::CREATED, Json(event)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Update a temporal event
pub async fn wiki_update_event_handler(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
    Json(request): Json<crate::api::wiki_events::UpdateTemporalEventRequest>,
) -> Response {
    api_response(crate::api::wiki_events::update_temporal_event(state.db.pool(), event_id, request).await)
}

/// Delete a temporal event
pub async fn wiki_delete_event_handler(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
) -> Response {
    match crate::api::wiki_events::delete_temporal_event(state.db.pool(), event_id).await {
        Ok(_) => success_message("Event deleted"),
        Err(e) => error_response(e),
    }
}

/// Delete all auto-generated events for a day (regeneration support)
pub async fn wiki_delete_auto_events_handler(
    State(state): State<AppState>,
    Path(day_id): Path<String>,
) -> Response {
    match crate::api::wiki_events::delete_auto_events_for_day(state.db.pool(), day_id).await {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({ "deleted": count })),
        )
            .into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct TimelineWindowQuery {
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: chrono::DateTime<chrono::Utc>,
}

/// The Timeline's derived stretches and moments overlapping `start`..`end`
/// (RFC 3339), with their places. The view asks for one local day at a time.
pub async fn timeline_derived_handler(
    State(state): State<AppState>,
    Query(q): Query<TimelineWindowQuery>,
) -> Response {
    if q.end <= q.start {
        return error_response(Error::InvalidInput("end must be after start".into()));
    }
    api_response(crate::timeline::window(state.db.pool(), q.start, q.end).await)
}

/// The transcription windows overlapping `start`..`end` (RFC 3339): the
/// rail's conversations and transcripts, and whether the mic was on.
pub async fn timeline_voice_handler(State(state): State<AppState>, Query(q): Query<TimelineWindowQuery>) -> Response {
    if q.end <= q.start {
        return error_response(Error::InvalidInput("end must be after start".into()));
    }
    api_response(crate::timeline::voice(state.db.pool(), q.start, q.end).await)
}

/// A local day's bounds, in the zone the day woke up in (`YYYY-MM-DD`).
pub async fn timeline_day_window_handler(State(state): State<AppState>, Path(date): Path<String>) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(d) => api_response(crate::timeline::day_window(state.db.pool(), d).await),
        Err(_) => error_response(Error::InvalidInput(format!("Invalid date format: {}", date))),
    }
}

/// Get timeline location chunks for a day (movement map)
pub async fn timeline_get_day_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::wiki_days::get_timeline_day(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// Optional `?tz=` query — the viewing device's IANA zone, used to anchor an
/// in-progress "today" to where the owner currently is. See agents/record/timezone-model.md.
#[derive(Debug, Deserialize, Default)]
pub struct DaySourcesQuery {
    pub tz: Option<String>,
}

/// Get the three raw record streams (location, calendar, audio) for a day, as
/// spans — the homepage's "day before synthesis" view.
/// GET /api/wiki/day/:date/heart-rate — the day's HR samples, for Autonomic.
pub async fn day_heart_rate_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
    Query(query): Query<DaySourcesQuery>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => api_response(
            crate::api::wiki_streams::get_day_heart_rate(state.db.pool(), parsed_date, query.tz.as_deref())
                .await,
        ),
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

pub async fn today_streams_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
    Query(query): Query<DaySourcesQuery>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => api_response(
            crate::api::wiki_streams::get_today_streams(state.db.pool(), parsed_date, query.tz.as_deref()).await,
        ),
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// `?limit=N` for the small home-page list endpoints.
#[derive(Debug, Deserialize, Default)]
pub struct LimitQuery {
    pub limit: Option<i64>,
}

/// Current weather for the home masthead (null until the weather_sync cron runs).
pub async fn weather_now_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::home::get_current_weather(state.db.pool()).await)
}

/// The next few calendar events (holidays/birthdays filtered).
pub async fn calendar_upcoming_handler(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Response {
    api_response(crate::api::home::get_calendar_upcoming(state.db.pool(), q.limit.unwrap_or(5)).await)
}

/// Places visited but never named — the home "name this place" ask.
pub async fn unnamed_places_handler(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Response {
    api_response(crate::api::home::get_unnamed_places(state.db.pool(), q.limit.unwrap_or(3)).await)
}

/// Get data sources (ontology records) for a day
pub async fn wiki_get_day_sources_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
    Query(query): Query<DaySourcesQuery>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => api_response(
            crate::api::wiki_streams::get_day_sources(state.db.pool(), parsed_date, query.tz.as_deref()).await,
        ),
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// Get AI chats (in-app Virtues + external imported) for a day
pub async fn wiki_get_day_chats_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::wiki_streams::get_day_chats(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// The day's facts for the strip under its Abstract: weather, coverage, chats.
pub async fn wiki_get_day_facts_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => api_response(crate::api::day_article::day_facts(state.db.pool(), parsed_date).await),
        Err(_) => error_response(Error::InvalidInput(format!("Invalid date format: {}", date))),
    }
}

/// Days whose pages read like this one's, nearest first.
pub async fn wiki_get_similar_days_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => api_response(crate::api::day_article::similar_days(state.db.pool(), parsed_date, 3).await),
        Err(_) => error_response(Error::InvalidInput(format!("Invalid date format: {}", date))),
    }
}

/// Get all ontology data streams for a day (dynamic query across all ontologies)
pub async fn wiki_get_day_streams_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::wiki_streams::get_day_streams(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}
