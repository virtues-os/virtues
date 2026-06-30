//! virtues-relay binary — thin wrapper around the library's [`serve`].
//! All logic lives in `lib.rs` so it can be integration-tested.

use anyhow::{Context, Result};
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use virtues_relay::config::Config;
use virtues_relay::state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,virtues_relay=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let state = AppState::new(Config::from_env());

    tracing::info!(
        client = %state.config.client_addr,
        control = %state.config.control_addr,
        "virtues-relay starting"
    );

    let client_listener = TcpListener::bind(&state.config.client_addr)
        .await
        .with_context(|| format!("bind client listener {}", state.config.client_addr))?;
    let control_listener = TcpListener::bind(&state.config.control_addr)
        .await
        .with_context(|| format!("bind control listener {}", state.config.control_addr))?;

    virtues_relay::serve(state, client_listener, control_listener).await
}
