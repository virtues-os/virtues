//! Shared error contract for HTTP-facing services.
//!
//! This is deliberately **framework-agnostic** (no axum/tower): `virtues-helpers`
//! is also linked by the 21 action binaries and `virtues-wg`, none of which are
//! web servers. The trait + body builder give every service ONE error wire shape
//! and ONE status-mapping pattern; each axum crate keeps a ~5-line `IntoResponse`
//! shim that calls into here (see `impl_into_response!`).
//!
//! Canonical wire shape:
//! ```json
//! { "error": { "code": "insufficient_budget", "message": "...", ...extra } }
//! ```

use serde_json::{json, Value};

/// Implemented by every HTTP-facing error enum. Domain crates keep their own
/// enum (separate processes, distinct failure modes) but converge on this shape.
pub trait StructuredError {
    /// HTTP status code (e.g. 402, 401, 502).
    fn status(&self) -> u16;
    /// Stable machine-readable code (e.g. "bearer_expired"). Clients branch on this.
    fn code(&self) -> &str;
    /// Human-readable message. Never include secrets or prompt/response bodies.
    fn message(&self) -> String;
    /// Optional extra fields merged into the error object (e.g. balances, caps).
    /// Default: none.
    fn extra(&self) -> Value {
        Value::Null
    }
}

/// Build the canonical error body `{"error": {"code", "message", ...extra}}`.
pub fn error_body(err: &impl StructuredError) -> Value {
    let mut obj = json!({ "code": err.code(), "message": err.message() });
    if let Some(map) = err.extra().as_object() {
        if let Some(target) = obj.as_object_mut() {
            for (k, v) in map {
                target.insert(k.clone(), v.clone());
            }
        }
    }
    json!({ "error": obj })
}

/// Generate the per-service axum glue for a `StructuredError` enum:
/// a `(StatusCode, Json(body))` `IntoResponse`. Kept as a macro (not a blanket
/// impl) because the axum dependency must stay out of this crate and orphan
/// rules forbid a blanket `impl IntoResponse`.
///
/// Usage in an axum crate:
/// ```ignore
/// virtues_helpers::impl_into_response!(BearerError);
/// ```
#[macro_export]
macro_rules! impl_into_response {
    ($ty:ty) => {
        impl axum::response::IntoResponse for $ty {
            fn into_response(self) -> axum::response::Response {
                let status = axum::http::StatusCode::from_u16(
                    $crate::error::StructuredError::status(&self),
                )
                .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
                let body = $crate::error::error_body(&self);
                (status, axum::Json(body)).into_response()
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    struct E;
    impl StructuredError for E {
        fn status(&self) -> u16 {
            402
        }
        fn code(&self) -> &str {
            "insufficient_budget"
        }
        fn message(&self) -> String {
            "wallet empty".into()
        }
        fn extra(&self) -> Value {
            json!({ "balance_micros": 0 })
        }
    }

    #[test]
    fn body_shape_merges_extra() {
        let b = error_body(&E);
        assert_eq!(b["error"]["code"], "insufficient_budget");
        assert_eq!(b["error"]["message"], "wallet empty");
        assert_eq!(b["error"]["balance_micros"], 0);
    }
}
