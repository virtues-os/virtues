//! Error types for Virtues

use thiserror::Error;

/// Main error type for Virtues
#[derive(Debug, Error)]
pub enum Error {
    /// Database-related errors
    #[error("Database error: {0}")]
    Database(String),

    /// Storage-related errors
    #[error("Storage error: {0}")]
    Storage(String),

    /// Source-related errors
    #[error("Source error: {0}")]
    Source(String),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Authentication errors
    #[error("Authentication error: {0}")]
    Authentication(String),

    /// Unauthorized access
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Not found errors
    #[error("Not found: {0}")]
    NotFound(String),

    /// Invalid input errors
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// HTTP errors
    #[error("HTTP error: {0}")]
    Http(String),

    /// External API errors (Google, etc.)
    #[error("External API error: {0}")]
    ExternalApi(String),

    /// Network errors
    #[error("Network error: {0}")]
    Network(String),

    /// Serialization errors
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// SQL errors
    #[error("SQL error: {0}")]
    Sql(#[from] sqlx::Error),

    /// Reqwest HTTP client errors
    #[error("HTTP client error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// Anyhow errors (from config loading, etc.)
    #[error("Error: {0}")]
    Anyhow(#[from] anyhow::Error),

    /// Missing device ID in push stream payload
    #[error("Missing device ID in payload")]
    MissingDeviceId,

    /// Empty payload in push stream
    #[error("Empty payload - no records to ingest")]
    EmptyPayload,

    /// Generic errors
    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Get HTTP status code for this error. The one mapping every HTTP
    /// handler answers with (via `IntoResponse` below).
    pub fn http_status(&self) -> u16 {
        match self {
            Error::Authentication(_) | Error::Unauthorized(_) => 401,
            Error::NotFound(_) => 404,
            Error::InvalidInput(_) => 400,
            Error::Configuration(_) => 503,
            // A second start of something that allows only one at a time.
            Error::Database(msg) if msg.contains("already has an active") => 409,
            _ => 500,
        }
    }

    /// Check if error should be logged at ERROR level (server errors)
    /// vs WARN level (client errors)
    pub fn is_server_error(&self) -> bool {
        matches!(
            self,
            Error::Database(_)
                | Error::Storage(_)
                | Error::Configuration(_)
                | Error::Network(_)
                | Error::Sql(_)
                | Error::Io(_)
                | Error::Anyhow(_)
                | Error::Other(_)
        )
    }
}

/// Every handler's error becomes `{status, {"error": "<message>"}}` — the
/// shape the web client's `request()` reads (`error`, then `message`).
impl axum::response::IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let status = axum::http::StatusCode::from_u16(self.http_status())
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);

        // A 500 that only the client sees is a 500 nobody finds. The bookmarks
        // list answered `no column found for name: timestamp` for a month while
        // a sweep of the journal for errors showed nothing, because this was the
        // one place the error passed through and it said nothing. The request
        // span already carries method, path and request id.
        if status.is_server_error() {
            tracing::error!(status = status.as_u16(), error = %self, "request failed");
        }

        (
            status,
            axum::Json(serde_json::json!({ "error": self.to_string() })),
        )
            .into_response()
    }
}

/// Result type alias for Virtues operations
pub type Result<T> = std::result::Result<T, Error>;

/// Macro for handling database query errors with consistent error messages
///
/// # Examples
///
/// ```ignore
/// use virtues::{db_query, error::Result};
/// use sqlx::PgPool;
///
/// async fn get_user(db: &PgPool, id: i32) -> Result<User> {
///     db_query!(
///         sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
///             .bind(id)
///             .fetch_one(db),
///         "Failed to fetch user"
///     )
/// }
/// ```
#[macro_export]
macro_rules! db_query {
    ($query:expr, $context:expr) => {
        $query
            .await
            .map_err(|e| $crate::error::Error::Database(format!("{}: {}", $context, e)))?
    };
}

/// Macro for handling database execute operations with consistent error messages
///
/// Similar to `db_query!` but specifically for execute operations that don't return data.
///
/// # Examples
///
/// ```rust
/// use virtues::{db_execute, error::Result};
/// use sqlx::PgPool;
///
/// async fn delete_user(db: &PgPool, id: i32) -> Result<()> {
///     db_execute!(
///         sqlx::query("DELETE FROM users WHERE id = $1")
///             .bind(id)
///             .execute(db),
///         "Failed to delete user"
///     );
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! db_execute {
    ($query:expr, $context:expr) => {
        $query
            .await
            .map_err(|e| $crate::error::Error::Database(format!("{}: {}", $context, e)))?
    };
}
