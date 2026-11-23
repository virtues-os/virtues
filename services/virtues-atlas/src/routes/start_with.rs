//! What excites the owner most about Virtues.
//!
//! `PUT /account/start-with { api_key, start_with }` sets it on the account
//! that owns the key, or clears it when `start_with` is `null`.
//!
//! The server sends this, not a browser: the owner answers in Setup (or
//! changes the answer in Settings) on their own server, and the server
//! passes the one answer on while it is linked to an account. A free account
//! is an account, so its key works here too. Nothing reads the answer back
//! through the API.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::put,
    Router,
};
use serde::Deserialize;
use serde_json::json;

use super::claim::sha256;
use crate::routes::AppState;

/// The answers a server may send: the ids of `api::profile::StartWith` in
/// virtues-core, and the values the column's CHECK accepts.
const ANSWERS: [&str; 5] = ["days", "patterns", "memory", "habit", "ownership"];

pub fn router() -> Router<AppState> {
    Router::new().route("/account/start-with", put(put_start_with))
}

#[derive(Debug, Deserialize)]
struct Body {
    api_key: String,
    /// One of `ANSWERS`, or `null` to clear it.
    start_with: Option<String>,
}

async fn put_start_with(State(state): State<AppState>, Json(body): Json<Body>) -> axum::response::Response {
    if body.start_with.as_deref().is_some_and(|s| !ANSWERS.contains(&s)) {
        return err(StatusCode::BAD_REQUEST, "invalid_start_with", "start_with is not one of the five answers");
    }
    match set(&state.pool, &body.api_key, body.start_with.as_deref()).await {
        Ok(true) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Ok(false) => err(StatusCode::UNAUTHORIZED, "invalid_api_key", "unknown api key"),
        // A failed query is a 500. A 401 would tell a server its key is dead.
        Err(e) => {
            tracing::warn!("start_with update failed: {e:#}");
            err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "couldn't save the answer")
        }
    }
}

/// Set or clear the answer on the account that owns `api_key`. `false` when
/// no account owns the key.
async fn set(pool: &sqlx::PgPool, api_key: &str, start_with: Option<&str>) -> Result<bool, sqlx::Error> {
    let key_hash = sha256(api_key.as_bytes());
    let Some(account_id) = super::claim::account_id_by_key_hash(pool, &key_hash[..]).await? else {
        return Ok(false);
    };
    let updated = sqlx::query("UPDATE accounts SET start_with = $2 WHERE account_id = $1")
        .bind(&account_id)
        .bind(start_with)
        .execute(pool)
        .await?;
    Ok(updated.rows_affected() == 1)
}

fn err(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (status, Json(json!({ "error": { "code": code, "message": message } }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn account_with_key(pool: &sqlx::PgPool, api_key: &str) {
        sqlx::query("INSERT INTO accounts (account_id, email) VALUES ('acct_x', 'owner@example.com')")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO box_key (api_key_hash, account_id) VALUES ($1, 'acct_x')")
            .bind(sha256(api_key.as_bytes()))
            .execute(pool)
            .await
            .unwrap();
    }

    async fn stored(pool: &sqlx::PgPool) -> Option<String> {
        sqlx::query_scalar("SELECT start_with FROM accounts WHERE account_id = 'acct_x'")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// A free account's key sets the answer, `null` clears it, and a key no
    /// account owns changes nothing.
    #[sqlx::test(migrations = "./migrations")]
    async fn the_key_owner_sets_and_clears_the_answer(pool: sqlx::PgPool) {
        account_with_key(&pool, "vk-example").await;
        assert_eq!(stored(&pool).await, None);

        assert!(set(&pool, "vk-example", Some("habit")).await.unwrap());
        assert_eq!(stored(&pool).await.as_deref(), Some("habit"));

        assert!(!set(&pool, "vk-someone-else", Some("days")).await.unwrap());
        assert_eq!(stored(&pool).await.as_deref(), Some("habit"));

        assert!(set(&pool, "vk-example", None).await.unwrap());
        assert_eq!(stored(&pool).await, None);
    }

    /// The column refuses an answer outside the five, so a route that forgot
    /// to check could not store one.
    #[sqlx::test(migrations = "./migrations")]
    async fn the_column_refuses_any_other_answer(pool: sqlx::PgPool) {
        account_with_key(&pool, "vk-example").await;
        assert!(set(&pool, "vk-example", Some("streaks")).await.is_err());
        assert_eq!(stored(&pool).await, None);
    }
}
