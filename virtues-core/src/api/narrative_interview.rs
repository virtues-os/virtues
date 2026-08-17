//! The interview behind "In your own words".
//!
//! `wiki_narrative_identity` holds the finished prose — the document a person
//! reads and the assistant is handed. This module holds the raw material: one
//! row per question, in their own words.
//!
//! Separate on purpose. The draft can be regenerated when the writing improves
//! without asking anyone to answer twelve questions again; an answer can be
//! revised years later without editing a paragraph that wove it together with
//! three others; and the answers are the person's actual words where the
//! document is a machine's arrangement of them.
//!
//! AUTOSAVE IS THE WHOLE DESIGN. This is an hour of writing about grief, vice
//! and family. Losing it to a reload is not an inconvenience — it is a betrayal
//! on the one document where trust matters most. So the write path is an upsert
//! cheap enough to call while someone is still typing.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::Result;
use crate::server::AppState;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InterviewAnswer {
    pub question_id: String,
    pub answer: String,
    pub word_count: i32,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Every answer written so far. Absent questions simply have no row — the
/// client owns the question set, so the server never needs to know it.
pub async fn list_answers(pool: &PgPool) -> Result<Vec<InterviewAnswer>> {
    let rows = sqlx::query_as::<_, InterviewAnswer>(
        "SELECT question_id, answer, word_count, completed_at \
         FROM wiki_narrative_interview ORDER BY question_id",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("list interview answers: {e}")))?;
    Ok(rows)
}

#[derive(Debug, Deserialize)]
pub struct SaveAnswer {
    pub question_id: String,
    pub answer: String,
    /// Set when the person moves on having written something real — distinct
    /// from a non-empty answer, which may be three words typed while thinking.
    #[serde(default)]
    pub completed: bool,
}

pub async fn save_answer(pool: &PgPool, req: &SaveAnswer) -> Result<()> {
    let words = req.answer.split_whitespace().count() as i32;
    sqlx::query(
        "INSERT INTO wiki_narrative_interview (question_id, answer, word_count, completed_at) \
         VALUES ($1, $2, $3, CASE WHEN $4 THEN now() ELSE NULL END) \
         ON CONFLICT (question_id) DO UPDATE SET \
           answer = EXCLUDED.answer, \
           word_count = EXCLUDED.word_count, \
           -- Completion is sticky. Revising an answer later must not un-finish
           -- a question someone already answered properly.
           completed_at = COALESCE(wiki_narrative_interview.completed_at, EXCLUDED.completed_at), \
           updated_at = now()",
    )
    .bind(&req.question_id)
    .bind(&req.answer)
    .bind(words)
    .bind(req.completed)
    .execute(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("save interview answer: {e}")))?;
    Ok(())
}

// ─── handlers ───────────────────────────────────────────────────────────────

pub async fn list_handler(
    State(state): State<AppState>,
    _user: crate::middleware::auth::AuthUser,
) -> impl IntoResponse {
    match list_answers(state.db.pool()).await {
        Ok(rows) => (StatusCode::OK, Json(serde_json::json!({ "answers": rows }))).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "interview: list failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

pub async fn save_handler(
    State(state): State<AppState>,
    _user: crate::middleware::auth::AuthUser,
    Json(req): Json<SaveAnswer>,
) -> impl IntoResponse {
    if req.question_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "question_id required" })),
        )
            .into_response();
    }
    match save_answer(state.db.pool(), &req).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response(),
        Err(e) => {
            // Loud, because a silent failure here loses somebody's hour.
            tracing::error!(error = %e, question = %req.question_id, "interview: SAVE FAILED");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}
