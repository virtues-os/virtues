//! Bearer-token authentication (WS-6b).
//!
//! Replaces the legacy `X-User-Id` model: every gated call presents
//! `Authorization: Bearer <token>` where `<token>` is whatever opaque
//! string Atlas minted at activation. We SHA-256 the raw header bytes
//! and look up the entitlement row by hash.
//!
//! The raw bearer is never stored — only the hash lives in
//! `entitlements.bearer_hash`. A leaked DB yields no usable credentials.

use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts},
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::entitlement::{self, Entitlement};
use crate::AppState;

/// Successful bearer auth carries the resolved entitlement row.
pub struct BearerAuth(pub Entitlement);

pub enum BearerError {
    MissingHeader,
    MalformedHeader,
    NotFound,
    Expired,
    Blocked,
    Internal(String),
}

impl virtues_helpers::error::StructuredError for BearerError {
    fn status(&self) -> u16 {
        match self {
            Self::MissingHeader | Self::MalformedHeader | Self::NotFound => 401,
            Self::Expired => 402,
            Self::Blocked => 403,
            Self::Internal(_) => 500,
        }
    }
    fn code(&self) -> &str {
        match self {
            Self::MissingHeader => "missing_bearer",
            Self::MalformedHeader => "malformed_bearer",
            Self::NotFound => "unknown_bearer",
            Self::Expired => "bearer_expired",
            Self::Blocked => "blocked",
            Self::Internal(_) => "internal",
        }
    }
    fn message(&self) -> String {
        match self {
            Self::MissingHeader => "Authorization: Bearer header required".into(),
            Self::MalformedHeader => "expected `Authorization: Bearer <token>`".into(),
            Self::NotFound => "bearer not recognized".into(),
            Self::Expired => "bearer expired — redeem a fresh voucher".into(),
            Self::Blocked => "bearer is on the behavioral blocklist".into(),
            Self::Internal(m) => m.clone(),
        }
    }
}
virtues_helpers::impl_into_response!(BearerError);

#[axum::async_trait]
impl FromRequestParts<Arc<AppState>> for BearerAuth {
    type Rejection = BearerError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let raw = parts
            .headers
            .get(header::AUTHORIZATION)
            .ok_or(BearerError::MissingHeader)?;
        let header_str = raw.to_str().map_err(|_| BearerError::MalformedHeader)?;
        let bearer = header_str
            .strip_prefix("Bearer ")
            .ok_or(BearerError::MalformedHeader)?
            .trim();
        if bearer.is_empty() {
            return Err(BearerError::MalformedHeader);
        }

        let hash = sha256(bearer.as_bytes());

        let pool = &state.db;

        let ent = entitlement::get_by_bearer_hash(pool, &hash)
            .await
            .map_err(|e| BearerError::Internal(e.to_string()))?
            .ok_or(BearerError::NotFound)?;

        if ent.expires_at < chrono::Utc::now() {
            return Err(BearerError::Expired);
        }

        // Behavioral blocklist (in-memory hot path). Already-blocked bearers
        // are rejected immediately. Otherwise record the request against the
        // per-bearer rate window; exceeding the ceiling is always flagged +
        // logged, but only *blocks* when enforcement is explicitly enabled
        // (BLOCKLIST_RATE_AUTOBLOCK). Default is observe-only.
        if state.blocklist.is_blocked(&hash) {
            return Err(BearerError::Blocked);
        }
        if state.blocklist.note_request(&hash) && state.blocklist.autoblock_enabled() {
            state
                .blocklist
                .block(pool, &hash, crate::blocklist::REASON_RATE_ABUSE, None)
                .await;
            tracing::warn!("bearer auto-blocked for request-rate abuse");
            return Err(BearerError::Blocked);
        }

        Ok(BearerAuth(ent))
    }
}

fn sha256(data: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().to_vec()
}
