//! OAuth flow and source registration API

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::sources::get_source;
use super::types::SourceConnection;
use crate::error::{Error, Result};
use crate::sources::base::TokenManager;
use crate::storage::{stream_writer::StreamWriter, Storage};

/// Allowed HTTPS domains for OAuth return URLs
/// This allowlist prevents open redirect vulnerabilities
const ALLOWED_HTTPS_DOMAINS: &[&str] = &[
    // Production
    "app.virtues.com",
    "virtues.com",
    // Staging
    "staging.virtues.com",
    "staging-app.virtues.com",
    // Cloudflare quick tunnel (dev)
    ".trycloudflare.com",
    // Vercel previews (if used)
    ".vercel.app",
];

/// Validate that a return URL is from an allowed origin
fn validate_return_url(url: &str) -> Result<()> {
    // Allow relative paths (they'll be resolved on the client's origin)
    if url.starts_with('/') {
        return Ok(());
    }

    // Check localhost/native schemes first
    if url.starts_with("http://localhost:")
        || url.starts_with("http://127.0.0.1:")
        || url.starts_with("virtues://")
        || url.starts_with("virtues-mac://")
    {
        return Ok(());
    }

    // For HTTPS URLs, validate against allowed domains
    if url.starts_with("https://") {
        let parsed = url::Url::parse(url).map_err(|_| {
            Error::InvalidInput(format!("Invalid URL format: {}", url))
        })?;

        let host = parsed.host_str().ok_or_else(|| {
            Error::InvalidInput(format!("URL missing host: {}", url))
        })?;

        // Check if domain matches any allowed pattern
        let is_allowed_domain = ALLOWED_HTTPS_DOMAINS.iter().any(|pattern| {
            if pattern.starts_with('.') {
                // Suffix match (e.g., ".trycloudflare.com" matches "random-words.trycloudflare.com")
                host.ends_with(pattern)
            } else {
                // Exact match
                host == *pattern
            }
        });

        if is_allowed_domain {
            return Ok(());
        }
    }

    Err(Error::InvalidInput(format!(
        "Return URL not from allowed origin: {}. Allowed: localhost, virtues://, virtues-mac://, or approved HTTPS domains.",
        url
    )))
}

/// Request parameters for initiating OAuth authorization
#[derive(Debug, serde::Deserialize)]
pub struct OAuthAuthorizeRequest {
    /// @deprecated - No longer used. The backend determines the OAuth callback URL.
    /// Kept for API backwards compatibility only.
    #[serde(default)]
    pub redirect_uri: Option<String>,
    
    /// The full URL where the user should be redirected after OAuth completes.
    /// This is stored in a signed state token and validated against an allowlist.
    /// 
    /// Examples:
    /// - `http://localhost:5173/data/sources/add` (web dev)
    /// - `https://app.virtues.com/data/sources/add` (web prod)  
    /// - `virtues://oauth/callback` (iOS app)
    /// - `/data/sources/add` (relative path)
    pub state: Option<String>,
}

/// Response containing OAuth authorization URL
#[derive(Debug, serde::Serialize)]
pub struct OAuthAuthorizeResponse {
    pub authorization_url: String,
    pub state: String,
}

/// Response from OAuth callback containing source and optional return URL
#[derive(Debug, serde::Serialize)]
pub struct OAuthCallbackResponse {
    #[serde(flatten)]
    pub source: SourceConnection,
    /// Return URL extracted from state parameter (if provided during initiation)
    pub return_url: Option<String>,
}

/// OAuth callback query parameters
#[derive(Debug, serde::Deserialize)]
pub struct OAuthCallbackParams {
    pub code: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
    pub provider: String,
    pub state: Option<String>,
    // Notion-specific fields
    pub workspace_id: Option<String>,
    pub workspace_name: Option<String>,
    pub bot_id: Option<String>,
}

/// Request for creating a source manually
#[derive(Debug, serde::Deserialize)]
pub struct CreateSourceRequest {
    #[serde(rename = "type")]
    pub source_type: String,
    pub name: String,
    pub refresh_token: Option<String>,
    pub access_token: Option<String>,
    pub token_expires_at: Option<DateTime<Utc>>,
    pub device_id: Option<String>,
}

/// Request for registering a device as a source
#[derive(Debug, serde::Deserialize)]
pub struct RegisterDeviceRequest {
    pub device_type: String,
    pub device_id: String,
    pub name: String,
}

/// Initiate OAuth authorization flow
///
/// # Arguments
/// * `provider` - OAuth provider name (e.g., "google", "notion")
/// * `redirect_uri` - Unused, kept for API compatibility
/// * `return_url` - Full URL where user should be redirected after OAuth completes.
///   This can be a full URL (validated against allowlist) or a relative path.
///   Examples:
///   - `http://localhost:5173/data/sources/add` (web dev)
///   - `https://app.virtues.com/data/sources/add` (web prod)
///   - `virtues://oauth/callback` (iOS app)
///   - `/data/sources/add` (relative path, resolved by client)
pub async fn initiate_oauth_flow(
    provider: &str,
    _redirect_uri: Option<String>,
    return_url: Option<String>,
) -> Result<OAuthAuthorizeResponse> {
    // Validate provider exists in registry
    super::validation::validate_provider_name(provider)?;

    let descriptor = crate::registry::get_source(provider)
        .ok_or_else(|| Error::InvalidInput(format!("Unknown provider: {provider}")))?;

    if descriptor.descriptor.auth_type != crate::registry::AuthType::OAuth2 {
        return Err(Error::InvalidInput(format!(
            "Provider {provider} does not use OAuth2 authentication"
        )));
    }

    let oauth_config = descriptor
        .descriptor
        .oauth_config
        .as_ref()
        .ok_or_else(|| Error::Configuration(format!("No OAuth config for provider: {provider}")))?;

    // Get backend URL for OAuth callback (where OAuth provider redirects)
    let backend_url = std::env::var("BACKEND_URL")
        .unwrap_or_else(|_| "http://localhost:8000".to_string());

    // Validate and store the full return URL in state
    // This is where the user will be redirected after OAuth completes
    let return_url = return_url.unwrap_or_else(|| "/data/sources/add".to_string());
    validate_return_url(&return_url)?;
    
    let state_token = crate::sources::base::oauth::state::generate_state(Some(&return_url))?;

    let proxy_url =
        std::env::var("OAUTH_PROXY_URL").unwrap_or_else(|_| "https://auth.virtues.com".to_string());

    // Backend callback URL - where OAuth provider redirects after authorization
    let backend_callback_url = format!("{}/oauth/callback", backend_url);

    let scopes = oauth_config.scopes.join(" ");

    let authorization_url = format!(
        "{proxy_url}/{provider}/auth?return_url={}&state={}&scope={}",
        urlencoding::encode(&backend_callback_url),
        urlencoding::encode(&state_token),
        urlencoding::encode(&scopes)
    );

    Ok(OAuthAuthorizeResponse {
        authorization_url,
        state: state_token,
    })
}

/// Fetch a meaningful source name based on the OAuth provider
/// Falls back to "{Provider} Account" if fetching fails
async fn fetch_source_name(provider: &str, access_token: &str, display_name: &str) -> String {
    let client = reqwest::Client::new();
    
    match provider {
        "google" => {
            // Fetch user info from Google to get email
            #[derive(serde::Deserialize)]
            struct GoogleUserInfo {
                email: Option<String>,
                name: Option<String>,
            }
            
            match client
                .get("https://www.googleapis.com/oauth2/v2/userinfo")
                .bearer_auth(access_token)
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    if let Ok(info) = response.json::<GoogleUserInfo>().await {
                        if let Some(email) = info.email {
                            return email;
                        }
                        if let Some(name) = info.name {
                            return name;
                        }
                    }
                }
                _ => {}
            }
        }
        "notion" => {
            // Notion workspace name could be fetched from the /users/me endpoint
            // but it requires additional setup - fallback to default for now
        }
        _ => {}
    }
    
    // Fallback
    format!("{} Account", display_name)
}

/// Handle OAuth callback and create source
/// Supports both direct token flow and code exchange flow
/// Triggers initial sync for all enabled streams if storage and stream_writer are provided.
pub async fn handle_oauth_callback(
    db: &SqlitePool,
    storage: Option<&Storage>,
    stream_writer: Option<Arc<Mutex<StreamWriter>>>,
    params: &OAuthCallbackParams,
) -> Result<OAuthCallbackResponse> {
    // SECURITY: Validate state parameter and extract return URL
    let return_url = if let Some(ref state) = params.state {
        crate::sources::base::oauth::state::validate_and_extract_state(state)?
    } else {
        return Err(Error::InvalidInput(
            "Missing state parameter - possible CSRF attempt".to_string(),
        ));
    };

    // Validate provider exists
    super::validation::validate_provider_name(&params.provider)?;

    let descriptor = crate::registry::get_source(&params.provider)
        .ok_or_else(|| Error::InvalidInput(format!("Unknown provider: {}", params.provider)))?;

    // Get tokens either directly from callback or by exchanging code
    let (access_token, refresh_token, expires_in) = if let Some(token) = &params.access_token {
        // Direct token flow (used by Notion, Google)
        (
            token.clone(),
            params.refresh_token.clone(),
            params.expires_in,
        )
    } else if let Some(code) = &params.code {
        // Code exchange flow
        let proxy_url = std::env::var("OAUTH_PROXY_URL")
            .unwrap_or_else(|_| "https://auth.virtues.com".to_string());

        let client = reqwest::Client::new();

        let response = client
            .post(&format!("{}/{}/token", proxy_url, params.provider))
            .json(&serde_json::json!({
                "code": code,
            }))
            .send()
            .await
            .map_err(|e| Error::Http(format!("Failed to exchange code for tokens: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::Http(format!(
                "Token exchange failed: {} {}",
                status, error_text
            )));
        }

        #[derive(serde::Deserialize)]
        struct TokenResponse {
            access_token: String,
            #[serde(default)]
            refresh_token: Option<String>,
            #[serde(default)]
            expires_in: Option<i64>,
        }

        let token_data: TokenResponse = response
            .json()
            .await
            .map_err(|e| Error::Http(format!("Failed to parse token response: {e}")))?;

        (
            token_data.access_token,
            token_data.refresh_token,
            token_data.expires_in,
        )
    } else {
        return Err(Error::Other(
            "OAuth callback must include either 'code' or 'access_token'".to_string(),
        ));
    };

    // Fetch a meaningful name based on the provider
    let source_name = fetch_source_name(&params.provider, &access_token, &descriptor.descriptor.display_name).await;
    let token_manager = std::sync::Arc::new(TokenManager::new(db.clone())?);

    let source_id = token_manager
        .store_initial_tokens(
            &params.provider,
            &source_name,
            access_token,
            refresh_token,
            expires_in,
        )
        .await?;

    super::streams::enable_default_streams(db, source_id.clone(), &params.provider).await?;

    // Trigger initial sync for all enabled streams (only if storage and stream_writer are provided)
    if let (Some(storage), Some(stream_writer)) = (storage, stream_writer) {
        let source_reg = crate::registry::get_source(&params.provider);
        if let Some(reg) = source_reg {
            for stream_reg in &reg.streams {
                let stream_desc = &stream_reg.descriptor;
                if !stream_desc.enabled {
                    continue;
                }

                let db_clone = db.clone();
                let storage_clone = storage.clone();
                let stream_writer_clone = stream_writer.clone();
                let stream_name = stream_desc.name.to_string();
                let source_id_clone = source_id.clone();
                let provider = params.provider.clone();

                tokio::spawn(async move {
                    match crate::api::jobs::trigger_stream_sync(
                        &db_clone,
                        &storage_clone,
                        stream_writer_clone,
                        source_id_clone.clone(),
                        &stream_name,
                        None,
                    )
                    .await
                    {
                        Ok(response) => {
                            tracing::info!(
                                source_id = %source_id_clone,
                                provider = %provider,
                                stream = %stream_name,
                                job_id = %response.job_id,
                                "Initial sync job created for OAuth stream"
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                source_id = %source_id_clone,
                                provider = %provider,
                                stream = %stream_name,
                                error = %e,
                                "Failed to create initial sync job for OAuth stream"
                            );
                        }
                    }
                });
            }
        }
    }

    let source = get_source(db, source_id).await?;
    Ok(OAuthCallbackResponse { source, return_url })
}

/// Create a source manually (for testing or direct token input)
pub async fn create_source(
    db: &SqlitePool,
    request: CreateSourceRequest,
) -> Result<SourceConnection> {
    let descriptor = crate::registry::get_source(&request.source_type)
        .ok_or_else(|| Error::Other(format!("Unknown source type: {}", request.source_type)))?;

    let timestamp = chrono::Utc::now().to_rfc3339();
    let source_id = crate::ids::generate_id(crate::ids::SOURCE_PREFIX, &[&request.name, &timestamp]);

    match descriptor.descriptor.auth_type {
        crate::registry::AuthType::OAuth2 => {
            let refresh_token = request.refresh_token.clone().ok_or_else(|| {
                Error::Other("refresh_token required for OAuth2 sources".to_string())
            })?;

            let access_token = request.access_token.clone().ok_or_else(|| {
                Error::Other("access_token required for OAuth2 sources".to_string())
            })?;

            let expires_in = request
                .token_expires_at
                .map(|expires_at| (expires_at - Utc::now()).num_seconds());

            let token_manager = std::sync::Arc::new(TokenManager::new(db.clone())?);
            token_manager
                .store_initial_tokens(
                    &request.source_type,
                    &request.name,
                    access_token,
                    Some(refresh_token),
                    expires_in,
                )
                .await?;
        }
        crate::registry::AuthType::Device => {}
        _ => {
            return Err(Error::Other(format!(
                "Source type {} not yet supported for manual creation",
                request.source_type
            )));
        }
    }

    sqlx::query(
        r#"
        INSERT INTO elt_source_connections (id, source, name, is_active, is_internal, created_at, updated_at)
        VALUES ($1, $2, $3, true, false, datetime('now'), datetime('now'))
        "#,
    )
    .bind(&source_id)
    .bind(&request.source_type)
    .bind(&request.name)
    .execute(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to create source: {e}")))?;

    super::streams::enable_default_streams(db, source_id.clone(), &request.source_type).await?;

    get_source(db, source_id).await
}

/// Register a device as a source
pub async fn register_device(
    db: &SqlitePool,
    request: RegisterDeviceRequest,
) -> Result<SourceConnection> {
    let descriptor = crate::registry::get_source(&request.device_type)
        .ok_or_else(|| Error::Other(format!("Unknown device type: {}", request.device_type)))?;

    if descriptor.descriptor.auth_type != crate::registry::AuthType::Device {
        return Err(Error::Other(format!(
            "Source type {} is not a device source",
            request.device_type
        )));
    }

    let timestamp = chrono::Utc::now().to_rfc3339();
    let source_id = crate::ids::generate_id(crate::ids::SOURCE_PREFIX, &[&request.name, &timestamp]);

    sqlx::query(
        r#"
        INSERT INTO elt_source_connections (id, source, name, is_active, is_internal, created_at, updated_at)
        VALUES ($1, $2, $3, true, false, datetime('now'), datetime('now'))
        "#,
    )
    .bind(&source_id)
    .bind(&request.device_type)
    .bind(&request.name)
    .execute(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to register device: {e}")))?;

    super::streams::enable_default_streams(db, source_id.clone(), &request.device_type).await?;

    get_source(db, source_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_return_url_relative_paths() {
        // Relative paths should always be allowed
        assert!(validate_return_url("/data/sources/add").is_ok());
        assert!(validate_return_url("/").is_ok());
        assert!(validate_return_url("/oauth/callback?foo=bar").is_ok());
    }

    #[test]
    fn test_validate_return_url_localhost() {
        // Localhost on any port should be allowed
        assert!(validate_return_url("http://localhost:5173/data/sources").is_ok());
        assert!(validate_return_url("http://localhost:8000/callback").is_ok());
        assert!(validate_return_url("http://localhost:3000/").is_ok());
        assert!(validate_return_url("http://127.0.0.1:5173/data").is_ok());
    }

    #[test]
    fn test_validate_return_url_native_schemes() {
        // Native app schemes should be allowed
        assert!(validate_return_url("virtues://oauth/callback").is_ok());
        assert!(validate_return_url("virtues://").is_ok());
        assert!(validate_return_url("virtues-mac://oauth/callback").is_ok());
        assert!(validate_return_url("virtues-mac://settings").is_ok());
    }

    #[test]
    fn test_validate_return_url_production_domains() {
        // Production domains should be allowed
        assert!(validate_return_url("https://app.virtues.com/data/sources").is_ok());
        assert!(validate_return_url("https://virtues.com/callback").is_ok());
        assert!(validate_return_url("https://staging.virtues.com/test").is_ok());
    }

    #[test]
    fn test_validate_return_url_tunnel() {
        // Cloudflare quick tunnel should be allowed for dev
        assert!(validate_return_url("https://random-words-here.trycloudflare.com/callback").is_ok());
    }

    #[test]
    fn test_validate_return_url_vercel() {
        // Vercel preview deployments should be allowed
        assert!(validate_return_url("https://my-app-xyz.vercel.app/callback").is_ok());
    }

    #[test]
    fn test_validate_return_url_rejects_malicious() {
        // Should reject arbitrary domains
        assert!(validate_return_url("https://evil.com/steal-tokens").is_err());
        assert!(validate_return_url("https://attacker.io/phishing").is_err());
        
        // Should reject HTTP on non-localhost
        assert!(validate_return_url("http://evil.com/callback").is_err());
        
        // Should reject other schemes
        assert!(validate_return_url("javascript:alert(1)").is_err());
        assert!(validate_return_url("file:///etc/passwd").is_err());
        
        // Should reject attempts to bypass with subdomains
        assert!(validate_return_url("https://virtues.com.evil.com/callback").is_err());
        assert!(validate_return_url("https://fake-virtues.com/callback").is_err());
    }

    #[test]
    fn test_validate_return_url_edge_cases() {
        // Empty string should fail
        assert!(validate_return_url("").is_err());
        
        // Invalid URL format should fail
        assert!(validate_return_url("not-a-url").is_err());
        
        // URL without scheme should fail (not a relative path)
        assert!(validate_return_url("example.com/callback").is_err());
    }
}
