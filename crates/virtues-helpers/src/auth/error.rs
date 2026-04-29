//! Unified error type for all auth helpers.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("unknown source: {0}")]
    UnknownSource(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("invalid or expired state token")]
    InvalidState,

    #[error("credential not found: {0}")]
    NotFound(String),

    #[error("credential conflict: {0}")]
    Conflict(String),

    /// Proxy call failed — either the proxy is unreachable, returned a
    /// non-2xx, or its body didn't deserialize. Single variant; the user
    /// experience is the same in all cases ("couldn't reach the provider,
    /// try again").
    #[error("proxy error: {0}")]
    Proxy(String),

    #[error("crypto error: {0}")]
    Crypto(#[from] crate::crypto::CryptoError),

    #[error("database error: {0}")]
    Database(String),

    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl AuthError {
    /// HTTP status to return when this error reaches an axum handler.
    pub fn http_status(&self) -> u16 {
        match self {
            AuthError::InvalidState => 401,
            AuthError::UnknownSource(_) | AuthError::NotFound(_) => 404,
            AuthError::InvalidInput(_) => 400,
            AuthError::Conflict(_) => 409,
            AuthError::Proxy(_) => 502,
            AuthError::Crypto(e) => e.http_status(),
            AuthError::Database(_) | AuthError::Serde(_) => 500,
        }
    }
}

impl From<sqlx::Error> for AuthError {
    fn from(err: sqlx::Error) -> Self {
        AuthError::Database(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AuthError>;
