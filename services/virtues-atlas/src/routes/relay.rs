//! Relay control plane for the iroh reach layer.
//!
//! - `POST /relay/config { api_key } -> { relay_url }` — the box learns which
//!   relay to home on. Any linked box; identity, not billing (0017 /
//!   open-relay-plan §Work 1b).
//! - `POST /iroh/register { api_key, endpoint_ids: [..] }` — the box reports its
//!   own + its paired devices' EndpointIds; atlas maps each → the account. Once
//!   the relay admission gate this fed, now an informational registry (fleet
//!   ops, future per-account tooling); kept because shipped boxes call it.
//!
//! `POST /relay/authorize` — the per-connection admission callout iroh-relay
//! used to make — is GONE (open-relay-plan, 2026-08-31). The relay admits
//! everyone and defends itself with rate limits; the callout was both a paywall
//! on the connectivity substrate and a live linkage between account state and
//! connection metadata, which contradicted the blind-relay doctrine. The real
//! security boundary always was the box's own EndpointId allowlist. atlas never
//! sees traffic, volume, or timing.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use serde::Deserialize;
use serde_json::json;

use crate::routes::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/relay/config", post(relay_config))
        .route("/iroh/register", post(iroh_register))
}

/// Resolve an api_key → owning account, with no subscription requirement —
/// these doors need to know WHO, not whether they pay. The money paths keep
/// [`crate::routes::credits::resolve_active_customer`].
async fn resolve_account(
    state: &AppState,
    api_key: &str,
) -> Result<String, axum::response::Response> {
    let key_hash = super::claim::sha256(api_key.as_bytes());
    match super::claim::account_id_by_key_hash(&state.pool, &key_hash).await {
        Ok(Some(account_id)) => Ok(account_id),
        Ok(None) => Err(err(StatusCode::UNAUTHORIZED, "invalid_api_key", "unknown api key")),
        Err(e) => {
            tracing::warn!("relay account lookup failed: {e:#}");
            Err(err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "account lookup failed"))
        }
    }
}

/// The `{error:{code,message}}` envelope every atlas route speaks (and the
/// airlock's `atlasPost` flattens) — same 3-line helper as the sibling route
/// modules, so the shape can't drift per construction site.
fn err(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (status, Json(json!({ "error": { "code": code, "message": message } }))).into_response()
}

#[derive(Debug, Deserialize)]
struct RelayConfigBody {
    api_key: String,
}

/// The box asks which relay to home on. Any linked box qualifies —
/// reachability is part of ownership, not the subscription.
async fn relay_config(
    State(state): State<AppState>,
    Json(body): Json<RelayConfigBody>,
) -> axum::response::Response {
    if let Err(resp) = resolve_account(&state, &body.api_key).await {
        return resp;
    }
    if state.relay.relay_url.is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "relay_not_configured",
                "message": "relay reachability is not enabled on this deployment",
            })),
        )
            .into_response();
    }
    (StatusCode::OK, Json(json!({ "relay_url": state.relay.relay_url }))).into_response()
}

#[derive(Debug, Deserialize)]
struct IrohRegisterBody {
    api_key: String,
    /// The box's own EndpointId plus every currently-paired device's EndpointId.
    endpoint_ids: Vec<String>,
}

/// The box reports the EndpointIds that belong to its account. Idempotent
/// upsert; a device that re-pairs to a different account moves with it.
async fn iroh_register(
    State(state): State<AppState>,
    Json(body): Json<IrohRegisterBody>,
) -> axum::response::Response {
    let account_id = match resolve_account(&state, &body.api_key).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let ids: Vec<String> = body
        .endpoint_ids
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Guard the reconcile below: `endpoint_id <> ALL('{}')` is vacuously TRUE, so
    // an empty set would DELETE every one of this account's registrations. A box
    // always reports at least its own EndpointId, so an empty list is a malformed
    // request — reject it rather than let it wipe the account's registry.
    if ids.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "no_endpoint_ids" })),
        )
            .into_response();
    }

    // Claim each EndpointId for this account. The `ON CONFLICT ... WHERE` makes
    // the update a no-op when the row is already owned by a DIFFERENT account, so
    // a caller can't hijack another account's EndpointId (they aren't secret —
    // they're handed out in pairing tickets).
    for eid in &ids {
        if let Err(e) = sqlx::query(
            "INSERT INTO iroh_endpoints (endpoint_id, account_id) VALUES ($1, $2) \
             ON CONFLICT (endpoint_id) DO UPDATE SET account_id = EXCLUDED.account_id \
             WHERE iroh_endpoints.account_id = EXCLUDED.account_id",
        )
        .bind(eid)
        .bind(&account_id)
        .execute(&state.pool)
        .await
        {
            tracing::error!(error = %e, "iroh_endpoints upsert failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "register_failed" })),
            )
                .into_response();
        }
    }

    // Reconcile: drop this account's EndpointIds no longer in the reported set
    // (revoked/unpaired devices). The box always includes its own + all
    // paired-device ids, so this is exact.
    if let Err(e) = sqlx::query(
        "DELETE FROM iroh_endpoints WHERE account_id = $1 AND endpoint_id <> ALL($2)",
    )
    .bind(&account_id)
    .bind(&ids)
    .execute(&state.pool)
    .await
    {
        tracing::error!(error = %e, "iroh_endpoints reconcile failed");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "register_failed" })),
        )
            .into_response();
    }
    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}
