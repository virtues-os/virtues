//! Mac activity ingest.
//!
//! Webhook-driven, per-credential. The Mac client posts a single batch payload
//! containing app events, browser history, and iMessages. This binary
//! dispatches each kind to the appropriate transform.

mod transform;

use anyhow::Result;
use serde_json::Value;
use virtues::storage::lake::{self, Envelope};
use virtues_helpers::{connect_from_env, output, read_input};

const PROVIDER: &str = "mac";
const STREAM_KEYS: [&str; 3] = ["app_events", "browser_history", "imessages"];

#[tokio::main]
async fn main() -> Result<()> {
    virtues_actions::init_tracing();

    let input = read_input()?;
    let pool = connect_from_env("virtues-action-mac_ingest").await?;

    let payload = input
        .payload
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("mac_ingest requires a payload"))?;

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

    // Land the raw records BEFORE transforming them. If a transform below is
    // wrong we still fail loudly (500 → the device retries), but the bytes are
    // already durable — so fixing it is a `virtues replay`, not a re-collection
    // we cannot perform (a webhook push has no upstream to re-fetch from).
    //
    // One object per stream, not one per envelope: each key is independently
    // optional above, so `{"imessages": [...]}` on its own is a complete, valid,
    // replayable payload.
    let storage = lake::storage_from_env()?;
    let residual = residual_envelope(payload);
    for (key, records) in [
        ("app_events", &app_events),
        ("browser_history", &browser),
        ("imessages", &imessages),
    ] {
        lake::archive(
            &pool,
            &storage,
            PROVIDER,
            PROVIDER,
            Envelope::MacKey(key),
            records,
            residual.clone(),
        )
        .await?;
    }

    let app_written = transform::write_app_events(&pool, &app_events).await?;
    let browser_written = transform::write_browser_history(&pool, &browser).await?;
    let imessage_written = transform::write_imessages(&pool, &imessages).await?;

    let summary = format!(
        "apps: {app_written} sessions, browser: {browser_written} visits, imessages: {imessage_written}"
    );
    output(&summary, &input.config)
}

/// Every top-level key of the body that no transform reads — `device_id`, a
/// client's `sent_at`, whatever the collector adds next. Preserved on the lake
/// object because this is exactly the class of field that gets silently dropped
/// today and is unrecoverable once the payload is gone.
fn residual_envelope(payload: &Value) -> Value {
    let Some(obj) = payload.as_object() else {
        return Value::Object(Default::default());
    };
    let residual: serde_json::Map<String, Value> = obj
        .iter()
        .filter(|(k, _)| !STREAM_KEYS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Value::Object(residual)
}
