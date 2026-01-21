//! Authentication API endpoints
//!
//! Implements magic link authentication via Resend email service.
//! Single-user per tenant - uses "Seed and Drift" pattern for owner email:
//! - Environment variable OWNER_EMAIL seeds the database on first boot
//! - Database becomes the source of truth after seeding
//! - Atlas can update via webhook without container restart
//! - First login becomes owner if OWNER_EMAIL not set (local dev)

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

use crate::middleware::auth::{
    cleanup_expired_sessions, cleanup_expired_tokens, create_session, delete_session,
    validate_session, AuthUser, SESSION_COOKIE_NAME, SESSION_COOKIE_NAME_SECURE,
};

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct SignInRequest {
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct SignInResponse {
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub token: String,
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub user: Option<SessionUser>,
    /// Session expiry time in RFC3339 format (for frontend expiry warning)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionUser {
    pub id: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_verified: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuthErrorResponse {
    pub error: String,
}

/// Webhook request from Atlas to update owner email
#[derive(Debug, Deserialize)]
pub struct UpdateOwnerEmailRequest {
    pub email: String,
    /// Secret key to authenticate the webhook (should match ATLAS_WEBHOOK_SECRET)
    pub secret: String,
}

#[derive(Debug, Serialize)]
pub struct UpdateOwnerEmailResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ============================================================================
// Rate Limiting
// ============================================================================

struct RateLimitRecord {
    count: u32,
    reset_at: i64, // Unix timestamp
}

lazy_static::lazy_static! {
    static ref AUTH_ATTEMPTS: Arc<RwLock<HashMap<String, RateLimitRecord>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

const AUTH_WINDOW_MS: i64 = 15 * 60 * 1000; // 15 minutes
const AUTH_MAX_ATTEMPTS: u32 = 5;

fn check_rate_limit(ip: &str) -> Result<(), (StatusCode, Json<AuthErrorResponse>)> {
    let now = Utc::now().timestamp_millis();

    let mut attempts = AUTH_ATTEMPTS.write().unwrap();

    // Cleanup old entries (1% chance per request)
    if rand::random::<f32>() < 0.01 {
        attempts.retain(|_, v| v.reset_at > now);
    }

    if let Some(record) = attempts.get_mut(ip) {
        if record.reset_at < now {
            // Window expired, reset
            record.count = 1;
            record.reset_at = now + AUTH_WINDOW_MS;
            return Ok(());
        }

        if record.count >= AUTH_MAX_ATTEMPTS {
            let retry_after = (record.reset_at - now) / 1000;
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                Json(AuthErrorResponse {
                    error: format!(
                        "Too many login attempts. Please try again in {} seconds.",
                        retry_after
                    ),
                }),
            ));
        }

        record.count += 1;
    } else {
        attempts.insert(
            ip.to_string(),
            RateLimitRecord {
                count: 1,
                reset_at: now + AUTH_WINDOW_MS,
            },
        );
    }

    Ok(())
}

// ============================================================================
// Owner Email (Seed and Drift)
// ============================================================================

/// Seed owner_email from OWNER_EMAIL env var on first boot
/// Called once at server startup, after migrations
pub async fn seed_owner_email(pool: &SqlitePool) -> crate::Result<()> {
    // Check if owner_email is already set in DB
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT owner_email FROM data_user_profile WHERE id = '00000000-0000-0000-0000-000000000001'"
    )
    .fetch_optional(pool)
    .await?
    .flatten();

    if existing.is_some() {
        tracing::debug!("Owner email already set in database, skipping seed");
        return Ok(());
    }

    // Seed from environment variable
    if let Ok(owner_email) = std::env::var("OWNER_EMAIL") {
        let email = owner_email.trim().to_lowercase();
        if !email.is_empty() && is_valid_email(&email) {
            sqlx::query(
                "UPDATE data_user_profile SET owner_email = $1 WHERE id = '00000000-0000-0000-0000-000000000001'"
            )
            .bind(&email)
            .execute(pool)
            .await?;
            tracing::info!("Seeded owner_email from OWNER_EMAIL env var");
        } else {
            tracing::warn!("OWNER_EMAIL env var is empty or invalid, skipping seed");
        }
    } else {
        tracing::info!(
            "OWNER_EMAIL env var not set - first login will become owner (local dev mode)"
        );
    }

    Ok(())
}

/// Get the current owner email from database
async fn get_owner_email(pool: &SqlitePool) -> Option<String> {
    // The singleton row always exists, so we can use fetch_one
    // owner_email column may be NULL, so we get Option<String>
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT owner_email FROM data_user_profile WHERE id = '00000000-0000-0000-0000-000000000001'"
    )
    .fetch_one(pool)
    .await
    .ok()
    .flatten()
}

/// Set owner email in database (used by webhook and first-login flow)
async fn set_owner_email(pool: &SqlitePool, email: &str) -> crate::Result<()> {
    sqlx::query(
        "UPDATE data_user_profile SET owner_email = $1 WHERE id = '00000000-0000-0000-0000-000000000001'"
    )
    .bind(email)
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// Email Validation
// ============================================================================

fn is_valid_email(email: &str) -> bool {
    // Basic email validation
    if email.is_empty() || email.len() > 254 {
        return false;
    }

    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 {
        return false;
    }

    let local_part = parts[0];
    let domain = parts[1];

    if local_part.is_empty() || local_part.len() > 64 {
        return false;
    }

    if domain.is_empty() || !domain.contains('.') {
        return false;
    }

    // Basic regex-like check
    let valid_local_chars = |c: char| c.is_alphanumeric() || "._%+-".contains(c);
    let valid_domain_chars = |c: char| c.is_alphanumeric() || ".-".contains(c);

    local_part.chars().all(valid_local_chars) && domain.chars().all(valid_domain_chars)
}

// ============================================================================
// Token Generation
// ============================================================================

fn generate_token() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let bytes: [u8; 32] = rng.random();
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

fn generate_session_token() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let bytes: [u8; 32] = rng.random();
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

// ============================================================================
// Handlers
// ============================================================================

/// POST /auth/signin - Send magic link email
pub async fn signin_handler(
    State(pool): State<SqlitePool>,
    headers: axum::http::HeaderMap,
    Json(req): Json<SignInRequest>,
) -> impl IntoResponse {
    // Get client IP for rate limiting
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .unwrap_or("unknown")
        .trim()
        .to_string();

    // Check rate limit
    if let Err(response) = check_rate_limit(&ip) {
        return response.into_response();
    }

    let email = req.email.trim().to_lowercase();

    // Validate email format
    if !is_valid_email(&email) {
        // Always return success to prevent email enumeration
        return (
            StatusCode::OK,
            Json(SignInResponse {
                message: "If this email is registered, you will receive a sign-in link.".into(),
            }),
        )
            .into_response();
    }

    // Get owner email from database (Seed and Drift pattern)
    let owner_email = get_owner_email(&pool).await;

    // Check authorization
    match owner_email {
        Some(ref owner) => {
            // Owner email is set - must match
            if email != owner.to_lowercase() {
                tracing::info!("Unauthorized login attempt from: {}", email);
                // Silent return to prevent enumeration
                return (
                    StatusCode::OK,
                    Json(SignInResponse {
                        message: "If this email is registered, you will receive a sign-in link."
                            .into(),
                    }),
                )
                    .into_response();
            }
        }
        None => {
            // No owner set - first login becomes owner (local dev mode)
            tracing::info!(
                "No owner email set - first login from {} will become owner",
                email
            );
            if let Err(e) = set_owner_email(&pool, &email).await {
                tracing::error!("Failed to set owner email: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(AuthErrorResponse {
                        error: "Failed to process request".into(),
                    }),
                )
                    .into_response();
            }
            tracing::info!("Set {} as owner (first login)", email);
        }
    }

    // Generate verification token
    let token = generate_token();
    let expires = Utc::now() + Duration::hours(24);

    // Store verification token
    if let Err(e) = sqlx::query!(
        r#"
        INSERT INTO app_auth_verification_token (identifier, token, expires)
        VALUES ($1, $2, $3)
        ON CONFLICT (identifier, token) DO UPDATE SET expires = $3
        "#,
        email,
        token,
        expires
    )
    .execute(&pool)
    .await
    {
        tracing::error!("Failed to store verification token: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AuthErrorResponse {
                error: "Failed to process request".into(),
            }),
        )
            .into_response();
    }

    // Build callback URL
    let base_url = std::env::var("AUTH_URL")
        .unwrap_or_else(|_| "http://localhost:5173".to_string());
    let callback_url = format!(
        "{}/auth/callback?token={}&email={}",
        base_url,
        urlencoding::encode(&token),
        urlencoding::encode(&email)
    );

    // Send email or log in development
    let environment = std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string());

    if environment == "development" {
        tracing::info!("\n========================================");
        tracing::info!("[Auth Dev] Magic link (click to sign in):");
        tracing::info!("{}", callback_url);
        tracing::info!("========================================\n");
    } else {
        // Send via Resend API
        if let Err(e) = send_magic_link_email(&email, &callback_url).await {
            tracing::error!("Failed to send magic link email: {}", e);
            // Don't reveal the error to the user
        }
    }

    (
        StatusCode::OK,
        Json(SignInResponse {
            message: "If this email is registered, you will receive a sign-in link.".into(),
        }),
    )
        .into_response()
}

/// Send magic link email via Resend API
async fn send_magic_link_email(email: &str, url: &str) -> crate::Result<()> {
    let api_key = std::env::var("RESEND_API_KEY")
        .map_err(|_| crate::Error::Configuration("RESEND_API_KEY not set".into()))?;

    let from =
        std::env::var("EMAIL_FROM").unwrap_or_else(|_| "Virtues <noreply@virtues.com>".into());

    let client = reqwest::Client::new();
    let response = client
        .post("https://api.resend.com/emails")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "from": from,
            "to": email,
            "subject": "Sign in to Virtues",
            "html": format!(r#"
                <div style="font-family: sans-serif; max-width: 400px; margin: 0 auto; padding: 20px;">
                    <h1 style="font-size: 24px; margin-bottom: 16px;">Sign in to Virtues</h1>
                    <p style="color: #666; margin-bottom: 24px;">Click the button below to sign in. This link expires in 24 hours.</p>
                    <a href="{}" style="display: inline-block; background: #000; color: #fff; padding: 12px 24px; text-decoration: none; border-radius: 6px; font-weight: 500;">Sign in</a>
                    <p style="color: #999; font-size: 12px; margin-top: 24px;">If you didn't request this email, you can safely ignore it.</p>
                </div>
            "#, url)
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(crate::Error::ExternalApi(format!(
            "Resend API error: {}",
            error_text
        )));
    }

    Ok(())
}

/// GET /auth/callback - Verify token and create session
pub async fn callback_handler(
    State(pool): State<SqlitePool>,
    Query(params): Query<CallbackParams>,
    jar: CookieJar,
) -> impl IntoResponse {
    let email = params.email.trim().to_lowercase();
    let token = params.token.trim();

    // Verify and consume the token
    let result = sqlx::query!(
        r#"
        DELETE FROM app_auth_verification_token
        WHERE identifier = $1 AND token = $2 AND expires > datetime('now')
        RETURNING identifier, token, expires
        "#,
        email,
        token
    )
    .fetch_optional(&pool)
    .await;

    match result {
        Ok(Some(_)) => {
            // Token is valid, create or get user
            let user = match get_or_create_user(&pool, &email).await {
                Ok(user) => user,
                Err(e) => {
                    tracing::error!("Failed to get/create user: {}", e);
                    // Redirect to error page
                    return (
                        StatusCode::FOUND,
                        [(
                            axum::http::header::LOCATION,
                            "/login/error?error=Configuration",
                        )],
                        jar,
                    )
                        .into_response();
                }
            };

            // Create session
            let session_token = generate_session_token();
            let expires = Utc::now() + Duration::days(30);

            if let Err(e) = create_session(&pool, user.id, &session_token, expires).await {
                tracing::error!("Failed to create session: {}", e);
                return (
                    StatusCode::FOUND,
                    [(
                        axum::http::header::LOCATION,
                        "/login/error?error=Configuration",
                    )],
                    jar,
                )
                    .into_response();
            }

            // Set session cookie
            let is_secure = std::env::var("ENVIRONMENT")
                .map(|v| v == "production")
                .unwrap_or(false);

            let cookie_name = if is_secure {
                SESSION_COOKIE_NAME_SECURE
            } else {
                SESSION_COOKIE_NAME
            };

            let cookie = Cookie::build((cookie_name, session_token))
                .path("/")
                .http_only(true)
                .secure(is_secure)
                .same_site(SameSite::Lax)
                .max_age(time::Duration::days(30))
                .build();

            let jar = jar.add(cookie);

            // Redirect to home
            (
                StatusCode::FOUND,
                [(axum::http::header::LOCATION, "/")],
                jar,
            )
                .into_response()
        }
        Ok(None) => {
            // Invalid or expired token
            (
                StatusCode::FOUND,
                [(
                    axum::http::header::LOCATION,
                    "/login/error?error=Verification",
                )],
                jar,
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Database error during callback: {}", e);
            (
                StatusCode::FOUND,
                [(
                    axum::http::header::LOCATION,
                    "/login/error?error=Configuration",
                )],
                jar,
            )
                .into_response()
        }
    }
}

/// Get or create a user by email
async fn get_or_create_user(pool: &SqlitePool, email: &str) -> crate::Result<AuthUser> {
    use crate::error::Error;

    // Try to get existing user
    let existing = sqlx::query!(
        r#"
        SELECT id, email, email_verified
        FROM app_auth_user
        WHERE email = $1
        "#,
        email
    )
    .fetch_optional(pool)
    .await?;

    if let Some(user) = existing {
        let user_id = user
            .id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| Error::Database("Invalid user ID".to_string()))?;

        // Update email_verified timestamp
        let id_str = user_id.to_string();
        sqlx::query!(
            "UPDATE app_auth_user SET email_verified = datetime('now') WHERE id = $1",
            id_str
        )
        .execute(pool)
        .await?;

        return Ok(AuthUser {
            id: user_id,
            email: user.email,
            email_verified: Some(Utc::now()),
        });
    }

    // Create new user - generate UUID in application
    let user_id = Uuid::new_v4();
    let user_id_str = user_id.to_string();

    sqlx::query!(
        r#"
        INSERT INTO app_auth_user (id, email, email_verified)
        VALUES ($1, $2, datetime('now'))
        "#,
        user_id_str,
        email
    )
    .execute(pool)
    .await?;

    Ok(AuthUser {
        id: user_id,
        email: email.to_string(),
        email_verified: Some(Utc::now()),
    })
}

/// POST /auth/signout - Delete session and clear cookie
pub async fn signout_handler(State(pool): State<SqlitePool>, jar: CookieJar) -> impl IntoResponse {
    // Get session token from cookie
    let session_token = jar
        .get(SESSION_COOKIE_NAME_SECURE)
        .or_else(|| jar.get(SESSION_COOKIE_NAME))
        .map(|c| c.value().to_string());

    if let Some(token) = session_token {
        // Delete session from database
        if let Err(e) = delete_session(&pool, &token).await {
            tracing::warn!("Failed to delete session: {}", e);
        }
    }

    // Clear both cookie variants
    let jar = jar
        .remove(Cookie::from(SESSION_COOKIE_NAME))
        .remove(Cookie::from(SESSION_COOKIE_NAME_SECURE));

    (StatusCode::OK, jar, Json(serde_json::json!({ "ok": true })))
}

/// GET /auth/session - Get current session for frontend
pub async fn session_handler(State(pool): State<SqlitePool>, jar: CookieJar) -> impl IntoResponse {
    // Get session token from cookie
    let session_token = jar
        .get(SESSION_COOKIE_NAME_SECURE)
        .or_else(|| jar.get(SESSION_COOKIE_NAME))
        .map(|c| c.value().to_string());

    let session_token = match session_token {
        Some(token) => token,
        None => {
            return (
                StatusCode::OK,
                Json(SessionResponse {
                    user: None,
                    expires: None,
                }),
            );
        }
    };

    // Validate session and get user
    let user = match validate_session(&pool, &session_token).await {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::OK,
                Json(SessionResponse {
                    user: None,
                    expires: None,
                }),
            );
        }
    };

    // Get session expiry separately (avoid compile-time query macro cache issues)
    let expires: Option<String> =
        sqlx::query_scalar("SELECT expires FROM app_auth_session WHERE session_token = $1")
            .bind(&session_token)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten()
            .map(|dt: chrono::DateTime<Utc>| dt.to_rfc3339());

    (
        StatusCode::OK,
        Json(SessionResponse {
            user: Some(SessionUser {
                id: user.id.to_string(),
                email: user.email,
                email_verified: user.email_verified.map(|dt| dt.to_rfc3339()),
            }),
            expires,
        }),
    )
}

/// Cleanup handler - removes expired sessions and tokens
/// Can be called on server startup or via scheduled task
pub async fn cleanup_auth_data(pool: &SqlitePool) -> crate::Result<(u64, u64)> {
    let sessions = cleanup_expired_sessions(pool).await?;
    let tokens = cleanup_expired_tokens(pool).await?;
    Ok((sessions, tokens))
}

/// POST /api/profile/owner-email - Webhook for Atlas to update owner email
/// Authenticated via ATLAS_WEBHOOK_SECRET env var
pub async fn update_owner_email_handler(
    State(pool): State<SqlitePool>,
    Json(req): Json<UpdateOwnerEmailRequest>,
) -> impl IntoResponse {
    // Authenticate the webhook
    let expected_secret = std::env::var("ATLAS_WEBHOOK_SECRET").ok();

    match expected_secret {
        Some(secret) if secret == req.secret => {
            // Authenticated - proceed
        }
        Some(_) => {
            tracing::warn!("Invalid webhook secret for owner email update");
            return (
                StatusCode::UNAUTHORIZED,
                Json(UpdateOwnerEmailResponse {
                    success: false,
                    message: Some("Invalid webhook secret".into()),
                }),
            );
        }
        None => {
            tracing::warn!("ATLAS_WEBHOOK_SECRET not set - rejecting webhook");
            return (
                StatusCode::UNAUTHORIZED,
                Json(UpdateOwnerEmailResponse {
                    success: false,
                    message: Some("Webhook not configured".into()),
                }),
            );
        }
    }

    // Validate and normalize email
    let email = req.email.trim().to_lowercase();
    if !is_valid_email(&email) {
        return (
            StatusCode::BAD_REQUEST,
            Json(UpdateOwnerEmailResponse {
                success: false,
                message: Some("Invalid email format".into()),
            }),
        );
    }

    // Update owner email in database
    if let Err(e) = set_owner_email(&pool, &email).await {
        tracing::error!("Failed to update owner email via webhook: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(UpdateOwnerEmailResponse {
                success: false,
                message: Some("Failed to update owner email".into()),
            }),
        );
    }

    tracing::info!("Owner email updated via Atlas webhook to: {}", email);

    (
        StatusCode::OK,
        Json(UpdateOwnerEmailResponse {
            success: true,
            message: Some("Owner email updated".into()),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_validation() {
        assert!(is_valid_email("test@example.com"));
        assert!(is_valid_email("user.name+tag@example.co.uk"));
        assert!(is_valid_email("user@sub.domain.com"));

        assert!(!is_valid_email(""));
        assert!(!is_valid_email("invalid"));
        assert!(!is_valid_email("@example.com"));
        assert!(!is_valid_email("user@"));
        assert!(!is_valid_email("user@domain"));
    }

    #[test]
    fn test_token_generation() {
        let token1 = generate_token();
        let token2 = generate_token();

        assert!(!token1.is_empty());
        assert!(!token2.is_empty());
        assert_ne!(token1, token2);
    }
}
