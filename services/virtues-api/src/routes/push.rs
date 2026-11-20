//! Change signals a box collects (the box side is
//! `virtues-core/src/scheduler/google_push.rs`).
//!
//!   POST /v1/push/id               — the box's push id, made on first ask
//!   POST /v1/push/google/calendar  — Google Calendar says a watched calendar
//!                                    changed
//!   GET  /v1/push/wait             — the box collects the channels that
//!                                    changed, waiting up to `WAIT_SECS`
//!
//! Any linked box may use these, paid or not, like the OAuth token proxy:
//! calendar sync does not need a paid plan.
//!
//! The box never takes a call from here. It holds `/wait` open and pulls.
//!
//! What passes through: Google's call has no body. Its headers are the
//! channel id and token, both chosen by the box (a random id, and the box's
//! push id), plus Google's opaque resource id and a state word. Only the
//! channel id is kept, until the box collects it. Nothing here names a
//! person, an account address or an event.

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use tokio::sync::Notify;

use axum::extract::FromRequestParts;
use axum::http::{header, request::Parts};
use sha2::{Digest, Sha256};
use crate::AppState;

/// A linked box, by its key. Unlike `BearerAuth` it does not look at the
/// wallet, and it never answers 402: on a 402 the box asks atlas for an
/// automatic top-up.
pub struct BoxKey {
    pub account_id: String,
}

#[axum::async_trait]
impl FromRequestParts<Arc<AppState>> for BoxKey {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Self, StatusCode> {
        let key = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let hash = Sha256::digest(key.as_bytes()).to_vec();
        if state.blocklist.is_blocked(&hash) {
            return Err(StatusCode::FORBIDDEN);
        }
        let account: Option<String> = sqlx::query_scalar("SELECT account_id FROM device_keys WHERE api_key_hash = $1")
            .bind(&hash)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "push key lookup failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        account.map(|account_id| BoxKey { account_id }).ok_or(StatusCode::UNAUTHORIZED)
    }
}

/// How long `/wait` holds a request. The box's client gives up at 60s.
const WAIT_SECS: u64 = 25;
/// Channels kept per box before new signals are dropped. A box watches a
/// handful of calendars per account, so this is never reached in normal use.
const MAX_PENDING_PER_BOX: i64 = 200;
const MAX_CHANNEL_ID_LEN: usize = 64;

/// Boxes waiting on `/wait` in this process, by push id. A signal that lands
/// on another instance is still found when this one re-reads the table.
static WAITERS: LazyLock<Mutex<HashMap<String, Arc<Notify>>>> = LazyLock::new(Default::default);

fn waiter(push_id: &str) -> Arc<Notify> {
    WAITERS.lock().expect("waiters lock").entry(push_id.to_string()).or_default().clone()
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/push/id", post(push_id))
        .route("/v1/push/google/calendar", post(google_calendar))
        .route("/v1/push/wait", get(wait))
}

async fn push_id(State(state): State<Arc<AppState>>, key: BoxKey) -> Response {
    match ensure_push_id(&state.db, &key.account_id).await {
        Ok(id) => Json(json!({ "push_id": id })).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "push id lookup failed");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "push id lookup failed" }))).into_response()
        }
    }
}

async fn ensure_push_id(db: &sqlx::PgPool, account_id: &str) -> sqlx::Result<String> {
    let fresh = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    sqlx::query("INSERT INTO push_ids (account_id, push_id) VALUES ($1, $2) ON CONFLICT (account_id) DO NOTHING")
        .bind(account_id)
        .bind(&fresh)
        .execute(db)
        .await?;
    sqlx::query_scalar("SELECT push_id FROM push_ids WHERE account_id = $1")
        .bind(account_id)
        .fetch_one(db)
        .await
}

/// Google's notification. It answers 200 to anything it cannot use, because
/// any other answer makes Google retry the same call.
async fn google_calendar(State(state): State<Arc<AppState>>, headers: HeaderMap) -> StatusCode {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or("");
    let (token, channel, resource_state) =
        (header("x-goog-channel-token"), header("x-goog-channel-id"), header("x-goog-resource-state"));
    // `sync` is the hello Google sends when a watch starts; nothing changed.
    if resource_state == "sync" || token.is_empty() || channel.is_empty() || channel.len() > MAX_CHANNEL_ID_LEN {
        return StatusCode::OK;
    }
    match record(&state.db, token, channel).await {
        Ok(true) => waiter(token).notify_waiters(),
        Ok(false) => {}
        Err(e) => tracing::error!(error = %e, "push signal insert failed"),
    }
    StatusCode::OK
}

/// Keep a channel's signal for its box. False when the token is no box's push
/// id or the box already has too many waiting.
async fn record(db: &sqlx::PgPool, push_id: &str, channel_id: &str) -> sqlx::Result<bool> {
    let res = sqlx::query(
        "INSERT INTO push_pending (push_id, channel_id)
         SELECT $1, $2 FROM push_ids
         WHERE push_id = $1
           AND (SELECT count(*) FROM push_pending WHERE push_id = $1) < $3
         ON CONFLICT (push_id, channel_id) DO NOTHING",
    )
    .bind(push_id)
    .bind(channel_id)
    .bind(MAX_PENDING_PER_BOX)
    .execute(db)
    .await?;
    Ok(res.rows_affected() > 0)
}

async fn take(db: &sqlx::PgPool, account_id: &str) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar(
        "DELETE FROM push_pending
         WHERE push_id = (SELECT push_id FROM push_ids WHERE account_id = $1)
         RETURNING channel_id",
    )
    .bind(account_id)
    .fetch_all(db)
    .await
}

async fn wait(State(state): State<Arc<AppState>>, key: BoxKey) -> Response {
    let push_id: Option<String> = match sqlx::query_scalar("SELECT push_id FROM push_ids WHERE account_id = $1")
        .bind(&key.account_id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(p) => p,
        Err(e) => return failed(e),
    };
    let Some(push_id) = push_id else {
        return Json(json!({ "channels": [] })).into_response();
    };

    // Subscribe before reading, so a signal between the read and the wait
    // still wakes this request.
    let notify = waiter(&push_id);
    let notified = notify.notified();
    tokio::pin!(notified);
    notified.as_mut().enable();

    let mut channels = match take(&state.db, &key.account_id).await {
        Ok(c) => c,
        Err(e) => return failed(e),
    };
    if channels.is_empty() {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(WAIT_SECS), notified).await;
        channels = match take(&state.db, &key.account_id).await {
            Ok(c) => c,
            Err(e) => return failed(e),
        };
    }
    Json(json!({ "channels": channels })).into_response()
}

fn failed(e: sqlx::Error) -> Response {
    tracing::error!(error = %e, "push wait failed");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "push wait failed" }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_push_routes_sit_beside_the_oauth_providers() {
        let _ = super::super::oauth::router().merge(router());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_signal_reaches_only_its_box_and_is_collected_once(db: sqlx::PgPool) {
        let a = ensure_push_id(&db, "acct_a").await.unwrap();
        assert_eq!(ensure_push_id(&db, "acct_a").await.unwrap(), a, "one id per box");
        let _b = ensure_push_id(&db, "acct_b").await.unwrap();

        assert!(record(&db, &a, "chan_1").await.unwrap());
        assert!(!record(&db, &a, "chan_1").await.unwrap(), "a repeat is one signal");
        assert!(!record(&db, "not-a-push-id", "chan_2").await.unwrap());

        assert_eq!(take(&db, "acct_b").await.unwrap(), Vec::<String>::new());
        assert_eq!(take(&db, "acct_a").await.unwrap(), vec!["chan_1".to_string()]);
        assert_eq!(take(&db, "acct_a").await.unwrap(), Vec::<String>::new());
    }
}
