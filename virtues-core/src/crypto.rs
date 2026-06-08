//! Crypto primitives — re-exported from `virtues_helpers::crypto`.
//!
//! The actual implementation lives in `crates/virtues-helpers/src/crypto/`.
//! This module is a thin shim that re-exports the public API and adds a
//! `From<CryptoError>` impl so existing callers can keep using
//! `crate::error::Error`.
//!
//! CI lints reject `Hmac::<Sha256>` outside `crates/virtues-helpers/src/crypto/`,
//! so all HMAC primitives flow through there.

pub use virtues_helpers::crypto::{CryptoError, OauthStateClaims, TokenEncryptor};

use crate::error::Error;

impl From<CryptoError> for Error {
    fn from(err: CryptoError) -> Self {
        match err {
            CryptoError::InvalidStateToken(msg) | CryptoError::KeyMissing(msg) => {
                Error::Unauthorized(msg)
            }
            CryptoError::StateTokenExpired => Error::Unauthorized("state token expired".into()),
            CryptoError::InvalidBase64(msg) | CryptoError::InvalidKey(msg) => {
                Error::InvalidInput(msg)
            }
            CryptoError::EncryptionFailed
            | CryptoError::DecryptionFailed
            | CryptoError::InvalidUtf8(_)
            | CryptoError::Hmac(_) => Error::Other(err.to_string()),
            CryptoError::Serde(e) => Error::Serialization(e),
        }
    }
}
