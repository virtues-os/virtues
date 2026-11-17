//! Device api-key authentication, and the wallet check paid routes add to it.
//!
//! Every gated call presents `Authorization: Bearer <api_key>` where
//! `<api_key>` is the rotatable device key atlas minted at link. Two questions
//! are kept apart:
//!
//! - [`BoxKey`]: is this a key we issued? It resolves `device_keys` to the
//!   account and nothing else, so it holds for an owner who never paid or
//!   whose subscription lapsed.
//! - [`BearerAuth`]: is the account paying? It adds a live wallet and the
//!   request-rate check. Every route that spends money uses it.
//!
//! The raw key is never stored — only the hash lives in
//! `device_keys.api_key_hash`. A leaked DB yields no usable credentials.

use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts},
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::sync::Arc;

use crate::blocklist::Blocklist;
use crate::entitlement::{self, Wallet};
use crate::AppState;

/// A box key we issued, and the account it belongs to.
pub struct BoxKey {
    pub account_id: String,
    hash: Vec<u8>,
}

/// A box key whose account has a live wallet.
pub struct BearerAuth(pub Wallet);

#[derive(Debug)]
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
            Self::MissingHeader => "missing_key",
            Self::MalformedHeader => "malformed_key",
            Self::NotFound => "unknown_key",
            Self::Expired => "wallet_expired",
            Self::Blocked => "blocked",
            Self::Internal(_) => "internal",
        }
    }
    fn message(&self) -> String {
        match self {
            Self::MissingHeader => "Authorization: Bearer header required".into(),
            Self::MalformedHeader => "expected `Authorization: Bearer <api_key>`".into(),
            Self::NotFound => "this server's key is not recognized — link it again".into(),
            Self::Expired => "no active subscription — renew it to continue".into(),
            Self::Blocked => "key is on the behavioral blocklist".into(),
            Self::Internal(m) => m.clone(),
        }
    }
}
virtues_helpers::impl_into_response!(BearerError);

#[axum::async_trait]
impl FromRequestParts<Arc<AppState>> for BoxKey {
    type Rejection = BearerError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        logged(box_key(parts, state).await)
    }
}

#[axum::async_trait]
impl FromRequestParts<Arc<AppState>> for BearerAuth {
    type Rejection = BearerError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        logged(paying(parts, state).await)
    }
}

/// Failed authentication is logged, with the reason and nothing that names
/// the key or its account. Ordinary use is not logged.
fn logged<T>(result: Result<T, BearerError>) -> Result<T, BearerError> {
    match &result {
        Err(BearerError::Internal(e)) => tracing::error!(error = %e, "bearer auth lookup failed"),
        Err(e) => {
            use virtues_helpers::error::StructuredError;
            tracing::warn!(code = e.code(), "bearer auth refused");
        }
        Ok(_) => {}
    }
    result
}

async fn box_key(parts: &Parts, state: &Arc<AppState>) -> Result<BoxKey, BearerError> {
    let raw = parts
        .headers
        .get(header::AUTHORIZATION)
        .ok_or(BearerError::MissingHeader)?;
    let header_str = raw.to_str().map_err(|_| BearerError::MalformedHeader)?;
    let api_key = header_str
        .strip_prefix("Bearer ")
        .ok_or(BearerError::MalformedHeader)?
        .trim();
    if api_key.is_empty() {
        return Err(BearerError::MalformedHeader);
    }
    resolve_box_key(&state.db, &state.blocklist, api_key).await
}

/// Whether a raw box key is one we issued and not blocked. Shared by the
/// `Authorization: Bearer` extractors and the OAuth proxy, which reads the key
/// from its own header.
pub async fn resolve_box_key(
    db: &PgPool,
    blocklist: &Blocklist,
    api_key: &str,
) -> Result<BoxKey, BearerError> {
    let hash = key_hash(api_key);
    let account_id = entitlement::key_account(db, &hash)
        .await
        .map_err(|e| BearerError::Internal(e.to_string()))?
        .ok_or(BearerError::NotFound)?;
    if blocklist.is_blocked(&hash) {
        return Err(BearerError::Blocked);
    }
    Ok(BoxKey { account_id, hash })
}

async fn paying(parts: &Parts, state: &Arc<AppState>) -> Result<BearerAuth, BearerError> {
    let key = box_key(parts, state).await?;
    let pool = &state.db;

    // No wallet and an expired one answer the same: the account never paid,
    // its subscription lapsed, or the lapsed wallet was swept.
    let wallet = entitlement::get_wallet(pool, &key.account_id)
        .await
        .map_err(|e| BearerError::Internal(e.to_string()))?
        .filter(|w| w.expires_at >= chrono::Utc::now())
        .ok_or(BearerError::Expired)?;

    // Record the request against the per-bearer rate window; exceeding the
    // ceiling is always flagged + logged, but only *blocks* when enforcement
    // is explicitly enabled (BLOCKLIST_RATE_AUTOBLOCK). Default is
    // observe-only. Only paid routes count toward it.
    let hash = &key.hash;
    if state.blocklist.note_request(hash) && state.blocklist.autoblock_enabled() {
        state
            .blocklist
            .block(pool, hash, crate::blocklist::REASON_RATE_ABUSE, None)
            .await;
        tracing::warn!("bearer auto-blocked for request-rate abuse");
        return Err(BearerError::Blocked);
    }

    Ok(BearerAuth(wallet))
}

/// The form a device key is stored and looked up in (`device_keys.api_key_hash`).
pub fn key_hash(api_key: &str) -> Vec<u8> {
    Sha256::digest(api_key.as_bytes()).to_vec()
}
