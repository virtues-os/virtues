//! OAuth CSRF state token signing and verification.
//!
//! Thin wrapper over `crate::crypto`. The state token format is
//! `<base64url(json(claims))>.<hex(hmac)>` — see `crate::crypto` for details.

use crate::auth::error::Result;
use crate::crypto::{OauthStateClaims, TokenEncryptor};

/// Sign a fresh OAuth state token for a connect flow.
///
/// `existing_credential_id` is `Some(...)` for re-auth / scope-change flows
/// (so we update an existing row), `None` for fresh connects (mint a new row
/// in the callback).
pub fn sign_oauth_state(
    source_id: &str,
    existing_credential_id: Option<&str>,
) -> Result<String> {
    let encryptor = TokenEncryptor::from_env()?;
    let claims = OauthStateClaims::new(
        source_id.to_string(),
        existing_credential_id.map(|s| s.to_string()),
    );
    Ok(encryptor.sign_oauth_state(&claims)?)
}

/// Verify an OAuth state token, returning the decoded claims if the HMAC
/// matches and `expires_at > now()`. Errors collapse to
/// `AuthError::InvalidState` (mapped to 401) — message intentionally generic
/// to avoid leaking whether the failure was signature or expiry.
pub fn verify_oauth_state(token: &str) -> Result<OauthStateClaims> {
    let encryptor = TokenEncryptor::from_env()?;
    encryptor
        .verify_oauth_state(token)
        .map_err(|e| match e {
            crate::crypto::CryptoError::StateTokenExpired
            | crate::crypto::CryptoError::InvalidStateToken(_) => {
                crate::auth::error::AuthError::InvalidState
            }
            other => crate::auth::error::AuthError::Crypto(other),
        })
}
