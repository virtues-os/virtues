//! Google Places for subscribers.
//!
//! Places is included in the subscription, so a lookup never touches the
//! wallet. It needs a live subscription (`BearerAuth`) and keeps working when
//! the month's AI credit is spent. A daily limit per account bounds what one
//! account can cost; ordinary use is a handful of lookups a day, far below it.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use chrono::{NaiveDate, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};

use crate::bearer_auth::BearerAuth;
use crate::AppState;

/// Lookups allowed per account per UTC day. Env: `PLACES_DAILY_LIMIT`.
const DEFAULT_DAILY_LIMIT: u32 = 500;

fn daily_limit() -> u32 {
    std::env::var("PLACES_DAILY_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|n: &u32| *n > 0)
        .unwrap_or(DEFAULT_DAILY_LIMIT)
}

/// Today's lookups per account. Held in memory: a restart forgets the day's
/// counts, which only ever lets an account through, and virtues-api runs as
/// one process. Nothing about a lookup is kept beyond the count.
fn counts() -> &'static DashMap<String, (NaiveDate, u32)> {
    static COUNTS: OnceLock<DashMap<String, (NaiveDate, u32)>> = OnceLock::new();
    COUNTS.get_or_init(DashMap::new)
}

/// Count one lookup for the account. `false` when today's limit is reached.
fn take_lookup(account_id: &str, today: NaiveDate, limit: u32) -> bool {
    let mut entry = counts().entry(account_id.to_string()).or_insert((today, 0));
    if entry.0 != today {
        *entry = (today, 0);
    }
    if entry.1 >= limit {
        return false;
    }
    entry.1 += 1;
    true
}

fn limit_reached() -> axum::response::Response {
    error_resp(
        StatusCode::TOO_MANY_REQUESTS,
        "fair_use_limit",
        "Place search is paused until tomorrow. Bank data, maps and AI keep working.",
    )
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/places/autocomplete", post(places_autocomplete))
        .route("/v1/places/:place_id", get(places_details))
}

#[derive(Debug, Deserialize, Serialize)]
struct AutocompleteRequest {
    input: String,
    #[serde(flatten)]
    other: Value,
}

async fn places_autocomplete(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    headers: HeaderMap,
    Json(request): Json<AutocompleteRequest>,
) -> axum::response::Response {
    let Some(api_key) = state.config.google_api_key.as_ref() else {
        return error_resp(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_not_configured",
            "Google Places API key not set",
        );
    };
    let _ = &headers; // X-Virtues-Purpose accepted (v3 no-op telemetry)
    if !take_lookup(&ent.account_id, Utc::now().date_naive(), daily_limit()) {
        return limit_reached();
    }

    // Forward the request body as-is — `#[serde(flatten)] other: Value`
    // ensures any extra fields flow through to Google without name munging.
    let upstream = state
        .http_client
        .post("https://places.googleapis.com/v1/places:autocomplete")
        .header("X-Goog-Api-Key", api_key)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await;

    passthrough(upstream).await
}

async fn places_details(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    headers: HeaderMap,
    Path(place_id): Path<String>,
) -> axum::response::Response {
    let Some(api_key) = state.config.google_api_key.as_ref() else {
        return error_resp(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_not_configured",
            "Google Places API key not set",
        );
    };
    let _ = &headers; // X-Virtues-Purpose accepted (v3 no-op telemetry)
    if !take_lookup(&ent.account_id, Utc::now().date_naive(), daily_limit()) {
        return limit_reached();
    }

    let upstream = state
        .http_client
        .get(format!(
            "https://places.googleapis.com/v1/places/{}",
            place_id
        ))
        .header("X-Goog-Api-Key", api_key)
        .header(
            "X-Goog-FieldMask",
            "id,displayName,formattedAddress,location",
        )
        .send()
        .await;

    passthrough(upstream).await
}

/// Return Google's answer, success or error, as is.
async fn passthrough(upstream: Result<reqwest::Response, reqwest::Error>) -> axum::response::Response {
    match upstream {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or_else(|_| json!({}));
            (
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(body),
            )
                .into_response()
        }
        Err(e) => error_resp(StatusCode::BAD_GATEWAY, "upstream_error", &e.without_url().to_string()),
    }
}

fn error_resp(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_daily_limit_resets_the_next_day() {
        let day1 = NaiveDate::from_ymd_opt(2030, 1, 1).unwrap();
        let day2 = NaiveDate::from_ymd_opt(2030, 1, 2).unwrap();
        let acct = "places-test-reset";
        assert!(take_lookup(acct, day1, 2));
        assert!(take_lookup(acct, day1, 2));
        assert!(!take_lookup(acct, day1, 2));
        assert!(take_lookup(acct, day2, 2));
    }

    #[test]
    fn accounts_have_separate_limits() {
        let day = NaiveDate::from_ymd_opt(2030, 1, 1).unwrap();
        assert!(take_lookup("places-test-a", day, 1));
        assert!(!take_lookup("places-test-a", day, 1));
        assert!(take_lookup("places-test-b", day, 1));
    }
}
