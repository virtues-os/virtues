//! Mac activity ingest.
//!
//! Webhook-driven, per-credential. The Mac client posts a single batch payload
//! containing app events, browser history, and iMessages. This binary
//! dispatches each kind to the appropriate transform.

mod transform;

use anyhow::Result;
use serde_json::Value;
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let input = read_input()?;
    let pool = connect_from_env().await?;

    let payload = input
        .payload
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("mac_activity requires a payload"))?;

    let app_events: Vec<Value> = payload
        .get("app_events")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let browser: Vec<Value> = payload
        .get("browser_history")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let imessages: Vec<Value> = payload
        .get("imessages")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let app_written = transform::write_app_events(&pool, &app_events).await?;
    let browser_written = transform::write_browser_history(&pool, &browser).await?;
    let imessage_written = transform::write_imessages(&pool, &imessages).await?;

    let summary = format!(
        "apps: {app_written} sessions, browser: {browser_written} visits, imessages: {imessage_written}"
    );
    output(&summary, &input.config)
}
