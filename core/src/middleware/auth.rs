//! Authentication middleware for Axum
//!
//! Validates session tokens from cookies and injects user info into request extensions.

use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Authenticated user information extracted from session cookie
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: Uuid,
    pub email: String,
    pub email_verified: Option<DateTime<Utc>>,
}

/// Session information
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_token: String,
    pub user_id: Uuid,
    pub expires: DateTime<Utc>,
}

/// Cookie name for session token
pub const SESSION_COOKIE_NAME: &str = "virtues.session-token";

/// Secure cookie name (for HTTPS)
pub const SESSION_COOKIE_NAME_SECURE: &str = "__Secure-virtues.session-token";

/// Auth error response
#[derive(Debug, Serialize)]
pub struct AuthError {
    pub error: String,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (StatusCode::UNAUTHORIZED, Json(self)).into_response()
    }
}

/// Extractor that requires authentication
///
/// Use this in handlers to ensure the request has a valid session:
///
/// ```ignore
/// async fn protected_handler(user: AuthUser) -> impl IntoResponse {
///     format!("Hello, {}!", user.email)
/// }
/// ```
#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    SqlitePool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Extract cookies
        let jar = CookieJar::from_headers(&parts.headers);

        // Try both cookie names (secure and non-secure)
        let session_token = jar
            .get(SESSION_COOKIE_NAME_SECURE)
            .or_else(|| jar.get(SESSION_COOKIE_NAME))
            .map(|c| c.value().to_string());

        let session_token = match session_token {
            Some(token) => token,
            None => {
                return Err(AuthError {
                    error: "Unauthorized".to_string(),
                });
            }
        };

        // Get database pool from state
        let pool = SqlitePool::from_ref(state);

        // Validate session and get user
        match validate_session(&pool, &session_token).await {
            Ok(user) => Ok(user),
            Err(_) => Err(AuthError {
                error: "Unauthorized".to_string(),
            }),
        }
    }
}

/// Validate a session token and return the associated user
pub async fn validate_session(pool: &SqlitePool, session_token: &str) -> crate::Result<AuthUser> {
    let row = sqlx::query!(
        r#"
        SELECT
            u.id,
            u.email,
            u.email_verified
        FROM app_auth_session s
        JOIN app_auth_user u ON s.user_id = u.id
        WHERE s.session_token = $1 AND s.expires > datetime('now')
        "#,
        session_token
    )
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => {
            // SQLite returns TEXT columns - parse to expected types
            let id_str = row
                .id
                .ok_or_else(|| crate::Error::Database("Missing user ID".into()))?;
            let id = Uuid::parse_str(&id_str)
                .map_err(|e| crate::Error::Database(format!("Invalid UUID: {}", e)))?;

            let email_verified = row
                .email_verified
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc));

            Ok(AuthUser {
                id,
                email: row.email,
                email_verified,
            })
        }
        None => Err(crate::Error::Unauthorized(
            "Invalid or expired session".into(),
        )),
    }
}

/// Create a new session for a user
pub async fn create_session(
    pool: &SqlitePool,
    user_id: Uuid,
    session_token: &str,
    expires: DateTime<Utc>,
) -> crate::Result<()> {
    // SQLite requires string conversion for UUID and ISO 8601 for datetime
    let session_id = Uuid::new_v4().to_string();
    let user_id_str = user_id.to_string();
    let expires_str = expires.to_rfc3339();

    sqlx::query!(
        r#"
        INSERT INTO app_auth_session (id, session_token, user_id, expires)
        VALUES ($1, $2, $3, $4)
        "#,
        session_id,
        session_token,
        user_id_str,
        expires_str
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Delete a session by token
pub async fn delete_session(pool: &SqlitePool, session_token: &str) -> crate::Result<()> {
    sqlx::query!(
        "DELETE FROM app_auth_session WHERE session_token = $1",
        session_token
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Delete all sessions for a user
pub async fn delete_all_user_sessions(pool: &SqlitePool, user_id: Uuid) -> crate::Result<()> {
    sqlx::query!("DELETE FROM app_auth_session WHERE user_id = $1", user_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// Cleanup expired sessions
pub async fn cleanup_expired_sessions(pool: &SqlitePool) -> crate::Result<u64> {
    let result = sqlx::query!("DELETE FROM app_auth_session WHERE expires < datetime('now')")
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

/// Cleanup expired verification tokens
pub async fn cleanup_expired_tokens(pool: &SqlitePool) -> crate::Result<u64> {
    let result =
        sqlx::query!("DELETE FROM app_auth_verification_token WHERE expires < datetime('now')")
            .execute(pool)
            .await?;

    Ok(result.rows_affected())
}

/// Middleware function that checks for valid authentication
///
/// Use this with `axum::middleware::from_fn_with_state` for routes that require auth:
///
/// ```ignore
/// let app = Router::new()
///     .route("/api/protected", get(protected_handler))
///     .layer(axum::middleware::from_fn_with_state(state.clone(), require_auth));
/// ```
pub async fn require_auth(
    jar: CookieJar,
    axum::extract::State(pool): axum::extract::State<SqlitePool>,
    mut req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Result<Response, AuthError> {
    // Try both cookie names
    let session_token = jar
        .get(SESSION_COOKIE_NAME_SECURE)
        .or_else(|| jar.get(SESSION_COOKIE_NAME))
        .map(|c| c.value().to_string());

    let session_token = match session_token {
        Some(token) => token,
        None => {
            return Err(AuthError {
                error: "Unauthorized".to_string(),
            });
        }
    };

    // Validate session
    let user = validate_session(&pool, &session_token)
        .await
        .map_err(|_| AuthError {
            error: "Unauthorized".to_string(),
        })?;

    // Insert user into request extensions
    req.extensions_mut().insert(user);

    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_cookie_names() {
        assert_eq!(SESSION_COOKIE_NAME, "virtues.session-token");
        assert_eq!(SESSION_COOKIE_NAME_SECURE, "__Secure-virtues.session-token");
    }
}
