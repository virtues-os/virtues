//! Owner and box settings: profile, assistant profile, models, streams,
//! billing, updates, and the places and Unsplash proxies.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json,
    Router,
};
use serde::Deserialize;

use super::{api_response, error_response};
use crate::server::AppState;

/// This area's authenticated routes. Merged into the protected router, whose
/// `route_layer` requires a resolved `AuthUser`.
pub fn routes() -> Router<AppState> {
    Router::new()
        // ─── Billing settings (BYO key) ───────────────────────────────
        // BYO routes inference around virtues-api entirely: box calls
        // upstream directly. Save/delete are sudo-gated (change_byo_key);
        // status is a non-secret read for the Billing page.
        .route("/api/settings/byo-key",   get(crate::api::settings_byo::status_handler)
                                          .post(crate::api::settings_byo::save_handler)
                                          .delete(crate::api::settings_byo::delete_handler))
        // ─── Web bundle (the box IS the update server) ────────────────
        // What UI build this box serves, and the build itself. A client can
        // be AHEAD of its box: an app update bakes newer UI, and the client
        // refuses to downgrade to the box's older bundle — see
        // api/web_bundle.rs for what that leaves open.
        .route("/api/web-bundle/version", get(crate::api::web_bundle::version_handler))
        .route("/api/web-bundle/tarball", get(crate::api::web_bundle::tarball_handler))
        // ─── Billing-state aggregator (local view) ────────────────────
        .route("/api/billing/state",           get(crate::api::billing_state::state_handler))
        .route("/api/billing/auto-topup",      post(crate::api::billing_state::set_auto_topup_handler))
        // Setup wizard transitions (agents/build/onboarding.md) — session-authed; the
        // wizard reads progress from the public /api/setup/state.
        .route("/api/setup/subscribe/start",   post(crate::api::setup::subscribe_start_handler))
        .route("/api/setup/login/start",       post(crate::api::setup::login_start_handler))
        .route("/api/setup/link/poll",         post(crate::api::setup::link_poll_handler))
        // Profile API
        .route(
            "/api/profile",
            get(get_profile_handler).put(update_profile_handler),
        )
        // Places API (Google Places proxy)
        .route(
            "/api/places/autocomplete",
            get(places_autocomplete_handler),
        )
        .route(
            "/api/places/details",
            get(places_details_handler),
        )
        // Assistant Profile API
        .route(
            "/api/assistant-profile",
            get(get_assistant_profile_handler).put(update_assistant_profile_handler),
        )
        // Models API
        .route("/api/models", get(list_models_handler))
        .route(
            "/api/models/recommended",
            get(list_models_with_slots_handler),
        )
        .route("/api/models/:id", get(get_model_handler))
        // Per-stream ingest freshness — surfaces a stalled source instead of
        // letting it rot silently.
        .route("/api/streams/health", get(stream_health_handler))
        // Everything that needs the owner, in one list: the Mac app turns a
        // new item into a notification (api/attention.rs).
        .route("/api/attention", get(attention_handler))
        .route("/api/streams/days", get(stream_days_handler))
        // Subscription & Billing API
        .route("/api/subscription", get(get_subscription_handler))
        .route(
            "/api/billing/portal",
            post(create_billing_portal_handler),
        )
        .route("/api/billing/subscribe", post(subscribe_billing_handler))
        // Wallet balance + recent ledger (proxied from virtues-api /v1/usage).
        .route("/api/billing/usage", get(billing_usage_handler))
        // Box-local AI spend breakdown (app_ai_calls) for the Usage tab.
        .route("/api/usage/summary", get(usage_summary_handler))
        // Paged individual AI calls (app_ai_calls) for the Usage page's log.
        .route("/api/telemetry/ai-calls", get(ai_calls_handler))
        // Device-authorization link flow (web "Connect subscription").
        .route(
            "/api/billing/link/start",
            post(billing_link_start_handler),
        )
        .route(
            "/api/billing/link/status",
            get(billing_link_status_handler),
        )
        // Unsplash API (cover image search)
        .route("/api/unsplash/search", post(unsplash_search_handler))
        // System (operator surface — apps + logs)
        // Live host snapshot + persisted history for the System/Telemetry views.
        .route(
            "/api/system/telemetry",
            get(crate::api::system_telemetry::telemetry_handler),
        )
        .route(
            "/api/system/history",
            get(crate::api::system_telemetry::history_handler),
        )
        // Box network management (Settings → Box → Network) — the authed
        // successors to the setup-phase /api/provision/* surface, which
        // correctly evaporates at claim time, so a box on a captive guest
        // network still has a way to leave. See api/network.rs.
        .route("/api/network/status", get(crate::api::network::status_handler))
        .route("/api/network/scan",   get(crate::api::network::scan_handler))
        .route("/api/network/join",   post(crate::api::network::join_handler))
        // The rendezvous, named and switchable (agents/record/open-relay.md).
        .route(
            "/api/network/relay",
            get(crate::api::network::relay_status_handler)
                .put(crate::api::network::relay_toggle_handler),
        )
        // Settings → Search: where search runs and how much it covers.
        .route("/api/search/status", get(search_status_handler))
        // Box updates (Settings → Box)
        .route("/api/system/update", get(update_status_handler))
        .route(
            "/api/system/update/channel",
            put(set_channel_handler),
        )
        .route(
            "/api/system/update/apply",
            post(apply_update_handler),
        )
        // The box's attached screen (Settings → Display). Deliberately NOT in
        // the loopback-only /api/display/* family: that module's uniform
        // box-local rule is its security argument, and these are the paired
        // device's side of the glass — panel facts, the ambient face choice,
        // and the restart verb. Nothing here carries the setup phrase.
        .route(
            "/api/system/display",
            get(crate::api::system_display::get_display_settings_handler),
        )
        .route(
            "/api/system/display/face",
            put(crate::api::system_display::set_display_face_handler),
        )
        .route(
            "/api/system/display/hours",
            put(crate::api::system_display::set_display_hours_handler),
        )
        .route(
            "/api/system/display/restart",
            post(crate::api::system_display::restart_display_handler),
        )
}


// =============================================================================
// Profile API
// =============================================================================

/// Get user profile
pub async fn get_profile_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::profile::get_profile(state.db.pool()).await)
}

/// Update user profile
pub async fn update_profile_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::profile::UpdateProfileRequest>,
) -> Response {
    api_response(crate::api::profile::update_profile(state.db.pool(), request).await)
}

// =============================================================================
// Assistant Profile API
// =============================================================================

/// Get assistant profile
pub async fn get_assistant_profile_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::assistant_profile::get_assistant_profile(state.db.pool()).await)
}

/// Update assistant profile
pub async fn update_assistant_profile_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::assistant_profile::UpdateAssistantProfileRequest>,
) -> Response {
    api_response(crate::api::assistant_profile::update_assistant_profile(state.db.pool(), request).await)
}

// =============================================================================
// Models API
// =============================================================================

/// List all available models
pub async fn list_models_handler() -> Response {
    api_response(crate::api::models::list_models().await)
}

/// Get a specific model by ID
pub async fn get_model_handler(Path(model_id): Path<String>) -> Response {
    api_response(crate::api::models::get_model(&model_id).await)
}

/// The picker plus the live slot map — what "Virtues default · <model>" needs.
///
/// `/api/models` stays a bare array (the picker's existing contract); this
/// route adds `slots`, so the settings UI can name the model a slot currently
/// resolves to without a second round trip.
pub async fn list_models_with_slots_handler() -> Response {
    api_response(crate::api::models::list_models_with_slots().await)
}

/// Per-stream ingest freshness, worst-first. The signal that was missing when
/// messages, the calendar sync, and finance each went dark unnoticed.
pub async fn stream_health_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::stream_health::stream_health(&state.db).await)
}

pub async fn attention_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::attention::attention(&state.db).await)
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

// =============================================================================
// Places API Handlers (Google Places proxy)
// =============================================================================

/// Resolve an autocomplete prediction to coordinates (the phone's "mute a
/// place I have never been" door creates the place from this).
pub async fn places_details_handler(
    State(state): State<AppState>,
    Query(request): Query<crate::api::places::PlaceDetailsRequest>,
) -> Response {
    api_response(crate::api::places::get_place_details(state.db.pool(), request).await)
}

/// Get autocomplete predictions for an address query
pub async fn places_autocomplete_handler(
    State(state): State<AppState>,
    Query(request): Query<crate::api::places::AutocompleteRequest>,
) -> Response {
    match crate::api::places::autocomplete(state.db.pool(), request).await {
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
// Unsplash API Handler
// =============================================================================

/// Search Unsplash photos for cover images
pub async fn unsplash_search_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::unsplash::SearchRequest>,
) -> Response {
    api_response(crate::api::unsplash::search(state.db.pool(), request).await)
}

// ============================================================================
// Update Handlers (Settings → Box)
// ============================================================================

/// GET /api/system/update — current version, channel, and what's available.
/// GET /api/search/status — Settings → Search.
pub async fn search_status_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::search_status::status(state.db.pool()).await)
}

pub async fn update_status_handler() -> Response {
    (StatusCode::OK, Json(crate::api::updates::status().await)).into_response()
}

/// PUT /api/system/update/channel — follow stable or prerelease.
pub async fn set_channel_handler(
    Json(request): Json<crate::api::updates::SetChannelRequest>,
) -> Response {
    api_response(crate::api::updates::set_channel(request))
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
    match crate::api::updates::apply() {
        Ok(body) => (StatusCode::ACCEPTED, Json(body)).into_response(),
        Err(e) => api_response::<crate::api::updates::ApplyResponse>(Err(e)),
    }
}
