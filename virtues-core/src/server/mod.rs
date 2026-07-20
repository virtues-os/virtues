//! HTTP server for data ingestion and API

pub mod api;
pub mod faces;
pub mod webhook;
pub mod yjs;

use axum::{
    extract::DefaultBodyLimit,
    middleware,
    response::IntoResponse,
    routing::{delete, get, patch, post, put},
    Json, Router,
};

use std::env;
use std::sync::Arc;

use self::webhook::AppState;
use self::yjs::yjs_websocket_handler;
use crate::error::Result;
use crate::mcp::{http::add_mcp_routes, VirtuesMcpServer};
use crate::middleware::auth::AuthUser;
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

    // Reap runs left in `running` by a crash/restart mid-execution, so a stale
    // lock doesn't survive a reboot. (The concurrency gate also age-bounds stale
    // runs at request time; this just keeps the runs table honest on boot.)
    match crate::scheduler::actions::cleanup_stale_runs(client.database.pool()).await {
        Ok(n) if n > 0 => tracing::info!("Reaped {} stale 'running' action run(s) on startup", n),
        Ok(_) => {}
        Err(e) => tracing::warn!("Failed to reap stale action runs: {}", e),
    }

    // Auto-detect server readiness (skips setup screen if previously hydrated)
    if let Err(e) = crate::api::ensure_server_status(client.database.pool()).await {
        tracing::warn!("Failed to ensure server status: {}", e);
    }

    // Seed home_timezone from the box's own system clock once, before the
    // scheduler resolves cron timezones. Idempotent. See docs/timezone-model.md.
    if let Err(e) = crate::api::profile::ensure_home_timezone(client.database.pool()).await {
        tracing::warn!("Failed to seed home_timezone: {}", e);
    }

    // Face-reader grants: idempotent default-deny SELECT surface for applet
    // faces (data_*/wiki_* tables + applet_* schemas). Best-effort.
    if let Err(e) = faces::ensure_face_reader_grants(client.database.pool()).await {
        tracing::warn!("face reader grants failed: {e}");
    }

    // Eager identity bringup: ensure the loopback console device exists so the
    // box's own browser is authenticated. Best-effort — a failure here must not
    // stop the box from serving. (The box's TLS identity is its own cert,
    // obtained at relay spawn; no keypair to mint here.)
    {
        let pool = client.database.pool();
        if let Err(e) = crate::middleware::auth::ensure_console_device(pool).await {
            tracing::warn!("identity bringup: ensure_console_device failed: {e}");
        }
    }

    // Sanity-check the pgvector-backed search_vectors table is reachable.
    // Schema creation happens via 0008_search_and_vectors.sql; this probe just
    // confirms the migration ran.
    {
        let search_engine = crate::search::SemanticSearchEngine::new(
            Arc::new(client.database.pool().clone()),
        );
        if let Err(e) = search_engine.ensure_vec_table().await {
            tracing::warn!("Failed to probe search_vectors table: {}", e);
        }
    }

    // Initialize Yjs state early (needed by both server and scheduler)
    let yjs_state = yjs::YjsState::new(client.database.pool().clone());
    yjs_state.start_save_processor();
    tracing::info!("Yjs WebSocket server initialized");

    // System telemetry: the Jetson GPU monitor (idle-gated tegrastats) and the
    // box-local time-series sampler (1/min → app_system_samples) behind the
    // System/Telemetry views. Both are no-ops/best-effort on non-Jetson hosts.
    crate::api::system_telemetry::start_gpu_monitor();
    crate::api::system_telemetry::start_system_sampler(client.database.pool().clone());

    // Reconcile action templates from per-folder manifests — creates/updates
    // system action rows. Safe to call on every startup (user-managed runtime
    // state preserved).
    if let Err(e) = crate::action_templates::reconcile_templates(client.database.pool()).await {
        tracing::warn!("Failed to reconcile action templates: {}", e);
    }

    // Boot the `app`-runtime supervisor — spawns one long-running child per
    // app-runtime action, watches/restarts on crash, exposes HTTP at
    // `/service/<action_id>/*` via the proxy handler. See `virtues-core/src/services/`.
    let service_supervisor = {
        // Resolve repo root: core's manifest dir is `<repo>/core`, so repo
        // root is one level up.
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let api_base = std::env::var("VIRTUES_CORE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8000".to_string());
        let supervisor = crate::services::ServiceSupervisor::new(repo_root, api_base);
        if let Err(e) = supervisor.start(client.database.pool()).await {
            tracing::warn!("app supervisor start failed: {}", e);
        }
        supervisor
    };

    // Model facts (prices, context windows, which ids still exist) are fetched
    // from virtues-api, never compiled in. Refreshes on boot and 6-hourly; an
    // unreachable cloud keeps the last snapshot rather than emptying the
    // picker. See api::model_catalog.
    crate::api::model_catalog::spawn(client.database.pool().clone());

    // Start the scheduler in the background
    let db_pool = client.database.pool().clone();
    let scheduler_yjs = yjs_state.clone();
    let _scheduler_handle = tokio::spawn(async move {
        match crate::Scheduler::new(db_pool, scheduler_yjs).await {
            Ok(sched) => {
                if let Err(e) = sched.schedule_all().await {
                    tracing::warn!("Failed to schedule cron actions: {}", e);
                }
                if let Err(e) = sched.start().await {
                    tracing::warn!("Failed to start scheduler: {}", e);
                } else {
                    tracing::info!("Scheduler started successfully");
                    // Keep the scheduler handle alive — tokio-cron-scheduler
                    // runs background tasks on its own tokio tasks, but the
                    // JobScheduler itself needs to stay in scope.
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

    // Auth-table sweeper: deletes expired pair tokens + sudo requests every
    // 10 minutes, archives `app_auth_event` rows older than 90 days. Lives
    // in-process — same lifecycle as the HTTP server, ends when the daemon
    // ends. See `crate::maintenance::sweeper`.
    crate::maintenance::sweeper::spawn(client.database.pool().clone());

    // Pair-code rotator: keeps a fresh universal standing pair code alive at all
    // times (with an overlap window) so the panel and `virtues pair` always have
    // a valid code to display. See `crate::maintenance::pair_rotator`.
    crate::maintenance::pair_rotator::spawn(client.database.pool().clone());

    // Entity resolver: periodically turns raw lake primitives (location points,
    // transactions, calendar attendees) into ontology surfaces (visits/places,
    // merchant orgs, people) via `entity_resolution::resolve_entities`. Without
    // this the resolution only ran from the CLI, so the day page / timeline had
    // nothing to show even while the lake filled. See `maintenance::entity_resolver`.
    crate::maintenance::entity_resolver::spawn(client.database.clone());

    // Create ToolExecutor (optional - fails gracefully if VIRTUES_API_INTERNAL_SECRET not set)
    let tool_executor = crate::tools::ToolExecutor::from_env(client.database.pool().clone())
        .map(Arc::new)
        .ok();

    if tool_executor.is_some() {
        tracing::info!("ToolExecutor initialized successfully");
    } else {
        tracing::warn!("ToolExecutor not initialized - VIRTUES_API_INTERNAL_SECRET may not be set");
    }

    // Initialize chat cancellation state for stopping in-progress requests
    let chat_cancel_state = crate::api::chat::ChatCancellationState::new();

    // Create drive config with shared storage backend
    let drive_config = crate::api::DriveConfig::new(client.storage.clone());

    let state = AppState {
        db: client.database.clone(),
        storage: client.storage.clone(),
        drive_config,
        tool_executor,
        yjs_state: yjs_state.clone(),
        chat_cancel_state,
        service_supervisor: Some(service_supervisor.clone()),
    };

    // ============================================================
    // Public routes (no authentication required)
    // ============================================================
    let public_routes = Router::new()
        // Health check
        .route("/health", get(health))
        // App server info (for device pairing)
        .route("/api/app/server-info", get(server_info))
        // Public, LAN-reachable box health — boot gates + inference resolution.
        // No secrets; the first-run web page / appliance screen poll this
        // before any owner session exists. (Full identity detail stays behind
        // the session-authed /api/box/status.)
        .route(
            "/api/box/health",
            get(crate::api::box_status::box_health_handler),
        )
        // Setup/onboarding state machine (docs/onboarding.md) — public-on-LAN
        // for the same reason as /api/box/health: the wizard + panel render it
        // pre-auth, and it carries only booleans + step copy.
        .route(
            "/api/setup/state",
            get(crate::api::box_status::setup_state_handler),
        )
        // Auth — pair-only model. Public consume + session probe (returns the
        // AuthUser resolved from the request's proven iroh key, if any).
        // /api/pair/{mint,confirm,deny,status} are auth'd and live under the
        // protected_routes block below.
        .route(
            "/api/pair/consume",
            post(crate::api::pair::consume_handler),
        )
        .route("/auth/session", get(api::auth_session_handler))
        // Internal API (virtues-api integration — has its own header-based auth)
        .route("/internal/hydrate", post(api::hydrate_profile_handler))
        .route(
            "/internal/server-status",
            get(api::get_server_status_handler),
        )
        .route("/internal/mark-ready", post(api::mark_server_ready_handler))
        // Applet faces: sandboxed-iframe runtime. Token-based (the mint route
        // rides normal transport auth; file/query routes are the only CORS-
        // permissive surface, which is the inner wall of the face jail).
        .route("/api/actions/:id/face-token", get(faces::mint_face_token_handler))
        .route(
            "/api/face/query",
            post(faces::face_query_handler).options(faces::face_query_preflight),
        )
        .route("/face/:action_id/", get(faces::face_index_handler))
        .route("/face/:action_id/*path", get(faces::face_file_handler))
        // Public page sharing (token-based access, no session needed)
        .route("/api/s/:token", get(api::get_shared_page_handler))
        .route(
            "/api/s/:token/files/:file_id",
            get(api::shared_file_download_handler),
        )
        // Webhook ingestion. Authenticated primarily by the proven iroh key
        // (Option<AuthUser>) — the owner's devices POST over iroh — with the
        // legacy Bearer device-token kept only as a fallback for external,
        // non-iroh callers. Lives in public_routes so the bearer fallback path
        // isn't force-rejected by the AuthUser route_layer.
        // Per-route body limit override (router-wide cap is 105MB): iOS audio
        // batches are base64 AAC and can dwarf the other streams on backfill.
        // A body over the cap is rejected by the Json extractor before the
        // handler runs, which historically surfaced as a bogus "no stream
        // selector" action error. See webhook.rs for the rejection handling.
        .route(
            "/webhook/:action_id",
            post(webhook::webhook).layer(DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        // Device re-fetch for stream → action_id map. Used by paired devices
        // whose Keychain entry predates the webhook unification, or after
        // templates.toml adds a new stream. Same device-token bearer auth as
        // the webhook endpoint.
        .route(
            "/api/devices/action-ids",
            get(api::device_action_ids_handler),
        )
        // Device-scoped run history for one of the caller's own actions, so the
        // app can show real server-side outcome per stream. Device-token bearer
        // auth + credential-ownership check (see handler). Distinct from the
        // session-authed /api/actions/:id/runs.
        .route(
            "/api/devices/actions/:id/runs",
            get(api::device_action_runs_handler),
        );

    // ============================================================
    // Protected routes (authentication required via route_layer)
    // ============================================================
    let protected_routes = Router::new()
        // Timeline day (location chunks for movement map)
        .route("/api/timeline/day/:date", get(api::timeline_get_day_handler))
        // Today streams — location/calendar/audio spans, pre-synthesis (homepage)
        .route("/api/today/:date/streams", get(api::today_streams_handler))
        // Map tiles — the Atlas: box-cached tiles (private + offline). docs/map-atlas-plan.md
        .route("/api/map/tiles/:style/:z/:x/:y", get(api::map_tile_handler))
        // Home-page loops — weather · upcoming calendar · unnamed-place backlog
        .route("/api/weather/current", get(api::weather_now_handler))
        .route("/api/calendar/upcoming", get(api::calendar_upcoming_handler))
        .route("/api/places/unnamed", get(api::unnamed_places_handler))
        // ─── Pair-only auth: "+ Add device" from a paired session ─────
        .route("/api/pair/mint",          post(crate::api::pair::mint_handler))
        .route("/api/pair/mint-collector", post(crate::api::pair::mint_collector_handler))
        .route("/api/pair/status/:id",    get(crate::api::pair::status_handler))
        .route("/api/pair/deny/:id",      post(crate::api::pair::deny_handler))
        // ─── Devices: unified list + revoke ───────────────────────────
        .route("/api/devices",            get(crate::api::devices::list_handler))
        .route("/api/devices/self/node-id", post(crate::api::devices::set_self_node_id))
        .route("/api/devices/self/reach",   get(crate::api::devices::get_self_reach))
        .route("/api/devices/enroll-peer",  post(crate::api::devices::enroll_peer))
        .route("/api/devices/:id",        axum::routing::delete(crate::api::devices::revoke_handler))
        // ─── Sudo: gate for high-sensitivity actions ──────────────────
        .route("/api/sudo/request",       post(crate::api::sudo::request_handler))
        .route("/api/sudo/status/:id",    get(crate::api::sudo::status_handler))
        // ─── Audit log ────────────────────────────────────────────────
        .route("/api/audit/auth",         get(crate::api::audit::list_handler))
        // ─── Billing settings (BYO key) ───────────────────────────────
        // BYO routes inference around virtues-api entirely: box calls
        // upstream directly. Save/delete are sudo-gated (change_byo_key);
        // status is a non-secret read for the Billing page.
        .route("/api/settings/byo-key",   get(crate::api::settings_byo::status_handler)
                                          .post(crate::api::settings_byo::save_handler)
                                          .delete(crate::api::settings_byo::delete_handler))
        // ─── Billing-state aggregator (local view) ────────────────────
        .route("/api/billing/state",           get(crate::api::billing_state::state_handler))
        .route("/api/billing/auto-topup",      post(crate::api::billing_state::set_auto_topup_handler))
        // Setup wizard transitions (docs/onboarding.md) — session-authed; the
        // wizard reads progress from the public /api/setup/state.
        .route("/api/setup/subscribe/start",   post(crate::api::setup::subscribe_start_handler))
        .route("/api/setup/login/start",       post(crate::api::setup::login_start_handler))
        .route("/api/setup/link/poll",         post(crate::api::setup::link_poll_handler))
        // ─── Source OAuth + API-key connect flows ────────────────────
        // Device pairing (iOS / Mac / sensor) lives at /api/pair/* (above).
        // The legacy /api/pairing/initiate + /api/pairing/complete routes
        // were removed in v1 — iOS now pairs via /api/pair/consume with
        // kind = "mobile_app".
        .route(
            "/api/connect/:source_id/start",
            post(crate::api::source_auth::oauth_start_handler),
        )
        .route(
            "/api/connect/:source_id/complete",
            post(crate::api::source_auth::apikey_complete_handler),
        )
        .route(
            "/oauth/callback",
            axum::routing::get(crate::api::source_auth::oauth_callback_handler),
        )
        // Box health for the phone app's status screen (same data as the
        // `virtues status` CLI — one source of truth in api::box_status).
        .route(
            "/api/box/status",
            get(crate::api::box_status::box_status_handler),
        )
        // Device-health endpoint (used by mobile/admin UIs).
        .route("/api/devices/health", get(api::device_health_check_handler))
        // Actions API
        .route(
            "/api/actions",
            get(api::list_actions_handler).post(api::create_action_handler),
        )
        .route(
            "/api/actions/:id",
            get(api::get_action_handler)
                .patch(api::patch_action_handler)
                .delete(api::delete_action_handler),
        )
        .route("/api/actions/:id/run", post(api::trigger_action_handler))
        // Chat-export upload (Tier 3 one-time import). Per-route body limit
        // overrides the router-wide 105MB cap — ChatGPT exports can be larger.
        .route(
            "/api/chat-import/upload",
            post(api::chat_import_upload_handler)
                .layer(DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        .route("/api/actions/:id/runs", get(api::list_action_runs_handler))
        .route("/api/actions/runs/:id", get(api::get_action_run_handler))
        .route("/api/runs", get(api::list_runs_handler))
        // Credentials API
        .route("/api/credentials", get(api::list_credentials_handler))
        .route(
            "/api/credentials/:id",
            patch(api::patch_credential_handler).delete(api::delete_credential_handler),
        )
        // Source catalog (drives the Sources tile grid)
        .route("/api/sources", get(api::list_sources_handler))
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
        // Models API
        .route("/api/models", get(api::list_models_handler))
        .route(
            "/api/models/recommended",
            get(api::list_models_with_slots_handler),
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
        // Subscription & Billing API
        .route("/api/subscription", get(api::get_subscription_handler))
        .route(
            "/api/billing/portal",
            post(api::create_billing_portal_handler),
        )
        .route("/api/billing/claim", post(api::claim_billing_handler))
        // Wallet balance + recent ledger (proxied from virtues-api /v1/usage).
        .route("/api/billing/usage", get(api::billing_usage_handler))
        // Box-local AI spend breakdown (app_ai_calls) for the Usage tab.
        .route("/api/usage/summary", get(api::usage_summary_handler))
        // Recent individual AI calls (app_ai_calls) for the Telemetry tab log.
        .route("/api/telemetry/ai-calls", get(api::ai_calls_handler))
        // Device-authorization link flow (web "Connect subscription").
        .route(
            "/api/billing/link/start",
            post(api::billing_link_start_handler),
        )
        .route(
            "/api/billing/link/status",
            get(api::billing_link_status_handler),
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
        .route("/api/drive/media", get(api::list_drive_media_handler))
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
        // Mention review queue (entity resolution HITL)
        .route("/api/mentions/queue", get(api::list_floating_surfaces_handler))
        .route("/api/mentions/link", post(api::link_surface_handler))
        .route("/api/mentions/create", post(api::create_from_surface_handler))
        .route("/api/mentions/dismiss", post(api::dismiss_surface_handler))
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
        // Wiki - Organization (table `wiki_orgs`; both URL forms supported)
        .route(
            "/api/wiki/organizations",
            get(api::wiki_list_organizations_handler),
        )
        .route(
            "/api/wiki/orgs",
            get(api::wiki_list_organizations_handler),
        )
        .route(
            "/api/wiki/organization/:id",
            get(api::wiki_get_organization_handler).put(api::wiki_update_organization_handler),
        )
        .route(
            "/api/wiki/org/:id",
            get(api::wiki_get_organization_handler).put(api::wiki_update_organization_handler),
        )
        // Wiki - Thing: retired; things live under /api/things (single source)
        // Wiki - Narrative Identity
        .route(
            "/api/wiki/narrative-identity",
            get(api::wiki_get_narrative_identity_handler)
                .put(api::wiki_update_narrative_identity_handler),
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
            "/api/wiki/day/:date/sources",
            get(api::wiki_get_day_sources_handler),
        )
        // Wiki - Day Chats (in-app + external AI conversations)
        .route(
            "/api/wiki/day/:date/chats",
            get(api::wiki_get_day_chats_handler),
        )
        // Wiki - Day Streams (dynamic ontology queries)
        .route(
            "/api/wiki/day/:date/streams",
            get(api::wiki_get_day_streams_handler),
        )
        // Code Execution API (AI Sandbox)
        .route("/api/code/execute", post(api::execute_code_handler))
        // Admin API — LLM-authoring on-ramp for new actions
        .route("/api/admin/reconcile", post(api::admin_reconcile_handler))
        .route(
            "/api/admin/actions/import-git",
            post(api::import_git_actions_handler),
        )
        // System (operator surface — apps + logs)
        .route("/api/system/apps", get(api::list_system_apps_handler))
        .route("/api/actions/:id/logs", get(api::get_action_logs_handler))
        // Live host snapshot + persisted history for the System/Telemetry views.
        .route(
            "/api/system/telemetry",
            get(crate::api::system_telemetry::telemetry_handler),
        )
        .route(
            "/api/system/history",
            get(crate::api::system_telemetry::history_handler),
        )
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
            "/api/pages/search/refs",
            get(api::search_refs_handler),
        )
        .route(
            "/api/pages/reflections/:date",
            get(api::get_reflections_handler).post(api::create_reflection_handler),
        )
        .route(
            "/api/pages/:id",
            get(api::get_page_handler)
                .put(api::update_page_handler)
                .delete(api::delete_page_handler),
        )
        // Raw record viewer — one life-graph row by (ontology, id)
        .route(
            "/api/records/:ontology/:record_id",
            get(api::get_record_handler),
        )
        // Page References (backlinks) API
        .route(
            "/api/pages/:id/backlinks",
            get(api::get_page_backlinks_handler),
        )
        // Page Share API
        .route(
            "/api/pages/:id/share",
            post(api::create_page_share_handler)
                .get(api::get_page_share_handler)
                .delete(api::delete_page_share_handler),
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
        // Things API (long-running named anchors — projects, pets, goals, ...)
        .route(
            "/api/things",
            get(api::list_things_handler).post(api::create_thing_handler),
        )
        .route(
            "/api/things/:id",
            get(api::get_thing_handler)
                .patch(api::update_thing_handler)
                .delete(api::delete_thing_handler),
        )
        // Sidebar pins API
        .route(
            "/api/pins",
            get(api::list_pins_handler).post(api::create_pin_handler),
        )
        .route("/api/pins/reorder", put(api::reorder_pins_handler))
        .route(
            "/api/pins/:id",
            patch(api::update_pin_handler).delete(api::delete_pin_handler),
        )
        // Notebooks API (the "room" a chat lives in)
        .route(
            "/api/notebooks",
            get(api::list_notebooks_handler).post(api::create_notebook_handler),
        )
        .route(
            "/api/notebooks/:id",
            get(api::get_notebook_handler)
                .put(api::update_notebook_handler)
                .delete(api::delete_notebook_handler),
        )
        // Notebook membership (items come back inside GET /api/notebooks/:id)
        .route(
            "/api/notebooks/:id/items",
            post(api::add_notebook_item_handler).delete(api::remove_notebook_item_handler),
        )
        .route(
            "/api/notebooks/:id/items/reorder",
            put(api::reorder_notebook_items_handler),
        )
        // Namespaces API
        .route("/api/namespaces", get(api::list_namespaces_handler))
        .route("/api/namespaces/:name", get(api::get_namespace_handler))
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
        .route("/api/ai/complete", post(api::ai_complete_handler))
        // Chat Edit Permissions API
        .route(
            "/api/chats/:id/permissions",
            get(api::list_chat_permissions_handler).post(api::add_chat_permission_handler),
        )
        .route(
            "/api/chats/:id/permissions/:entity_id",
            delete(api::remove_chat_permission_handler),
        )
        // Terminal API (WebSocket)
        .route(
            "/ws/terminal",
            get(crate::api::terminal::terminal_ws_handler),
        )
        // Paste/drop a file into the terminal: writes it under the user's home
        // and returns the path, which the frontend types at the cursor.
        .route(
            "/api/terminal/paste",
            post(crate::api::terminal::terminal_paste_handler)
                .layer(DefaultBodyLimit::max(25 * 1024 * 1024)),
        )
        // Yjs WebSocket (real-time collaborative editing)
        .route("/ws/yjs/:page_id", get(yjs_websocket_handler))
        // Blanket auth: all routes in this group require a resolved AuthUser
        // (proven iroh key / loopback console / dev fallback).
        .route_layer(middleware::from_extractor_with_state::<AuthUser, _>(state.clone()));

    // Merge public + protected, apply shared state and body limits, then
    // wrap in the security layers (response headers).
    let app = public_routes
        .merge(protected_routes)
        .with_state(state.clone())
        .layer(middleware::from_fn(crate::middleware::security::headers_layer))
        .layer(DefaultBodyLimit::max(105 * 1024 * 1024)); // 105MB (slightly above 100MB file limit for multipart overhead)

    // Add MCP routes to the same server
    let mcp_server = VirtuesMcpServer::new(client.database.pool().clone());
    let app = add_mcp_routes(app, mcp_server);

    // App-runtime proxy: forwards `/service/:action_id/*` to the supervised
    // localhost child. Has its own State (the supervisor), so we mount it
    // as a separate sub-router with its own .with_state(...) before merging.
    let service_proxy_routes = Router::new()
        .route(
            "/service/:action_id",
            axum::routing::any(crate::services::proxy::handle_service_proxy),
        )
        .route(
            "/service/:action_id/*path",
            axum::routing::any(crate::services::proxy::handle_service_proxy_rest),
        )
        .with_state(service_supervisor.clone());
    let app = app.merge(service_proxy_routes);

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

    // CORS: the mobile app is a bundled SPA at its own `tauri://` origin that
    // calls this API cross-origin over the iroh loopback. Auth is the proven
    // iroh key (not Origin/cookies), so a permissive CORS is safe — it only
    // relaxes the browser's same-origin policy, never the transport allowlist.
    // Same-origin (desktop/browser served by the box) is unaffected.
    let app = app.layer(
        tower_http::cors::CorsLayer::new()
            .allow_origin(tower_http::cors::Any)
            .allow_methods(tower_http::cors::Any)
            .allow_headers(tower_http::cors::Any),
    );

    // iroh reach: the box is an iroh Endpoint that serves this same axum app
    // (LAN-direct → hole-punch → our relay), reachable by EndpointId with no
    // public inbound port. Serves a clone of `app`; the :8000 TCP listener below
    // keeps serving LAN/loopback + the desktop :7117 helper. See `crate::relay`.
    crate::relay::maybe_spawn(client.database.pool().clone(), app.clone());

    let transport = build_transport(host, port);
    let listener = transport.bind().await?;

    tracing::info!("Server listening on {}", transport.describe());

    // DIY discovery aid: the operator ran `compose up` and knows their host, so
    // just point them at the web UI + the CLI dashboard. `0.0.0.0` means "all
    // interfaces" — they reach it at this box's LAN IP.
    {
        let shown = if host == "0.0.0.0" || host == "::" {
            format!("http://<this-box-ip>:{port}")
        } else {
            format!("http://{host}:{port}")
        };
        tracing::info!("Open the Virtues web UI at {shown}  ·  run `virtues status` for setup steps");
    }

    // Run the server with graceful shutdown — Ctrl+C / SIGTERM triggers
    // SIGTERM to all `app`-runtime children before we exit.
    let shutdown_supervisor = service_supervisor.clone();
    let shutdown_signal = async move {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutdown signal received");
        shutdown_supervisor.shutdown().await;
        // Give children ~3s to flush state on SIGTERM before we drop them
        // (which sends SIGKILL via `kill_on_drop(true)`).
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    };

    // Plain HTTP on :8000 is the only listener. The box has no TLS surface —
    // paired daemons reach the box over a WG tunnel (which provides encryption
    // + authentication), and the box's own browser hits localhost (Secure
    // Context per W3C, no cert required). See [[localhost-daemon-trust]] in
    // MEMORY.md for the architectural commitment.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal)
    .await?;

    // Note: No flush needed on shutdown - StreamWriter is in-memory only now.
    // Records are written directly to filesystem during sync/ingest.
    tracing::info!("Server shutting down gracefully");

    // Note: scheduler runs in background and will stop when the process exits
    // The handle is dropped here, but the task continues running

    Ok(())
}

/// Build the server transport for this build profile.
///
/// Selection is compile-time — the `dev-transport` feature flips to the
/// loopback-only dev profile. Release builds compile only the `real`
/// arm; the dev profile is not reachable at runtime.
#[cfg(feature = "dev-transport")]
fn build_transport(
    _host: &str,
    port: u16,
) -> Box<dyn virtues_helpers::transport::ServerTransport> {
    Box::new(virtues_helpers::transport::DevLocalServerTransport::new(port))
}

#[cfg(not(feature = "dev-transport"))]
fn build_transport(
    host: &str,
    port: u16,
) -> Box<dyn virtues_helpers::transport::ServerTransport> {
    Box::new(virtues_helpers::transport::RealServerTransport::new(host, port))
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
            "version": crate::codename::version(),
            "channel": crate::codename::channel(),
            "commit": env!("GIT_COMMIT"),
            "built_at": env!("BUILD_TIME"),
            "min_ios_version": min_ios_version,
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

