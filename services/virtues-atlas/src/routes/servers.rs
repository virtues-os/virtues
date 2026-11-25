//! The servers on an account, and removing one:
//! `POST /billing/servers { api_key }` → `{ servers: [...] }` and
//! `POST /billing/servers/remove { api_key, id }`.
//!
//! Any server on the account can see the others and remove one. That is how
//! an owner undoes a box attached by mistake: the link confirm page can be
//! clicked through, and nothing else revokes a key. Removing one also ends
//! the account's approved links that no server has collected yet. A server cannot remove
//! itself here (that is unlinking, done on the box).
//!
//! `id` is the hex sha256 of the key, the same form virtues-api's
//! `/internal/device` takes. Two unlabeled keys can share an account, so the
//! endpoint label cannot pick one out. Labels are not returned: they are iroh
//! endpoint ids or random, and mean nothing to a person.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

use super::claim::{account_id_by_key_hash, sha256};
use crate::routes::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/billing/servers", post(list))
        .route("/billing/servers/remove", post(remove))
}

#[derive(Debug, Deserialize)]
struct ListBody {
    api_key: String,
}

#[derive(Debug, Deserialize)]
struct RemoveBody {
    api_key: String,
    id: String,
}

/// The caller's account, from its key. `Err` is the response to send.
async fn caller_account(state: &AppState, api_key: &str) -> Result<(String, Vec<u8>), axum::response::Response> {
    let hash = sha256(api_key.as_bytes());
    match account_id_by_key_hash(&state.pool, &hash).await {
        Ok(Some(a)) => Ok((a, hash)),
        Ok(None) => Err(err(StatusCode::UNAUTHORIZED, "invalid_api_key", "unknown api key")),
        Err(e) => {
            tracing::warn!("servers: key lookup failed: {e:#}");
            Err(err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "lookup failed"))
        }
    }
}

async fn list(State(state): State<AppState>, Json(body): Json<ListBody>) -> axum::response::Response {
    let (account_id, own) = match caller_account(&state, &body.api_key).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let rows: Vec<(Vec<u8>, DateTime<Utc>)> = match sqlx::query_as(
        "SELECT api_key_hash, created_at FROM box_key WHERE account_id = $1 ORDER BY created_at",
    )
    .bind(&account_id)
    .fetch_all(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("servers: list failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not list servers");
        }
    };
    let servers: Vec<_> = rows
        .into_iter()
        .map(|(hash, linked_at)| {
            json!({
                "id": hex::encode(&hash),
                "linked_at": linked_at,
                "this_server": hash == own,
            })
        })
        .collect();
    (StatusCode::OK, Json(json!({ "servers": servers }))).into_response()
}

async fn remove(State(state): State<AppState>, Json(body): Json<RemoveBody>) -> axum::response::Response {
    let (account_id, own) = match caller_account(&state, &body.api_key).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Ok(target) = hex::decode(body.id.trim()) else {
        return err(StatusCode::BAD_REQUEST, "invalid_id", "id must be hex");
    };
    if target == own {
        return err(StatusCode::BAD_REQUEST, "this_server", "a server cannot remove itself here");
    }
    // Only a key on the caller's own account.
    match sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM box_key WHERE account_id = $1 AND api_key_hash = $2)",
    )
    .bind(&account_id)
    .bind(&target)
    .fetch_one(&state.pool)
    .await
    {
        Ok(true) => {}
        Ok(false) => return err(StatusCode::NOT_FOUND, "not_found", "no such server on this account"),
        Err(e) => {
            tracing::warn!("servers: remove lookup failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "lookup failed");
        }
    }
    // virtues-api first: once it refuses the key, the key spends nothing,
    // even if the atlas-side delete below fails and is retried.
    if let Err(e) = state
        .virtues_api
        .revoke_device(&crate::virtues_api_client::RevokeDevice {
            api_key_hash: hex::encode(&target),
            account_id: account_id.clone(),
        })
        .await
    {
        tracing::warn!("servers: revoke in virtues-api failed: {e:#}");
        return err(StatusCode::BAD_GATEWAY, "revoke_failed", "could not remove the server; try again");
    }
    let done = async {
        let mut tx = state.pool.begin().await?;
        sqlx::query("DELETE FROM box_key WHERE account_id = $1 AND api_key_hash = $2")
            .bind(&account_id)
            .bind(&target)
            .execute(&mut *tx)
            .await?;
        // An atlas image older than this one also authenticates by the
        // `customers.api_key_hash` mirror, and it is the rollback image.
        // TODO(2026-11-20): drop this and the column once no rollback image
        // reads the mirror.
        sqlx::query("UPDATE customers SET api_key_hash = NULL WHERE account_id = $1 AND api_key_hash = $2")
            .bind(&account_id)
            .bind(&target)
            .execute(&mut *tx)
            .await?;
        // An approved link no server has collected would hand out a new key
        // after this; the attach being undone may have come through one.
        super::link::expire_approved_links(&mut tx, &account_id).await?;
        tx.commit().await
    }
    .await;
    if let Err(e) = done {
        tracing::warn!("servers: atlas-side remove failed: {e:#}");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "could not finish removing the server; try again");
    }
    tracing::info!(account_id = %account_id, "server removed by its owner");
    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}

fn err(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (status, Json(json!({ "error": { "code": code, "message": message } }))).into_response()
}
