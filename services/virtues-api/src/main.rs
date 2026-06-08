//! virtues-api - Open Source API Proxy with Budget Enforcement
//!
//! A "prepaid arcade card" model for AI API access:
//! - Check budget in RAM (0ms latency) using DashMap + atomic floats
//! - Forward requests via litellm-rs (100+ providers)
//! - Optionally sync with Atlas orchestrator for production budgets
//!
//! Two modes:
//! - Standalone: Default budget for all users, usage tracking in RAM only
//! - Production: Hydrate budgets from Atlas on startup, report usage back
//!
//! This code is open source so users can verify we don't log their data.

mod auth;
mod bearer_auth;
mod blocklist;
mod atlas_hydrator;
mod config;
mod db;
mod entitlement;
mod providers;
mod proxy;
mod routes;
mod subscription;
mod sweeper;
mod tier;
pub mod version;
mod voucher;

use anyhow::Result;
use axum::{routing::get, Router};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::atlas_hydrator::AtlasHydrator;
use crate::blocklist::Blocklist;
use crate::config::Config;
use crate::subscription::SubscriptionManager;
use crate::tier::TierManager;
use crate::version::VersionCache;

/// Shared application state
pub struct AppState {
    pub config: Arc<Config>,
    pub atlas: AtlasHydrator,
    pub tier: TierManager,
    pub subscription: SubscriptionManager,
    pub version_cache: VersionCache,
    pub http_client: reqwest::Client,
    /// Postgres pool for WS-6b entitlement queries. `None` until the
    /// `VIRTUES_API_DATABASE_URL` env var is set — existing RAM paths
    /// continue to work in the meantime.
    pub db: Option<sqlx::PgPool>,
    /// Behavioral abuse blocklist (in-memory hot path, DB-snapshotted).
    pub blocklist: Blocklist,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "virtues_api=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration from root .env file (shared across all apps)
    // Try workspace root first, then fall back to current directory
    let root_env = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join(".env"));

    if let Some(env_path) = root_env {
        if env_path.exists() {
            dotenvy::from_path(&env_path).ok();
            tracing::debug!("Loaded .env from {}", env_path.display());
        }
    }
    // Also load local .env if present (for overrides)
    dotenvy::dotenv().ok();
    let config = Arc::new(Config::from_env()?);

    let mode = if config.has_atlas() { "production (Atlas sync)" } else { "standalone" };
    tracing::info!(
        "Starting virtues-api on port {} in {} mode",
        config.port,
        mode
    );

    // Log AI Gateway configuration
    tracing::info!(
        "Vercel AI Gateway: url={}",
        config.ai_gateway_url
    );

    // Log external services configuration
    tracing::info!(
        "External services: Exa={}, GooglePlaces={}, Plaid={}",
        config.exa_api_key.is_some(),
        config.google_api_key.is_some(),
        config.has_plaid()
    );

    // Initialize tier manager
    let tier = TierManager::new();

    // Initialize subscription manager
    let subscription = SubscriptionManager::new();

    // Initialize version cache (shared between route handlers)
    let version_cache = VersionCache::new();

    // Atlas hydrator: populates the tier + subscription managers from Atlas on
    // startup and on a periodic re-hydrate (catches trial expirations, plan
    // changes, cancellations). No metered budgets — paid calls charge the DB
    // entitlement wallet instead.
    let atlas = AtlasHydrator::new(&config, &tier, &subscription).await?;
    let atlas_rehydrator = atlas.clone();
    let rehydrate_interval = config.atlas_rehydrate_interval_secs;
    tokio::spawn(async move {
        atlas_rehydrator.run_rehydrator(rehydrate_interval).await;
    });

    // Build HTTP client for embeddings and other direct API calls
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300)) // 5 min for long completions
        .build()?;

    // Optional DB connect + migrate. Skip if env var is unset so the
    // existing RAM-budget paths still run while WS-6b refactor proceeds.
    let db = match std::env::var("VIRTUES_API_DATABASE_URL") {
        Ok(url) => Some(db::connect_and_migrate(&url).await?),
        Err(_) => {
            tracing::warn!(
                "VIRTUES_API_DATABASE_URL not set — running RAM-only; \
                 entitlement schema unavailable until WS-6b sets this"
            );
            None
        }
    };

    // Behavioral blocklist: in-memory hot path, snapshotted from the table.
    let blocklist = Blocklist::from_env();
    if let Some(pool) = db.as_ref() {
        blocklist.load_snapshot(pool).await;
    }
    blocklist.spawn_pruner();

    // Background housekeeping: reclaim expired entitlements + dead vouchers.
    if let Some(pool) = db.as_ref() {
        sweeper::spawn(pool.clone());
    }

    // Build shared state
    let state = Arc::new(AppState {
        config,
        atlas,
        tier,
        subscription,
        version_cache,
        http_client,
        db,
        blocklist,
    });

    // Build router
    let app = Router::new()
        // Health check (no auth required)
        .route("/health", get(routes::health::health_check))
        .route("/ready", get(routes::health::readiness_check))
        // Internal voucher registration (Atlas → virtues-api)
        .merge(routes::internal::router())
        // Device-facing voucher redemption
        .merge(routes::redeem::router())
        // OAuth proxy (google/notion/strava/plaid) — folded in from the Node
        // oauth-proxy (WS-4). Mounted at root: /{provider}/start|callback|...
        .merge(routes::oauth::router())
        // Blind rendezvous: PUT (bearer-authed) / GET (capability) endpoint
        // discovery. The only Virtues touchpoint in direct WireGuard access.
        .merge(routes::rendezvous::router())
        // WS-6b bearer-auth smoke endpoints (whoami + charge-test)
        .merge(routes::bearer_test::router())
        // Bearer-auth + entitlement charge: Places, Exa, Unsplash, AI.
        // AI (`/v1/ai/*`) covers both streaming and non-streaming chat; the
        // charge fires from a callback once the upstream usage is known. The
        // `/v1/services/*` proxies above still use the RAM budget until they
        // migrate to entitlement too.
        .merge(routes::places::router())
        .merge(routes::exa::router())
        .merge(routes::unsplash::router())
        .merge(routes::ai::router())
        // Connection limits (tier-based)
        .nest("/v1", routes::limits::router())
        // Subscription status and billing portal
        .nest("/v1", routes::subscription::router())
        // Version checking and updates
        .nest("/v1", routes::version::router())
        // Middleware
        .layer(TraceLayer::new_for_http())
        // F2: no CORS. virtues-api is a server-to-server sidecar (the home box
        // calls it with a Bearer or X-Internal-Secret header). No browser
        // should ever talk to it. The previous `allow_origin: Any +
        // allow_headers: Any` made any public web page a potential confused
        // deputy if a credential ever leaked into a browser context.
        .with_state(state.clone());

    // Start server
    let addr = format!("0.0.0.0:{}", state.config.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("virtues-api listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
