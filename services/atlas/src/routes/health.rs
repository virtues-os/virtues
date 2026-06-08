use axum::{extract::State, response::Json, routing::get, Router};
use serde_json::{json, Value};

use super::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    let db_ok = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .is_ok();
    Json(json!({
        "service": "atlas",
        "status": if db_ok { "ok" } else { "degraded" },
        "db": db_ok,
    }))
}
