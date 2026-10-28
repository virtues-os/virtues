//! Auth activity log, served at `GET /api/audit/auth`.
//!
//! Append-only log of auth-shaped events: pairings, enrollments,
//! revocations, pair codes, AI key changes, and each password or key a device
//! revealed. [`record`] is the one way to write it. No page lists it today;
//! the route is the one way to read it.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::middleware::auth::AuthUser;
use crate::middleware::OWNER_USER_ID;

/// Append one event to `app_auth_event`. `device_id` is the device the
/// event is about. The action has already happened when this runs, so a
/// failed write is logged and never fails the caller's request.
pub async fn record(
    pool: &PgPool,
    device_id: Option<&str>,
    event_type: &str,
    detail: Value,
    ip: Option<String>,
    user_agent: Option<String>,
) {
    if let Err(e) = sqlx::query(
        "INSERT INTO app_auth_event \
         (user_id, device_id, event_type, detail, ip, user_agent) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(OWNER_USER_ID)
    .bind(device_id)
    .bind(event_type)
    .bind(&detail)
    .bind(ip)
    .bind(user_agent)
    .execute(pool)
    .await
    {
        tracing::warn!(event_type, error = %e, "auth event not written");
    }
}

/// Hard cap on a single page. The UI can request fewer; more requires an
/// older-than cursor (added later if/when needed).
const MAX_LIMIT: i64 = 200;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AuthEvent {
    pub id: i64,
    pub device_id: Option<String>,
    pub event_type: String,
    pub detail: Value,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

/// `GET /api/audit/auth` — most-recent-first list.
pub async fn list_handler(State(pool): State<PgPool>, user: AuthUser) -> impl IntoResponse {
    let rows: Result<
        Vec<(i64, Option<String>, String, Value, Option<String>, Option<String>, DateTime<Utc>)>,
        _,
    > = sqlx::query_as(
        "SELECT id, device_id, event_type, detail, ip, user_agent, occurred_at \
         FROM app_auth_event \
         WHERE user_id = $1 \
         ORDER BY occurred_at DESC \
         LIMIT $2",
    )
    .bind(&user.id)
    .bind(MAX_LIMIT)
    .fetch_all(&pool)
    .await;

    match rows {
        Ok(rows) => {
            let events: Vec<AuthEvent> = rows
                .into_iter()
                .map(|(id, device_id, event_type, detail, ip, user_agent, occurred_at)| AuthEvent {
                    id,
                    device_id,
                    event_type,
                    detail,
                    ip,
                    user_agent,
                    occurred_at,
                })
                .collect();
            (StatusCode::OK, Json(json!({"events": events}))).into_response()
        }
        Err(e) => {
            tracing::warn!("audit list failed: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "list_failed"})),
            )
                .into_response()
        }
    }
}
