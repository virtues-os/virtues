//! Auth activity log — surfaced at `/settings/activity`.
//!
//! Append-only log of auth-shaped events: pairings, revocations, session
//! starts/ends, sudo requests + approvals/denials. The web UI lists these so
//! the user can spot anything that doesn't match their own actions.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::middleware::auth::AuthUser;

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
