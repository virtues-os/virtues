//! Relay control plane for the iroh reach layer.
//!
//! - `POST /relay/config { api_key } -> { relay_url }` — the box learns which
//!   relay to home on. Gated on an active subscription.
//! - `POST /iroh/register { api_key, endpoint_ids: [..] }` — the box reports its
//!   own + its paired devices' EndpointIds; atlas maps each → the account so the
//!   gate below can recognise them.
//! - `POST /relay/authorize` — called **by iroh-relay** (service bearer) with a
//!   header `X-Iroh-NodeId`; returns `200 "true"` iff that EndpointId belongs to
//!   an active-subscription account, else a non-OK/`"false"` (iroh-relay admits
//!   only on 200 + body `"true"`).
//!
//! This is an **anti-freeloading** gate only — the real security boundary is the
//! box's own EndpointId allowlist. atlas never sees traffic, volume, or timing;
//! relay use is flat (covered by the subscription), never per-byte metered.

use axum::{
    extract::State,
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use serde::Deserialize;
use serde_json::json;

use crate::routes::{credits::resolve_active_customer, AppState};

/// Header iroh-relay sets to the hex EndpointId attempting to connect.
const X_IROH_ENDPOINT_ID: &str = "X-Iroh-NodeId";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/relay/config", post(relay_config))
        .route("/iroh/register", post(iroh_register))
        .route("/relay/authorize", post(relay_authorize))
}

#[derive(Debug, Deserialize)]
struct RelayConfigBody {
    api_key: String,
}

/// The box asks which relay to home on. Paid capability → active sub required.
async fn relay_config(
    State(state): State<AppState>,
    Json(body): Json<RelayConfigBody>,
) -> axum::response::Response {
    if let Err(resp) = resolve_active_customer(&state, &body.api_key).await {
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

/// The box reports the EndpointIds that belong to its account, so the relay gate
/// can recognise them. Idempotent upsert; a device that re-pairs to a different
/// account moves with it.
async fn iroh_register(
    State(state): State<AppState>,
    Json(body): Json<IrohRegisterBody>,
) -> axum::response::Response {
    let (_customer_id, account_id) = match resolve_active_customer(&state, &body.api_key).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let ids: Vec<String> = body
        .endpoint_ids
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

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
    // (revoked/unpaired devices) so the relay gate stops recognising them. The
    // box always includes its own + all paired-device ids, so this is exact.
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

/// iroh-relay access-control callout. Admits only EndpointIds that map to an
/// active-subscription account. iroh-relay treats **200 + body `"true"`** as
/// allow and everything else as deny.
async fn relay_authorize(State(state): State<AppState>, headers: HeaderMap) -> axum::response::Response {
    // Fail CLOSED when unconfigured: with no shared secret this endpoint would be
    // an unauthenticated subscription-enumeration oracle, so deny instead (the
    // relay treats non-200 as "deny"). Configure VIRTUES_RELAY_AUTH_SECRET (and
    // the relay's access.http.bearer_token to match) to enable the gate.
    if state.relay.relay_auth_secret.is_empty() {
        return (StatusCode::SERVICE_UNAVAILABLE, "false").into_response();
    }
    // Service-to-service bearer (shared with iroh-relay's access.http.bearer_token).
    let ok = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|h| h == format!("Bearer {}", state.relay.relay_auth_secret))
        .unwrap_or(false);
    if !ok {
        return (StatusCode::UNAUTHORIZED, "false").into_response();
    }
    let Some(endpoint_id) = headers.get(X_IROH_ENDPOINT_ID).and_then(|v| v.to_str().ok()) else {
        return (StatusCode::BAD_REQUEST, "false").into_response();
    };
    let active: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM iroh_endpoints e
            JOIN customers c ON c.account_id = e.account_id
            WHERE e.endpoint_id = $1
              AND (SELECT s.status FROM subscriptions s
                   WHERE s.stripe_customer_id = c.stripe_customer_id
                   ORDER BY s.current_period_end DESC NULLS LAST
                   LIMIT 1) = 'active'
        )
        "#,
    )
    .bind(endpoint_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(false);

    if active {
        (StatusCode::OK, "true").into_response()
    } else {
        (StatusCode::FORBIDDEN, "false").into_response()
    }
}
