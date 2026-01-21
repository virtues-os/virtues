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
//! 6. Backend exchanges public_token for access_token and stores it (encrypted)

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::sources::base::oauth::encryption::TokenEncryptor;
use crate::sources::plaid::client::PlaidClient;

/// Plaid source metadata stored in source_connections.metadata
/// Note: access_token is stored separately in the encrypted access_token column
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaidSourceMetadata {
    /// Plaid Item ID
    pub item_id: String,
    /// Institution ID
    pub institution_id: Option<String>,
    /// Institution name
    pub institution_name: Option<String>,
    /// Connected account types (e.g., ["depository", "credit", "investment"])
    /// Used to filter which streams are relevant for this connection
    #[serde(default)]
    pub connected_account_types: Vec<String>,
}

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
    /// Summary of connected accounts (for UI display)
    pub connected_accounts: Vec<ConnectedAccountSummary>,
}

/// Summary of a connected Plaid account
#[derive(Debug, Clone, Serialize)]
pub struct ConnectedAccountSummary {
    pub account_id: String,
    pub name: String,
    /// Plaid's standardized account type: depository, credit, loan, investment, brokerage, other
    pub account_type: String,
    /// More specific subtype: checking, savings, credit card, mortgage, 401k, etc.
    pub subtype: Option<String>,
    /// Last 4 digits of account number
    pub mask: Option<String>,
}

/// Create a Plaid Link token for initializing Plaid Link
///
/// This is called by the frontend before showing the Plaid Link UI.
pub async fn create_link_token(
    _db: &SqlitePool,
    _request: CreateLinkTokenRequest,
) -> Result<CreateLinkTokenResponse> {
    let client = PlaidClient::from_env()?;

    // Use a unique identifier for the user session
    // In a multi-user system, this would be the actual user ID
    let user_client_id = format!("virtues-user-{}", Uuid::new_v4());

    // Request financial products (investments/liabilities require additional Plaid approval)
    // Note: We use accounts_get() instead of accounts_balance_get() to avoid $0.10/call balance fees
    let products = vec!["transactions"];
    let country_codes = vec!["US"];

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
/// Creates a new source connection with the access token (encrypted).
/// Also fetches connected accounts to determine which streams are relevant.
pub async fn exchange_public_token(
    db: &SqlitePool,
    request: ExchangeTokenRequest,
) -> Result<ExchangeTokenResponse> {
    let client = PlaidClient::from_env()?;

    // Exchange public token for access token
    let exchange_response = client
        .item_public_token_exchange(&request.public_token)
        .await?;

    let access_token = exchange_response.access_token;
    let item_id = exchange_response.item_id;

    // Fetch connected accounts to analyze types
    let accounts_response = client.accounts_get(&access_token).await?;

    // Extract unique account types for stream filtering
    let connected_account_types: Vec<String> = accounts_response
        .accounts
        .iter()
        .map(|a| a.account_type.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // Build account summaries for frontend display
    let connected_accounts: Vec<ConnectedAccountSummary> = accounts_response
        .accounts
        .iter()
        .map(|acc| ConnectedAccountSummary {
            account_id: acc.account_id.clone(),
            name: acc.name.clone(),
            account_type: acc.account_type.clone(),
            subtype: acc.subtype.clone(),
            mask: acc.mask.clone(),
        })
        .collect();

    tracing::info!(
        account_types = ?connected_account_types,
        account_count = connected_accounts.len(),
        "Analyzed connected Plaid accounts"
    );

    // Encrypt the access token before storing
    let encryptor = TokenEncryptor::from_env()?;
    let encrypted_token = encryptor.encrypt(&access_token)?;

    // Create source connection
    let source_id = Uuid::new_v4();
    let source_name = request
        .institution_name
        .clone()
        .unwrap_or_else(|| "Plaid Account".to_string());

    // Store metadata with account types for stream filtering
    let metadata = PlaidSourceMetadata {
        item_id: item_id.clone(),
        institution_id: request.institution_id.clone(),
        institution_name: request.institution_name.clone(),
        connected_account_types,
    };

    let metadata_json = serde_json::to_value(&metadata)
        .map_err(|e| Error::Other(format!("Failed to serialize metadata: {e}")))?;

    // Insert source connection with encrypted access token
    sqlx::query(
        r#"
        INSERT INTO data_source_connections (id, source, name, auth_type, access_token, is_active, is_internal, metadata, created_at, updated_at)
        VALUES ($1, 'plaid', $2, 'plaid', $3, true, false, $4, datetime('now'), datetime('now'))
        "#,
    )
    .bind(source_id)
    .bind(&source_name)
    .bind(&encrypted_token)
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
        "Plaid source connection created with encrypted token"
    );

    Ok(ExchangeTokenResponse {
        source_id,
        item_id,
        institution_name: request.institution_name,
        connected_accounts,
    })
}

/// Get accounts for an existing Plaid connection
///
/// Useful for showing the user which accounts are connected.
pub async fn get_plaid_accounts(db: &SqlitePool, source_id: Uuid) -> Result<Vec<PlaidAccount>> {
    // Load encrypted access token from source_connections
    let row = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT access_token FROM data_source_connections WHERE id = $1 AND source = 'plaid'",
    )
    .bind(source_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Plaid source not found: {source_id}")))?;

    let encrypted_token = row
        .0
        .ok_or_else(|| Error::Configuration("Plaid source has no access token".to_string()))?;

    // Decrypt the access token
    let encryptor = TokenEncryptor::from_env()?;
    let access_token = encryptor.decrypt(&encrypted_token)?;

    let client = PlaidClient::from_env()?;
    let response = client.accounts_get(&access_token).await?;

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
pub async fn remove_plaid_item(db: &SqlitePool, source_id: Uuid) -> Result<()> {
    // Load encrypted access token
    let row = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT access_token FROM data_source_connections WHERE id = $1 AND source = 'plaid'",
    )
    .bind(source_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Plaid source not found: {source_id}")))?;

    let encrypted_token = row
        .0
        .ok_or_else(|| Error::Configuration("Plaid source has no access token".to_string()))?;

    // Decrypt the access token
    let encryptor = TokenEncryptor::from_env()?;
    let access_token = encryptor.decrypt(&encrypted_token)?;

    // Revoke access with Plaid
    let client = PlaidClient::from_env()?;
    client.item_remove(&access_token).await?;

    // Delete source connection
    super::sources::delete_source(db, source_id).await?;

    tracing::info!(source_id = %source_id, "Plaid item removed");

    Ok(())
}
