//! Plaid API client wrapper
//!
//! All Plaid API calls are proxied through Tollbooth for:
//! 1. Security: Plaid keys stay on Tollbooth, not in distributed client code
//! 2. Budget enforcement: Tollbooth can deduct costs from user's balance
//! 3. Tier validation: Tollbooth can restrict Plaid access to certain tiers
//!
//! The client sends requests to Tollbooth's /v1/services/plaid/* endpoints,
//! which then forward them to the actual Plaid API with proper credentials.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

use crate::error::{Error, Result};

/// Plaid API environment (used for display/logging only - actual env is on Tollbooth)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaidEnvironment {
    Sandbox,
    Development,
    Production,
}

impl PlaidEnvironment {
    pub fn from_env() -> Self {
        let env_val = env::var("PLAID_ENV").unwrap_or_else(|_| "sandbox".to_string());
        // Handle both URL format and simple name format
        if env_val.contains("sandbox") {
            Self::Sandbox
        } else if env_val.contains("development") {
            Self::Development
        } else if env_val.contains("production") {
            Self::Production
        } else {
            Self::Sandbox
        }
    }
}

/// Rate limiter for Plaid API calls
pub struct PlaidRateLimiter {
    global_semaphore: Arc<Semaphore>,
    min_request_interval: Duration,
}

impl PlaidRateLimiter {
    pub fn new() -> Self {
        Self {
            global_semaphore: Arc::new(Semaphore::new(100)),
            min_request_interval: Duration::from_millis(50),
        }
    }

    pub async fn acquire(&self) -> Result<tokio::sync::SemaphorePermit<'_>> {
        self.global_semaphore
            .acquire()
            .await
            .map_err(|e| Error::Other(format!("Rate limiter error: {e}")))
    }

    pub async fn wait_interval(&self) {
        tokio::time::sleep(self.min_request_interval).await;
    }
}

impl Default for PlaidRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// Plaid API client that proxies through Tollbooth
pub struct PlaidClient {
    http: Client,
    pub environment: PlaidEnvironment,
    rate_limiter: PlaidRateLimiter,
    /// Tollbooth URL (e.g., "http://localhost:9002")
    tollbooth_url: String,
    /// Internal secret for Tollbooth authentication
    internal_secret: String,
    /// User ID for budget tracking
    user_id: String,
}

impl PlaidClient {
    /// Create a new Plaid client from environment variables
    /// Requires TOLLBOOTH_URL and TOLLBOOTH_INTERNAL_SECRET
    pub fn from_env() -> Result<Self> {
        Self::new(None)
    }

    /// Create a new Plaid client with an optional user ID
    pub fn new(user_id: Option<String>) -> Result<Self> {
        let tollbooth_url = env::var("TOLLBOOTH_URL")
            .unwrap_or_else(|_| "http://localhost:9002".to_string());

        let internal_secret = env::var("TOLLBOOTH_INTERNAL_SECRET")
            .map_err(|_| Error::Configuration("TOLLBOOTH_INTERNAL_SECRET not set".to_string()))?;

        let environment = PlaidEnvironment::from_env();

        let http = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| Error::Other(format!("Failed to create HTTP client: {e}")))?;

        Ok(Self {
            http,
            environment,
            rate_limiter: PlaidRateLimiter::new(),
            tollbooth_url,
            internal_secret,
            user_id: user_id.unwrap_or_else(|| "system".to_string()),
        })
    }

    /// Create a new Plaid client with a specific user ID for budget tracking
    pub fn with_user_id(user_id: String) -> Result<Self> {
        Self::new(Some(user_id))
    }

    /// Get the Tollbooth base URL
    pub fn base_url(&self) -> &str {
        &self.tollbooth_url
    }

    /// Make a POST request to Tollbooth's Plaid proxy
    async fn post<Req: Serialize, Res: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        body: &Req,
    ) -> Result<Res> {
        let _permit = self.rate_limiter.acquire().await?;

        // Map Plaid endpoints to Tollbooth proxy endpoints
        let tollbooth_endpoint = match endpoint {
            "/link/token/create" => "/v1/services/plaid/link-token",
            "/item/public_token/exchange" => "/v1/services/plaid/exchange-token",
            "/transactions/sync" => "/v1/services/plaid/transactions/sync",
            "/accounts/get" | "/accounts/balance/get" => "/v1/services/plaid/accounts/get",
            "/item/remove" => "/v1/services/plaid/item/remove",
            // For endpoints not yet proxied, return an error
            _ => return Err(Error::Source(format!(
                "Plaid endpoint {} is not available through Tollbooth proxy",
                endpoint
            ))),
        };

        let url = format!("{}{}", self.tollbooth_url, tollbooth_endpoint);

        let response = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Internal-Secret", &self.internal_secret)
            .header("X-User-Id", &self.user_id)
            .json(body)
            .send()
            .await
            .map_err(|e| Error::Source(format!("Tollbooth request failed: {e}")))?;

        let status = response.status();
        let body_text = response
            .text()
            .await
            .map_err(|e| Error::Source(format!("Failed to read response: {e}")))?;

        if !status.is_success() {
            // Try to parse as Tollbooth/Plaid error
            if let Ok(error) = serde_json::from_str::<TollboothPlaidError>(&body_text) {
                return Err(Error::Source(format!(
                    "Plaid error [{}]: {}",
                    error.error.code, error.error.message
                )));
            }
            // Try legacy Plaid error format
            if let Ok(error) = serde_json::from_str::<PlaidError>(&body_text) {
                return Err(Error::Source(format!(
                    "Plaid error [{}]: {}",
                    error.error_code, error.error_message
                )));
            }
            return Err(Error::Source(format!(
                "Plaid request failed with status {}: {}",
                status, body_text
            )));
        }

        self.rate_limiter.wait_interval().await;

        serde_json::from_str(&body_text)
            .map_err(|e| Error::Source(format!("Failed to parse response: {e}")))
    }

    /// Create a link token for initializing Plaid Link
    pub async fn link_token_create(
        &self,
        user_client_id: &str,
        products: Vec<&str>,
        country_codes: Vec<&str>,
        redirect_uri: Option<&str>,
        _webhook_url: Option<&str>,
    ) -> Result<LinkTokenCreateResponse> {
        // Use Tollbooth's request format
        let request = TollboothLinkTokenRequest {
            user_client_id: user_client_id.to_string(),
            products: products.iter().map(|s| s.to_string()).collect(),
            country_codes: country_codes.iter().map(|s| s.to_string()).collect(),
            redirect_uri: redirect_uri.map(String::from),
        };

        self.post("/link/token/create", &request).await
    }

    /// Exchange a public token for an access token
    pub async fn item_public_token_exchange(
        &self,
        public_token: &str,
    ) -> Result<ItemPublicTokenExchangeResponse> {
        self.item_public_token_exchange_with_count(public_token, None).await
    }

    /// Exchange a public token for an access token with connection count for limit enforcement
    pub async fn item_public_token_exchange_with_count(
        &self,
        public_token: &str,
        current_plaid_count: Option<i64>,
    ) -> Result<ItemPublicTokenExchangeResponse> {
        // Use Tollbooth's request format
        let request = TollboothExchangeTokenRequest {
            public_token: public_token.to_string(),
            current_plaid_count,
        };

        self.post("/item/public_token/exchange", &request).await
    }

    /// Get accounts for an Item
    pub async fn accounts_get(&self, access_token: &str) -> Result<AccountsGetResponse> {
        let request = AccessTokenRequest {
            access_token: access_token.to_string(),
        };

        self.post("/accounts/get", &request).await
    }

    /// Get account balances
    pub async fn accounts_balance_get(&self, access_token: &str) -> Result<AccountsGetResponse> {
        let request = AccessTokenRequest {
            access_token: access_token.to_string(),
        };

        self.post("/accounts/balance/get", &request).await
    }

    /// Sync transactions using cursor-based pagination
    pub async fn transactions_sync(
        &self,
        access_token: &str,
        cursor: Option<&str>,
        count: Option<i32>,
    ) -> Result<TransactionsSyncResponse> {
        let request = TransactionsSyncRequest {
            access_token: access_token.to_string(),
            cursor: cursor.map(String::from),
            count,
        };

        self.post("/transactions/sync", &request).await
    }

    /// Get Item information
    pub async fn item_get(&self, access_token: &str) -> Result<ItemGetResponse> {
        let request = AccessTokenRequest {
            access_token: access_token.to_string(),
        };

        self.post("/item/get", &request).await
    }

    /// Remove an Item (revoke access)
    pub async fn item_remove(&self, access_token: &str) -> Result<ItemRemoveResponse> {
        let request = AccessTokenRequest {
            access_token: access_token.to_string(),
        };

        self.post("/item/remove", &request).await
    }

    /// Get investment holdings for an Item
    pub async fn investments_holdings_get(
        &self,
        access_token: &str,
    ) -> Result<InvestmentsHoldingsResponse> {
        let request = AccessTokenRequest {
            access_token: access_token.to_string(),
        };

        self.post("/investments/holdings/get", &request).await
    }

    /// Get liabilities for an Item (credit cards, mortgages, student loans)
    pub async fn liabilities_get(&self, access_token: &str) -> Result<LiabilitiesResponse> {
        let request = AccessTokenRequest {
            access_token: access_token.to_string(),
        };

        self.post("/liabilities/get", &request).await
    }
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Tollbooth proxy error format
#[derive(Debug, Deserialize)]
struct TollboothPlaidError {
    error: TollboothPlaidErrorDetails,
}

#[derive(Debug, Deserialize)]
struct TollboothPlaidErrorDetails {
    message: String,
    code: String,
}

/// Legacy Plaid error format (for direct API errors)
#[derive(Debug, Deserialize)]
struct PlaidError {
    error_code: String,
    error_message: String,
}

/// Tollbooth link token request format
#[derive(Debug, Serialize)]
struct TollboothLinkTokenRequest {
    user_client_id: String,
    products: Vec<String>,
    country_codes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_uri: Option<String>,
}

/// Tollbooth exchange token request format
#[derive(Debug, Serialize)]
struct TollboothExchangeTokenRequest {
    public_token: String,
    /// Current number of Plaid connections for tier-based limit enforcement
    #[serde(skip_serializing_if = "Option::is_none")]
    current_plaid_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct LinkTokenCreateResponse {
    pub link_token: String,
    pub expiration: String,
    pub request_id: String,
}


#[derive(Debug, Deserialize)]
pub struct ItemPublicTokenExchangeResponse {
    pub access_token: String,
    pub item_id: String,
    pub request_id: String,
}

#[derive(Debug, Serialize)]
struct AccessTokenRequest {
    access_token: String,
}

#[derive(Debug, Deserialize)]
pub struct AccountsGetResponse {
    pub accounts: Vec<Account>,
    pub item: Item,
    pub request_id: String,
}

#[derive(Debug, Deserialize)]
pub struct Account {
    pub account_id: String,
    pub name: String,
    pub official_name: Option<String>,
    #[serde(rename = "type")]
    pub account_type: String,
    pub subtype: Option<String>,
    pub mask: Option<String>,
    pub balances: AccountBalances,
}

#[derive(Debug, Deserialize)]
pub struct AccountBalances {
    pub current: Option<f64>,
    pub available: Option<f64>,
    pub limit: Option<f64>,
    pub iso_currency_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Item {
    pub item_id: String,
    pub institution_id: Option<String>,
    pub webhook: Option<String>,
    pub error: Option<serde_json::Value>,
    pub available_products: Vec<String>,
    pub billed_products: Vec<String>,
    pub consent_expiration_time: Option<String>,
    pub update_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct TransactionsSyncRequest {
    access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionsSyncResponse {
    pub added: Vec<Transaction>,
    pub modified: Vec<Transaction>,
    pub removed: Vec<RemovedTransaction>,
    pub next_cursor: String,
    pub has_more: bool,
    pub request_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Transaction {
    pub transaction_id: String,
    pub account_id: String,
    pub amount: f64,
    pub iso_currency_code: Option<String>,
    pub unofficial_currency_code: Option<String>,
    pub date: String,
    pub datetime: Option<String>,
    pub authorized_date: Option<String>,
    pub authorized_datetime: Option<String>,
    pub name: String,
    pub merchant_name: Option<String>,
    pub merchant_entity_id: Option<String>,
    pub logo_url: Option<String>,
    pub website: Option<String>,
    pub payment_channel: String,
    pub pending: bool,
    pub pending_transaction_id: Option<String>,
    pub account_owner: Option<String>,
    pub transaction_type: Option<String>,
    pub transaction_code: Option<String>,
    pub category: Option<Vec<String>>,
    pub category_id: Option<String>,
    pub personal_finance_category: Option<PersonalFinanceCategory>,
    pub location: Option<TransactionLocation>,
    pub payment_meta: Option<PaymentMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalFinanceCategory {
    pub primary: String,
    pub detailed: String,
    pub confidence_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionLocation {
    pub address: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub store_number: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentMeta {
    pub by_order_of: Option<String>,
    pub payee: Option<String>,
    pub payer: Option<String>,
    pub payment_method: Option<String>,
    pub payment_processor: Option<String>,
    pub ppd_id: Option<String>,
    pub reason: Option<String>,
    pub reference_number: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemovedTransaction {
    pub transaction_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ItemGetResponse {
    pub item: Item,
    pub status: Option<ItemStatus>,
    pub request_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ItemStatus {
    pub investments: Option<ProductStatus>,
    pub transactions: Option<ProductStatus>,
    pub last_webhook: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ProductStatus {
    pub last_successful_update: Option<String>,
    pub last_failed_update: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ItemRemoveResponse {
    pub request_id: String,
}

// ============================================================================
// Investments Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct InvestmentsHoldingsResponse {
    pub accounts: Vec<Account>,
    pub holdings: Vec<Holding>,
    pub securities: Vec<Security>,
    pub item: Item,
    pub request_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Holding {
    pub account_id: String,
    pub security_id: String,
    pub institution_price: Option<f64>,
    pub institution_price_as_of: Option<String>,
    pub institution_price_datetime: Option<String>,
    pub institution_value: Option<f64>,
    pub cost_basis: Option<f64>,
    pub quantity: f64,
    pub iso_currency_code: Option<String>,
    pub unofficial_currency_code: Option<String>,
    pub vested_quantity: Option<f64>,
    pub vested_value: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Security {
    pub security_id: String,
    pub isin: Option<String>,
    pub cusip: Option<String>,
    pub sedol: Option<String>,
    pub institution_security_id: Option<String>,
    pub institution_id: Option<String>,
    pub proxy_security_id: Option<String>,
    pub name: String,
    pub ticker_symbol: Option<String>,
    #[serde(rename = "type")]
    pub security_type: Option<String>,
    pub close_price: Option<f64>,
    pub close_price_as_of: Option<String>,
    pub iso_currency_code: Option<String>,
    pub unofficial_currency_code: Option<String>,
    pub is_cash_equivalent: Option<bool>,
    pub market_identifier_code: Option<String>,
    pub sector: Option<String>,
    pub industry: Option<String>,
    pub option_contract: Option<serde_json::Value>,
}

// ============================================================================
// Liabilities Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct LiabilitiesResponse {
    pub accounts: Vec<Account>,
    pub liabilities: LiabilitiesData,
    pub item: Item,
    pub request_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LiabilitiesData {
    pub credit: Option<Vec<CreditLiability>>,
    pub mortgage: Option<Vec<MortgageLiability>>,
    pub student: Option<Vec<StudentLoanLiability>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreditLiability {
    pub account_id: Option<String>,
    pub aprs: Vec<Apr>,
    pub is_overdue: Option<bool>,
    pub last_payment_amount: Option<f64>,
    pub last_payment_date: Option<String>,
    pub last_statement_issue_date: Option<String>,
    pub last_statement_balance: Option<f64>,
    pub minimum_payment_amount: Option<f64>,
    pub next_payment_due_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Apr {
    pub apr_percentage: f64,
    pub apr_type: String,
    pub balance_subject_to_apr: Option<f64>,
    pub interest_charge_amount: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MortgageLiability {
    pub account_id: String,
    pub account_number: String,
    pub current_late_fee: Option<f64>,
    pub escrow_balance: Option<f64>,
    pub has_pmi: Option<bool>,
    pub has_prepayment_penalty: Option<bool>,
    pub interest_rate: Option<MortgageInterestRate>,
    pub last_payment_amount: Option<f64>,
    pub last_payment_date: Option<String>,
    pub loan_type_description: Option<String>,
    pub loan_term: Option<String>,
    pub maturity_date: Option<String>,
    pub next_monthly_payment: Option<f64>,
    pub next_payment_due_date: Option<String>,
    pub origination_date: Option<String>,
    pub origination_principal_amount: Option<f64>,
    pub past_due_amount: Option<f64>,
    pub property_address: Option<MortgagePropertyAddress>,
    pub ytd_interest_paid: Option<f64>,
    pub ytd_principal_paid: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MortgageInterestRate {
    pub percentage: Option<f64>,
    #[serde(rename = "type")]
    pub rate_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MortgagePropertyAddress {
    pub city: Option<String>,
    pub country: Option<String>,
    pub postal_code: Option<String>,
    pub region: Option<String>,
    pub street: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StudentLoanLiability {
    pub account_id: Option<String>,
    pub account_number: Option<String>,
    pub disbursement_dates: Option<Vec<String>>,
    pub expected_payoff_date: Option<String>,
    pub guarantor: Option<String>,
    pub interest_rate_percentage: f64,
    pub is_overdue: Option<bool>,
    pub last_payment_amount: Option<f64>,
    pub last_payment_date: Option<String>,
    pub last_statement_issue_date: Option<String>,
    pub loan_name: Option<String>,
    pub loan_status: Option<StudentLoanStatus>,
    pub minimum_payment_amount: Option<f64>,
    pub next_payment_due_date: Option<String>,
    pub origination_date: Option<String>,
    pub origination_principal_amount: Option<f64>,
    pub outstanding_interest_amount: Option<f64>,
    pub pslf_status: Option<PslfStatus>,
    pub repayment_plan: Option<StudentLoanRepaymentPlan>,
    pub servicer_address: Option<ServicerAddress>,
    pub ytd_interest_paid: Option<f64>,
    pub ytd_principal_paid: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StudentLoanStatus {
    pub end_date: Option<String>,
    #[serde(rename = "type")]
    pub status_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PslfStatus {
    pub estimated_eligibility_date: Option<String>,
    pub payments_made: Option<i32>,
    pub payments_remaining: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StudentLoanRepaymentPlan {
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub plan_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServicerAddress {
    pub city: Option<String>,
    pub country: Option<String>,
    pub postal_code: Option<String>,
    pub region: Option<String>,
    pub street: Option<String>,
}

/// Plaid error categories for retry logic
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaidErrorCategory {
    UserActionRequired,
    Retryable,
    Terminal,
    Unknown,
}

impl PlaidErrorCategory {
    pub fn from_error_code(code: &str) -> Self {
        match code {
            "ITEM_LOGIN_REQUIRED" | "ACCESS_NOT_GRANTED" | "USER_SETUP_REQUIRED" => {
                Self::UserActionRequired
            }
            "PRODUCT_NOT_READY"
            | "INSTITUTION_DOWN"
            | "INSTITUTION_NO_LONGER_SUPPORTED"
            | "RATE_LIMIT_EXCEEDED"
            | "INTERNAL_SERVER_ERROR"
            | "PLANNED_MAINTENANCE" => Self::Retryable,
            "PRODUCT_NOT_SUPPORTED"
            | "NO_ACCOUNTS"
            | "INVALID_ACCESS_TOKEN"
            | "INVALID_PUBLIC_TOKEN"
            | "INVALID_PRODUCT" => Self::Terminal,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_detection() {
        // Test URL format
        std::env::set_var("PLAID_ENV", "https://sandbox.plaid.com");
        assert_eq!(PlaidEnvironment::from_env(), PlaidEnvironment::Sandbox);

        std::env::set_var("PLAID_ENV", "https://production.plaid.com");
        assert_eq!(PlaidEnvironment::from_env(), PlaidEnvironment::Production);

        // Test simple name format
        std::env::set_var("PLAID_ENV", "sandbox");
        assert_eq!(PlaidEnvironment::from_env(), PlaidEnvironment::Sandbox);

        std::env::remove_var("PLAID_ENV");
    }

    #[test]
    fn test_environment_variants() {
        // Just verify the enum variants exist and are distinct
        assert_ne!(PlaidEnvironment::Sandbox, PlaidEnvironment::Development);
        assert_ne!(PlaidEnvironment::Development, PlaidEnvironment::Production);
        assert_ne!(PlaidEnvironment::Sandbox, PlaidEnvironment::Production);
    }

    #[test]
    fn test_error_categories() {
        assert_eq!(
            PlaidErrorCategory::from_error_code("ITEM_LOGIN_REQUIRED"),
            PlaidErrorCategory::UserActionRequired
        );
        assert_eq!(
            PlaidErrorCategory::from_error_code("RATE_LIMIT_EXCEEDED"),
            PlaidErrorCategory::Retryable
        );
        assert_eq!(
            PlaidErrorCategory::from_error_code("NO_ACCOUNTS"),
            PlaidErrorCategory::Terminal
        );
    }
}
