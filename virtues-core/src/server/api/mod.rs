//! REST API handlers.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use super::webhook::AppState;
use crate::error::Error;

mod applets;
pub use applets::*;

/// Sanitize a filename for use in Content-Disposition headers.
/// Removes characters that could cause header injection or parsing issues.
fn sanitize_content_disposition(filename: &str) -> String {
    filename
        .replace('"', "'")
        .replace('\\', "_")
        .replace('\r', "")
        .replace('\n', "")
}

/// Helper to convert Result to Response with proper status code
fn api_response<T: Serialize>(result: crate::error::Result<T>) -> Response {
    result.map(Json).into_response()
}

/// Helper to convert Error to Response with appropriate status code
fn error_response(error: Error) -> Response {
    error.into_response()
}

/// Helper to create a success message response
fn success_message(message: &str) -> Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "message": message })),
    )
        .into_response()
}

// =============================================================================
// Profile API
// =============================================================================

/// Get user profile
pub async fn get_profile_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_profile(state.db.pool()).await)
}

/// Update user profile
pub async fn update_profile_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::UpdateProfileRequest>,
) -> Response {
    api_response(crate::api::update_profile(state.db.pool(), request).await)
}

// =============================================================================
// Assistant Profile API
// =============================================================================

/// Get assistant profile
pub async fn get_assistant_profile_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_assistant_profile(state.db.pool()).await)
}

/// Update assistant profile
pub async fn update_assistant_profile_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::UpdateAssistantProfileRequest>,
) -> Response {
    api_response(crate::api::update_assistant_profile(state.db.pool(), request).await)
}

// =============================================================================
// Models API
// =============================================================================

/// List all available models
pub async fn list_models_handler() -> Response {
    api_response(crate::api::list_models().await)
}

/// Get a specific model by ID
pub async fn get_model_handler(Path(model_id): Path<String>) -> Response {
    api_response(crate::api::get_model(&model_id).await)
}

/// The picker plus the live slot map — what "Virtues default · <model>" needs.
///
/// `/api/models` stays a bare array (the picker's existing contract); this
/// route adds `slots`, so the settings UI can name the model a slot currently
/// resolves to without a second round trip.
pub async fn list_models_with_slots_handler() -> Response {
    api_response(crate::api::list_models_with_slots().await)
}

/// Per-stream ingest freshness, worst-first. The signal that was missing when
/// messages, the calendar sync, and finance each went dark unnoticed.
pub async fn stream_health_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::stream_health(&state.db).await)
}

#[derive(Debug, Deserialize)]
pub struct StreamDaysQuery {
    /// Window length. Twelve weeks by default — long enough to show a rhythm
    /// and a stoppage without the cells becoming unreadably thin.
    #[serde(default)]
    pub days: Option<i64>,
}

pub async fn stream_days_handler(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<StreamDaysQuery>,
) -> Response {
    api_response(crate::api::stream_health::stream_days(&state.db, q.days.unwrap_or(84)).await)
}

// ============================================================================
// Onboarding API
// ============================================================================

// =============================================================================
// Places API Handlers (Google Places proxy)
// =============================================================================

/// Resolve an autocomplete prediction to coordinates (the phone's "mute a
/// place I have never been" door creates the place from this).
pub async fn places_details_handler(
    State(state): State<AppState>,
    Query(request): Query<crate::api::places::PlaceDetailsRequest>,
) -> Response {
    api_response(crate::api::get_place_details(state.db.pool(), request).await)
}

/// Get autocomplete predictions for an address query
pub async fn places_autocomplete_handler(
    State(state): State<AppState>,
    Query(request): Query<crate::api::AutocompleteRequest>,
) -> Response {
    match crate::api::autocomplete(state.db.pool(), request).await {
        Ok(response) => {
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Subscription & Billing API Handlers
// =============================================================================

/// GET /api/subscription - Local subscription signal (api_key present?).
///
/// Derived from the credential vault: reports whether an api_key has
/// been claimed on this box. Gating itself is by bearer expiry, not this
/// endpoint — see `crate::api::subscription`.
pub async fn get_subscription_handler(
    State(pool): State<sqlx::PgPool>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    // `?fresh=1` bypasses the entitlement cache — the UI polls with it while
    // a checkout the owner just opened is in flight.
    let fresh = q.get("fresh").is_some_and(|v| v == "1" || v == "true");
    match crate::api::subscription::get_subscription_status(&pool, fresh).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            // NOT a fallback to `active`. It was one, and that turned a blipped
            // vault read into a subscription the owner does not have — the
            // banned "broken query becomes a plausible value" (CLAUDE.md), on
            // the one endpoint whose whole job is to say where they stand.
            // Unknown is a state the UI can render; a lie is not.
            tracing::warn!("subscription status check failed: {e}");
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "unknown",
                    "linked": false,
                    "subscribed": false,
                    "entitlement_known": false,
                    "is_active": false
                })),
            )
                .into_response()
        }
    }
}

/// POST /api/billing/portal - Stripe billing portal.
///
/// The portal belongs to Atlas (which holds the Stripe customer). We read the
/// api_key from the local vault, ask Atlas to mint a Stripe-hosted
/// Customer Portal session, and return its `url` for BillingView to open.
/// Any failure (no api_key yet, inactive subscription, Stripe hiccup)
/// returns a clean `{error, code}` the button renders inline — never a 500.
///
/// `code` exists because the prose cannot carry the branch: three unrelated
/// conditions printed "Couldn't open the billing portal. Try again.", so a
/// screenshot from an owner told us nothing about which one to go fix. The
/// vocabulary is atlas's own codes passed through (`no_subscription`,
/// `subscription_inactive`, `stripe_error`, `invalid_api_key`, `internal`)
/// plus the three failures that never reach atlas:
/// `not_linked`, `vault_unreadable`, `atlas_unreachable`.
pub async fn create_billing_portal_handler(State(pool): State<sqlx::PgPool>) -> Response {
    /// Every refusal is a 200 with prose for the owner and a code for us.
    fn refuse(code: &str, message: &str) -> Response {
        (
            StatusCode::OK,
            Json(serde_json::json!({ "error": message, "code": code })),
        )
            .into_response()
    }
    // The sentence the owner sees when the failure is ours, not theirs — the
    // code beside it is what says which "ours".
    const TRY_AGAIN: &str = "Couldn't open the billing portal. Try again.";

    let api_key = match crate::virtues_api::renew::read_api_key(&pool).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return refuse(
                "not_linked",
                "Connect your subscription first, then you can manage billing here.",
            );
        }
        Err(e) => {
            tracing::warn!("billing portal [vault_unreadable]: {e}");
            return refuse("vault_unreadable", TRY_AGAIN);
        }
    };

    let atlas_url =
        crate::virtues_api::atlas_url();
    // The box has no stable public URL, so we don't supply a return_url —
    // Atlas defaults it to its own public billing page (where Stripe sends the
    // customer after they click "Return to Virtues").
    let http = crate::http_client::virtues_api_client();

    match crate::virtues_api::renew::fetch_portal_session(&http, &atlas_url, &api_key, "")
        .await
    {
        Ok(crate::virtues_api::renew::PortalSession::Url(url)) => {
            (StatusCode::OK, Json(serde_json::json!({ "url": url }))).into_response()
        }
        // A linked account with no active subscription (free, or lapsed):
        // valid key, nothing to open. Permanent until they subscribe —
        // "try again" would be a lie.
        Ok(crate::virtues_api::renew::PortalSession::NoSubscription { code }) => refuse(
            &code,
            "No active subscription on this account - start one and you can manage billing here.",
        ),
        // atlas answered and refused for its own reason. Its code is the one
        // worth showing: it is the one their logs are keyed on too.
        Ok(crate::virtues_api::renew::PortalSession::Failed { code, status }) => {
            tracing::warn!("billing portal [{code}]: atlas answered {status}");
            refuse(&code, TRY_AGAIN)
        }
        // Never got an answer: DNS, TLS, timeout, or a body we couldn't read.
        Err(e) => {
            tracing::warn!("billing portal [atlas_unreachable]: {e}");
            refuse("atlas_unreachable", TRY_AGAIN)
        }
    }
}

/// POST /api/billing/subscribe — a Stripe Checkout URL for the account this
/// box is linked to. The door for a linked FREE account (0017): the connect
/// flow would start a new device link and mint a second account, and the
/// portal has nothing to open. Same `{url}` / `{error, code}` contract as the
/// portal, for the same reason.
pub async fn subscribe_billing_handler(State(pool): State<sqlx::PgPool>) -> Response {
    fn refuse(code: &str, message: &str) -> Response {
        (StatusCode::OK, Json(serde_json::json!({ "error": message, "code": code }))).into_response()
    }
    const TRY_AGAIN: &str = "Couldn't open checkout. Try again.";

    let api_key = match crate::virtues_api::renew::read_api_key(&pool).await {
        Ok(Some(t)) => t,
        Ok(None) => return refuse("not_linked", "Connect your Virtues account first."),
        Err(e) => {
            tracing::warn!("billing subscribe [vault_unreadable]: {e}");
            return refuse("vault_unreadable", TRY_AGAIN);
        }
    };
    let atlas_url = crate::virtues_api::atlas_url();
    let http = crate::http_client::virtues_api_client();
    match crate::virtues_api::renew::fetch_checkout_session(&http, &atlas_url, &api_key).await {
        Ok(crate::virtues_api::renew::CheckoutSession::Url(url)) => {
            // Whatever we cached is about to be stale; the UI polls fresh.
            crate::api::subscription::invalidate();
            (StatusCode::OK, Json(serde_json::json!({ "url": url }))).into_response()
        }
        Ok(crate::virtues_api::renew::CheckoutSession::AlreadySubscribed) => {
            crate::api::subscription::invalidate();
            refuse("already_subscribed", "This account already has an active subscription.")
        }
        Ok(crate::virtues_api::renew::CheckoutSession::Failed { code, status }) => {
            tracing::warn!("billing subscribe [{code}]: atlas answered {status}");
            refuse(&code, TRY_AGAIN)
        }
        Err(e) => {
            tracing::warn!("billing subscribe [atlas_unreachable]: {e}");
            refuse("atlas_unreachable", TRY_AGAIN)
        }
    }
}

/// GET /api/billing/usage — wallet balance + recent ledger for BillingView.
///
/// Proxies virtues-api `GET /v1/usage` (authenticated with the box's device
/// api_key). Returns `{ balance_micros, month_to_date_micros, expires_at,
/// entries: [{ ts, micros, kind, real_micros }] }`, or a clean `{error}`
/// (never a 500) when not linked / the proxy is unreachable.
pub async fn billing_usage_handler(State(pool): State<sqlx::PgPool>) -> Response {
    if !crate::virtues_api::renew::has_api_key(&pool).await.unwrap_or(false) {
        return (
            StatusCode::OK,
            Json(serde_json::json!({ "error": "Connect your Virtues account to see your balance." })),
        )
            .into_response();
    }
    let client = crate::virtues_api::client::BearerClient::from_env(pool);
    match client.get_json("/v1/usage").await {
        Ok(resp) if resp.is_success() => (StatusCode::OK, Json(resp.body)).into_response(),
        Ok(resp) => {
            tracing::warn!("billing usage: proxy returned {}", resp.status);
            (StatusCode::OK, Json(serde_json::json!({ "error": "Couldn't load your balance. Try again." }))).into_response()
        }
        Err(e) => {
            tracing::warn!("billing usage: proxy call failed: {e}");
            (StatusCode::OK, Json(serde_json::json!({ "error": "Couldn't load your balance. Try again." }))).into_response()
        }
    }
}

/// GET /api/usage/summary — box-local AI spend breakdown for the Usage tab.
///
/// Reads `app_ai_calls` (the per-call cost log) and returns spend grouped by
/// feature and by model since the start of the current UTC month, plus the
/// month boundary. The wallet headline (balance/month-to-date) comes from the
/// separate `/api/billing/usage` proxy — this endpoint is purely the local
/// "where did my money go" detail. No egress.
pub async fn usage_summary_handler(State(state): State<AppState>) -> Response {
    use chrono::Datelike;
    let now = chrono::Utc::now();
    let month_start = chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc())
        .unwrap_or(now);

    let pool = state.db.pool();
    // Errors surface. `.unwrap_or_default()` here rendered a broken query as
    // an empty month — a wallet with no spend is a real state, and this made
    // a failure indistinguishable from it (CLAUDE.md, "Do not swallow").
    let by_feature = match crate::api::ai_calls::spend_by_feature(pool, month_start).await {
        Ok(v) => v,
        Err(e) => return error_response(e.into()),
    };
    let by_model = match crate::api::ai_calls::spend_by_model(pool, month_start).await {
        Ok(v) => v,
        Err(e) => return error_response(e.into()),
    };
    // The Billing chart: the last 14 UTC days, today included. Days with no
    // calls are absent from the rows; the client fills the run.
    let since = (now - chrono::Duration::days(13)).date_naive().and_hms_opt(0, 0, 0)
        .map(|dt| dt.and_utc())
        .unwrap_or(now);
    let by_day = match crate::api::ai_calls::spend_by_day(pool, since).await {
        Ok(v) => v,
        Err(e) => return error_response(e.into()),
    };
    // Last month, to the same point: the fair comparison on the 4th. None
    // when the calendar cannot supply it (the 31st of a month after a 30-day
    // one) — the client then shows no delta rather than a wrong one.
    let prev_start = month_start
        .date_naive()
        .checked_sub_months(chrono::Months::new(1))
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc());
    let prev_same_point = prev_start.and_then(|ps| ps.checked_add_signed(now - month_start));
    let last_month_to_date_micros = match (prev_start, prev_same_point) {
        (Some(a), Some(b)) => match crate::api::ai_calls::wallet_spend_between(pool, a, b).await {
            Ok(v) => Some(v),
            Err(e) => return error_response(e.into()),
        },
        _ => None,
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "month_start": month_start,
            "by_feature": by_feature,
            "by_model": by_model,
            "by_day": by_day,
            "last_month_to_date_micros": last_month_to_date_micros,
        })),
    )
        .into_response()
}

/// GET /api/telemetry/ai-calls — one page of the AI-call log for the Usage page
/// (the window that was missing when the transcription runaway burned the
/// wallet invisibly). Box-local `app_ai_calls`, newest first.
pub async fn ai_calls_handler(
    State(state): State<AppState>,
    Query(query): Query<crate::api::ai_calls::AiCallsQuery>,
) -> Response {
    match crate::api::ai_calls::list_calls(state.db.pool(), query).await {
        Ok(page) => (StatusCode::OK, Json(page)).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "ai_calls query failed");
            (
                StatusCode::OK,
                Json(crate::api::ai_calls::AiCallPage {
                    items: Vec::new(),
                    total: 0,
                }),
            )
                .into_response()
        }
    }
}

/// POST /api/billing/link/start — begin the device-authorization link flow.
///
/// The web "Connect subscription" button calls this, then opens the returned
/// `verification_uri_complete` and polls `link/status`. The secret device_code
/// stays box-side; only the user-facing bits are returned to the browser.
pub async fn billing_link_start_handler(State(pool): State<sqlx::PgPool>) -> Response {
    let atlas_url =
        crate::virtues_api::atlas_url();
    let http = crate::http_client::virtues_api_client();
    match crate::virtues_api::link::start(&pool, &http, &atlas_url).await {
        Ok(s) => (StatusCode::OK, Json(serde_json::json!(s))).into_response(),
        Err(e) => {
            tracing::warn!("billing link start failed: {e}");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

/// GET /api/billing/link/status — poll the in-flight link. On `ready` this
/// stores the api_key (atlas registers the device + funds the wallet).
pub async fn billing_link_status_handler(State(pool): State<sqlx::PgPool>) -> Response {
    let atlas_url =
        crate::virtues_api::atlas_url();
    let http = crate::http_client::virtues_api_client();
    match crate::virtues_api::link::poll(&pool, &http, &atlas_url).await {
        Ok(status) => {
            // A fresh link is a new key and possibly a new subscription; the
            // cached entitlement is now about the previous account.
            if status == crate::virtues_api::link::LinkStatus::Ready {
                crate::api::subscription::invalidate();
            }
            (StatusCode::OK, Json(serde_json::json!({ "status": status }))).into_response()
        }
        Err(e) => {
            tracing::warn!("billing link status failed: {e}");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

// =============================================================================
// System Update API Handlers
// =============================================================================

// =============================================================================
// Unsplash API Handler
// =============================================================================

/// Search Unsplash photos for cover images
pub async fn unsplash_search_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::UnsplashSearchRequest>,
) -> Response {
    api_response(crate::api::unsplash_search(state.db.pool(), request).await)
}


// =============================================================================
// Entities API - Places
// =============================================================================

/// List all known places
pub async fn list_places_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_places(state.db.pool()).await)
}

/// Create a place by hand — the phone's "Mute here" and its Google-places door.
pub async fn create_place_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreatePlaceRequest>,
) -> Response {
    api_response(crate::api::create_place(state.db.pool(), request).await)
}

/// Get a specific place by ID
pub async fn get_place_handler(
    State(state): State<AppState>,
    Path(place_id): Path<String>,
) -> Response {
    api_response(crate::api::get_place(state.db.pool(), place_id).await)
}

/// Update an existing place
pub async fn update_place_handler(
    State(state): State<AppState>,
    Path(place_id): Path<String>,
    Json(request): Json<crate::api::UpdatePlaceRequest>,
) -> Response {
    api_response(crate::api::update_place(state.db.pool(), place_id, request).await)
}

/// Delete a place
pub async fn delete_place_handler(
    State(state): State<AppState>,
    Path(place_id): Path<String>,
) -> Response {
    match crate::api::delete_place(state.db.pool(), place_id).await {
        Ok(_) => success_message("Place deleted successfully"),
        Err(e) => error_response(e),
    }
}

#[derive(serde::Deserialize)]
pub struct CreateEntityBody {
    pub name: String,
}

/// Create a person by hand.
pub async fn create_person_handler(
    State(state): State<AppState>,
    Json(b): Json<CreateEntityBody>,
) -> Response {
    match crate::api::entities::create_person(state.db.pool(), &b.name).await {
        Ok(id) => api_response(Ok::<_, crate::error::Error>(
            serde_json::json!({ "id": id, "route": format!("/person/{id}") }),
        )),
        Err(e) => error_response(e),
    }
}

/// Delete a person, and everything that pointed at them.
pub async fn delete_person_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::entities::delete_person(state.db.pool(), id).await {
        Ok(()) => success_message("Person deleted"),
        Err(e) => error_response(e),
    }
}

/// Delete an organization, and everything that pointed at it.
pub async fn delete_org_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::entities::delete_organization(state.db.pool(), id).await {
        Ok(()) => success_message("Organization deleted"),
        Err(e) => error_response(e),
    }
}

#[derive(serde::Deserialize)]
pub struct LifelineQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub buckets: Option<i32>,
    /// Comma-separated lane ids; absent = every lane.
    pub lanes: Option<String>,
    /// Comma-separated lane ids to split into their member tables.
    pub expand: Option<String>,
    /// Comma-separated `lane:measure_id` pairs — what each lane should plot
    /// instead of a row count. Unknown pairs fall back to the default.
    pub measures: Option<String>,
    /// Feed paging.
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// IANA zone the day-clock's hours are read in. One zone for the whole
    /// raster, so travel shows as a dislocation rather than being normalised
    /// away. Unknown names fall back to UTC.
    pub tz: Option<String>,
}

/// Per-lane density over a window — the lifeline's only data source.
pub async fn lifeline_handler(
    State(state): State<AppState>,
    Query(q): Query<LifelineQuery>,
) -> Response {
    // No window given = the whole life. Computed from the data, because a
    // lifeline that defaults to the last 365 days is not a lifeline.
    let (span_from, span_to) = match crate::api::lifeline::corpus_span(state.db.pool()).await {
        Ok(s) => s,
        Err(e) => return error_response(e),
    };
    let to = q
        .to
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or(span_to);
    let from = q
        .from
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or(span_from);
    let csv = |v: Option<String>| {
        v.map(|s| {
            s.split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
        })
    };
    let lanes = csv(q.lanes);
    let expand = csv(q.expand);
    let measures = csv(q.measures);

    api_response(
        crate::api::lifeline::get_lifeline(
            state.db.pool(),
            from,
            to,
            q.buckets.unwrap_or(365),
            lanes,
            expand,
            measures,
        )
        .await,
    )
}

/// Where a window was spent — the location lane's second view.
pub async fn lifeline_ground_handler(
    State(state): State<AppState>,
    Query(q): Query<LifelineQuery>,
) -> Response {
    let (span_from, span_to) = match crate::api::lifeline::corpus_span(state.db.pool()).await {
        Ok(s) => s,
        Err(e) => return error_response(e),
    };
    let parse = |s: Option<String>, fallback: chrono::DateTime<chrono::Utc>| {
        s.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or(fallback)
    };
    api_response(
        crate::api::lifeline::get_ground(
            state.db.pool(),
            parse(q.from, span_from),
            parse(q.to, span_to),
        )
        .await,
    )
}

/// Time-of-day against date — the lifeline's primary band.
pub async fn lifeline_clock_handler(
    State(state): State<AppState>,
    Query(q): Query<LifelineQuery>,
) -> Response {
    let (span_from, span_to) = match crate::api::lifeline::corpus_span(state.db.pool()).await {
        Ok(s) => s,
        Err(e) => return error_response(e),
    };
    let parse = |s: Option<String>, fallback: chrono::DateTime<chrono::Utc>| {
        s.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or(fallback)
    };
    api_response(
        crate::api::lifeline::get_clock(
            state.db.pool(),
            parse(q.from, span_from),
            parse(q.to, span_to),
            q.buckets.unwrap_or(720),
            q.tz.as_deref().unwrap_or("UTC"),
        )
        .await,
    )
}

/// The records inside a window — what a selection actually contains.
pub async fn lifeline_feed_handler(
    State(state): State<AppState>,
    Query(q): Query<LifelineQuery>,
) -> Response {
    let (span_from, span_to) = match crate::api::lifeline::corpus_span(state.db.pool()).await {
        Ok(s) => s,
        Err(e) => return error_response(e),
    };
    let parse = |s: Option<String>, fallback: chrono::DateTime<chrono::Utc>| {
        s.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or(fallback)
    };
    let lanes = q.lanes.map(|s| {
        s.split(',')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>()
    });
    api_response(
        crate::api::lifeline::get_feed(
            state.db.pool(),
            parse(q.from, span_from),
            parse(q.to, span_to),
            lanes,
            q.limit.unwrap_or(50),
            q.offset.unwrap_or(0),
        )
        .await,
    )
}

/// What Virtues has interpreted inside a window — days and events.
pub async fn lifeline_processed_handler(
    State(state): State<AppState>,
    Query(q): Query<LifelineQuery>,
) -> Response {
    let (span_from, span_to) = match crate::api::lifeline::corpus_span(state.db.pool()).await {
        Ok(s) => s,
        Err(e) => return error_response(e),
    };
    let parse = |s: Option<String>, fallback: chrono::DateTime<chrono::Utc>| {
        s.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or(fallback)
    };
    api_response(
        crate::api::lifeline::get_processed(
            state.db.pool(),
            parse(q.from, span_from),
            parse(q.to, span_to),
            q.limit.unwrap_or(80),
        )
        .await,
    )
}

/// Notes on a subject.
/// GET /api/assistant/memories — the live memories, lane-grouped.
pub async fn list_assistant_memories_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::assistant_memories::list_memories(state.db.pool()).await)
}

/// PUT /api/assistant/memories/:id — the person rewrites one in their words.
pub async fn edit_assistant_memory_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<crate::api::assistant_memories::EditMemoryRequest>,
) -> Response {
    api_response(crate::api::assistant_memories::edit_memory(state.db.pool(), id, &req.body).await)
}

/// DELETE /api/assistant/memories/:id — soft-retire with provenance.
pub async fn retire_assistant_memory_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Response {
    api_response(
        crate::api::assistant_memories::retire_memory(state.db.pool(), id)
            .await
            .map(|_| serde_json::json!({ "retired": true })),
    )
}

pub async fn list_notes_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
    Query(q): Query<NotesQuery>,
) -> Response {
    api_response(
        crate::api::wiki_notes::list_notes(
            state.db.pool(),
            &subject_type,
            &subject_id,
            q.include_resolved.unwrap_or(false),
        )
        .await,
    )
}

#[derive(serde::Deserialize)]
pub struct NotesQuery {
    pub include_resolved: Option<bool>,
}

/// Open notes across the whole record — the Overview's what-changed count.
pub async fn open_notes_count_handler(State(state): State<AppState>) -> Response {
    api_response(
        crate::api::wiki_notes::count_open_total(state.db.pool())
            .await
            .map(|n| serde_json::json!({ "open": n })),
    )
}

#[derive(serde::Deserialize)]
pub struct CreateNoteBody {
    pub body: String,
    pub kind: Option<String>,
}

/// Leave a note on a subject.
pub async fn create_note_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
    Json(b): Json<CreateNoteBody>,
) -> Response {
    api_response(
        crate::api::wiki_notes::create_note(
            state.db.pool(),
            &subject_type,
            &subject_id,
            b.kind.as_deref().unwrap_or("memo"),
            &b.body,
        )
        .await,
    )
}

#[derive(serde::Deserialize)]
pub struct ResolveNoteBody {
    pub resolution: String,
}

/// Accept or dismiss a note.
pub async fn resolve_note_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<ResolveNoteBody>,
) -> Response {
    match crate::api::wiki_notes::resolve_note(state.db.pool(), id, &b.resolution).await {
        Ok(()) => success_message("Note closed"),
        Err(e) => error_response(e),
    }
}

/// One article's edit history, with diffs.
pub async fn article_history_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
) -> Response {
    api_response(
        crate::api::wiki_articles::get_article_history(
            state.db.pool(),
            &subject_type,
            &subject_id,
        )
        .await,
    )
}

/// The wiki's History room: every recent edit to any article.
pub async fn history_feed_handler(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Response {
    api_response(
        crate::api::wiki_articles::get_history_feed(state.db.pool(), q.limit.unwrap_or(50)).await,
    )
}

/// Everything that mentions this subject.
pub async fn subject_backlinks_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
) -> Response {
    api_response(
        crate::api::wiki_articles::get_subject_backlinks(
            state.db.pool(),
            &subject_type,
            &subject_id,
        )
        .await,
    )
}

/// Write a subject's article, now, because someone asked for it.
///
/// A plain handler rather than a trip through the applet runner. The applet
/// path looked available — `entity_article` declares a `manual` trigger — but
/// it ships `default_enabled = false` and `prepare_run` refuses disabled
/// applets, so the button would 404 on a fresh box; its singleton concurrency
/// gate turns a second click into a `skipped` run, which is wrong for
/// per-subject work; and its entry point takes no target, so there is no way to
/// say *this one*. The applet stays the cron host; this is the door.
///
/// Synchronous on purpose: it is one model call the user is waiting for, and
/// returning 202 would mean polling `app_applet_runs` to find out whether your
/// own click worked.
/// GET one subject's article row — the join, not the prose. The frontend
/// uses `page_id` to open the article in the page editor.
pub async fn get_article_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
) -> Response {
    match crate::api::wiki_articles::get_article(state.db.pool(), &subject_type, &subject_id).await
    {
        Ok(Some(a)) => Json(a).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => error_response(e),
    }
}

pub async fn write_article_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
) -> Response {
    api_response(
        crate::api::entity_article_gen::write_entity_article_now(
            state.db.pool(),
            &subject_type,
            &subject_id,
        )
        .await,
    )
}

/// The owner's own page: their name, their document, and the apparatus.
///
/// Also the one place that ensures they have a row among the people of their
/// own wiki — nothing else ever created one.
pub async fn wiki_me_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::me::get_me(state.db.pool()).await)
}

/// The stories: subjects the person named because they mattered.
pub async fn wiki_list_stories_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::stories::list_stories(state.db.pool()).await)
}

pub async fn wiki_get_story_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::stories::get_story(state.db.pool(), &id).await)
}

/// Start a story. Only the person may: the editor's constitution forbids it
/// from creating a subject, because naming one is a claim about what mattered.
pub async fn wiki_create_story_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("");
    api_response(crate::api::stories::create_story(state.db.pool(), title).await)
}

pub async fn wiki_update_story_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(fields): Json<crate::api::stories::StoryFields>,
) -> Response {
    api_response(crate::api::stories::update_story(state.db.pool(), &id, &fields).await)
}

pub async fn wiki_delete_story_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::stories::delete_story(state.db.pool(), &id).await {
        Ok(()) => success_message("Removed"),
        Err(e) => error_response(e),
    }
}

/// Give a story a page. Seeded with THEIR words, not a machine draft — a story
/// has nothing beneath it to draft from, and the editor fills it by searching.
pub async fn wiki_start_story_article_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::stories::start_article(state.db.pool(), &id).await)
}

/// Edit a chapter. There has never been a way to change one.
pub async fn wiki_update_chapter_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(edit): Json<crate::api::narrative_draft::ChapterEdit>,
) -> Response {
    api_response(crate::api::narrative_draft::update_chapter(state.db.pool(), &id, &edit).await)
}

/// Replace every chapter with the timeline the person drew in Getting started.
/// Refused once the chapters have pages of their own; see `replace_chapters`.
pub async fn wiki_replace_chapters_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::api::narrative_draft::ReplaceChapters>,
) -> Response {
    api_response(
        crate::api::narrative_draft::replace_chapters(state.db.pool(), &req.chapters)
            .await
            .map(|chapters| serde_json::json!({ "chapters": chapters })),
    )
}

/// Unname a chapter, leaving the years it covered as an unnamed stretch.
pub async fn wiki_delete_chapter_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::narrative_draft::delete_chapter(state.db.pool(), &id).await)
}

/// Every year of the life, newest first. Derived — opening the index writes
/// nothing.
pub async fn wiki_list_years_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::years::list_years(state.db.pool()).await)
}

/// One year's page. Creates its row on the way, which is the lazy half of the
/// partition: every year is there to read, and none is written until asked for.
pub async fn wiki_get_year_handler(
    State(state): State<AppState>,
    Path(year): Path<i32>,
) -> Response {
    api_response(crate::api::years::get_year(state.db.pool(), year).await)
}

/// Set what only the person can say about a year.
pub async fn wiki_update_year_handler(
    State(state): State<AppState>,
    Path(year): Path<i32>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let title = body.get("title").and_then(|v| v.as_str());
    let summary = body.get("summary").and_then(|v| v.as_str());
    match crate::api::years::update_year(state.db.pool(), year, title, summary).await {
        Ok(()) => success_message("Saved"),
        Err(e) => error_response(e),
    }
}

/// Write a year's first article.
pub async fn wiki_write_year_article_handler(
    State(state): State<AppState>,
    Path(year): Path<i32>,
) -> Response {
    api_response(crate::api::years::write_year_article(state.db.pool(), year).await)
}

/// Put a named version of an article back.
///
/// Rule 4 of the wiki's paradigm — every edit is a revision you can read AND
/// revert — has been half true since the history feed shipped: the diff was
/// readable and there was no way to undo it.
pub async fn revert_article_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let Some(version) = body.get("version_number").and_then(|v| v.as_i64()) else {
        return error_response(Error::InvalidInput(
            "version_number is required".to_string(),
        ));
    };
    match crate::api::wiki_editor::revert_article(
        state.db.pool(),
        &state.yjs_state,
        &subject_type,
        &subject_id,
        version,
    )
    .await
    {
        Ok(change) => success_message(&format!("Reverted to v{version} — {change}")),
        Err(e) => error_response(e),
    }
}

/// Set how an article is maintained: always, auto, or never.
pub async fn set_article_maintenance_handler(
    State(state): State<AppState>,
    Path((subject_type, subject_id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let mode = body.get("maintenance").and_then(|v| v.as_str()).unwrap_or("");
    match crate::api::wiki_articles::set_maintenance(
        state.db.pool(),
        &subject_type,
        &subject_id,
        mode,
    )
    .await
    {
        Ok(()) => success_message(match mode {
            "always" => "The record will revisit this whenever anything changes",
            "never" => "The record will leave this article alone",
            _ => "The record will keep this up to date",
        }),
        Err(e) => error_response(e),
    }
}


/// Reclassify a person as an organization.
///
/// Returns the new org id so the caller can navigate to it — the person route
/// it came from stops resolving the moment this succeeds.
pub async fn reclassify_person_handler(
    State(state): State<AppState>,
    Path(person_id): Path<String>,
) -> Response {
    match crate::api::entities::reclassify_person_as_organization(state.db.pool(), person_id).await {
        Ok(org_id) => api_response(Ok::<_, crate::error::Error>(
            serde_json::json!({ "id": org_id, "route": format!("/org/{org_id}") }),
        )),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Wiki API Handlers
// ============================================================================

// --- Person ---

/// Get a person by ID
pub async fn wiki_get_person_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_person(state.db.pool(), id).await)
}

/// List all people
pub async fn wiki_list_people_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_people(state.db.pool()).await)
}

/// Update a person by ID
pub async fn wiki_update_person_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdateWikiPersonRequest>,
) -> Response {
    api_response(crate::api::update_person(state.db.pool(), id, request).await)
}

// --- Place ---

/// Get a place by ID
pub async fn wiki_get_place_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_wiki_place(state.db.pool(), id).await)
}

/// List all places (wiki view)
pub async fn wiki_list_places_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_wiki_places(state.db.pool()).await)
}

/// Update a place by ID (wiki fields)
pub async fn wiki_update_place_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdateWikiPlaceRequest>,
) -> Response {
    api_response(crate::api::update_wiki_place(state.db.pool(), id, request).await)
}

// --- Organization ---

/// Get an organization by ID
pub async fn wiki_get_organization_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_organization(state.db.pool(), id).await)
}

/// List all organizations
pub async fn wiki_list_organizations_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_organizations(state.db.pool()).await)
}

/// Update an organization by ID
pub async fn wiki_update_organization_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdateWikiOrganizationRequest>,
) -> Response {
    api_response(crate::api::update_organization(state.db.pool(), id, request).await)
}

// --- Narrative Identity ---

/// Get narrative identity
pub async fn wiki_get_narrative_identity_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_narrative_identity(state.db.pool()).await)
}

// --- Day ---

#[derive(Deserialize)]
pub struct WikiDayQuery {
    pub start_date: Option<chrono::NaiveDate>,
    pub end_date: Option<chrono::NaiveDate>,
}

/// Get a day by date
pub async fn wiki_get_day_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_or_create_day(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}



/// List days in a date range
pub async fn wiki_list_days_handler(
    State(state): State<AppState>,
    Query(query): Query<WikiDayQuery>,
) -> Response {
    let today = chrono::Utc::now().date_naive();
    let start_date = query
        .start_date
        .unwrap_or(today - chrono::Duration::days(30));
    let end_date = query.end_date.unwrap_or(today);
    api_response(crate::api::list_days(state.db.pool(), start_date, end_date).await)
}

/// Per-day activity counts for the wiki calendar heatmap
pub async fn wiki_day_activity_handler(
    State(state): State<AppState>,
    Query(query): Query<WikiDayQuery>,
) -> Response {
    let today = chrono::Utc::now().date_naive();
    let start_date = query
        .start_date
        .unwrap_or(today - chrono::Duration::days(365));
    let end_date = query.end_date.unwrap_or(today);
    api_response(crate::api::day_activity(state.db.pool(), start_date, end_date).await)
}

#[derive(Deserialize)]
pub struct OnThisDayQuery {
    pub date: Option<chrono::NaiveDate>,
}

/// Past years' entries for the same calendar date
pub async fn wiki_on_this_day_handler(
    State(state): State<AppState>,
    Query(query): Query<OnThisDayQuery>,
) -> Response {
    let date = query.date.unwrap_or_else(|| chrono::Utc::now().date_naive());
    api_response(crate::api::on_this_day(state.db.pool(), date).await)
}

#[derive(Deserialize)]
pub struct EntityRecordsQuery {
    pub offset: Option<i64>,
    pub limit: Option<i64>,
    pub search: Option<String>,
    /// Comma-separated raw source_types to include (empty/absent = all).
    pub types: Option<String>,
    /// "asc" for oldest-first; anything else is newest-first.
    pub dir: Option<String>,
}

/// One page of the records linked to an entity (the entity page's evidence feed)
pub async fn wiki_entity_records_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<EntityRecordsQuery>,
) -> Response {
    let types: Vec<String> = q
        .types
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    api_response(
        crate::api::get_entity_records_page(
            state.db.pool(),
            &id,
            q.offset.unwrap_or(0),
            q.limit.unwrap_or(10),
            q.search.as_deref().unwrap_or(""),
            &types,
            q.dir.as_deref() != Some("asc"),
        )
        .await,
    )
}

/// Facet counts over all of an entity's records, for the chip rail
pub async fn wiki_entity_record_facets_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_entity_record_facets(state.db.pool(), &id).await)
}

// =============================================================================
// =============================================================================
// Wiki Temporal Events API
// =============================================================================

/// Get events for a day by date
pub async fn wiki_get_day_events_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_events_by_date(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// Create a temporal event
pub async fn wiki_create_event_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreateTemporalEventRequest>,
) -> Response {
    match crate::api::create_temporal_event(state.db.pool(), request).await {
        Ok(event) => (StatusCode::CREATED, Json(event)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Update a temporal event
pub async fn wiki_update_event_handler(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
    Json(request): Json<crate::api::UpdateTemporalEventRequest>,
) -> Response {
    api_response(crate::api::update_temporal_event(state.db.pool(), event_id, request).await)
}

/// Delete a temporal event
pub async fn wiki_delete_event_handler(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
) -> Response {
    match crate::api::delete_temporal_event(state.db.pool(), event_id).await {
        Ok(_) => success_message("Event deleted"),
        Err(e) => error_response(e),
    }
}

/// Delete all auto-generated events for a day (regeneration support)
pub async fn wiki_delete_auto_events_handler(
    State(state): State<AppState>,
    Path(day_id): Path<String>,
) -> Response {
    match crate::api::delete_auto_events_for_day(state.db.pool(), day_id).await {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({ "deleted": count })),
        )
            .into_response(),
        Err(e) => error_response(e),
    }
}

/// Get timeline location chunks for a day (movement map)
pub async fn timeline_get_day_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_timeline_day(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// Optional `?tz=` query — the viewing device's IANA zone, used to anchor an
/// in-progress "today" to where the owner currently is. See agents/record/timezone-model.md.
#[derive(Debug, Deserialize, Default)]
pub struct DaySourcesQuery {
    pub tz: Option<String>,
}

/// Get the three raw record streams (location, calendar, audio) for a day, as
/// spans — the homepage's "day before synthesis" view.
/// GET /api/wiki/day/:date/heart-rate — the day's HR samples, for Autonomic.
pub async fn day_heart_rate_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
    Query(query): Query<DaySourcesQuery>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => api_response(
            crate::api::wiki_streams::get_day_heart_rate(state.db.pool(), parsed_date, query.tz.as_deref())
                .await,
        ),
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

pub async fn today_streams_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
    Query(query): Query<DaySourcesQuery>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => api_response(
            crate::api::get_today_streams(state.db.pool(), parsed_date, query.tz.as_deref()).await,
        ),
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// `?limit=N` for the small home-page list endpoints.
#[derive(Debug, Deserialize, Default)]
pub struct LimitQuery {
    pub limit: Option<i64>,
}

/// Current weather for the home masthead (null until the weather_sync cron runs).
pub async fn weather_now_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_current_weather(state.db.pool()).await)
}

/// The next few calendar events (holidays/birthdays filtered).
pub async fn calendar_upcoming_handler(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Response {
    api_response(crate::api::get_calendar_upcoming(state.db.pool(), q.limit.unwrap_or(5)).await)
}

/// Places visited but never named — the home "name this place" ask.
pub async fn unnamed_places_handler(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Response {
    api_response(crate::api::get_unnamed_places(state.db.pool(), q.limit.unwrap_or(3)).await)
}

/// Get data sources (ontology records) for a day
pub async fn wiki_get_day_sources_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
    Query(query): Query<DaySourcesQuery>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => api_response(
            crate::api::get_day_sources(state.db.pool(), parsed_date, query.tz.as_deref()).await,
        ),
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// Get AI chats (in-app Virtues + external imported) for a day
pub async fn wiki_get_day_chats_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_day_chats(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// Get all ontology data streams for a day (dynamic query across all ontologies)
pub async fn wiki_get_day_streams_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_day_streams(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}



// =============================================================================
// Chat Usage & Compaction API Handlers
// =============================================================================

/// Get token usage for a chat
pub async fn get_chat_usage_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::get_chat_usage(state.db.pool(), chat_id).await)
}

/// Compact a chat (summarize older messages)
pub async fn compact_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(request): Json<Option<CompactChatRequest>>,
) -> Response {
    let options = request.unwrap_or_default().into();
    api_response(crate::api::compaction::compact_chat(state.db.pool(), chat_id, options).await)
}

/// Request body for compaction
#[derive(Debug, Deserialize, Default)]
pub struct CompactChatRequest {
    /// Number of recent exchanges to keep verbatim (default: 4)
    pub keep_recent_exchanges: Option<usize>,
    /// Force compaction even if under threshold
    #[serde(default)]
    pub force: bool,
}

impl From<CompactChatRequest> for crate::api::compaction::CompactionOptions {
    fn from(req: CompactChatRequest) -> Self {
        let default_opts = crate::api::compaction::CompactionOptions::default();
        Self {
            keep_recent_exchanges: req
                .keep_recent_exchanges
                .unwrap_or(default_opts.keep_recent_exchanges),
            force: req.force,
            model_id: None, // API compaction uses default model context window
        }
    }
}

// =============================================================================
// Chats API Handlers
// =============================================================================

/// List chats
pub async fn list_chats_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::chats::list_chats(state.db.pool(), 25).await)
}

/// Create a new chat with initial messages
pub async fn create_chat_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::chats::CreateChatRequest>,
) -> Response {
    api_response(crate::api::chats::create_chat_from_request(state.db.pool(), request).await)
}

/// Get a chat by ID
pub async fn get_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::chats::get_chat(state.db.pool(), chat_id).await)
}

/// Update a chat (title and/or icon)
pub async fn update_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(request): Json<crate::api::chats::UpdateChatRequest>,
) -> Response {
    api_response(
        crate::api::chats::update_chat(state.db.pool(), chat_id, &request).await,
    )
}

/// Delete a chat
pub async fn delete_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::chats::delete_chat(state.db.pool(), chat_id).await)
}

/// Generate a title for a chat
pub async fn generate_chat_title_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::chats::GenerateTitleRequest>,
) -> Response {
    api_response(
        crate::api::chats::generate_title(state.db.pool(), request.chat_id, &request.messages)
            .await,
    )
}

// =============================================================================
// Chat API Handler
// =============================================================================

/// POST /api/chat - Stream chat completion (requires authentication)
pub async fn chat_handler(
    State(state): State<AppState>,
    user: crate::middleware::auth::AuthUser,
    Json(request): Json<crate::api::chat::ChatRequest>,
) -> Response {
    crate::api::chat::chat_handler(
        axum::extract::State(state.db.pool().clone()),
        axum::extract::State(state.yjs_state.clone()),
        axum::extract::State(state.chat_cancel_state.clone()),
        axum::extract::State(state.live_turns.clone()),
        axum::extract::State(state.ghost_permissions.clone()),
        user,
        Json(request),
    )
    .await
}

/// GET /api/chat/:id/stream - Rejoin the turn still running for a chat (VIR-323)
pub async fn live_turn_stream_handler(
    State(state): State<AppState>,
    user: crate::middleware::auth::AuthUser,
    axum::extract::Path(chat_id): axum::extract::Path<String>,
) -> Response {
    crate::api::chat::live_turn_stream_handler(
        axum::extract::State(state.live_turns.clone()),
        user,
        axum::extract::Path(chat_id),
    )
    .await
}

/// POST /api/ai/complete - Lean inline AI completion (live AI cursor)
pub async fn ai_complete_handler(
    State(state): State<AppState>,
    user: crate::middleware::auth::AuthUser,
    Json(request): Json<crate::api::ai_complete::AiCompleteRequest>,
) -> Response {
    crate::api::ai_complete::ai_complete_handler(
        axum::extract::State(state.db.pool().clone()),
        user,
        Json(request),
    )
    .await
}

/// POST /api/chat/cancel - Cancel an in-progress chat request
pub async fn cancel_chat_handler(
    State(state): State<AppState>,
    user: crate::middleware::auth::AuthUser,
    Json(request): Json<crate::api::chat::CancelChatRequest>,
) -> impl IntoResponse {
    crate::api::chat::cancel_chat_handler(
        axum::extract::State(state.chat_cancel_state.clone()),
        user,
        Json(request),
    )
    .await
}

// =============================================================================
// Chat Edit Permissions API Handlers
// =============================================================================

/// GET /api/chats/:id/permissions - List edit permissions for a chat
pub async fn list_chat_permissions_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::chat_permissions::list_permissions(state.db.pool(), &chat_id).await)
}

/// POST /api/chats/:id/permissions - Add an edit permission
pub async fn add_chat_permission_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(request): Json<crate::api::chat_permissions::AddPermissionRequest>,
) -> Response {
    api_response(
        crate::api::chat_permissions::add_permission(
            state.db.pool(),
            &state.ghost_permissions,
            &chat_id,
            request,
        )
        .await,
    )
}

/// DELETE /api/chats/:id/permissions/:entity_id - Remove an edit permission
pub async fn remove_chat_permission_handler(
    State(state): State<AppState>,
    Path((chat_id, entity_id)): Path<(String, String)>,
) -> Response {
    match crate::api::chat_permissions::remove_permission(state.db.pool(), &chat_id, &entity_id)
        .await
    {
        Ok(_) => success_message("Permission removed"),
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Auth API Handlers
// =============================================================================

/// GET /auth/session — current session (or null if not paired). Authenticated by
/// the `AuthUser` extractor (proven iroh key / loopback console / dev fallback);
/// there is no cookie/signout — the credential is the device's iroh key.
pub async fn auth_session_handler(user: Option<crate::middleware::auth::AuthUser>) -> Response {
    crate::api::auth::session_handler(user).await.into_response()
}

// =============================================================================
// Drive API Handlers (User File Storage)
// =============================================================================

/// GET /api/drive/usage - Get drive usage statistics
pub async fn get_drive_usage_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_drive_usage(state.db.pool(), &state.drive_config).await)
}

/// GET /api/backup/status - age of the newest good backup, per volume
pub async fn get_backup_status_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_backup_status(state.db.pool()).await)
}

/// Query params for listing drive files
#[derive(Debug, Deserialize)]
pub struct ListDriveFilesQuery {
    #[serde(default = "default_drive_path")]
    pub path: String,
}

fn default_drive_path() -> String {
    String::new() // Empty string = root directory
}

/// GET /api/drive/files - List files in a directory
pub async fn list_drive_files_handler(
    State(state): State<AppState>,
    Query(params): Query<ListDriveFilesQuery>,
) -> Response {
    api_response(crate::api::list_drive_files(state.db.pool(), &params.path).await)
}

/// GET /api/drive/files/:id - Get file metadata
pub async fn get_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    api_response(crate::api::get_drive_file(state.db.pool(), &file_id).await)
}

/// GET /api/drive/files/:id/download - Download file content
/// Query parameters for drive downloads.
#[derive(Debug, Deserialize)]
pub struct DriveDownloadQuery {
    /// `inline` renders in-browser (viewer surfaces); default is attachment.
    pub disposition: Option<String>,
}

/// Outcome of resolving a Range header against an object size.
#[derive(Debug, PartialEq)]
enum RangeOutcome {
    /// No (or ignorable) range — serve the full object with 200.
    Full,
    /// Serve `(start, len)` with 206.
    Partial(u64, u64),
    /// Range present but unsatisfiable — 416.
    Unsatisfiable,
}

/// Resolve a single-range `Range: bytes=…` header against a total size.
/// Malformed and multi-range headers are ignored (RFC 7233 permits a full 200
/// response); syntactically valid but out-of-bounds ranges are unsatisfiable.
fn resolve_range(header: Option<&str>, total: u64) -> RangeOutcome {
    let Some(header) = header else {
        return RangeOutcome::Full;
    };
    let Some(spec) = header.trim().strip_prefix("bytes=") else {
        return RangeOutcome::Full;
    };
    if spec.contains(',') {
        return RangeOutcome::Full;
    }
    let Some((start_s, end_s)) = spec.split_once('-') else {
        return RangeOutcome::Full;
    };
    let (start_s, end_s) = (start_s.trim(), end_s.trim());
    match (start_s.is_empty(), end_s.is_empty()) {
        (true, true) => RangeOutcome::Full,
        // Suffix form: last N bytes.
        (true, false) => {
            let Ok(n) = end_s.parse::<u64>() else {
                return RangeOutcome::Full;
            };
            if n == 0 || total == 0 {
                return RangeOutcome::Unsatisfiable;
            }
            let start = total.saturating_sub(n);
            RangeOutcome::Partial(start, total - start)
        }
        // Open-ended: from start to EOF.
        (false, true) => {
            let Ok(start) = start_s.parse::<u64>() else {
                return RangeOutcome::Full;
            };
            if start >= total {
                return RangeOutcome::Unsatisfiable;
            }
            RangeOutcome::Partial(start, total - start)
        }
        // Bounded: start–end inclusive, end clamped to EOF.
        (false, false) => {
            let (Ok(start), Ok(end)) = (start_s.parse::<u64>(), end_s.parse::<u64>()) else {
                return RangeOutcome::Full;
            };
            if start > end {
                return RangeOutcome::Full;
            }
            if start >= total {
                return RangeOutcome::Unsatisfiable;
            }
            let end = end.min(total - 1);
            RangeOutcome::Partial(start, end - start + 1)
        }
    }
}

pub async fn download_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
    Query(query): Query<DriveDownloadQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    let disposition = if query.disposition.as_deref() == Some("inline") {
        "inline"
    } else {
        "attachment"
    };

    // Lake objects use in-memory download (different storage layer)
    if crate::api::is_lake_object_id(&file_id) {
        let result =
            crate::api::download_lake_object(state.db.pool(), &state.storage, &file_id).await;
        return match result {
            Ok((file, content)) => {
                let content_type = file
                    .mime_type
                    .unwrap_or_else(|| "application/octet-stream".to_string());
                let filename = sanitize_content_disposition(&file.filename);
                (
                    [
                        (axum::http::header::CONTENT_TYPE, content_type),
                        (
                            axum::http::header::CONTENT_DISPOSITION,
                            format!("{disposition}; filename=\"{filename}\""),
                        ),
                        (
                            axum::http::header::CONTENT_LENGTH,
                            content.len().to_string(),
                        ),
                    ],
                    content,
                )
                    .into_response()
            }
            Err(e) => error_response(e),
        };
    }

    // Regular drive files: resolve any Range against the stored size, then
    // stream straight from disk — 206 for partials, 416 when unsatisfiable.
    let meta = match crate::api::get_drive_file(state.db.pool(), &file_id).await {
        Ok(f) => f,
        Err(e) => return error_response(e),
    };
    let total = meta.size_bytes.max(0) as u64;
    let range_header = headers
        .get(axum::http::header::RANGE)
        .and_then(|v| v.to_str().ok());
    let range = match resolve_range(range_header, total) {
        RangeOutcome::Unsatisfiable => {
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [
                    (axum::http::header::ACCEPT_RANGES, "bytes".to_string()),
                    (
                        axum::http::header::CONTENT_RANGE,
                        format!("bytes */{total}"),
                    ),
                ],
            )
                .into_response();
        }
        RangeOutcome::Full => None,
        RangeOutcome::Partial(start, len) => Some((start, len)),
    };

    let result = crate::api::download_drive_file_stream(
        state.db.pool(),
        &state.drive_config,
        &file_id,
        range,
    )
    .await;
    match result {
        Ok((file, _disk_total, stream)) => {
            let content_type = file
                .mime_type
                .unwrap_or_else(|| "application/octet-stream".to_string());
            let filename = sanitize_content_disposition(&file.filename);
            let (status, content_length, content_range) = match range {
                Some((start, len)) => (
                    StatusCode::PARTIAL_CONTENT,
                    len,
                    Some(format!("bytes {}-{}/{}", start, start + len - 1, total)),
                ),
                None => (StatusCode::OK, total, None),
            };
            let mut builder = axum::http::Response::builder()
                .status(status)
                .header(axum::http::header::CONTENT_TYPE, content_type)
                .header(
                    axum::http::header::CONTENT_DISPOSITION,
                    format!("{disposition}; filename=\"{filename}\""),
                )
                .header(axum::http::header::ACCEPT_RANGES, "bytes")
                .header(axum::http::header::CONTENT_LENGTH, content_length);
            if let Some(cr) = content_range {
                builder = builder.header(axum::http::header::CONTENT_RANGE, cr);
            }
            match builder.body(Body::from_stream(stream)) {
                Ok(resp) => resp.into_response(),
                Err(e) => error_response(crate::error::Error::Other(format!(
                    "Failed to build response: {e}"
                ))),
            }
        }
        Err(e) => error_response(e),
    }
}

// ── Annotations (document highlights + margin notes, researcher-plan D2) ──

/// GET /api/annotations?file_id=… — list a file's annotations.
#[derive(Debug, Deserialize)]
pub struct ListAnnotationsQuery {
    pub file_id: String,
}
pub async fn list_annotations_handler(
    State(state): State<AppState>,
    Query(q): Query<ListAnnotationsQuery>,
) -> Response {
    api_response(crate::api::list_annotations(state.db.pool(), &q.file_id).await)
}

/// GET /api/annotations/export?file_id=… — a file's highlights as markdown.
pub async fn export_file_annotations_handler(
    State(state): State<AppState>,
    Query(q): Query<ListAnnotationsQuery>,
) -> Response {
    match crate::api::export_file_annotations_md(state.db.pool(), &q.file_id).await {
        Ok(md) => markdown_response(md),
        Err(e) => error_response(e),
    }
}

/// Serve markdown as a downloadable text body.
fn markdown_response(md: String) -> Response {
    (
        [(axum::http::header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        md,
    )
        .into_response()
}

/// POST /api/annotations — create (or upsert) a highlight.
pub async fn create_annotation_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::api::CreateAnnotationRequest>,
) -> Response {
    api_response(crate::api::create_annotation(state.db.pool(), req).await)
}

/// PATCH /api/annotations/:id — edit note/color.
pub async fn update_annotation_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<crate::api::UpdateAnnotationRequest>,
) -> Response {
    api_response(crate::api::update_annotation(state.db.pool(), &id, req).await)
}

/// DELETE /api/annotations/:id
pub async fn delete_annotation_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::delete_annotation(state.db.pool(), &id).await {
        Ok(_) => success_message("Annotation deleted"),
        Err(e) => error_response(e),
    }
}

/// POST /api/drive/files/:id/reextract — queue a file for (re-)extraction.
pub async fn reextract_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    api_response(crate::api::reextract_drive_file(state.db.pool(), &file_id).await)
}

/// DELETE /api/drive/files/:id - Delete a file or folder
pub async fn delete_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    match crate::api::delete_drive_file(state.db.pool(), &state.drive_config, &file_id).await {
        Ok(_) => success_message("File deleted"),
        Err(e) => error_response(e),
    }
}

/// PUT /api/drive/files/:id/move - Move or rename a file
pub async fn move_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
    Json(request): Json<crate::api::DriveMoveFileRequest>,
) -> Response {
    api_response(
        crate::api::move_drive_file(
            state.db.pool(),
            &state.drive_config,
            &file_id,
            &request.new_path,
        )
        .await,
    )
}

/// POST /api/drive/upload - Upload a file (multipart form)
pub async fn upload_drive_file_handler(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Response {
    use sha2::Digest as _;
    use tokio::io::AsyncWriteExt;

    /// Per-file ceiling, enforced while streaming. The router body limit
    /// (260MB) is only a backstop above this, so the honest 413 below fires
    /// first and the client gets a real message instead of a connection reset.
    const MAX_UPLOAD_FILE_BYTES: u64 = 250 * 1024 * 1024;

    let too_large = || {
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({
                "error": "Your server didn't take that file: it's over the 250 MB upload limit. Split it or compress it, then upload again."
            })),
        )
            .into_response()
    };

    // Stream the multipart form. The file field is written chunk-by-chunk to a
    // staging file on the drive filesystem while hashing incrementally — the
    // upload is never held in memory (committing later is a rename).
    let mut path: Option<String> = None;
    let mut filename: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut staged: Option<crate::api::StagedUpload> = None;

    let cleanup = |staged: &Option<crate::api::StagedUpload>| {
        if let Some(s) = staged {
            let p = s.temp_path.clone();
            tokio::spawn(async move {
                let _ = tokio::fs::remove_file(p).await;
            });
        }
    };

    loop {
        let mut field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => {
                // With the in-stream cap below the router backstop, an error
                // here is a malformed body or an aborted connection.
                cleanup(&staged);
                return error_response(crate::error::Error::InvalidInput(
                    "Upload interrupted or malformed".into(),
                ));
            }
        };
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "path" => {
                if let Ok(text) = field.text().await {
                    path = Some(text);
                }
            }
            "file" => {
                filename = field.file_name().map(|s| s.to_string());
                mime_type = field.content_type().map(|s| s.to_string());

                // Repeated file fields: keep the last, drop the earlier stage.
                cleanup(&staged);

                let staging_dir = match state.drive_config.storage.staging_dir().await {
                    Ok(d) => d,
                    Err(e) => return error_response(e),
                };
                static STAGING_SEQ: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(0);
                let temp_path = staging_dir.join(format!(
                    "{}-{}.part",
                    std::process::id(),
                    STAGING_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ));

                let mut out = match tokio::fs::File::create(&temp_path).await {
                    Ok(f) => f,
                    Err(e) => {
                        return error_response(crate::error::Error::Storage(format!(
                            "Failed to stage upload: {e}"
                        )))
                    }
                };
                let mut hasher = sha2::Sha256::new();
                let mut written: u64 = 0;
                loop {
                    match field.chunk().await {
                        Ok(Some(chunk)) => {
                            written += chunk.len() as u64;
                            if written > MAX_UPLOAD_FILE_BYTES {
                                drop(out);
                                let _ = tokio::fs::remove_file(&temp_path).await;
                                return too_large();
                            }
                            hasher.update(&chunk);
                            if let Err(e) = out.write_all(&chunk).await {
                                let _ = tokio::fs::remove_file(&temp_path).await;
                                return error_response(crate::error::Error::Storage(format!(
                                    "Failed to stage upload: {e}"
                                )));
                            }
                        }
                        Ok(None) => break,
                        Err(_) => {
                            let _ = tokio::fs::remove_file(&temp_path).await;
                            return error_response(crate::error::Error::InvalidInput(
                                "Upload interrupted or malformed".into(),
                            ));
                        }
                    }
                }
                if let Err(e) = out.flush().await {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    return error_response(crate::error::Error::Storage(format!(
                        "Failed to stage upload: {e}"
                    )));
                }
                staged = Some(crate::api::StagedUpload {
                    temp_path,
                    size_bytes: written as i64,
                    sha256: format!("{:x}", hasher.finalize()),
                });
            }
            _ => {}
        }
    }

    let request = crate::api::DriveUploadRequest {
        path: path.unwrap_or_else(|| "uploads".to_string()),
        filename: filename.unwrap_or_else(|| "unnamed".to_string()),
        mime_type,
    };

    match staged {
        Some(staged) => {
            let temp_path = staged.temp_path.clone();
            match crate::api::upload_drive_file(
                state.db.pool(),
                &state.drive_config,
                request,
                staged,
            )
            .await
            {
                Ok(file) => (StatusCode::CREATED, Json(file)).into_response(),
                Err(e) => {
                    // Commit failed before the rename — drop the staged file.
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    error_response(e)
                }
            }
        }
        None => error_response(crate::error::Error::InvalidInput(
            "No file data provided".into(),
        )),
    }
}

/// POST /api/drive/folders - Create a folder
pub async fn create_drive_folder_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::DriveCreateFolderRequest>,
) -> Response {
    match crate::api::create_drive_folder(state.db.pool(), &state.drive_config, request).await {
        Ok(folder) => (StatusCode::CREATED, Json(folder)).into_response(),
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Drive Trash Handlers
// =============================================================================

/// GET /api/drive/trash - List files in trash
/// GET /api/drive/media — the app's internal assets (.media/). Read-only.
pub async fn list_drive_media_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_drive_media(state.db.pool()).await)
}

pub async fn list_drive_trash_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_drive_trash(state.db.pool()).await)
}

/// POST /api/drive/files/:id/restore - Restore a file from trash
pub async fn restore_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    api_response(crate::api::restore_drive_file(state.db.pool(), &file_id).await)
}

/// DELETE /api/drive/files/:id/purge - Permanently delete a file (skip trash)
pub async fn purge_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    match crate::api::purge_drive_file(state.db.pool(), &state.drive_config, &file_id).await {
        Ok(_) => success_message("File permanently deleted"),
        Err(e) => error_response(e),
    }
}

/// POST /api/drive/trash/empty - Empty all files from trash
pub async fn empty_drive_trash_handler(State(state): State<AppState>) -> Response {
    match crate::api::empty_drive_trash(state.db.pool(), &state.drive_config).await {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({ "deleted_count": count })),
        )
            .into_response(),
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Media Handlers
// =============================================================================

/// POST /api/media/upload - Upload media file with content-addressed dedup
///
/// Accepts multipart form with:
/// - `file`: The file data (required)
/// - `filename`: Override filename (optional, uses file's name by default)
///
/// Returns MediaFile with URL for embedding in pages.
/// If identical content already exists, returns existing file (dedup).
pub async fn upload_media_handler(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Response {
    // Parse multipart form
    let mut filename: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut data: Option<axum::body::Bytes> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "filename" => {
                if let Ok(text) = field.text().await {
                    filename = Some(text);
                }
            }
            "file" => {
                // Use form field filename if no explicit filename provided
                if filename.is_none() {
                    filename = field.file_name().map(|s| s.to_string());
                }
                mime_type = field.content_type().map(|s| s.to_string());
                if let Ok(bytes) = field.bytes().await {
                    data = Some(bytes);
                }
            }
            _ => {}
        }
    }

    let filename = filename.unwrap_or_else(|| "unnamed".to_string());

    match data {
        Some(bytes) => {
            match crate::api::upload_media(
                state.db.pool(),
                &state.drive_config,
                &filename,
                mime_type,
                bytes,
            )
            .await
            {
                Ok(file) => (StatusCode::CREATED, Json(file)).into_response(),
                Err(e) => error_response(e),
            }
        }
        None => error_response(crate::error::Error::InvalidInput(
            "No file data provided".into(),
        )),
    }
}

/// GET /api/media/:id - Get media file metadata
pub async fn get_media_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    api_response(crate::api::get_media(state.db.pool(), &file_id).await)
}

// ============================================================================
// Pages Handlers
// ============================================================================

/// Query params for pages list
#[derive(Debug, Deserialize)]
pub struct ListPagesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub workspace_id: Option<String>,
}

/// GET /api/pages - List all pages
pub async fn list_pages_handler(
    State(state): State<AppState>,
    Query(query): Query<ListPagesQuery>,
) -> Response {
    // Note: workspace_id filter removed - views handle filtering now
    api_response(crate::api::list_pages(state.db.pool(), query.limit, query.offset).await)
}

/// GET /api/pages/:id - Get a single page
pub async fn get_page_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(crate::api::get_page(state.db.pool(), &id).await)
}

/// GET /api/records/:ontology/:record_id - fetch one raw life-graph record.
pub async fn get_record_handler(
    State(state): State<AppState>,
    Path((ontology, record_id)): Path<(String, String)>,
) -> Response {
    api_response(crate::api::records::get_record(state.db.pool(), &ontology, &record_id).await)
}

/// POST /api/pages - Create a new page
pub async fn create_page_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreatePageRequest>,
) -> Response {
    match crate::api::create_page(state.db.pool(), request).await {
        Ok(page) => (StatusCode::CREATED, Json(page)).into_response(),
        Err(e) => error_response(e),
    }
}

/// PUT /api/pages/:id - Update a page
pub async fn update_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdatePageRequest>,
) -> Response {
    api_response(crate::api::update_page(state.db.pool(), &id, request).await)
}

/// DELETE /api/pages/:id - Delete a page
pub async fn delete_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::delete_page(state.db.pool(), &id).await {
        Ok(_) => success_message("Page deleted successfully"),
        Err(e) => error_response(e),
    }
}

/// POST /api/pages/:id/append — append a markdown block to a page THROUGH Yjs.
///
/// The synthesis-bridge write path (researcher-plan D4): sending a highlight
/// into a page must not clobber an editor that's currently open on it, so this
/// goes through the authoritative Yjs doc (transaction + broadcast) rather than
/// a REST content replace.
#[derive(Debug, Deserialize)]
pub struct AppendPageRequest {
    pub markdown: String,
}

pub async fn append_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AppendPageRequest>,
) -> Response {
    if req.markdown.trim().is_empty() {
        return error_response(crate::error::Error::InvalidInput(
            "markdown cannot be empty".into(),
        ));
    }
    match state.yjs_state.append_markdown(&id, &req.markdown).await {
        Ok(content) => api_response(Ok(serde_json::json!({ "content": content }))),
        Err(e) => error_response(crate::error::Error::Other(e)),
    }
}

/// GET /api/pages/:id/backlinks - Get inbound references (pages linking here)
pub async fn get_page_backlinks_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_page_backlinks(state.db.pool(), &id).await)
}

/// Query params for entity search
#[derive(Debug, Deserialize)]
pub struct EntitySearchQuery {
    pub q: String,
}

/// GET /api/pages/search/refs - Search entities for autocomplete
pub async fn search_refs_handler(
    State(state): State<AppState>,
    Query(query): Query<EntitySearchQuery>,
) -> Response {
    api_response(crate::api::search_refs(state.db.pool(), &query.q).await)
}

// ============================================================================
// Page Sharing Handlers
// ============================================================================

/// POST /api/pages/:id/share - Create or replace a share link for a page
pub async fn create_page_share_handler(
    State(state): State<AppState>,
    Path(page_id): Path<String>,
) -> Response {
    match crate::api::create_page_share(state.db.pool(), &page_id).await {
        Ok(share) => (StatusCode::CREATED, Json(share)).into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/pages/:id/share - Get the active share for a page
pub async fn get_page_share_handler(
    State(state): State<AppState>,
    Path(page_id): Path<String>,
) -> Response {
    api_response(crate::api::get_page_share(state.db.pool(), &page_id).await)
}

/// DELETE /api/pages/:id/share - Revoke the share for a page
pub async fn delete_page_share_handler(
    State(state): State<AppState>,
    Path(page_id): Path<String>,
) -> Response {
    match crate::api::delete_page_share(state.db.pool(), &page_id).await {
        Ok(_) => success_message("Share revoked"),
        Err(e) => error_response(e),
    }
}

/// GET /api/s/:token - Get a shared page (public, no auth)
pub async fn get_shared_page_handler(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Response {
    api_response(crate::api::get_shared_page(state.db.pool(), &token).await)
}

/// GET /api/s/:token/files/:file_id - Download a file from a shared page (public, no auth)
/// Validates that the file is referenced by the shared page's content
pub async fn shared_file_download_handler(
    State(state): State<AppState>,
    Path((token, file_id)): Path<(String, String)>,
) -> Response {
    // Validate the share token and that this file belongs to the shared page
    if let Err(e) = crate::api::validate_shared_file(state.db.pool(), &token, &file_id).await {
        return error_response(e);
    }

    // Lake objects use in-memory download
    if crate::api::is_lake_object_id(&file_id) {
        let result =
            crate::api::download_lake_object(state.db.pool(), &state.storage, &file_id).await;
        return match result {
            Ok((file, content)) => {
                let content_type = file
                    .mime_type
                    .unwrap_or_else(|| "application/octet-stream".to_string());
                let filename = sanitize_content_disposition(&file.filename);
                (
                    [
                        (axum::http::header::CONTENT_TYPE, content_type),
                        (
                            axum::http::header::CONTENT_DISPOSITION,
                            format!("inline; filename=\"{}\"", filename),
                        ),
                        (
                            axum::http::header::CONTENT_LENGTH,
                            content.len().to_string(),
                        ),
                    ],
                    content,
                )
                    .into_response()
            }
            Err(e) => error_response(e),
        };
    }

    // Regular drive files: stream from storage
    let result = crate::api::download_drive_file_stream(
        state.db.pool(),
        &state.drive_config,
        &file_id,
        None,
    )
    .await;
    match result {
        Ok((file, total, stream)) => {
            let content_type = file
                .mime_type
                .unwrap_or_else(|| "application/octet-stream".to_string());
            let filename = sanitize_content_disposition(&file.filename);
            (
                [
                    (axum::http::header::CONTENT_TYPE, content_type),
                    (
                        axum::http::header::CONTENT_DISPOSITION,
                        format!("inline; filename=\"{}\"", filename),
                    ),
                    (axum::http::header::CONTENT_LENGTH, total.to_string()),
                ],
                axum::body::Body::from_stream(stream),
            )
                .into_response()
        }
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Page Versions Handlers
// ============================================================================

/// Query params for versions list
#[derive(Debug, Deserialize)]
pub struct ListVersionsQuery {
    pub limit: Option<i64>,
}

/// GET /api/pages/:id/versions - List versions for a page
pub async fn list_page_versions_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ListVersionsQuery>,
) -> Response {
    api_response(crate::api::list_versions(state.db.pool(), &id, query.limit).await)
}

/// POST /api/pages/:id/versions - Create a new version snapshot
pub async fn create_page_version_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::CreateVersionRequest>,
) -> Response {
    match crate::api::create_version(state.db.pool(), &id, request).await {
        Ok(version) => (StatusCode::CREATED, Json(version)).into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/pages/versions/:version_id - Get a single version (with snapshot for restore)
pub async fn get_page_version_handler(
    State(state): State<AppState>,
    Path(version_id): Path<String>,
) -> Response {
    api_response(crate::api::get_version(state.db.pool(), &version_id).await)
}

// ============================================================================
// Local content search (⌘K)
// ============================================================================

/// POST /api/search/local — content hits for the command palette.
///
/// POST rather than GET because the query is user text: a GET would put what
/// someone is searching their own life for into the URL, where it lands in
/// history and any access log. Same reason the rest of the search surface
/// posts.
pub async fn search_local_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::LocalSearchRequest>,
) -> Response {
    api_response(crate::api::search_local(state.db.pool(), request).await)
}

// ============================================================================
// Update Handlers (Settings → Box)
// ============================================================================

/// GET /api/system/update — current version, channel, and what's available.
pub async fn update_status_handler() -> Response {
    (StatusCode::OK, Json(crate::api::update_status().await)).into_response()
}

/// PUT /api/system/update/channel — follow stable or prerelease.
pub async fn set_channel_handler(
    Json(request): Json<crate::api::SetChannelRequest>,
) -> Response {
    api_response(crate::api::set_channel(request))
}

/// POST /api/system/update/apply — start an upgrade.
///
/// 202, not 200: the work has been handed to a transient systemd unit and is
/// still running when this returns. It cannot be otherwise — the upgrade
/// restarts this very process, so a handler that waited for the result would be
/// killed before it could report one. The client's next signal is the
/// connection dropping and coming back.
///
/// Runs it detached and answers immediately for the same reason.
pub async fn apply_update_handler() -> Response {
    match crate::api::apply_update() {
        Ok(body) => (StatusCode::ACCEPTED, Json(body)).into_response(),
        Err(e) => api_response::<crate::api::ApplyResponse>(Err(e)),
    }
}


// ============================================================================
// Bookmarks Handlers (saved web content — data_content_bookmark)
// ============================================================================

/// GET /api/bookmarks — one page of saved bookmarks, newest first.
/// GET /api/bookmarks/:id — one bookmark, for its detail view.
pub async fn get_bookmark_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_bookmark(state.db.pool(), &id).await)
}

/// PATCH /api/bookmarks/:id/note — write the user's marginalia.
pub async fn update_bookmark_note_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<crate::api::UpdateNoteRequest>,
) -> Response {
    api_response(crate::api::update_note(state.db.pool(), &id, req).await)
}

pub async fn list_bookmarks_handler(
    State(state): State<AppState>,
    Query(query): Query<crate::api::ListBookmarksQuery>,
) -> Response {
    api_response(crate::api::list_bookmarks(state.db.pool(), query).await)
}

/// POST /api/bookmarks — save a URL (idempotent on canonical URL). The manual
/// capture door; enrichment backfills titles/extraction later.
pub async fn save_bookmark_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::SaveBookmarkRequest>,
) -> Response {
    match crate::api::save_bookmark(state.db.pool(), request).await {
        Ok(saved) => (StatusCode::CREATED, Json(saved)).into_response(),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Pins Handlers (sidebar pinned URLs)
// ============================================================================

/// GET /api/pins — list all pins, ordered by `sort_order`.
pub async fn list_pins_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_pins(state.db.pool()).await)
}

/// POST /api/pins — pin a URL (idempotent on URL).
pub async fn create_pin_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreatePinRequest>,
) -> Response {
    match crate::api::create_pin(state.db.pool(), request).await {
        Ok(pin) => (StatusCode::CREATED, Json(pin)).into_response(),
        Err(e) => error_response(e),
    }
}

/// PATCH /api/pins/:id — update label / icon / sort_order.
pub async fn update_pin_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdatePinRequest>,
) -> Response {
    api_response(crate::api::update_pin(state.db.pool(), &id, request).await)
}

/// DELETE /api/pins/:id — unpin.
pub async fn delete_pin_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::delete_pin(state.db.pool(), &id).await {
        Ok(_) => success_message("Pin removed"),
        Err(e) => error_response(e),
    }
}

#[derive(serde::Deserialize)]
pub struct ReorderPinsRequest {
    pub urls: Vec<String>,
}

/// PUT /api/pins/reorder — reorder all pins to match the supplied URL list.
pub async fn reorder_pins_handler(
    State(state): State<AppState>,
    Json(request): Json<ReorderPinsRequest>,
) -> Response {
    match crate::api::reorder_pins(state.db.pool(), &request.urls).await {
        Ok(_) => success_message("Pins reordered"),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Projects Handlers
// ============================================================================

/// GET /api/projects - List all projects
pub async fn list_projects_handler(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<ListProjectsQuery>,
) -> Response {
    api_response(
        crate::api::projects::list_projects(state.db.pool(), q.include_archived.unwrap_or(false))
            .await,
    )
}

#[derive(Deserialize)]
pub struct ListProjectsQuery {
    /// `?include_archived=true` — the projects page, which folds them.
    pub include_archived: Option<bool>,
}

/// POST /api/projects/:id/archive — close a project (kept, out of the working view)
pub async fn archive_project_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::projects::archive_project(state.db.pool(), &id).await {
        Ok(()) => success_message("Project archived"),
        Err(e) => error_response(e),
    }
}

/// POST /api/projects/:id/unarchive
pub async fn unarchive_project_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::projects::unarchive_project(state.db.pool(), &id).await {
        Ok(()) => success_message("Project reopened"),
        Err(e) => error_response(e),
    }
}

/// GET /api/projects/:id - Get a single project with its members
pub async fn get_project_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(crate::api::projects::get_project(state.db.pool(), &id).await)
}

/// POST /api/projects - Create a project
pub async fn create_project_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::projects::CreateProjectRequest>,
) -> Response {
    match crate::api::projects::create_project(state.db.pool(), request).await {
        Ok(project) => (StatusCode::CREATED, Json(project)).into_response(),
        Err(e) => error_response(e),
    }
}

/// PUT /api/projects/:id - Update a project
pub async fn update_project_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::projects::UpdateProjectRequest>,
) -> Response {
    api_response(crate::api::projects::update_project(state.db.pool(), &id, request).await)
}

/// DELETE /api/projects/:id - Delete a project
pub async fn delete_project_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::projects::delete_project(state.db.pool(), &id).await {
        Ok(_) => success_message("Project deleted"),
        Err(e) => error_response(e),
    }
}

/// POST /api/projects/:id/items - Add a member URL to a project
pub async fn add_project_item_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::projects::AddProjectItemRequest>,
) -> Response {
    match crate::api::projects::add_project_item(state.db.pool(), &id, request).await {
        Ok(item) => (StatusCode::CREATED, Json(item)).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct RemoveProjectItemRequest {
    pub url: String,
}

/// DELETE /api/projects/:id/items - Remove a member URL from a project
pub async fn remove_project_item_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<RemoveProjectItemRequest>,
) -> Response {
    match crate::api::projects::remove_project_item(state.db.pool(), &id, &request.url).await {
        Ok(_) => success_message("Item removed from project"),
        Err(e) => error_response(e),
    }
}

/// PUT /api/projects/:id/items/reorder - Reorder project members
pub async fn reorder_project_items_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::projects::ReorderProjectItemsRequest>,
) -> Response {
    match crate::api::projects::reorder_project_items(state.db.pool(), &id, request).await {
        Ok(_) => success_message("Project items reordered"),
        Err(e) => error_response(e),
    }
}

/// PUT /api/projects/:id/items/role - Set a member's role
pub async fn set_project_item_role_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::projects::SetProjectItemRoleRequest>,
) -> Response {
    match crate::api::projects::set_project_item_role(state.db.pool(), &id, request).await {
        Ok(item) => Json(item).into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/projects/:id/graph - Entities referenced across the members
pub async fn project_graph_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::projects::project_graph(state.db.pool(), &id).await {
        Ok(graph) => Json(graph).into_response(),
        Err(e) => error_response(e),
    }
}


// ============================================================================
// Lake API handlers
// ============================================================================

/// GET /api/lake/summary - Get lake summary statistics
pub async fn get_lake_summary_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::lake::get_lake_summary(state.db.pool()).await)
}

/// GET /api/lake/streams - List all streams in the lake
pub async fn list_lake_streams_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::lake::list_lake_streams(state.db.pool()).await)
}

// ============================================================================
// Recently deleted — the trash for chats, pages and projects (`api::trash`)
// ============================================================================

/// GET /api/trash — everything in the trash, all kinds, newest deletion first.
pub async fn list_trash_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::trash::list_trash(state.db.pool()).await)
}

/// POST /api/trash/:kind/:id/restore
pub async fn restore_trash_handler(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
) -> Response {
    let kind = match crate::api::trash::TrashKind::parse(&kind) {
        Ok(k) => k,
        Err(e) => return error_response(e),
    };
    match crate::api::trash::restore(state.db.pool(), kind, &id).await {
        Ok(()) => success_message("Restored"),
        Err(e) => error_response(e),
    }
}

/// DELETE /api/trash/:kind/:id — delete forever. Refuses anything not in the
/// trash; the 30 days cannot be skipped from here.
pub async fn purge_trash_handler(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
) -> Response {
    let kind = match crate::api::trash::TrashKind::parse(&kind) {
        Ok(k) => k,
        Err(e) => return error_response(e),
    };
    match crate::api::trash::purge_trashed(state.db.pool(), kind, &id).await {
        Ok(()) => success_message("Deleted forever"),
        Err(e) => error_response(e),
    }
}

/// POST /api/trash/empty
pub async fn empty_trash_handler(State(state): State<AppState>) -> Response {
    api_response(
        crate::api::trash::empty_trash(state.db.pool())
            .await
            .map(|deleted_count| serde_json::json!({ "deleted_count": deleted_count })),
    )
}

// ============================================================================
// Visits — the frecency log behind ⌘K (`api::visits`)
// ============================================================================

/// POST /api/visits — the owner opened a chat, page or project themselves.
pub async fn record_visit_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::api::visits::RecordVisitRequest>,
) -> Response {
    match crate::api::visits::record_visit(state.db.pool(), req).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/visits/frecency — every visited record's score, highest first.
pub async fn frecency_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::visits::frecency(state.db.pool()).await)
}

#[cfg(test)]
mod range_tests {
    use super::{resolve_range, RangeOutcome};

    #[test]
    fn no_header_serves_full() {
        assert_eq!(resolve_range(None, 100), RangeOutcome::Full);
    }

    #[test]
    fn bounded_range() {
        assert_eq!(
            resolve_range(Some("bytes=0-49"), 100),
            RangeOutcome::Partial(0, 50)
        );
        assert_eq!(
            resolve_range(Some("bytes=10-19"), 100),
            RangeOutcome::Partial(10, 10)
        );
        // End past EOF clamps
        assert_eq!(
            resolve_range(Some("bytes=90-199"), 100),
            RangeOutcome::Partial(90, 10)
        );
    }

    #[test]
    fn open_ended_range() {
        assert_eq!(
            resolve_range(Some("bytes=40-"), 100),
            RangeOutcome::Partial(40, 60)
        );
    }

    #[test]
    fn suffix_range() {
        assert_eq!(
            resolve_range(Some("bytes=-10"), 100),
            RangeOutcome::Partial(90, 10)
        );
        // Suffix longer than the object serves the whole object
        assert_eq!(
            resolve_range(Some("bytes=-500"), 100),
            RangeOutcome::Partial(0, 100)
        );
    }

    #[test]
    fn unsatisfiable_ranges() {
        assert_eq!(
            resolve_range(Some("bytes=100-"), 100),
            RangeOutcome::Unsatisfiable
        );
        assert_eq!(
            resolve_range(Some("bytes=200-300"), 100),
            RangeOutcome::Unsatisfiable
        );
        assert_eq!(
            resolve_range(Some("bytes=-0"), 100),
            RangeOutcome::Unsatisfiable
        );
        // Any range against an empty object is unsatisfiable
        assert_eq!(
            resolve_range(Some("bytes=0-10"), 0),
            RangeOutcome::Unsatisfiable
        );
        assert_eq!(
            resolve_range(Some("bytes=-5"), 0),
            RangeOutcome::Unsatisfiable
        );
    }

    #[test]
    fn ignored_forms_serve_full() {
        // Multi-range: permitted to ignore, serve 200
        assert_eq!(resolve_range(Some("bytes=0-1,5-9"), 100), RangeOutcome::Full);
        // Malformed
        assert_eq!(resolve_range(Some("bytes=abc-def"), 100), RangeOutcome::Full);
        assert_eq!(resolve_range(Some("bytes=50-10"), 100), RangeOutcome::Full);
        assert_eq!(resolve_range(Some("bytes=-"), 100), RangeOutcome::Full);
        assert_eq!(resolve_range(Some("items=0-10"), 100), RangeOutcome::Full);
    }
}
