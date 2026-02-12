//! HTTP server for data ingestion and API

pub mod api;
pub mod ingest;
pub mod yjs;

use axum::{
    extract::DefaultBodyLimit,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};

use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;

use self::ingest::AppState;
use self::yjs::yjs_websocket_handler;
use crate::error::Result;
use crate::mcp::{http::add_mcp_routes, VirtuesMcpServer};
use crate::storage::stream_writer::StreamWriter;
use crate::Virtues;

/// Run the HTTP ingestion server with integrated scheduler
pub async fn run(client: Virtues, host: &str, port: u16) -> Result<()> {
    // Validate required environment variables early
    validate_environment()?;

    // Initialize usage limits from TIER env var
    if let Err(e) = crate::api::init_limits_from_tier(client.database.pool()).await {
        tracing::warn!("Failed to initialize usage limits: {}", e);
    }

    // Initialize drive quota from TIER env var
    if let Err(e) = crate::api::init_drive_quota(client.database.pool()).await {
        tracing::warn!("Failed to initialize drive quota: {}", e);
    }

    // Seed owner email from OWNER_EMAIL env var (Seed and Drift pattern)
    if let Err(e) = crate::api::seed_owner_email(client.database.pool()).await {
        tracing::warn!("Failed to seed owner email: {}", e);
    }

    // Auto-detect server readiness (skips setup screen if previously hydrated)
    if let Err(e) = crate::api::ensure_server_status(client.database.pool()).await {
        tracing::warn!("Failed to ensure server status: {}", e);
    }

    // Initialize vec_search virtual table for semantic search (requires sqlite-vec extension)
    {
        let search_engine = crate::search::SemanticSearchEngine::new(
            Arc::new(client.database.pool().clone()),
        );
        if let Err(e) = search_engine.ensure_vec_table().await {
            tracing::warn!("Failed to initialize vec_search table: {}", e);
        }
    }

    // Initialize StreamWriter (simple in-memory buffer)
    let stream_writer = StreamWriter::new();
    let stream_writer_arc = Arc::new(Mutex::new(stream_writer));

    // Start the scheduler in the background
    let db_pool = client.database.pool().clone();
    let storage = (*client.storage).clone();
    let scheduler_stream_writer = stream_writer_arc.clone();
    let _scheduler_handle = tokio::spawn(async move {
        match crate::Scheduler::new(db_pool, storage, scheduler_stream_writer).await {
            Ok(sched) => {
                if let Err(e) = sched.start().await {
                    tracing::warn!("Failed to start scheduler: {}", e);
                } else {
                    tracing::info!("Scheduler started successfully");

                    // Schedule drive trash purge job (daily at 3am)
                    if let Err(e) = sched.schedule_drive_trash_purge_job().await {
                        tracing::warn!("Failed to schedule drive trash purge job: {}", e);
                    }

                    // Schedule daily summary job (runs at user's maintenance hour)
                    if let Err(e) = sched.schedule_daily_summary_job().await {
                        tracing::warn!("Failed to schedule daily summary job: {}", e);
                    }

                    // Schedule embedding indexer job (every 15 minutes)
                    if let Err(e) = sched.schedule_embedding_job().await {
                        tracing::warn!("Failed to schedule embedding job: {}", e);
                    }

                    // Keep scheduler alive - it will be dropped when the server shuts down
                    // The JobScheduler runs background tasks that need to stay active
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to create scheduler: {}", e);
            }
        }
    });

    // Create ToolExecutor (optional - fails gracefully if TOLLBOOTH_INTERNAL_SECRET not set)
    let tool_executor = crate::tools::ToolExecutor::from_env(client.database.pool().clone())
        .map(Arc::new)
        .ok();

    if tool_executor.is_some() {
        tracing::info!("ToolExecutor initialized successfully");
    } else {
        tracing::warn!("ToolExecutor not initialized - TOLLBOOTH_INTERNAL_SECRET may not be set");
    }

    // Initialize Yjs state for real-time collaborative editing
    let yjs_state = yjs::YjsState::new(client.database.pool().clone());
    yjs_state.start_save_processor();
    tracing::info!("Yjs WebSocket server initialized");

    // Initialize chat cancellation state for stopping in-progress requests
    let chat_cancel_state = crate::api::chat::ChatCancellationState::new();

    // Create drive config with shared storage backend
    let drive_config = crate::api::DriveConfig::new(client.storage.clone());

    let state = AppState {
        db: client.database.clone(),
        storage: client.storage.clone(),
        drive_config,
        stream_writer: stream_writer_arc.clone(),
        tool_executor,
        yjs_state: yjs_state.clone(),
        chat_cancel_state,
    };

    let app = Router::new()
        // Health check
        .route("/health", get(health))
        // App server info (for device pairing)
        .route("/api/app/server-info", get(server_info))
        // Timeline day (location chunks for movement map)
        .route("/api/timeline/day/:date", get(api::timeline_get_day_handler))
        // Data ingestion
        .route("/ingest", post(ingest::ingest))
        // OAuth flow
        .route(
            "/api/sources/:provider/authorize",
            post(api::oauth_authorize_handler),
        )
        // OAuth callback from OAuth proxy (returns HTML redirect)
        .route("/oauth/callback", get(api::oauth_callback_handler))
        // Source management API
        .route("/api/sources", get(api::list_sources_handler))
        .route("/api/sources", post(api::create_source_handler))
        .route(
            "/api/sources/register-device",
            post(api::register_device_handler),
        )
        // Device pairing endpoints
        .route(
            "/api/devices/pairing/initiate",
            post(api::initiate_device_pairing_handler),
        )
        .route(
            "/api/devices/pairing/complete",
            post(api::complete_device_pairing_handler),
        )
        .route(
            "/api/devices/pairing/link",
            post(api::link_device_manual_handler),
        )
        .route(
            "/api/devices/pairing/:source_id/complete",
            post(api::complete_qr_pairing_handler),
        )
        .route(
            "/api/devices/pairing/:source_id",
            get(api::check_pairing_status_handler),
        )
        .route(
            "/api/devices/pending-pairings",
            get(api::list_pending_pairings_handler),
        )
        .route("/api/devices/health", get(api::device_health_check_handler))
        .route("/api/sources/:id", get(api::get_source_handler))
        .route("/api/sources/:id", delete(api::delete_source_handler))
        .route("/api/sources/:id/pause", post(api::pause_source_handler))
        .route("/api/sources/:id/resume", post(api::resume_source_handler))
        .route(
            "/api/sources/:id/status",
            get(api::get_source_status_handler),
        )
        // Stream management API
        .route("/api/sources/:id/streams", get(api::list_streams_handler))
        .route(
            "/api/sources/:id/streams",
            post(api::bulk_update_streams_handler),
        )
        .route(
            "/api/sources/:id/streams/:name",
            get(api::get_stream_handler),
        )
        .route(
            "/api/sources/:id/streams/:name/enable",
            post(api::enable_stream_handler),
        )
        .route(
            "/api/sources/:id/streams/:name/disable",
            post(api::disable_stream_handler),
        )
        .route(
            "/api/sources/:id/streams/:name",
            delete(api::disable_stream_handler),
        )
        .route(
            "/api/sources/:id/streams/:name/config",
            put(api::update_stream_config_handler),
        )
        .route(
            "/api/sources/:id/streams/:name/schedule",
            put(api::update_stream_schedule_handler),
        )
        .route(
            "/api/sources/:id/streams/:name/sync",
            post(api::sync_stream_handler),
        )
        .route(
            "/api/sources/:id/streams/:name/jobs",
            get(api::get_stream_jobs_handler),
        )
        // Catalog/Registry API
        .route(
            "/api/catalog/sources",
            get(api::list_catalog_sources_handler),
        )
        // Ontologies API
        .route(
            "/api/ontologies/available",
            get(api::list_available_ontologies_handler),
        )
        .route(
            "/api/ontologies/overview",
            get(api::get_ontologies_overview_handler),
        )
        // Jobs API
        .route("/api/jobs/:id", get(api::get_job_handler))
        .route("/api/jobs", get(api::query_jobs_handler))
        .route("/api/jobs/:id/cancel", post(api::cancel_job_handler))
        // Profile API
        .route("/api/profile", get(api::get_profile_handler))
        .route("/api/profile", put(api::update_profile_handler))
        // Entities API - Places
        .route(
            "/api/entities/places",
            get(api::list_places_handler).post(api::create_place_handler),
        )
        .route(
            "/api/entities/places/:id",
            get(api::get_place_handler)
                .put(api::update_place_handler)
                .delete(api::delete_place_handler),
        )
        .route(
            "/api/entities/places/:id/set-home",
            post(api::set_place_as_home_handler),
        )
        // Places API (Google Places proxy)
        .route(
            "/api/places/autocomplete",
            get(api::places_autocomplete_handler),
        )
        .route("/api/places/details", get(api::places_details_handler))
        // Assistant Profile API
        .route(
            "/api/assistant-profile",
            get(api::get_assistant_profile_handler),
        )
        .route(
            "/api/assistant-profile",
            put(api::update_assistant_profile_handler),
        )
        // Tools API
        .route("/api/tools", get(api::list_tools_handler))
        .route("/api/tools/:id", get(api::get_tool_handler))
        // Note: Built-in tools cannot be updated (read-only from registry)
        // MCP tools management endpoints to be added separately
        // Models API
        .route("/api/models", get(api::list_models_handler))
        .route(
            "/api/models/recommended",
            get(api::list_recommended_models_handler),
        )
        .route("/api/models/:id", get(api::get_model_handler))
        // Agents API
        .route("/api/agents", get(api::list_agents_handler))
        .route("/api/agents/:id", get(api::get_agent_handler))
        // Personas API
        .route("/api/personas", get(api::list_personas_handler))
        .route("/api/personas", post(api::create_persona_handler))
        .route("/api/personas/:id", get(api::get_persona_handler))
        .route("/api/personas/:id", put(api::update_persona_handler))
        .route("/api/personas/:id", delete(api::hide_persona_handler))
        .route(
            "/api/personas/:id/unhide",
            post(api::unhide_persona_handler),
        )
        .route("/api/personas/reset", post(api::reset_personas_handler))
        // Seed Testing API
        .route(
            "/api/seed/pipeline-status",
            get(api::seed_pipeline_status_handler),
        )
        .route(
            "/api/seed/data-quality",
            get(api::seed_data_quality_handler),
        )
        // Metrics API
        .route(
            "/api/metrics/activity",
            get(api::get_activity_metrics_handler),
        )
        // Plaid Link API (different from standard OAuth)
        .route(
            "/api/plaid/link-token",
            post(api::create_plaid_link_token_handler),
        )
        .route(
            "/api/plaid/exchange-token",
            post(api::exchange_plaid_token_handler),
        )
        .route(
            "/api/plaid/:source_id/accounts",
            get(api::get_plaid_accounts_handler),
        )
        .route(
            "/api/plaid/:source_id",
            delete(api::remove_plaid_item_handler),
        )
        // Usage API
        .route("/api/usage", get(api::usage_handler))
        .route("/api/usage/check", get(api::usage_check_handler))
        // Subscription & Billing API
        .route("/api/subscription", get(api::get_subscription_handler))
        .route(
            "/api/billing/portal",
            post(api::create_billing_portal_handler),
        )
        // Search API (Exa)
        .route("/api/search/web", post(api::exa_search_handler))
        // Unsplash API (cover image search)
        .route("/api/unsplash/search", post(api::unsplash_search_handler))
        // Storage API
        .route(
            "/api/storage/objects",
            get(api::list_storage_objects_handler),
        )
        .route(
            "/api/storage/objects/:id/content",
            get(api::get_storage_object_content_handler),
        )
        // Drive API (user file storage)
        .route("/api/drive/usage", get(api::get_drive_usage_handler))
        .route("/api/drive/warnings", get(api::get_drive_warnings_handler))
        .route("/api/drive/files", get(api::list_drive_files_handler))
        .route(
            "/api/drive/files/:id",
            get(api::get_drive_file_handler).delete(api::delete_drive_file_handler),
        )
        .route(
            "/api/drive/files/:id/download",
            get(api::download_drive_file_handler),
        )
        .route(
            "/api/drive/files/:id/move",
            put(api::move_drive_file_handler),
        )
        .route("/api/drive/upload", post(api::upload_drive_file_handler))
        .route("/api/drive/folders", post(api::create_drive_folder_handler))
        .route(
            "/api/drive/reconcile",
            post(api::reconcile_drive_usage_handler),
        )
        // Drive trash endpoints
        .route("/api/drive/trash", get(api::list_drive_trash_handler))
        .route(
            "/api/drive/trash/empty",
            post(api::empty_drive_trash_handler),
        )
        .route(
            "/api/drive/files/:id/restore",
            post(api::restore_drive_file_handler),
        )
        .route(
            "/api/drive/files/:id/purge",
            delete(api::purge_drive_file_handler),
        )
        // Media API (content-addressed storage for page-embedded media)
        .route("/api/media/upload", post(api::upload_media_handler))
        .route("/api/media/:id", get(api::get_media_handler))
        // Wiki API
        .route("/api/wiki/resolve/:id", get(api::wiki_resolve_id_handler))
        // Wiki - Person
        .route("/api/wiki/people", get(api::wiki_list_people_handler))
        .route(
            "/api/wiki/person/:id",
            get(api::wiki_get_person_handler).put(api::wiki_update_person_handler),
        )
        // Wiki - Place
        .route("/api/wiki/places", get(api::wiki_list_places_handler))
        .route(
            "/api/wiki/place/:id",
            get(api::wiki_get_place_handler).put(api::wiki_update_place_handler),
        )
        // Wiki - Organization
        .route(
            "/api/wiki/organizations",
            get(api::wiki_list_organizations_handler),
        )
        .route(
            "/api/wiki/organization/:id",
            get(api::wiki_get_organization_handler).put(api::wiki_update_organization_handler),
        )
        // Wiki - Telos
        .route(
            "/api/wiki/telos/active",
            get(api::wiki_get_active_telos_handler),
        )
        .route("/api/wiki/telos/:id", get(api::wiki_get_telos_handler))
        // Wiki - Act
        .route("/api/wiki/acts", get(api::wiki_list_acts_handler))
        .route("/api/wiki/act/:id", get(api::wiki_get_act_handler))
        // Wiki - Chapter
        .route("/api/wiki/chapter/:id", get(api::wiki_get_chapter_handler))
        .route(
            "/api/wiki/act/:act_id/chapters",
            get(api::wiki_list_chapters_handler),
        )
        // Wiki - Day
        .route("/api/wiki/days", get(api::wiki_list_days_handler))
        .route(
            "/api/wiki/day/:date",
            get(api::wiki_get_day_handler).put(api::wiki_update_day_handler),
        )
        // Wiki - Citations
        .route(
            "/api/wiki/:source_type/:source_id/citations",
            get(api::wiki_get_citations_handler).post(api::wiki_create_citation_handler),
        )
        .route(
            "/api/wiki/citations/:id",
            put(api::wiki_update_citation_handler).delete(api::wiki_delete_citation_handler),
        )
        // Wiki - Temporal Events
        .route(
            "/api/wiki/day/:date/events",
            get(api::wiki_get_day_events_handler),
        )
        .route("/api/wiki/events", post(api::wiki_create_event_handler))
        .route(
            "/api/wiki/events/:id",
            put(api::wiki_update_event_handler).delete(api::wiki_delete_event_handler),
        )
        .route(
            "/api/wiki/day/:day_id/auto-events",
            delete(api::wiki_delete_auto_events_handler),
        )
        // Wiki - Day Sources (ontology data)
        .route(
            "/api/wiki/day/:date/summary",
            post(api::wiki_generate_day_summary_handler),
        )
        .route(
            "/api/wiki/day/:date/sources",
            get(api::wiki_get_day_sources_handler),
        )
        // Wiki - Day Streams (dynamic ontology queries)
        .route(
            "/api/wiki/day/:date/streams",
            get(api::wiki_get_day_streams_handler),
        )
        // Code Execution API (AI Sandbox)
        .route("/api/code/execute", post(api::execute_code_handler))
        // Developer API
        .route("/api/developer/sql", post(api::execute_sql_handler))
        .route("/api/developer/tables", get(api::list_tables_handler))
        // Lake API
        .route("/api/lake/summary", get(api::get_lake_summary_handler))
        .route("/api/lake/streams", get(api::list_lake_streams_handler))
        // Pages API
        .route(
            "/api/pages",
            get(api::list_pages_handler).post(api::create_page_handler),
        )
        .route(
            "/api/pages/search/entities",
            get(api::search_entities_handler),
        )
        .route(
            "/api/pages/:id",
            get(api::get_page_handler)
                .put(api::update_page_handler)
                .delete(api::delete_page_handler),
        )
        // Page Versions API
        .route(
            "/api/pages/:id/versions",
            get(api::list_page_versions_handler).post(api::create_page_version_handler),
        )
        .route(
            "/api/pages/versions/:version_id",
            get(api::get_page_version_handler),
        )
        // Spaces API
        .route(
            "/api/spaces",
            get(api::list_spaces_handler).post(api::create_space_handler),
        )
        .route(
            "/api/spaces/:id",
            get(api::get_space_handler)
                .put(api::update_space_handler)
                .delete(api::delete_space_handler),
        )
        .route("/api/spaces/:id/tabs", put(api::save_space_tabs_handler))
        .route("/api/spaces/:id/views", get(api::list_space_views_handler))
        // Space Items API (root-level items at space level, not in any folder)
        .route(
            "/api/spaces/:id/items",
            get(api::list_space_items_handler)
                .post(api::add_space_item_handler)
                .delete(api::remove_space_item_handler),
        )
        .route(
            "/api/spaces/:id/items/reorder",
            put(api::reorder_space_items_handler),
        )
        // Namespaces API
        .route("/api/namespaces", get(api::list_namespaces_handler))
        .route("/api/namespaces/:name", get(api::get_namespace_handler))
        // Views API
        .route("/api/views", post(api::create_view_handler))
        .route(
            "/api/views/:id",
            get(api::get_view_handler)
                .put(api::update_view_handler)
                .delete(api::delete_view_handler),
        )
        .route("/api/views/:id/resolve", post(api::resolve_view_handler))
        .route(
            "/api/views/:id/items",
            get(api::list_view_items_handler)
                .post(api::add_view_item_handler)
                .delete(api::remove_view_item_handler),
        )
        .route(
            "/api/views/:id/items/reorder",
            put(api::reorder_view_items_handler),
        )
        // Chats API
        .route(
            "/api/chats",
            get(api::list_chats_handler).post(api::create_chat_handler),
        )
        .route(
            "/api/chats/:id",
            get(api::get_chat_handler)
                .patch(api::update_chat_handler)
                .delete(api::delete_chat_handler),
        )
        .route("/api/chats/title", post(api::generate_chat_title_handler))
        // Chat Usage & Compaction API
        .route("/api/chats/:id/usage", get(api::get_chat_usage_handler))
        .route("/api/chats/:id/compact", post(api::compact_chat_handler))
        // Chat API (streaming)
        .route("/api/chat", post(api::chat_handler))
        .route("/api/chat/cancel", post(api::cancel_chat_handler))
        // Chat Edit Permissions API
        .route(
            "/api/chats/:id/permissions",
            get(api::list_chat_permissions_handler).post(api::add_chat_permission_handler),
        )
        .route(
            "/api/chats/:id/permissions/:entity_id",
            delete(api::remove_chat_permission_handler),
        )
        // Auth API
        .route("/auth/signin", post(api::auth_signin_handler))
        .route("/auth/callback", get(api::auth_callback_handler))
        .route("/auth/signout", post(api::auth_signout_handler))
        .route("/auth/session", get(api::auth_session_handler))
        // Atlas webhook for owner email updates (Seed and Drift pattern)
        .route(
            "/api/profile/owner-email",
            post(api::auth_owner_email_handler),
        )
        // Internal API (Tollbooth integration)
        .route("/internal/hydrate", post(api::hydrate_profile_handler))
        .route(
            "/internal/server-status",
            get(api::get_server_status_handler),
        )
        .route("/internal/mark-ready", post(api::mark_server_ready_handler))
        // Feedback API
        .route("/api/feedback", post(crate::api::feedback::submit_feedback))
        // Terminal API (WebSocket)
        .route(
            "/ws/terminal",
            get(crate::api::terminal::terminal_ws_handler),
        )
        // Yjs WebSocket (real-time collaborative editing)
        .route("/ws/yjs/:page_id", get(yjs_websocket_handler))
        .with_state(state.clone())
        .layer(DefaultBodyLimit::disable()) // Disable default 2MB limit
        .layer(DefaultBodyLimit::max(20 * 1024 * 1024)); // Set 20MB limit for audio files

    // Add MCP routes to the same server
    let mcp_server = VirtuesMcpServer::new(client.database.pool().clone());
    let app = add_mcp_routes(app, mcp_server);

    tracing::info!("MCP endpoint enabled at /mcp");

    // Add static file serving for SPA frontend
    // This serves the SvelteKit static build and falls back to 200.html for SPA routing
    let static_dir =
        std::env::var("STATIC_DIR").unwrap_or_else(|_| "../../apps/web/build".to_string());
    let static_path = std::path::Path::new(&static_dir);

    let app = if static_path.exists() && static_path.is_dir() {
        use tower_http::services::{ServeDir, ServeFile};

        let fallback_file = static_path.join("200.html");
        let serve_dir = if fallback_file.exists() {
            ServeDir::new(&static_dir).fallback(ServeFile::new(fallback_file))
        } else {
            // Try index.html as fallback if 200.html doesn't exist
            let index_file = static_path.join("index.html");
            ServeDir::new(&static_dir).fallback(ServeFile::new(index_file))
        };

        tracing::info!("Static file serving enabled from: {}", static_dir);
        app.fallback_service(serve_dir)
    } else {
        tracing::info!(
            "No static directory found at: {} - static serving disabled",
            static_dir
        );
        app
    };

    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("Server listening on {}", addr);

    // Run the server (this blocks forever until shutdown)
    axum::serve(listener, app).await?;

    // Note: No flush needed on shutdown - StreamWriter is in-memory only now.
    // Records are written directly to filesystem during sync/ingest.
    tracing::info!("Server shutting down gracefully");

    // Note: scheduler runs in background and will stop when the process exits
    // The handle is dropped here, but the task continues running

    Ok(())
}

/// Validate required environment variables at startup
fn validate_environment() -> Result<()> {
    // Log storage path being used
    let storage_path = env::var("STORAGE_PATH").unwrap_or_else(|_| "./data/lake".to_string());
    tracing::info!("Using storage path: {}", storage_path);

    tracing::debug!("Environment validation passed");
    Ok(())
}

async fn health(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    // Check database connectivity with a simple query
    let db_status = match sqlx::query("SELECT 1").execute(state.db.pool()).await {
        Ok(_) => "connected",
        Err(_) => "disconnected",
    };

    // Read update_check_hour for Atlas health check sync
    let update_check_hour = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT update_check_hour FROM app_user_profile WHERE id = '00000000-0000-0000-0000-000000000001'"
    )
    .fetch_optional(state.db.pool())
    .await
    .ok()
    .flatten()
    .flatten();

    let is_healthy = db_status == "connected";
    let status_code = if is_healthy {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    };

    let min_ios_version =
        std::env::var("MIN_IOS_APP_VERSION").unwrap_or_else(|_| "1.0".to_string());

    (
        status_code,
        Json(serde_json::json!({
            "status": if is_healthy { "healthy" } else { "unhealthy" },
            "version": env!("CARGO_PKG_VERSION"),
            "commit": env!("GIT_COMMIT"),
            "built_at": env!("BUILD_TIME"),
            "min_ios_version": min_ios_version,
            "update_check_hour": update_check_hour,
            "database": db_status,
            "pool": {
                "size": state.db.pool().size(),
                "idle": state.db.pool().num_idle(),
            }
        })),
    )
}

/// Server info endpoint for device pairing
/// Returns the API endpoint URL for iOS device configuration
async fn server_info() -> impl IntoResponse {
    // Resolution: PUBLIC_API_URL (explicit override) → BACKEND_URL → localhost fallback
    let api_endpoint = std::env::var("PUBLIC_API_URL")
        .or_else(|_| std::env::var("BACKEND_URL"))
        .unwrap_or_else(|_| "http://localhost:8000".to_string());

    Json(serde_json::json!({
        "apiEndpoint": api_endpoint
    }))
}
