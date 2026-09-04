//! Relay control plane for the iroh reach layer — one door.
//!
//! - `POST /relay/config { api_key } -> { relay_url }` — the box learns which
//!   relay to home on. Any linked box; identity, not billing (0017 /
//!   open-relay-plan §Work 1b).
//!
//! **Two endpoints were deleted here and neither should come back casually.**
//!
//! `POST /relay/authorize` — the per-connection admission callout iroh-relay
//! used to make (open-relay-plan, 2026-08-31). It was both a paywall on the
//! connectivity substrate and a live linkage between account state and
//! connection metadata. The relay now admits everyone and defends itself with
//! rate limits; the real security boundary always was the box's own EndpointId
//! allowlist.
//!
//! `POST /iroh/register` — the box reporting its own + every paired device's
//! EndpointId, upserted into `iroh_endpoints` and reconciled on unpair
//! (dropped by migration 0018). It existed ONLY to feed the callout above, and
//! outlived it by a day: for that day atlas kept a live, reconcile-refreshed
//! inventory of every box and every paired device, joined to a billing
//! account, that nothing read. A gate's data outliving the gate is the failure
//! class — when admission logic goes, its registry goes in the same change.
//!
//! atlas never sees traffic, volume, or timing, and now holds no map from an
//! EndpointId to an account.
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
