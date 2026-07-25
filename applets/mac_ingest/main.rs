//! Mac activity ingest.
//!
//! Webhook-driven, per-credential. The Mac client posts a single batch payload
//! containing app events, browser history, and iMessages. This binary
//! dispatches each kind to the appropriate transform.

mod sessionize;
mod transform;

use anyhow::Result;
use serde_json::Value;
use virtues::storage::lake::{self, Envelope};
use virtues_helpers::{connect_from_env, output, read_input};

const PROVIDER: &str = "mac";
const STREAM_KEYS: [&str; 3] = ["app_events", "browser_history", "imessages"];

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

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

    // Sessions are held OPEN across batches, so the sessionizer must know whose
    // machine this is: two Macs would otherwise close each other's sessions.
    let device_id = payload
        .get("device_id")
        .and_then(|v| v.as_str())
        .unwrap_or("mac");

    let app_written = sessionize::ingest(&pool, device_id, &app_events).await?;
    let browser_written = transform::write_browser_history(&pool, &browser).await?;
    let imessage_written = transform::write_imessages(&pool, &imessages).await?;

    // A batch with zero messages because the Mac has none, and one with zero
    // because macOS is denying the collector `chat.db`, are identical on the
    // wire. The collector now says which, so refuse to report a clean run when
    // a source is actually shut off: state it in the summary (the run history
    // is where someone looks) and warn in the log.
    // Park the collector's self-report on the device row so the UI can show it.
    // `device_info` already carries the client's build identity the same way;
    // permissions belong beside it, as a property of the device rather than of
    // any one run — a run scrolls out of history, a revoked permission persists.
    if let Some(health) = payload.get("collector_health").filter(|h| h.is_object()) {
        if let Err(e) = record_device_permissions(&pool, device_id, health).await {
            // Never fail an ingest over telemetry — the records matter more.
            tracing::warn!(device_id, error = %e, "could not record collector permissions");
        }
    }

    let denied = denied_capabilities(payload);
    let summary = format!(
        "apps: {app_written} sessions, browser: {browser_written} visits, imessages: {imessage_written}"
    );
    let summary = if denied.is_empty() {
        summary
    } else {
        tracing::warn!(
            device_id,
            denied = %denied.join(", "),
            "mac collector is missing permissions — affected streams cannot be read \
             and will look merely idle until this is granted"
        );
        format!("{summary} — DENIED: {} (grant on the Mac)", denied.join(", "))
    };
    output(&summary, &input.config)
}

/// Store the collector's self-reported permissions on its device row, beside
/// the build identity that already lives in `device_info`.
async fn record_device_permissions(
    pool: &sqlx::PgPool,
    device_id: &str,
    health: &Value,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE app_device
            SET device_info = jsonb_set(
                COALESCE(device_info, '{}'::jsonb), '{permissions}', $2::jsonb, true)
          WHERE id = $1",
    )
    .bind(device_id)
    .bind(health)
    .execute(pool)
    .await?;
    Ok(())
}

/// Capabilities the Mac collector reports it does NOT currently have.
///
/// Absent for collectors older than the health field — which is not the same as
/// "everything is fine", so it maps to "nothing reported" rather than a
/// fabricated all-clear.
fn denied_capabilities(payload: &Value) -> Vec<String> {
    payload
        .get("collector_health")
        .and_then(|h| h.get("denied"))
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
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
