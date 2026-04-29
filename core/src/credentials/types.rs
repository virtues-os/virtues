//! Domain types for the credentials Vault.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Lifecycle state of a credential. Serialized as a lowercase string for the
/// `credentials.status` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialStatus {
    /// Handshake in flight (e.g. QR rendered, awaiting device callback).
    Pending,
    /// Working; runner can use this credential.
    Active,
    /// User-initiated revocation, or superseded by scope change.
    /// Terminal: rotate by reconnecting (yields a new credential).
    Revoked,
    /// Token expired and refresh failed, or provider signaled re-auth needed
    /// (e.g. Plaid `ITEM_LOGIN_REQUIRED`). User must reconnect to recover.
    ReauthRequired,
    /// Transient provider failure. Retried by the refresh loop or next run.
    Error,
}

impl CredentialStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Revoked => "revoked",
            Self::ReauthRequired => "reauth_required",
            Self::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "pending" => Ok(Self::Pending),
            "active" => Ok(Self::Active),
            "revoked" => Ok(Self::Revoked),
            "reauth_required" => Ok(Self::ReauthRequired),
            "error" => Ok(Self::Error),
            other => Err(Error::Other(format!("unknown credential status: {other}"))),
        }
    }
}

impl fmt::Display for CredentialStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row of the `credentials` Vault, with secrets already decrypted.
///
/// `secrets` is parsed into `serde_json::Value` because the shape varies by
/// connector. Today only `IosSecrets` exists; future connectors define their
/// own shapes (still as JSON inside the encrypted payload).
#[derive(Debug, Clone)]
pub struct Credential {
    pub id: String,
    pub source_id: String,
    pub name: String,
    pub status: CredentialStatus,
    pub status_reason: Option<String>,
    /// Decrypted secret payload. Shape declared by the connector manifest.
    pub secrets: serde_json::Value,
    pub scopes: Option<Vec<String>>,
    pub expires_at: Option<String>,
    pub next_refresh_at: Option<String>,
    pub metadata: serde_json::Value,
    pub last_seen_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Decrypted secret shape for `source_id = 'ios'` (and future
/// `auth.kind = self_issued_bearer` connectors like Mac).
///
/// Stored encrypted as JSON `{"token": "..."}` inside `secrets_ciphertext`.
/// The lookup HMAC of `token` lives in `secret_lookup_hash`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IosSecrets {
    pub token: String,
}
