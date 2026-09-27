//! REST API handlers.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use crate::error::Error;

// One file per area of the app, each with the handlers and the
// authenticated `routes()` for that area.
pub mod applets;
pub mod chat;
pub mod drive;
pub mod library;
pub mod pages;
pub mod settings;
pub mod wiki;

/// Sanitize a filename for use in Content-Disposition headers.
/// Removes characters that could cause header injection or parsing issues.
fn sanitize_content_disposition(filename: &str) -> String {
    filename
        .replace('"', "'")
        .replace('\\', "_")
        .replace('\r', "")
        .replace('\n', "")
}

/// Helper to convert Result to Response with proper status code
fn api_response<T: Serialize>(result: crate::error::Result<T>) -> Response {
    result.map(Json).into_response()
}

/// Helper to convert Error to Response with appropriate status code
fn error_response(error: Error) -> Response {
    error.into_response()
}

/// Helper to create a success message response
fn success_message(message: &str) -> Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "message": message })),
    )
        .into_response()
}

#[cfg(test)]
mod range_tests {
    use super::drive::{resolve_range, RangeOutcome};

    #[test]
    fn no_header_serves_full() {
        assert_eq!(resolve_range(None, 100), RangeOutcome::Full);
    }

    #[test]
    fn bounded_range() {
        assert_eq!(
            resolve_range(Some("bytes=0-49"), 100),
            RangeOutcome::Partial(0, 50)
        );
        assert_eq!(
            resolve_range(Some("bytes=10-19"), 100),
            RangeOutcome::Partial(10, 10)
        );
        // End past EOF clamps
        assert_eq!(
            resolve_range(Some("bytes=90-199"), 100),
            RangeOutcome::Partial(90, 10)
        );
    }

    #[test]
    fn open_ended_range() {
        assert_eq!(
            resolve_range(Some("bytes=40-"), 100),
            RangeOutcome::Partial(40, 60)
        );
    }

    #[test]
    fn suffix_range() {
        assert_eq!(
            resolve_range(Some("bytes=-10"), 100),
            RangeOutcome::Partial(90, 10)
        );
        // Suffix longer than the object serves the whole object
        assert_eq!(
            resolve_range(Some("bytes=-500"), 100),
            RangeOutcome::Partial(0, 100)
        );
    }

    #[test]
    fn unsatisfiable_ranges() {
        assert_eq!(
            resolve_range(Some("bytes=100-"), 100),
            RangeOutcome::Unsatisfiable
        );
        assert_eq!(
            resolve_range(Some("bytes=200-300"), 100),
            RangeOutcome::Unsatisfiable
        );
        assert_eq!(
            resolve_range(Some("bytes=-0"), 100),
            RangeOutcome::Unsatisfiable
        );
        // Any range against an empty object is unsatisfiable
        assert_eq!(
            resolve_range(Some("bytes=0-10"), 0),
            RangeOutcome::Unsatisfiable
        );
        assert_eq!(
            resolve_range(Some("bytes=-5"), 0),
            RangeOutcome::Unsatisfiable
        );
    }

    #[test]
    fn ignored_forms_serve_full() {
        // Multi-range: permitted to ignore, serve 200
        assert_eq!(resolve_range(Some("bytes=0-1,5-9"), 100), RangeOutcome::Full);
        // Malformed
        assert_eq!(resolve_range(Some("bytes=abc-def"), 100), RangeOutcome::Full);
        assert_eq!(resolve_range(Some("bytes=50-10"), 100), RangeOutcome::Full);
        assert_eq!(resolve_range(Some("bytes=-"), 100), RangeOutcome::Full);
        assert_eq!(resolve_range(Some("items=0-10"), 100), RangeOutcome::Full);
    }
}
