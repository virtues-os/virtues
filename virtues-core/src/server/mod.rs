//! HTTP server for data ingestion and API

pub mod api;
pub mod faces;
pub mod webhook;
pub mod yjs;

use axum::{
    extract::DefaultBodyLimit,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};

use std::env;
use std::sync::Arc;

// Re-exported: `AppState` is the handler state for the whole crate, and
// modules outside `server` (api::display, …) legitimately name it. Importing it
// privately here made `crate::server::AppState` fail to resolve for them.
pub use self::webhook::AppState;
use crate::error::Result;
use crate::middleware::auth::AuthUser;
use crate::Virtues;

/// Run the HTTP ingestion server with integrated scheduler
pub async fn run(client: Virtues, host: &str, port: u16) -> Result<()> {
    // Awaited before anything is spawned: the scheduler resolves cron
    // timezones from home_timezone and schedules the templates reconciled here.
    preflight(&client).await;

    // Yjs state is shared by the server and the scheduler.
    let yjs_state = yjs::YjsState::new(client.database.pool().clone());
    yjs_state.start_save_processor();
    tracing::info!("Yjs WebSocket server initialized");

    spawn_background(&client, &yjs_state);

    let app = build_app(build_state(&client, yjs_state.clone()));

    // iroh reach: the box is an iroh Endpoint that serves this same axum app
    // (LAN-direct → hole-punch → our relay), reachable by EndpointId with no
    // public inbound port. Serves a clone of the fully-layered `app`; the
    // :8000 TCP listener below keeps serving LAN/loopback + the desktop :7117
    // helper. See `crate::relay`.
    crate::relay::maybe_spawn(client.database.pool().clone(), app.clone());

    let transport = build_transport(host, port);
    let listener = transport.bind().await?;

    tracing::info!("Server listening on {}", transport.describe());

    // DIY discovery aid: the operator ran `compose up` and knows their host, so
    // just point them at the web UI + the CLI dashboard. `0.0.0.0` means "all
    // interfaces" — they reach it at this box's LAN IP.
    let shown = if host == "0.0.0.0" || host == "::" {
        format!("http://<this-box-ip>:{port}")
    } else {
        format!("http://{host}:{port}")
    };
    tracing::info!("Open the Virtues web UI at {shown}  ·  run `virtues status` for setup steps");

    // Plain HTTP on :8000 is the only listener. The box has no TLS surface —
    // paired daemons reach the box over iroh (which provides encryption
    // + authentication), and the box's own browser hits localhost (Secure
    // Context per W3C, no cert required). See [[localhost-daemon-trust]] in
    // MEMORY.md for the architectural commitment.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    // The Yjs save queue holds the owner's most recent typing for up to
    // ~2.5s; flush it so a restart or self-update doesn't drop it.
    yjs_state.flush_pending_saves().await;
    tracing::info!("Server shutting down gracefully");

    // The scheduler and other background tasks stop when the process exits.
    Ok(())
}

/// Best-effort startup checks, awaited in order. A failure is logged and the
/// box serves anyway.
async fn preflight(client: &Virtues) {
    let pool = client.database.pool();

    // Resolved, not re-derived — a log line that disagreed with the writer
    // would be worse than no log line at all.
    tracing::info!("Using storage path: {}", crate::storage::lake::lake_root().display());

    // Prove the lake is writable before serving. Every applet that ingests
    // anything writes here as the `virtues` user, and when the directory was
    // root-owned the only symptom was each applet failing with EACCES on its
    // own schedule — a 500 every five minutes, forever, that no health surface
    // asked about and no human saw. One probe at boot turns days of silent
    // failure into a line in the log at the moment it becomes true.
    //
    // A warning, not a refusal: the box still serves chat, search and the UI
    // without ingest, and refusing to boot would take away the surfaces someone
    // needs in order to FIX this. `is_healthy` carries the remedy.
    match crate::storage::Storage::file(
        crate::storage::lake::lake_root().display().to_string(),
    ) {
        Ok(storage) => match storage.health_check().await {
            Ok(h) if h.is_healthy => tracing::info!("{}", h.message),
            Ok(h) => tracing::error!("{}", h.message),
            Err(e) => tracing::error!(error = %e, "could not probe the lake for writability"),
        },
        Err(e) => tracing::error!(error = %e, "could not open the lake for a write probe"),
    }

    // Reap runs left in `running` by a crash/restart mid-execution, so a stale
    // lock doesn't survive a reboot. (The concurrency gate also age-bounds stale
    // runs at request time; this just keeps the runs table honest on boot.)
    match crate::scheduler::applets::cleanup_stale_runs(pool).await {
        Ok(n) if n > 0 => tracing::info!("Reaped {} stale 'running' action run(s) on startup", n),
        Ok(_) => {}
        Err(e) => tracing::warn!("Failed to reap stale action runs: {}", e),
    }

    // Auto-detect server readiness (skips setup screen if previously hydrated)
    if let Err(e) = crate::api::internal::ensure_server_status(pool).await {
        tracing::warn!("Failed to ensure server status: {}", e);
    }

    // Seed home_timezone from the box's own system clock once, before the
    // scheduler resolves cron timezones. Idempotent. See agents/record/timezone-model.md.
    if let Err(e) = crate::api::profile::ensure_home_timezone(pool).await {
        tracing::warn!("Failed to seed home_timezone: {}", e);
    }

    // Face-reader grants: idempotent default-deny SELECT surface for applet
    // faces (data_*/wiki_* tables + applet_* schemas). Best-effort.
    if let Err(e) = faces::ensure_applet_db_grants(pool).await {
        tracing::warn!("face reader grants failed: {e}");
    }

    // Eager identity bringup: ensure the loopback console device exists so the
    // box's own browser is authenticated. (The box's TLS identity is its own
    // cert, obtained at relay spawn; no keypair to mint here.)
    if let Err(e) = crate::middleware::auth::ensure_console_device(pool).await {
        tracing::warn!("identity bringup: ensure_console_device failed: {e}");
    }

    // Sanity-check the pgvector-backed search_vectors table is reachable.
    // Schema creation happens via 0008_search_and_vectors.sql; this probe just
    // confirms the migration ran.
    let search_engine = crate::search::SemanticSearchEngine::new(Arc::new(pool.clone()));
    if let Err(e) = search_engine.ensure_vec_table().await {
        tracing::warn!("Failed to probe search_vectors table: {}", e);
    }

    // Reconcile action templates from per-folder manifests — creates/updates
    // system action rows. Safe to call on every startup (user-managed runtime
    // state preserved).
    if let Err(e) = crate::applet_templates::reconcile_templates(pool).await {
        tracing::warn!("Failed to reconcile action templates: {}", e);
    }
}

/// Everything that runs beside the server for the life of the process.
fn spawn_background(client: &Virtues, yjs_state: &yjs::YjsState) {
    let pool = client.database.pool();

    // System telemetry: the Jetson GPU monitor (idle-gated tegrastats) and the
    // box-local time-series sampler (1/min → app_system_samples) behind the
    // System/Telemetry views. Both are no-ops/best-effort on non-Jetson hosts.
    crate::api::system_telemetry::start_gpu_monitor();
    crate::api::system_telemetry::start_system_sampler(pool.clone());

    // Model facts (prices, context windows, which ids still exist) are fetched
    // from virtues-api, never compiled in. Refreshes on boot and 6-hourly; an
    // unreachable cloud keeps the last snapshot rather than emptying the
    // picker. See api::model_catalog.
    crate::api::model_catalog::spawn(pool.clone());

    spawn_scheduler(pool.clone(), yjs_state.clone());

    // Auth-table sweeper: deletes expired pair tokens + sudo requests every
    // 10 minutes, archives `app_auth_event` rows older than 90 days. See
    // `crate::maintenance::sweeper`.
    crate::maintenance::sweeper::spawn(pool.clone());

    // Release preparation: on the stable channel, fetch + preflight the next
    // release ahead of time so installing it is a restart rather than a
    // download. Never activates anything — see `api::updates`.
    crate::api::updates::spawn();

    // Pair-code rotator: keeps a fresh universal standing pair code alive at all
    // times (with an overlap window) so the panel and `virtues pair` always have
    // a valid code to display. See `crate::maintenance::pair_rotator`.
    crate::maintenance::pair_rotator::spawn(pool.clone());

    // The box's own maps: daily, fetch the map squares its owner's life
    // covers and drop the ones it no longer does. See `crate::maps::sync`.
    crate::maps::sync::spawn(pool.clone());

    // Setup access point. An appliance arrives with no network and a display
    // its owner cannot type on, so the box raises its own wifi and the phone
    // does the typing. Up while unclaimed, down once a device pairs — NOT down
    // when the box gets wifi, which would drop the network the phone is still
    // sitting on mid-provision. No-op on a DIY box. See maintenance::setup_ap.
    crate::maintenance::setup_ap::spawn(pool.clone());

    // BLE wifi provisioning — the Improv service, and the PRIMARY setup path
    // (the AP above is the frozen fallback). Advertised while unclaimed, gone
    // once a device pairs. No-op on a DIY box and on non-Linux dev hosts. See
    // maintenance::ble_provision for the week of hardware findings that led
    // here.
    crate::maintenance::ble_provision::spawn(pool.clone());

    // The button behind the case. Held for three seconds, it forgets every
    // paired device — and nothing else: not the network, not the account, not
    // the data, and not the phrase. Anyone who can open the case can make that
    // nuisance; only someone holding the four words can then claim the box.
    // No-op off an appliance. See maintenance::reset_button.
    crate::maintenance::reset_button::spawn(pool.clone());

    spawn_review_pair_code(pool.clone());

    // Entity resolver: periodically turns raw lake primitives (location points,
    // transactions, calendar attendees) into ontology surfaces (visits/places,
    // merchant orgs, people) via `entity_resolution::resolve_entities`, so the
    // day page / timeline fill as the lake does. See `maintenance::entity_resolver`.
    crate::maintenance::entity_resolver::spawn(client.database.clone());

    // Timeline builder: rebuilds the Timeline's derived stays, drives, nights
    // and moments from the raw record on the same fast clock. See
    // `maintenance::timeline_builder`.
    crate::maintenance::timeline_builder::spawn(client.database.clone());

    // Hours — the screen's sleep schedule, enforced server-side because sleep
    // is a precedence state (a held button must wake dark glass). No-op off
    // an appliance. See api::system_display::sleep_engine.
    crate::api::system_display::sleep_engine::spawn(pool.clone());
}

fn spawn_scheduler(pool: sqlx::PgPool, yjs_state: yjs::YjsState) {
    tokio::spawn(async move {
        match crate::Scheduler::new(pool, yjs_state).await {
            Ok(mut sched) => {
                match sched.sync_jobs().await {
                    Ok(n) => tracing::info!("Scheduled {n} cron actions"),
                    Err(e) => tracing::warn!("Failed to schedule cron actions: {}", e),
                }
                if let Err(e) = sched.start().await {
                    tracing::warn!("Failed to start scheduler: {}", e);
                } else {
                    tracing::info!("Scheduler started successfully");
                    // Never returns: re-derives the job set on a timer (so a
                    // source connected while the box is running gets scheduled
                    // without a restart) and owns the JobScheduler, which must
                    // stay in scope for its jobs to keep firing.
                    sched.run_refresh_loop().await;
                }
            }
            Err(e) => {
                tracing::warn!("Failed to create scheduler: {}", e);
            }
        }
    });
}

/// Persistent review pair code, for App Store review boxes only. No-op
/// unless VIRTUES_REVIEW_PAIR_CODE is set, so customer boxes are untouched.
/// A failure here is loud but not fatal: a demo box that came up without
/// its code is useless to a reviewer, and the operator needs to see that,
/// but it must not take down a box that is otherwise healthy.
fn spawn_review_pair_code(pool: sqlx::PgPool) {
    tokio::spawn(async move {
        match crate::api::pair::ensure_review_code(&pool).await {
            Ok(Some(_)) => {
                tracing::warn!(
                    "REVIEW PAIR CODE ACTIVE — this box accepts a permanent pairing code. \
                     Only ever correct on a disposable box holding synthetic data."
                );
                // A review box is public and therefore behind a reverse
                // proxy, and that combination silently disarms the only
                // thing standing between a 6-digit code and a permanent
                // allowlisted device. `rate_limit_ip` believes
                // `X-Forwarded-For` only when VIRTUES_TRUSTED_PROXY is set;
                // otherwise `consume_handler` falls back to the socket peer,
                // which behind a proxy is loopback — and loopback is exempt
                // from the limiter by design. Net effect: unlimited guesses
                // at a 1M keyspace, with nothing logged and nothing to see.
                if !crate::middleware::trusted_proxy_configured() {
                    tracing::error!(
                        "REVIEW PAIR CODE IS UNRATE-LIMITED — VIRTUES_TRUSTED_PROXY is not \
                         set. If this box is public behind a reverse proxy, /api/pair/consume \
                         sees every request as loopback and the 10-per-IP-per-30-min limit \
                         never runs, so the code can be brute-forced. Set \
                         VIRTUES_TRUSTED_PROXY=1 and restart."
                    );
                }
            }
            Ok(None) => {}
            Err(e) => tracing::error!("review pair code not installed: {e:#}"),
        }
    });
}

fn build_state(client: &Virtues, yjs_state: yjs::YjsState) -> AppState {
    // Optional — fails gracefully if VIRTUES_API_INTERNAL_SECRET is not set.
    let tool_executor = crate::tools::ToolExecutor::from_env(client.database.pool().clone())
        .map(Arc::new)
        .ok();

    if tool_executor.is_some() {
        tracing::info!("ToolExecutor initialized successfully");
    } else {
        tracing::warn!("ToolExecutor not initialized - VIRTUES_API_INTERNAL_SECRET may not be set");
    }

    AppState {
        db: client.database.clone(),
        storage: client.storage.clone(),
        drive_config: crate::api::drive::DriveConfig::new(client.storage.clone()),
        tool_executor,
        yjs_state,
        // Stops in-progress chat requests.
        chat_cancel_state: crate::api::chat::ChatCancellationState::new(),
        ghost_permissions: crate::api::chat_permissions::GhostPermissions::new(),
        // Turns outlive their requests; this is where a client finds one to rejoin.
        live_turns: crate::api::live_turn::LiveTurns::new(),
    }
}

/// The whole HTTP app: both routers, the shared layers, the API 404s, the SPA
/// and CORS. What the TCP listener and the iroh endpoint both serve.
fn build_app(state: AppState) -> Router {
    // Merge public + protected, apply shared state and body limits, then
    // wrap in the security layers (response headers).
    let app = public_routes()
        .merge(protected_routes(&state))
        .with_state(state)
        .layer(middleware::from_fn(crate::middleware::security::headers_layer))
        .layer(DefaultBodyLimit::max(260 * 1024 * 1024)); // 260MB (slightly above 250MB file limit for multipart overhead)

    // API namespaces must NEVER fall through to the SPA fallback below: an
    // unknown /api path answered with a cacheable 200 index.html poisons
    // clients — the browser caches HTML against the API URL and keeps serving
    // it after the route ships (same failure class as the /health story in
    // apps/web/vite.config.ts). Unknown API routes are an honest JSON 404.
    let app = app
        .route("/api/*__unmatched", axum::routing::any(api_not_found_handler))
        .route("/auth/*__unmatched", axum::routing::any(api_not_found_handler));

    let app = match static_service() {
        Some(service) => app.fallback_service(service),
        None => app,
    };

    let app = app.layer(cors_layer());

    // Outermost, so every response — API, static, fallback — carries the build
    // stamp the SPA's staleness watcher compares.
    let app = app.layer(axum::middleware::from_fn(stamp_box_build));

    // Outside even that: the request span has to be open before any other
    // layer logs, or the first lines of a request are the ones without a key.
    app.layer(axum::middleware::from_fn(request_id))
}

/// The SvelteKit static build, falling back to 200.html for SPA routing, or
/// `None` when there is no build to serve.
///
/// The directory comes from `api::web_bundle` rather than being read again
/// here: `/api/web-bundle/version` describes whatever this serves, and two
/// copies of the same `STATIC_DIR` default were one edit away from making
/// that a lie.
fn static_service() -> Option<
    impl tower::Service<
            axum::extract::Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
            Future = impl Send + 'static,
        > + Clone
        + Send
        + 'static,
> {
    use tower_http::services::{ServeDir, ServeFile};

    let static_path = crate::api::web_bundle::static_dir();
    if !static_path.is_dir() {
        tracing::info!(
            "No static directory found at: {} - static serving disabled",
            static_path.display()
        );
        return None;
    }

    let fallback_file = static_path.join("200.html");
    // `precompressed_gzip`: the build writes a `.gz` beside every
    // compressible asset (apps/web/scripts/precompress.mjs), and ServeDir
    // hands that sibling to a client that accepts gzip and the original to
    // one that does not. The box never compresses at runtime; the Mac,
    // which fetches the SPA from the box on every cold start, moves ~0.8 MB
    // instead of ~2.6 MB.
    let serve_dir = if fallback_file.exists() {
        ServeDir::new(&static_path)
            .precompressed_gzip()
            .fallback(ServeFile::new(fallback_file))
    } else {
        // Try index.html as fallback if 200.html doesn't exist
        let index_file = static_path.join("index.html");
        ServeDir::new(&static_path)
            .precompressed_gzip()
            .fallback(ServeFile::new(index_file))
    };

    tracing::info!("Static file serving enabled from: {}", static_path.display());
    // HTML DOCUMENTS ARE NEVER CACHED. `ServeDir` sends `last-modified` and
    // no `cache-control`, which licenses a browser to cache heuristically —
    // and that once made the appliance's panel keep rendering a three-day-old
    // UI after an upgrade, through a service restart and a power cycle. The
    // shell names content-hashed JS chunks, so a stale shell resurrects the
    // entire stale page while the box serves the new one, and the only
    // symptom is a screen that quietly lies about its own version.
    //
    // The other half of that rule: `/_app/immutable/*` is content-hashed and
    // IS cached hard. The shell is the only thing that must be re-fetched,
    // because it is the thing that names the rest.
    Some(
        tower::ServiceBuilder::new()
            .layer(axum::middleware::from_fn(static_cache_policy))
            .service(serve_dir),
    )
}

/// CORS: the app is a bundled SPA at its own `tauri://` origin that calls
/// this API cross-origin over the iroh loopback, so some cross-origin access
/// must be allowed. It is an ALLOWLIST, not `Any`.
///
/// There are two ways to be the owner:
///
///   1. a paired iroh key, and
///   2. being on loopback (`middleware/auth.rs` — a request from 127.0.0.1
///      with no forwarding header IS the owner).
///
/// And the desktop app binds `127.0.0.1:7117` and splices whatever connects
/// to it over its own paired identity. So with `Any`, the owner runs the app,
/// then visits any web page — an ad, a forum, a compromised site — and that
/// page's `fetch('http://127.0.0.1:7117/api/drive/files')` is authenticated
/// as the owner and its reply readable; with `allow_methods(Any)` +
/// `allow_headers(Any)` preflighted POSTs succeed too, including
/// `/api/developer/sql`.
///
/// A remote page's origin is `https://whatever.example`, which matches none
/// of the arms below, so the browser refuses to hand it the response. The
/// app, the box's own web UI, and local development all still match.
///
/// `server/faces.rs` keeps its own `*` header deliberately: faces are served
/// into an opaque-origin iframe under a strict CSP and carry no ambient
/// authority. `api/terminal.rs` does an explicit same-origin check for the
/// same reason this layer exists.
fn cors_layer() -> tower_http::cors::CorsLayer {
    tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(
            |origin: &axum::http::HeaderValue, req| {
                origin.to_str().is_ok_and(|o| {
                    // A face lives in `<iframe sandbox="allow-scripts">`,
                    // whose opaque origin serializes as the literal
                    // "null". This layer answers the CORS preflight before
                    // faces.rs's own `*` header can, so without this arm
                    // the sandboxed face's fetch to its bridge is refused
                    // and the panel silently renders no data. Scoped to
                    // the face routes only: a face carries no ambient
                    // authority (face token + face_reader role), and
                    // everything else keeps rejecting "null".
                    face_origin_allowed(
                        o,
                        req.uri.path(),
                        req.headers
                            .get(axum::http::header::HOST)
                            .and_then(|h| h.to_str().ok()),
                    )
                })
            },
        ))
        .allow_credentials(false)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any)
        // Cross-origin callers (the app's own tauri:// origin) may READ the
        // build stamp — it's how a page notices the box moved under it.
        .expose_headers([axum::http::HeaderName::from_static(
            "x-virtues-box-build",
        )])
}

/// Ctrl+C / SIGTERM. SIGTERM is the one that matters: systemd sends it on
/// every `systemctl restart virtues` (so every self-update), and its default
/// action kills the process outright, skipping the Yjs flush after `serve`.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            // Registering the handler failed — fall back rather than
            // refusing to start. A box that cannot shut down cleanly is
            // still better than a box that will not run.
            Err(e) => {
                tracing::warn!(error = %e, "could not listen for SIGTERM; Ctrl+C only");
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    tracing::info!("shutdown signal received");
}

/// The unauthenticated surface. Kept in one function so it can be reviewed
/// as a whole: everything here answers without an `AuthUser`.
fn public_routes() -> Router<AppState> {
    Router::new()
        // Health check
        .route("/health", get(health))
        // Public, LAN-reachable box health — boot gates + inference resolution.
        // No secrets; the first-run web page / appliance screen poll this
        // before any owner session exists. Full identity detail stays in the
        // `virtues status` CLI.
        .route(
            "/api/box/health",
            get(crate::api::box_status::box_health_handler),
        )
        // Who is this box — name + claimed, for discovery chips. Public like
        // its neighbours; the name is already broadcast over the air (AP SSID,
        // BLE advertisement), so the LAN learns nothing new. See api/identity.
        .route(
            "/api/box/identity",
            get(crate::api::identity::identity_handler),
        )
        // Setup/onboarding state machine (agents/build/onboarding.md) — public-on-LAN
        // for the same reason as /api/box/health: the wizard + panel render it
        // pre-auth, and it carries only booleans + step copy.
        .route(
            "/api/setup/state",
            get(crate::api::box_status::setup_state_handler),
        )
        // Authenticated by the handler's own `AuthUser`, like the
        // getting-started routes below.
        .route(
            "/api/setup/skip-onboarding",
            post(crate::api::box_status::skip_onboarding_handler),
        )
        // The rules the assistant must obey (`<rules>` in the prompt). Read to
        // review them, POST to replace the set. The only writer of wiki_rules.
        // Authenticated by each handler's own `AuthUser`.
        .route(
            "/api/narrative/rules",
            get(crate::api::narrative_draft::rules_handler)
                .post(crate::api::narrative_draft::save_rules_handler),
        )
        // Getting started, derived: the four steps, the lock, the first day.
        // Authenticated, unlike /api/setup/state — it reads the profile — by
        // an `AuthUser` in each handler rather than the route_layer.
        .route(
            "/api/getting-started",
            get(crate::api::getting_started::state_handler),
        )
        .route(
            "/api/getting-started/skip",
            post(crate::api::getting_started::skip_handler),
        )
        .route(
            "/api/getting-started/interview",
            post(crate::api::getting_started::start_interview_handler),
        )
        // What the attached 7" display renders. Registered here because the
        // kiosk draws before any device is paired, but UNLIKE its neighbours
        // above it carries the live pair code — so the handler itself refuses
        // anything that isn't loopback. Proximity is the authority: a stranger
        // on the wifi who cannot see the screen must not be able to claim the
        // box. See api/display.rs.
        .route(
            "/api/display/state",
            get(crate::api::display::display_state_handler),
        )
        // Lets the panel latch "an upgrade is running" while this server is
        // still up to say so — after it stops, the kiosk's page is gone with
        // it. See api/display.rs.
        .route(
            "/api/display/updating",
            get(crate::api::display::display_updating_handler),
        )
        // The case button at 1s cadence — the 30s ambient poll cannot see a
        // 3s hold. See api/display.rs.
        .route(
            "/api/display/button",
            get(crate::api::display::display_button_handler),
        )
        // Wifi provisioning over the setup AP. The one unauthenticated WRITE
        // surface on the box, and unauthenticated by necessity: the phone that
        // just joined the AP has no credential yet, because obtaining one is
        // what the rest of onboarding is for. Each handler re-checks both gates
        // itself — caller is on the AP subnet (or loopback), and the box is
        // still unclaimed — rather than trusting placement in this router.
        // See api/provision.rs.
        .route(
            "/api/provision/networks",
            get(crate::api::provision::networks_handler),
        )
        .route(
            "/api/provision/join",
            post(crate::api::provision::join_handler),
        )
        .route(
            "/api/provision/status",
            get(crate::api::provision::status_handler),
        )
        // No browser provisioning (`/portal`, `/provision`), on purpose: pairing
        // needs a held iroh key a browser cannot have, and iOS captive sheets
        // misbehave. BLE (`maintenance::ble_provision`) and `/api/network/*` do it.
        // Auth — pair-only model. Public consume + session probe (returns the
        // AuthUser resolved from the request's proven iroh key, if any).
        // /api/pair/{mint,confirm,deny,status} are auth'd and live under the
        // protected_routes block below.
        .route(
            "/api/pair/consume",
            post(crate::api::pair::consume_handler),
        )
        .route("/auth/session", get(api::chat::auth_session_handler))
        // Applet faces — the CORS-permissive, token-gated leaves only. The
        // mint route is AUTHENTICATED (in protected_routes): the token is the
        // sole gate on the data door, so obtaining one must require owner auth.
        // The query bridge validates the token; the file routes serve inert
        // assets. These carry no data without a token minted by the authed app.
        .route(
            "/api/face/query",
            post(faces::face_query_handler).options(faces::face_query_preflight),
        )
        .route("/face/:applet_id/", get(faces::face_index_handler))
        .route("/face/:applet_id/*path", get(faces::face_file_handler))
        // Public page sharing (token-based access, no session needed)
        .route("/api/s/:token", get(api::pages::get_shared_page_handler))
        .route(
            "/api/s/:token/files/:file_id",
            get(api::pages::shared_file_download_handler),
        )
        // Webhook ingestion. Authenticated by the proven iroh key — the
        // handler takes a hard `AuthUser`, so an unauthenticated caller is
        // rejected there rather than by the route_layer.
        // Per-route body limit override (router-wide cap is 260MB): iOS audio
        // batches are base64 AAC and can dwarf the other streams on backfill.
        // A body over the cap is rejected by the Json extractor before the
        // handler runs, which historically surfaced as a bogus "no stream
        // selector" action error. See webhook.rs for the rejection handling.
        .route(
            "/webhook/:applet_id",
            post(webhook::webhook).layer(DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        // Device re-fetch for stream → applet_id map. Used by paired devices
        // whose Keychain entry predates the webhook unification, or after
        // templates.toml adds a new stream. Authenticated like the webhook:
        // a hard `AuthUser` in the handler.
        .route(
            "/api/devices/applet-ids",
            get(api::applets::device_applet_ids_handler),
        )
}

/// Everything that requires a resolved `AuthUser` (proven iroh key /
/// loopback console / dev fallback): each area's `routes()`, plus the
/// devices, pairing, sudo, audit, maps and terminal routes that belong to no
/// area.
fn protected_routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .merge(api::applets::routes())
        .merge(api::settings::routes())
        .merge(api::wiki::routes())
        .merge(api::chat::routes())
        .merge(api::drive::routes())
        .merge(api::pages::routes())
        .merge(api::library::routes())
        // The box's own maps: tiles out of local .pmtiles archives, never a
        // tile provider. agents/plan/offline-maps-plan.md
        .route("/api/map/sources", get(crate::maps::sources_handler))
        .route("/api/map/vt/:tier/:z/:x/:y", get(crate::maps::tile_handler))
        .route("/api/map/fonts/:fontstack/:range", get(crate::maps::glyphs_handler))
        .route("/api/map/sprite/:file", get(crate::maps::sprite_handler))
        // ─── Pair-only auth: "+ Add device" from a paired session ─────
        .route("/api/pair/mint",          post(crate::api::pair::mint_handler))
        .route("/api/pair/mint-collector", post(crate::api::pair::mint_collector_handler))
        .route("/api/pair/status/:id",    get(crate::api::pair::status_handler))
        .route("/api/pair/deny/:id",      post(crate::api::pair::deny_handler))
        // Re-open onboarding: revoke every device, keep the data — a box-wide
        // action a paired device may take, guarded by being paired.
        .route(
            "/api/pair/reopen-onboarding",
            post(crate::api::pair::reopen_onboarding_handler),
        )
        // ─── Devices: unified list + revoke ───────────────────────────
        .route("/api/devices",            get(crate::api::devices::list_handler))
        .route("/api/devices/self/push-address", post(crate::api::devices::set_self_push_address))
        .route("/api/devices/self/reach",   get(crate::api::devices::get_self_reach))
        .route("/api/devices/enroll-peer",  post(crate::api::devices::enroll_peer))
        .route("/api/devices/:id",        axum::routing::delete(crate::api::devices::revoke_handler))
        // ─── Sudo: gate for high-sensitivity actions ──────────────────
        .route("/api/sudo/request",       post(crate::api::sudo::request_handler))
        .route("/api/sudo/status/:id",    get(crate::api::sudo::status_handler))
        // ─── Audit log ────────────────────────────────────────────────
        // No UI calls this; it is the one way to read app_auth_event, which
        // agents/record/auth-model.md relies on. Not dead code.
        .route("/api/audit/auth",         get(crate::api::audit::list_handler))
        // ─── Client reports ───────────────────────────────────────────
        // The one door a paired device reports its OWN failures through; they
        // become lines in this box's journal. Body limit is small on purpose:
        // this takes diagnostics, and anything larger is a client shipping a
        // document. See `api/events.rs`.
        .route(
            "/api/events",
            post(crate::api::events::report_handler)
                .layer(DefaultBodyLimit::max(64 * 1024)),
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
        // Blanket auth: all routes in this group require a resolved AuthUser
        // (proven iroh key / loopback console / dev fallback). `route_layer`
        // covers only the routes present when it is called, so every merge
        // and route above must stay above it.
        .route_layer(middleware::from_extractor_with_state::<AuthUser, _>(state.clone()))
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

/// Stamp `Cache-Control: no-store` on every HTML document the static server
/// hands out, leaving hashed assets alone.
///
/// See the call site for the incident. Short version: `ServeDir` sends
/// `last-modified` and no `cache-control`, a browser may then cache
/// heuristically, and the appliance's kiosk did — pinning the panel to a
/// three-day-old UI across an upgrade, a service restart, and a power cycle.
///
/// Documents are keyed on the response's own content type rather than the
/// request path, so the rule covers the SPA fallback (`200.html`, served for
/// arbitrary routes) without having to enumerate which paths are documents.
///
/// Hashed assets are keyed on the request path, because that is what makes
/// them safe to cache for a year: a byte of change is a new name, and the
/// (never cached) shell is what names them. Fonts are not hashed, so they get
/// a week and revalidate on `last-modified` after that.
async fn static_cache_policy(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let path = req.uri().path().to_owned();
    let mut res = next.run(req).await;
    let is_document = res
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/html"));
    if is_document {
        res.headers_mut().insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        );
        // A validator left beside `no-store` is a mixed message, and some
        // caches honour the weaker half.
        res.headers_mut().remove(axum::http::header::LAST_MODIFIED);
        res.headers_mut().remove(axum::http::header::ETAG);
    } else if res.status().is_success() {
        if let Some(policy) = static_cache_control(&path) {
            res.headers_mut().insert(
                axum::http::header::CACHE_CONTROL,
                axum::http::HeaderValue::from_static(policy),
            );
        }
    }
    res
}

/// The `cache-control` a successful non-document static response gets, by path.
fn static_cache_control(path: &str) -> Option<&'static str> {
    if path.starts_with("/_app/immutable/") {
        Some("public, max-age=31536000, immutable")
    } else if path.starts_with("/fonts/") {
        Some("public, max-age=604800")
    } else {
        None
    }
}

#[cfg(test)]
mod request_id_tests {
    use super::*;
    use axum::{routing::get, Router};
    use tower::Service;

    async fn probe(inbound: Option<&str>) -> String {
        let mut app = Router::new()
            .route("/x", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(request_id));
        let mut req = axum::http::Request::builder().uri("/x");
        if let Some(v) = inbound {
            req = req.header("x-request-id", v);
        }
        let res = app
            .call(req.body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        res.headers()
            .get("x-request-id")
            .expect("every response carries one")
            .to_str()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn mints_one_when_the_client_sends_none() {
        let a = probe(None).await;
        let b = probe(None).await;
        assert!(a.starts_with('r'), "unexpected shape: {a}");
        assert_ne!(a, b, "two requests must not share an id");
    }

    #[tokio::test]
    async fn honors_a_client_supplied_id() {
        assert_eq!(probe(Some("abc-123_XYZ")).await, "abc-123_XYZ");
    }

    /// The header goes back out in a response and into a log line, so a
    /// caller-controlled value is bounded and filtered rather than trusted.
    ///
    /// Not tested here: CR/LF injection, the classic attack on a value that
    /// reaches a log line. `http` refuses to construct such a header at all —
    /// this test could not even build the request — so it never reaches this
    /// middleware. The filter still earns its place on quoting and spacing,
    /// which are perfectly legal in a header value and ugly in a log field.
    #[tokio::test]
    async fn strips_junk_and_bounds_length() {
        assert_eq!(probe(Some(r#""quoted; id" 42"#)).await, "quotedid42");
        let long = "a".repeat(500);
        assert_eq!(probe(Some(&long)).await.len(), 64);
        // Nothing usable left → mint instead of returning an empty header.
        assert!(probe(Some("!!!")).await.starts_with('r'));
    }
}

#[cfg(test)]
mod static_cache_policy_tests {
    use super::*;
    use axum::{routing::get, Router};
    // `tower::Service` alone: the crate does not enable tower's `util`
    // feature, and a Router is always ready, so `call` needs no `oneshot`.
    use tower::Service;

    async fn probe(path: &str, content_type: &'static str) -> axum::http::HeaderMap {
        let mut app = Router::new()
            .route(
                path,
                get(move || async move {
                    (
                        [
                            (axum::http::header::CONTENT_TYPE, content_type),
                            (axum::http::header::LAST_MODIFIED, "Mon, 01 Jan 2024 00:00:00 GMT"),
                        ],
                        "x",
                    )
                }),
            )
            .layer(axum::middleware::from_fn(static_cache_policy));
        let res = app
            .call(
                axum::http::Request::builder()
                    .uri(path)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        res.headers().clone()
    }

    fn cache(h: &axum::http::HeaderMap) -> Option<&str> {
        h.get(axum::http::header::CACHE_CONTROL).and_then(|v| v.to_str().ok())
    }

    #[tokio::test]
    async fn documents_are_never_cached_wherever_they_live() {
        let h = probe("/anything/at/all", "text/html; charset=utf-8").await;
        assert_eq!(cache(&h), Some("no-store"));
        assert!(h.get(axum::http::header::LAST_MODIFIED).is_none());
    }

    #[tokio::test]
    async fn hashed_chunks_are_immutable_for_a_year() {
        let h = probe("/_app/immutable/chunks/Cd0FKR9-.js", "text/javascript").await;
        assert_eq!(cache(&h), Some("public, max-age=31536000, immutable"));
        // The validator stays: harmless beside `immutable`, useful to a proxy.
        assert!(h.get(axum::http::header::LAST_MODIFIED).is_some());
    }

    #[tokio::test]
    async fn fonts_get_a_week_and_revalidate() {
        let h = probe("/fonts/EBGaramond-Regular-latin.woff2", "font/woff2").await;
        assert_eq!(cache(&h), Some("public, max-age=604800"));
    }

    #[tokio::test]
    async fn other_static_files_keep_the_default() {
        let h = probe("/favicon.png", "image/png").await;
        assert_eq!(cache(&h), None);
    }

    /// The real serving stack: ServeDir with a `.gz` sibling on disk hands the
    /// sibling to a client that accepts gzip, the original to one that does
    /// not, and the immutable header rides on both.
    #[tokio::test]
    async fn precompressed_sibling_is_served_by_negotiation() {
        use tower_http::services::ServeDir;
        let dir = std::env::temp_dir().join(format!("virtues-static-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("_app/immutable/chunks")).unwrap();
        let raw = b"console.log('raw')";
        std::fs::write(dir.join("_app/immutable/chunks/a.js"), raw).unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        std::io::Write::write_all(&mut gz, raw).unwrap();
        let gz = gz.finish().unwrap();
        std::fs::write(dir.join("_app/immutable/chunks/a.js.gz"), &gz).unwrap();

        let mut app = Router::new().fallback_service(
            tower::ServiceBuilder::new()
                .layer(axum::middleware::from_fn(static_cache_policy))
                .service(ServeDir::new(&dir).precompressed_gzip()),
        );

        let req = |accept: Option<&'static str>| {
            let mut b = axum::http::Request::builder().uri("/_app/immutable/chunks/a.js");
            if let Some(a) = accept {
                b = b.header(axum::http::header::ACCEPT_ENCODING, a);
            }
            b.body(axum::body::Body::empty()).unwrap()
        };

        let res = app.call(req(Some("gzip, br"))).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::OK);
        assert_eq!(
            res.headers().get(axum::http::header::CONTENT_ENCODING).map(|v| v.to_str().unwrap()),
            Some("gzip")
        );
        assert_eq!(cache(res.headers()), Some("public, max-age=31536000, immutable"));
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], &gz[..], "the sibling's bytes, untouched");

        let res = app.call(req(None)).await.unwrap();
        assert!(res.headers().get(axum::http::header::CONTENT_ENCODING).is_none());
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], raw);

        std::fs::remove_dir_all(&dir).ok();
    }
}

/// Stamp every response with the running build identity, so an open page can
/// notice the box changed underneath it.
///
/// After an upgrade the flipped `web/` slot no longer contains the OLD page's
/// content-hashed `/_app/immutable/*` chunks, so that page's first lazy
/// navigation 404s — and nothing told any connected surface to reload: only
/// the tab that pressed the update button (`location.reload()`) and the kiosk
/// (`restart_display`) recovered. Browsers on other machines and the Mac
/// webview kept a page whose chunks were gone. The SPA watches this header
/// across its own requests and soft-reloads from the background when it moves
/// (see `$lib/build.ts`).
/// Give every request an id, put it on a span so all downstream lines inherit
/// it, and hand it back on the response.
///
/// The header is the half that makes this usable from outside: a client that
/// saw a failure can quote `x-request-id`, and that string alone finds every
/// line the box logged while serving it. Without it the id would be a fact the
/// box knows and nobody can ask for.
///
/// An inbound `x-request-id` is honored rather than replaced — a client (or a
/// future proxy) that already minted one is trying to correlate across a hop,
/// and overwriting it would break exactly the case the header exists for. It
/// is bounded and sanitized first: this value goes into a log line and back
/// out in a header, and unbounded caller-controlled text in either is how you
/// get log injection.
async fn request_id(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use tracing::Instrument;

    let inbound = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| {
            s.chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .take(64)
                .collect::<String>()
        })
        .filter(|s| !s.is_empty());
    let id = inbound.unwrap_or_else(crate::observe::new_request_id);

    let method = req.method().as_str().to_owned();
    // The path, never the query string: query strings carry search terms and
    // ids, and this lands in a log line.
    let path = req.uri().path().to_owned();

    let span = crate::observe::request_span(&id, &method, &path);
    let mut res = next.run(req).instrument(span).await;

    if let Ok(value) = axum::http::HeaderValue::from_str(&id) {
        res.headers_mut().insert("x-request-id", value);
    }
    res
}

async fn stamp_box_build(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    static VALUE: std::sync::OnceLock<axum::http::HeaderValue> = std::sync::OnceLock::new();
    let value = VALUE.get_or_init(|| {
        let commit: &str = env!("GIT_COMMIT");
        let short = &commit[..commit.len().min(7)];
        axum::http::HeaderValue::from_str(&format!("{} {}", crate::codename::version(), short))
            .unwrap_or(axum::http::HeaderValue::from_static("unknown"))
    });
    let mut res = next.run(req).await;
    res.headers_mut()
        .insert("x-virtues-box-build", value.clone());
    res
}

/// Honest 404 for unknown /api and /auth paths — see the comment where this
/// is routed. `no-store` so a transient miss can never poison an HTTP cache.
async fn api_not_found_handler(uri: axum::http::Uri) -> impl IntoResponse {
    (
        axum::http::StatusCode::NOT_FOUND,
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        axum::Json(serde_json::json!({
            "error": "not_found",
            "path": uri.path(),
        })),
    )
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
            // What a UI compares against its floor: see crate::api_version.
            "api_version": crate::api_version::API_VERSION,
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

/// The routes a sandboxed (opaque-origin) applet face is allowed to reach:
/// its own static files and the query bridge. See the CORS predicate.
fn is_face_path(path: &str) -> bool {
    path.starts_with("/api/face/") || path.starts_with("/face/")
}

/// The CORS predicate, named so it can be tested: our own origins anywhere,
/// and the opaque origin `null` only on the face routes. `request_host` is the
/// request's `Host` header — a loopback origin must be that authority.
fn face_origin_allowed(origin: &str, path: &str, request_host: Option<&str>) -> bool {
    origin_is_ours(origin, request_host) || (origin == "null" && is_face_path(path))
}

/// Is this `Origin` one of ours?
///
/// The allowlist behind the CORS layer. Kept as a named function with tests
/// because it is the thing standing between a random web page and the owner's
/// record, and a subtle parsing slip here is invisible in review.
///
/// Allowed: the app's `tauri://` origin, loopback on any port (the desktop
/// proxy on 7117, the box's own UI, `pnpm dev` on 5173), and the box's `.virtues`
/// name. A page served from a remote host has none of these origins.
pub(crate) fn origin_is_ours(origin: &str, request_host: Option<&str>) -> bool {
    // The app's own origin: `tauri://localhost` on macOS/iOS,
    // `https://tauri.localhost` on Windows — and `virtues://` on the phone,
    // which registers its OWN scheme so an OTA bundle can answer requests
    // (see apps/web/src-tauri/src/lib.rs; the window opens at
    // `virtues://localhost/connect.html`).
    //
    // Missing `virtues://` here silently broke every data request the iOS app
    // made from 2026-08-18 until 2026-08-28: the box answered 200 and omitted
    // `Access-Control-Allow-Origin`, so WebKit discarded the response and the
    // app reported "Load failed" while the box's own logs showed the device
    // authenticating perfectly. Ten days, because the symptom looks like a
    // network fault and every trace says the network is fine.
    //
    // Safe for the same reason `tauri://` is: a custom scheme can only be
    // claimed by an installed app, so no remote page can present this origin.
    if origin.starts_with("tauri://")
        || origin.starts_with("virtues://")
        || origin == "https://tauri.localhost"
    {
        return true;
    }

    // Everything else must be an http(s) origin; anything else (file://, data:,
    // a bare "null") is not ours.
    let rest = match origin.split_once("://") {
        Some(("http", r)) | Some(("https", r)) => r,
        _ => return false,
    };

    // Strip the port. An IPv6 literal is bracketed (`[::1]:8000`), so splitting
    // on the LAST colon leaves the brackets intact and does not cut inside the
    // address.
    let host = match rest.rsplit_once(':') {
        // Only treat the tail as a port if it looks like one; otherwise the
        // colon belonged to the host (an unbracketed IPv6, which is invalid in
        // an origin anyway).
        Some((h, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => h,
        _ => rest,
    };

    // Exact matches only. `localhost.evil.example` must NOT pass, which is why
    // this is not a `contains` or a suffix test.
    if matches!(host, "localhost" | "127.0.0.1" | "[::1]") {
        // Loopback on ANY port used to pass. That was a hole, not a
        // convenience: the desktop app splices 127.0.0.1:7117 to the box as
        // the owner, so a page served by any other local process — a dev
        // server, a Jupyter notebook, another app's UI — could call it and read the
        // reply. The app's own pages are always served by the authority they
        // dial, so a loopback origin must equal the request's Host, exactly.
        return request_host == Some(rest);
    }
    host == "virtues" || host.ends_with(".virtues")
}

#[cfg(test)]
mod cors_tests {
    use super::{face_origin_allowed, origin_is_ours};

    /// A remote page must not be allowed to read the box's responses.
    ///
    /// This is the CRITICAL case. The desktop app binds 127.0.0.1:7117 and
    /// splices to the box over its paired identity, and loopback counts as the
    /// owner — so with `allow_origin(Any)` any site the owner visited could
    /// `fetch()` the box and READ the reply.
    #[test]
    fn a_remote_page_is_refused() {
        for o in [
            "https://evil.example",
            "http://evil.example:7117",
            // Near-misses that a substring or suffix test would wave through.
            "http://localhost.evil.example",
            "https://tauri.localhost.evil.example",
            "http://notvirtues",
            // Near-misses on the scheme itself: only the exact `virtues://`
            // prefix is ours, never a host or path that merely contains it.
            "https://virtues.evil.example",
            "http://evil.example/virtues://",
            "http://evil.example/localhost",
            // Non-http schemes and the opaque origin.
            "null",
            "file://",
            "data:text/html,x",
        ] {
            assert!(!origin_is_ours(o, Some("127.0.0.1:7117")), "must refuse {o}");
        }
    }

    /// ...while everything that is genuinely ours still works, or the app
    /// breaks and someone reverts the whole fix.
    #[test]
    fn our_own_origins_are_allowed() {
        for (o, host) in [
            ("tauri://localhost", None),
            // The iOS app's own scheme. Absent from this list until
            // 2026-08-28, which is exactly how the phone lost every data
            // request for ten days without a single test going red.
            ("virtues://localhost", None),
            ("https://tauri.localhost", None),
            // Loopback: the page is served by the authority it dials.
            ("http://localhost:5173", Some("localhost:5173")),
            ("http://127.0.0.1:7117", Some("127.0.0.1:7117")),
            ("http://[::1]:8000", Some("[::1]:8000")),
            ("http://localhost", Some("localhost")),
            ("http://box.virtues:8000", None),
            ("http://virtues:8000", None),
        ] {
            assert!(origin_is_ours(o, host), "must allow {o} for host {host:?}");
        }
    }

    /// Loopback on a DIFFERENT port is another process's page, not ours.
    /// The desktop splice on 7117 is the owner; a dev server on 8888 is not.
    #[test]
    fn a_loopback_origin_on_another_port_is_refused() {
        for (o, host) in [
            ("http://localhost:8888", Some("127.0.0.1:7117")),
            ("http://127.0.0.1:8888", Some("127.0.0.1:7117")),
            ("http://127.0.0.1:7117", Some("127.0.0.1:8000")),
            ("http://127.0.0.1:7117", None),
        ] {
            assert!(!origin_is_ours(o, host), "must refuse {o} for host {host:?}");
        }
    }

    /// The face is hung in `<iframe sandbox="allow-scripts">`, so its origin
    /// is the literal "null". It must clear CORS on its own routes: the panel
    /// drew background stars and nothing else from cohort launch to
    /// 2026-09-03 because it didn't, and no test went red.
    #[test]
    fn the_sandboxed_face_reaches_its_own_routes() {
        for p in [
            "/face/applet_dot_cloud/",
            "/face/applet_dot_cloud/virtues.js",
            "/api/face/query",
        ] {
            assert!(face_origin_allowed("null", p, None), "must allow null on {p}");
        }
    }

    /// ...and nowhere else. A sandboxed or file:// page still cannot read
    /// the box. Near-misses included: only the exact prefixes are face routes.
    #[test]
    fn null_is_refused_everywhere_else() {
        for p in [
            "/api/status",
            "/api/applets/applet_dot_cloud/face-token",
            "/display",
            "/api/faces/x",
            "/faces/x",
            "/face",
        ] {
            assert!(!face_origin_allowed("null", p, None), "must refuse null on {p}");
        }
    }
}

/// Route paths must be spelled for the axum this crate actually depends on.
///
/// axum 0.8 captures a path parameter as `{id}`; 0.7 — what we are on — spells
/// it `:id` and treats braces as ordinary characters. So a 0.8-style path is
/// not a compile error and not a warning: it registers a literal route named
/// after the parameter, and every real id falls through to the fallback as a
/// 404. `/api/bookmarks/{id}` shipped that way and every bookmark's detail page
/// answered "Failed to load bookmark: 404" until 2026-09-21.
///
/// The models that write most of this code are fluent in the newer syntax, so
/// this will be attempted again. A text scan is the right shape of check: a
/// Router cannot be asked what paths it holds, and the mistake is legible in
/// the source and nowhere else.
#[cfg(test)]
mod route_syntax_tests {
    use std::path::{Path, PathBuf};

    fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read src dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                rs_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    #[test]
    fn no_axum_0_8_path_params() {
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rs_files(&src, &mut files);

        // Built rather than written so this test's own prose cannot trip it.
        let needle = format!(".{}(\"", "route");
        let mut offenders = Vec::new();

        for file in &files {
            let text = std::fs::read_to_string(file).expect("read source");
            for (n, line) in text.lines().enumerate() {
                let Some(rest) = line.split_once(&needle).map(|(_, r)| r) else {
                    continue;
                };
                let Some(path) = rest.split('"').next() else {
                    continue;
                };
                if path.contains('{') {
                    offenders.push(format!(
                        "{}:{} — {path}",
                        file.strip_prefix(&src).unwrap_or(file).display(),
                        n + 1
                    ));
                }
            }
        }

        assert!(
            offenders.is_empty(),
            "axum 0.7 path parameters are `:name`, not `{{name}}`. A braced path \
             registers a literal route and 404s every real id:\n  {}",
            offenders.join("\n  ")
        );
    }
}
