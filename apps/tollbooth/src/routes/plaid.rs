//! Plaid Proxy Routes
//!
//! All Plaid API calls are proxied through Tollbooth for:
//! 1. Security: Plaid keys stay on the server, not in distributed client code
//! 2. Budget enforcement: Can deduct costs from user's balance
//! 3. Tier validation: Can restrict Plaid access to certain tiers
//!
//! Endpoints:
//! - POST /v1/services/plaid/link-token - Create a Plaid Link token
//! - POST /v1/services/plaid/exchange-token - Exchange public token for access token
//! - POST /v1/services/plaid/sync - Sync transactions (called by Core scheduler)

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

use crate::auth::AuthenticatedRequest;
use crate::AppState;

// =============================================================================
// Pricing Constants
// =============================================================================

/// Plaid pricing: ~$0.25 per Item per month (bundled transactions)
/// We don't charge per-sync, but could deduct on initial connection
const PLAID_CONNECTION_COST: f64 = 0.25;

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Serialize)]
struct PlaidProxyError {
    error: PlaidProxyErrorDetails,
}

#[derive(Debug, Serialize)]
struct PlaidProxyErrorDetails {
    message: String,
    code: String,
}

impl PlaidProxyError {
    fn insufficient_budget() -> Self {
        Self {
            error: PlaidProxyErrorDetails {
                message: "Insufficient budget for Plaid connection".to_string(),
                code: "insufficient_budget".to_string(),
            },
        }
    }

    fn connection_limit_reached(limit: u8, tier: &str) -> Self {
        Self {
            error: PlaidProxyErrorDetails {
                message: format!(
                    "You've reached your limit of {} bank connections on the {} plan. Upgrade to add more.",
                    limit, tier
                ),
                code: "connection_limit_reached".to_string(),
            },
        }
    }

    fn service_not_configured() -> Self {
        Self {
            error: PlaidProxyErrorDetails {
                message: "Plaid is not configured on this server".to_string(),
                code: "service_not_configured".to_string(),
            },
        }
    }

    fn upstream_error(message: String) -> Self {
        Self {
            error: PlaidProxyErrorDetails {
                message,
                code: "upstream_error".to_string(),
            },
        }
    }
}

// =============================================================================
// Plaid API Types
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateLinkTokenRequest {
    /// User client ID for Plaid (unique per user)
    pub user_client_id: String,
    /// Products to request (e.g., ["transactions"])
    #[serde(default = "default_products")]
    pub products: Vec<String>,
    /// Country codes (e.g., ["US"])
    #[serde(default = "default_country_codes")]
    pub country_codes: Vec<String>,
    /// Optional redirect URI for OAuth-based institutions
    pub redirect_uri: Option<String>,
}

fn default_products() -> Vec<String> {
    vec!["transactions".to_string()]
}

fn default_country_codes() -> Vec<String> {
    vec!["US".to_string()]
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateLinkTokenResponse {
    pub link_token: String,
    pub expiration: String,
    pub request_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ExchangeTokenRequest {
    /// The public_token from Plaid Link
    pub public_token: String,
    /// Current number of Plaid connections (provided by Core for limit enforcement)
    #[serde(default)]
    pub current_plaid_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExchangeTokenResponse {
    pub access_token: String,
    pub item_id: String,
    pub request_id: String,
}

#[derive(Debug, Deserialize)]
pub struct TransactionsSyncRequest {
    /// The access_token for the Item
    pub access_token: String,
    /// Optional cursor for incremental sync
    pub cursor: Option<String>,
    /// Number of transactions to fetch (max 500)
    #[serde(default = "default_count")]
    pub count: i32,
}

fn default_count() -> i32 {
    500
}

#[derive(Debug, Deserialize)]
pub struct AccountsGetRequest {
    /// The access_token for the Item
    pub access_token: String,
}

// =============================================================================
// Plaid Client Helper
// =============================================================================

/// Make a POST request to the Plaid API
async fn plaid_post<Req: Serialize, Res: for<'de> Deserialize<'de>>(
    state: &AppState,
    endpoint: &str,
    body: &Req,
) -> Result<Res, PlaidProxyError> {
    let client_id = state
        .config
        .plaid_client_id
        .as_ref()
        .ok_or_else(PlaidProxyError::service_not_configured)?;
    let secret = state
        .config
        .plaid_secret
        .as_ref()
        .ok_or_else(PlaidProxyError::service_not_configured)?;

    let base_url = match state.config.plaid_env.as_str() {
        "production" => "https://production.plaid.com",
        "development" => "https://development.plaid.com",
        _ => "https://sandbox.plaid.com",
    };

    let url = format!("{}{}", base_url, endpoint);

    let response = state
        .http_client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("PLAID-CLIENT-ID", client_id)
        .header("PLAID-SECRET", secret)
        .header("Plaid-Version", "2020-09-14")
        .json(body)
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| PlaidProxyError::upstream_error(format!("Request failed: {}", e)))?;

    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|e| PlaidProxyError::upstream_error(format!("Failed to read response: {}", e)))?;

    if !status.is_success() {
        // Try to extract Plaid error message
        if let Ok(plaid_error) = serde_json::from_str::<serde_json::Value>(&body_text) {
            let error_msg = plaid_error
                .get("error_message")
                .and_then(|v| v.as_str())
                .unwrap_or(&body_text);
            return Err(PlaidProxyError::upstream_error(format!(
                "Plaid error ({}): {}",
                status, error_msg
            )));
        }
        return Err(PlaidProxyError::upstream_error(format!(
            "Plaid request failed ({}): {}",
            status, body_text
        )));
    }

    serde_json::from_str(&body_text)
        .map_err(|e| PlaidProxyError::upstream_error(format!("Failed to parse response: {}", e)))
}

// =============================================================================
// Route Handlers
// =============================================================================

/// POST /v1/services/plaid/link-token
/// Create a Plaid Link token for initializing Plaid Link SDK
async fn create_link_token(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
    Json(request): Json<CreateLinkTokenRequest>,
) -> Response {
    // Check budget (optional - could allow link token creation without budget)
    if !state.budget.has_budget(&auth.user_id) {
        return (
            StatusCode::PAYMENT_REQUIRED,
            Json(PlaidProxyError::insufficient_budget()),
        )
            .into_response();
    }

    // Build Plaid request
    let plaid_request = serde_json::json!({
        "client_name": "Virtues",
        "user": {
            "client_user_id": request.user_client_id
        },
        "products": request.products,
        "country_codes": request.country_codes,
        "language": "en",
        "redirect_uri": request.redirect_uri
    });

    match plaid_post::<_, CreateLinkTokenResponse>(&state, "/link/token/create", &plaid_request)
        .await
    {
        Ok(response) => {
            tracing::info!(
                user_id = %auth.user_id,
                "Plaid link token created"
            );
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(e)).into_response(),
    }
}

/// POST /v1/services/plaid/exchange-token
/// Exchange a public token for an access token
async fn exchange_token(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
    Json(request): Json<ExchangeTokenRequest>,
) -> Response {
    // Check connection limit BEFORE calling Plaid API
    if let Some(limit_check) = state.tier.check_connection_limit(
        &auth.user_id,
        "plaid",
        request.current_plaid_count,
    ) {
        if !limit_check.can_add {
            tracing::warn!(
                user_id = %auth.user_id,
                tier = %limit_check.tier,
                limit = %limit_check.limit,
                current = %limit_check.current,
                "Plaid connection limit reached"
            );
            return (
                StatusCode::FORBIDDEN,
                Json(PlaidProxyError::connection_limit_reached(
                    limit_check.limit,
                    &limit_check.tier,
                )),
            )
                .into_response();
        }
    }

    // Check budget before allowing connection
    if !state.budget.has_budget(&auth.user_id) {
        return (
            StatusCode::PAYMENT_REQUIRED,
            Json(PlaidProxyError::insufficient_budget()),
        )
            .into_response();
    }

    let plaid_request = serde_json::json!({
        "public_token": request.public_token
    });

    match plaid_post::<_, ExchangeTokenResponse>(&state, "/item/public_token/exchange", &plaid_request)
        .await
    {
        Ok(response) => {
            // Deduct connection cost from budget
            state.budget.deduct(&auth.user_id, PLAID_CONNECTION_COST);
            tracing::info!(
                user_id = %auth.user_id,
                item_id = %response.item_id,
                cost_usd = PLAID_CONNECTION_COST,
                "Plaid token exchanged, budget deducted"
            );
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(e)).into_response(),
    }
}

/// POST /v1/services/plaid/transactions/sync
/// Sync transactions using cursor-based pagination
async fn transactions_sync(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
    Json(request): Json<TransactionsSyncRequest>,
) -> Response {
    // No budget check for sync - already paid on connection
    // Could add rate limiting here if needed

    let plaid_request = serde_json::json!({
        "access_token": request.access_token,
        "cursor": request.cursor,
        "count": request.count
    });

    match plaid_post::<_, serde_json::Value>(&state, "/transactions/sync", &plaid_request).await {
        Ok(response) => {
            tracing::debug!(
                user_id = %auth.user_id,
                has_more = response.get("has_more").and_then(|v| v.as_bool()).unwrap_or(false),
                "Plaid transactions sync"
            );
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(e)).into_response(),
    }
}

/// POST /v1/services/plaid/accounts/get
/// Get accounts for an Item (free endpoint)
async fn accounts_get(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
    Json(request): Json<AccountsGetRequest>,
) -> Response {
    let plaid_request = serde_json::json!({
        "access_token": request.access_token
    });

    match plaid_post::<_, serde_json::Value>(&state, "/accounts/get", &plaid_request).await {
        Ok(response) => {
            tracing::debug!(
                user_id = %auth.user_id,
                account_count = response.get("accounts").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
                "Plaid accounts fetched"
            );
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(e)).into_response(),
    }
}

/// POST /v1/services/plaid/item/remove
/// Remove an Item (disconnect bank account)
async fn item_remove(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
    Json(request): Json<AccountsGetRequest>, // Same shape - just needs access_token
) -> Response {
    let plaid_request = serde_json::json!({
        "access_token": request.access_token
    });

    match plaid_post::<_, serde_json::Value>(&state, "/item/remove", &plaid_request).await {
        Ok(response) => {
            tracing::info!(
                user_id = %auth.user_id,
                "Plaid item removed"
            );
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(e)).into_response(),
    }
}

// =============================================================================
// Router
// =============================================================================

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/services/plaid/link-token", post(create_link_token))
        .route("/services/plaid/exchange-token", post(exchange_token))
        .route("/services/plaid/transactions/sync", post(transactions_sync))
        .route("/services/plaid/accounts/get", post(accounts_get))
        .route("/services/plaid/item/remove", post(item_remove))
}
