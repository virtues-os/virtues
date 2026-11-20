//! Deleting ingested records (`crate::api::deletions`): the routes the chat's
//! deletion card, Query and Settings · Deleted records call.
//!
//! Allowing a deletion and deleting rows in Query both need the
//! app's owner proof, as the admin applet's commands do: any process on a
//! paired Mac reaches the server as that Mac, and only the app can sign.
//! Reading a deletion, setting one aside and lifting its tombstones do not.
//!
//! Also here: writing a day's page again after its owner removed it.

use axum::{
    extract::{OriginalUri, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;

use super::{api_response, error_response};
use crate::api::{day_summary, deletions};
use crate::error::Error;
use crate::middleware::auth::AuthUser;
use crate::server::AppState;

/// This area's authenticated routes. Merged into the protected router, whose
/// `route_layer` requires a resolved `AuthUser`.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/deletions", get(list_handler))
        .route("/api/deletions/:id", get(get_handler))
        .route("/api/deletions/:id/allow", post(allow_handler))
        .route("/api/deletions/:id/cancel", post(cancel_handler))
        .route("/api/deletions/:id/tombstones", delete(lift_handler))
        .route("/api/developer/rows/delete", post(delete_rows_handler))
        // TODO(2026-11-19): move these two routes beside the day's other
        // routes in `wiki.rs`.
        .route(
            "/api/wiki/day/:date/page/write-again",
            get(day_write_again_status_handler).post(write_day_again_handler),
        )
}

/// A refusal the day view keys its copy on: `{ "error": code }`.
fn refusal(code: &str) -> Response {
    (StatusCode::CONFLICT, Json(serde_json::json!({ "error": code }))).into_response()
}

/// POST /api/wiki/day/:date/page/write-again — the owner removed this day's
/// page and now wants one.
///
/// Answers 202 at once and writes in a task of its own
/// (`day_summary::write_removed_day_again`): the writer takes minutes. The
/// GET beside this says how it went. Refused with 409 and a code when the
/// day's page was not removed (`not_removed`) or one is already running
/// (`running`).
async fn write_day_again_handler(State(state): State<AppState>, Path(date): Path<String>) -> Response {
    let Ok(date) = date.parse::<chrono::NaiveDate>() else {
        return error_response(Error::InvalidInput(format!("Invalid date format: {date}")));
    };
    let pool = state.db.pool().clone();
    match day_summary::day_page_is_removed(&pool, date).await {
        Ok(true) => {}
        Ok(false) => return refusal("not_removed"),
        Err(e) => return error_response(e),
    }
    if !day_summary::write_again_begin(date) {
        return refusal("running");
    }
    tokio::spawn(async move {
        let code = match day_summary::write_removed_day_again(&pool, date).await {
            Ok(outcome) => {
                tracing::info!(date = %date, outcome = outcome.code(), "write it again finished");
                outcome.code()
            }
            Err(e) => {
                tracing::warn!(date = %date, error = %e, "write it again failed");
                "failed"
            }
        };
        day_summary::write_again_end(date, code);
    });
    (StatusCode::ACCEPTED, Json(serde_json::json!({ "state": "running" }))).into_response()
}

/// GET /api/wiki/day/:date/page/write-again — where a day's Write it again
/// stands: `{state: "idle"}` when this server has run none since it started,
/// `running`, or `ended` with the `code` it ended on (`written`,
/// `not_enough`, `not_over`, `empty`, `busy`, `not_removed` or `failed`).
async fn day_write_again_status_handler(Path(date): Path<String>) -> Response {
    let Ok(date) = date.parse::<chrono::NaiveDate>() else {
        return error_response(Error::InvalidInput(format!("Invalid date format: {date}")));
    };
    let body = match day_summary::write_again_state(date) {
        None => serde_json::json!({ "state": "idle" }),
        Some(None) => serde_json::json!({ "state": "running" }),
        Some(Some(code)) => serde_json::json!({ "state": "ended", "code": code }),
    };
    Json(body).into_response()
}

/// GET /api/deletions — every deletion that ran or is running, newest first.
async fn list_handler(State(state): State<AppState>) -> Response {
    api_response(deletions::list(state.db.pool()).await)
}

/// GET /api/deletions/:id — a deletion as it stands: what it would remove
/// while it waits, the receipt once it has run.
async fn get_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(deletions::get(state.db.pool(), &id).await)
}

/// POST /api/deletions/:id/allow — the owner allows it, and it runs.
async fn allow_handler(
    State(state): State<AppState>,
    user: AuthUser,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(refused) = crate::api::owner_proof::check(state.db.pool(), &user, "POST", uri.path(), &headers).await {
        return refused;
    }
    api_response(deletions::approve(state.db.pool(), &id).await)
}

/// POST /api/deletions/:id/cancel — the owner sets it aside.
async fn cancel_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(deletions::cancel(state.db.pool(), &id).await)
}

/// DELETE /api/deletions/:id/tombstones — let its records be collected again.
async fn lift_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(
        deletions::lift(state.db.pool(), &id)
            .await
            .map(|lifted| serde_json::json!({ "lifted": lifted })),
    )
}

#[derive(Debug, Deserialize)]
struct DeleteRows {
    table: String,
    ids: Vec<String>,
}

/// POST /api/developer/rows/delete — delete and tombstone rows of one data
/// table, picked by hand in Query.
async fn delete_rows_handler(
    State(state): State<AppState>,
    user: AuthUser,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Json(request): Json<DeleteRows>,
) -> Response {
    if let Err(refused) = crate::api::owner_proof::check(state.db.pool(), &user, "POST", uri.path(), &headers).await {
        return refused;
    }
    api_response(
        deletions::delete_rows(state.db.pool(), &request.table, &request.ids)
            .await
            .map(|removed| removed.receipt()),
    )
}
