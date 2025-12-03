//! Plaid Link API
//!
//! Plaid uses a different auth flow than standard OAuth2. Instead of redirecting
//! to a provider's auth page, Plaid Link is a client-side JavaScript SDK that
//! handles the bank authentication UI.
//!
//! Flow:
//! 1. Frontend calls `POST /api/plaid/link-token` to get a link_token
//! 2. Frontend initializes Plaid Link SDK with the link_token
//! 3. User authenticates with their bank in Plaid's UI
//! 4. Plaid returns a public_token to the frontend
//! 5. Frontend calls `POST /api/plaid/exchange-token` with the public_token
//! 6. Backend exchanges public_token for access_token and stores it

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::sources::plaid::client::PlaidClient;
use crate::sources::plaid::transactions::PlaidSourceMetadata;

/// Request to create a Plaid Link token
#[derive(Debug, Deserialize)]
pub struct CreateLinkTokenRequest {
    /// Optional: existing source_id if re-linking an existing connection
    pub source_id: Option<Uuid>,
}

/// Response containing a Plaid Link token
#[derive(Debug, Serialize)]
pub struct CreateLinkTokenResponse {
    /// The link_token to initialize Plaid Link
    pub link_token: String,
    /// Expiration time (link tokens expire in 4 hours)
    pub expiration: String,
}

/// Request to exchange a public token for an access token
#[derive(Debug, Deserialize)]
pub struct ExchangeTokenRequest {
    /// The public_token from Plaid Link
    pub public_token: String,
    /// Institution metadata from Plaid Link
    pub institution_id: Option<String>,
    pub institution_name: Option<String>,
}

/// Response after successful token exchange
#[derive(Debug, Serialize)]
pub struct ExchangeTokenResponse {
    /// The source connection ID
    pub source_id: Uuid,
    /// Item ID from Plaid
    pub item_id: String,
    /// Institution name if available
    pub institution_name: Option<String>,
}

/// Create a Plaid Link token for initializing Plaid Link
///
/// This is called by the frontend before showing the Plaid Link UI.
pub async fn create_link_token(
    _db: &PgPool,
    _request: CreateLinkTokenRequest,
) -> Result<CreateLinkTokenResponse> {
    let client = PlaidClient::from_env()?;

    // Use a unique identifier for the user session
    // In a multi-user system, this would be the actual user ID
    let user_client_id = format!("virtues-user-{}", Uuid::new_v4());

    // Request transactions product (primary use case)
    let products = vec!["transactions"];
    let country_codes = vec!["US", "CA", "GB"];

    // Get redirect URI from environment (optional - only needed for OAuth-based institutions)
    // In sandbox mode, we can skip this. In production, configure in Plaid dashboard.
    let redirect_uri = std::env::var("PLAID_REDIRECT_URI").ok();

    let response = client
        .link_token_create(
            &user_client_id,
            products,
            country_codes,
            redirect_uri.as_deref(),
            None, // webhook_url
        )
        .await?;

    Ok(CreateLinkTokenResponse {
        link_token: response.link_token,
        expiration: response.expiration,
    })
}

/// Exchange a public token for an access token
///
/// Called after the user completes the Plaid Link flow.
/// Creates a new source connection with the access token.
pub async fn exchange_public_token(
    db: &PgPool,
    request: ExchangeTokenRequest,
) -> Result<ExchangeTokenResponse> {
    let client = PlaidClient::from_env()?;

    // Exchange public token for access token
    let exchange_response = client
        .item_public_token_exchange(&request.public_token)
        .await?;

    let access_token = exchange_response.access_token;
    let item_id = exchange_response.item_id;

    // Create source connection
    let source_id = Uuid::new_v4();
    let source_name = request
        .institution_name
        .clone()
        .unwrap_or_else(|| "Plaid Account".to_string());

    // Store metadata with access token
    let metadata = PlaidSourceMetadata {
        item_id: item_id.clone(),
        access_token: access_token.clone(),
        institution_id: request.institution_id.clone(),
        institution_name: request.institution_name.clone(),
    };

    let metadata_json = serde_json::to_value(&metadata)
        .map_err(|e| Error::Other(format!("Failed to serialize metadata: {e}")))?;

    // Insert source connection
    sqlx::query(
        r#"
        INSERT INTO data.source_connections (id, source, name, auth_type, is_active, is_internal, metadata, created_at, updated_at)
        VALUES ($1, 'plaid', $2, 'plaid', true, false, $3, NOW(), NOW())
        "#,
    )
    .bind(source_id)
    .bind(&source_name)
    .bind(&metadata_json)
    .execute(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to create Plaid source: {e}")))?;

    // Enable default streams for Plaid
    super::streams::enable_default_streams(db, source_id, "plaid").await?;

    tracing::info!(
        source_id = %source_id,
        item_id = %item_id,
        institution = ?request.institution_name,
        "Plaid source connection created"
    );

    Ok(ExchangeTokenResponse {
        source_id,
        item_id,
        institution_name: request.institution_name,
    })
}

/// Get accounts for an existing Plaid connection
///
/// Useful for showing the user which accounts are connected.
pub async fn get_plaid_accounts(db: &PgPool, source_id: Uuid) -> Result<Vec<PlaidAccount>> {
    // Load access token from source metadata
    let row = sqlx::query_as::<_, (serde_json::Value,)>(
        "SELECT metadata FROM data.source_connections WHERE id = $1 AND source = 'plaid'",
    )
    .bind(source_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Plaid source not found: {source_id}")))?;

    let metadata: PlaidSourceMetadata = serde_json::from_value(row.0)
        .map_err(|e| Error::Other(format!("Invalid Plaid metadata: {e}")))?;

    let client = PlaidClient::from_env()?;
    let response = client.accounts_get(&metadata.access_token).await?;

    let accounts = response
        .accounts
        .into_iter()
        .map(|acc| PlaidAccount {
            account_id: acc.account_id,
            name: acc.name,
            official_name: acc.official_name,
            account_type: acc.account_type,
            subtype: acc.subtype,
            mask: acc.mask,
            balance_current: acc.balances.current,
            balance_available: acc.balances.available,
            iso_currency_code: acc.balances.iso_currency_code,
        })
        .collect();

    Ok(accounts)
}

/// Plaid account information
#[derive(Debug, Serialize)]
pub struct PlaidAccount {
    pub account_id: String,
    pub name: String,
    pub official_name: Option<String>,
    pub account_type: String,
    pub subtype: Option<String>,
    pub mask: Option<String>,
    pub balance_current: Option<f64>,
    pub balance_available: Option<f64>,
    pub iso_currency_code: Option<String>,
}

/// Remove a Plaid Item (disconnect bank account)
pub async fn remove_plaid_item(db: &PgPool, source_id: Uuid) -> Result<()> {
    // Load access token
    let row = sqlx::query_as::<_, (serde_json::Value,)>(
        "SELECT metadata FROM data.source_connections WHERE id = $1 AND source = 'plaid'",
    )
    .bind(source_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Plaid source not found: {source_id}")))?;

    let metadata: PlaidSourceMetadata = serde_json::from_value(row.0)
        .map_err(|e| Error::Other(format!("Invalid Plaid metadata: {e}")))?;

    // Revoke access with Plaid
    let client = PlaidClient::from_env()?;
    client.item_remove(&metadata.access_token).await?;

    // Delete source connection
    super::sources::delete_source(db, source_id).await?;

    tracing::info!(source_id = %source_id, "Plaid item removed");

    Ok(())
}
