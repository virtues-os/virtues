//! Tollbooth - Open Source API Proxy with Budget Enforcement
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
mod budget;
mod config;
mod models;
mod providers;
mod proxy;
mod routes;

use anyhow::Result;
use axum::{routing::get, Router};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::budget::BudgetManager;
use crate::config::Config;

/// Shared application state
pub struct AppState {
    pub config: Arc<Config>,
    pub budget: BudgetManager,
    pub http_client: reqwest::Client,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tollbooth=info,tower_http=debug".into()),
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

    // Initialize model registry (from config file or defaults)
    models::init(config.models_config_path.as_deref())?;

    let mode = if config.has_atlas() { "production (Atlas sync)" } else { "standalone" };
    tracing::info!(
        "Starting Tollbooth on port {} in {} mode",
        config.port,
        mode
    );

    // Log configured providers for debugging
    tracing::info!(
        "LLM providers configured: OpenAI={}, Anthropic={}, Cerebras={}, VertexAI={}, xAI={}",
        config.openai_api_key.is_some(),
        config.anthropic_api_key.is_some(),
        config.cerebras_api_key.is_some(),
        config.google_cloud_project.is_some(),
        config.xai_api_key.is_some()
    );

    if config.google_cloud_project.is_some() {
        tracing::info!(
            "Vertex AI: project={}, region={}",
            config.google_cloud_project.as_deref().unwrap_or("(not set)"),
            config.google_cloud_region
        );
    }

    // Log external services configuration
    tracing::info!(
        "External services: Exa={}, GooglePlaces={}",
        config.exa_api_key.is_some(),
        config.google_api_key.is_some()
    );

    // Verify at least one provider is configured
    if !config.has_llm_provider() {
        tracing::warn!("No LLM provider configured. Set OPENAI_API_KEY, ANTHROPIC_API_KEY, GOOGLE_CLOUD_PROJECT, CEREBRAS_API_KEY, or XAI_API_KEY.");
    }

    // Initialize budget manager (hydrates from Atlas if configured)
    let budget = BudgetManager::new(&config).await?;
    let budget_clone = budget.clone();
    let report_interval = config.atlas_report_interval_secs;

    // Start usage reporter (only reports if Atlas is configured)
    tokio::spawn(async move {
        budget_clone.run_reporter(report_interval).await;
    });

    // Build HTTP client for embeddings and other direct API calls
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300)) // 5 min for long completions
        .build()?;

    // Build shared state
    let state = Arc::new(AppState {
        config,
        budget,
        http_client,
    });

    // Build router
    let app = Router::new()
        // Health check (no auth required)
        .route("/health", get(routes::health::health_check))
        .route("/ready", get(routes::health::readiness_check))
        // Chat completions (LLM proxy - all providers)
        .nest("/v1", routes::chat::router())
        // External service proxies (Exa, Google Places)
        .nest("/v1", routes::services::router())
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state.clone());

    // Start server
    let addr = format!("0.0.0.0:{}", state.config.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Tollbooth listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
