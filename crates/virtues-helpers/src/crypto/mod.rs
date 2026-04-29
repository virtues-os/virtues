//! Token encryption (AES-256-GCM), HMAC lookup hashing, and signed
//! short-lived state tokens.
//!
//! Everything keyed off the single master key in the `VIRTUES_ENCRYPTION_KEY`
//! env var (base64-encoded 32 bytes). Sub-purpose peppers are derived from
//! the master key with a fixed domain separator — no second env var.
//!
//! This module is the **only** home for HMAC and AES primitives in the
//! workspace. CI lints reject `Hmac::<Sha256>` outside this path.

use base64::Engine;
use hmac::{Hmac, Mac};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;

const NONCE_LENGTH: usize = 12;

/// Domain separator for deriving the credential-lookup HMAC pepper. Bumping
/// the version invalidates all existing `secret_lookup_hash` values, so
/// don't bump it casually.
const LOOKUP_PEPPER_DOMAIN: &[u8] = b"credentials.lookup.v1";

/// Domain separator for deriving the OAuth-state signing pepper. Bumping the
/// version invalidates any in-flight OAuth state tokens (10-min window) but
/// is otherwise harmless.
const OAUTH_STATE_PEPPER_DOMAIN: &[u8] = b"oauth.state.v1";

/// Errors from the crypto layer. Mapped to HTTP status codes via `http_status()`.
#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("encryption key not configured: {0}")]
    KeyMissing(String),

    #[error("invalid key: {0}")]
    InvalidKey(String),

    #[error("encryption failed")]
    EncryptionFailed,

    #[error("decryption failed or data tampered")]
    DecryptionFailed,

    #[error("invalid utf-8 after decryption: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),

    #[error("invalid base64: {0}")]
    InvalidBase64(String),

    #[error("hmac error: {0}")]
    Hmac(String),

    #[error("invalid state token: {0}")]
    InvalidStateToken(String),

    #[error("expired state token")]
    StateTokenExpired,

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl CryptoError {
    /// HTTP status code for this error kind.
    pub fn http_status(&self) -> u16 {
        match self {
            CryptoError::InvalidStateToken(_) | CryptoError::StateTokenExpired => 401,
            CryptoError::InvalidBase64(_) => 400,
            _ => 500,
        }
    }
}

pub type Result<T> = std::result::Result<T, CryptoError>;

pub struct TokenEncryptor {
    key: LessSafeKey,
    /// Raw 32-byte master key, retained so we can derive sub-keys (HMAC pepper).
    key_bytes: [u8; 32],
    rng: SystemRandom,
}

impl TokenEncryptor {
    pub fn from_env() -> Result<Self> {
        let key_b64 = std::env::var("VIRTUES_ENCRYPTION_KEY").map_err(|_| {
            CryptoError::KeyMissing(
                "VIRTUES_ENCRYPTION_KEY not set. Generate with: openssl rand -base64 32"
                    .to_string(),
            )
        })?;
        Self::from_base64_key(&key_b64)
    }

    pub fn from_base64_key(key_b64: &str) -> Result<Self> {
        let key_bytes = base64::engine::general_purpose::STANDARD
            .decode(key_b64)
            .map_err(|e| CryptoError::InvalidBase64(format!("master key: {e}")))?;

        if key_bytes.len() != 32 {
            return Err(CryptoError::InvalidKey(format!(
                "expected 32 bytes, got {}",
                key_bytes.len()
            )));
        }

        let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes)
            .map_err(|_| CryptoError::InvalidKey("failed to create AES key".to_string()))?;

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);

        Ok(Self {
            key: LessSafeKey::new(unbound_key),
            key_bytes: key_array,
            rng: SystemRandom::new(),
        })
    }

    /// Derive a sub-purpose HMAC pepper from the master key with a domain
    /// separator. The result is itself an HMAC key, used to sign or hash
    /// purpose-specific values (lookup hashes, state tokens, etc.).
    fn derive_pepper(&self, domain: &[u8]) -> Result<[u8; 32]> {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.key_bytes)
            .map_err(|_| CryptoError::Hmac("HMAC key length error".to_string()))?;
        mac.update(domain);
        Ok(mac.finalize().into_bytes().into())
    }

    /// Compute a stable HMAC-SHA256 hash over a plaintext bearer token, used
    /// for O(1) lookup of `secret_lookup_hash` rows in the `credentials`
    /// table. The pepper is derived from the master key, so rotating the
    /// master key invalidates all existing lookup hashes (currently out of
    /// scope per the charter).
    ///
    /// Returns hex-encoded for stable column representation.
    pub fn lookup_hash(&self, plaintext: &str) -> Result<String> {
        let pepper = self.derive_pepper(LOOKUP_PEPPER_DOMAIN)?;
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&pepper)
            .map_err(|_| CryptoError::Hmac("HMAC pepper length error".to_string()))?;
        mac.update(plaintext.as_bytes());
        Ok(hex::encode(mac.finalize().into_bytes()))
    }

    /// Sign an OAuth state payload, producing a self-contained CSRF state
    /// token. Format: `<base64url(json(claims))>.<hex(hmac)>`.
    ///
    /// The verifier (`verify_oauth_state`) checks the HMAC against the same
    /// derived pepper, so an attacker cannot forge a state without the
    /// master key. Replay within the 10-minute expiry window is harmless —
    /// state is purely a CSRF defense; the actual auth happens via the
    /// proxy's token exchange.
    pub fn sign_oauth_state(&self, claims: &OauthStateClaims) -> Result<String> {
        let pepper = self.derive_pepper(OAUTH_STATE_PEPPER_DOMAIN)?;
        let payload_bytes = serde_json::to_vec(claims)?;
        let payload_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&payload_bytes);

        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&pepper)
            .map_err(|_| CryptoError::Hmac("HMAC pepper length error".to_string()))?;
        mac.update(payload_b64.as_bytes());
        let sig = hex::encode(mac.finalize().into_bytes());

        Ok(format!("{payload_b64}.{sig}"))
    }

    /// Verify and decode an OAuth state token. Returns the claims if the
    /// HMAC matches *and* `expires_at` is still in the future.
    pub fn verify_oauth_state(&self, state_token: &str) -> Result<OauthStateClaims> {
        let (payload_b64, sig_hex) = state_token
            .split_once('.')
            .ok_or_else(|| CryptoError::InvalidStateToken("malformed state token".into()))?;

        let pepper = self.derive_pepper(OAUTH_STATE_PEPPER_DOMAIN)?;
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&pepper)
            .map_err(|_| CryptoError::Hmac("HMAC pepper length error".to_string()))?;
        mac.update(payload_b64.as_bytes());
        let expected = mac.finalize().into_bytes();

        let presented = hex::decode(sig_hex)
            .map_err(|_| CryptoError::InvalidStateToken("malformed state signature".into()))?;
        // Constant-time comparison to avoid signature-timing oracles.
        if presented.len() != expected.len()
            || !presented
                .iter()
                .zip(expected.iter())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                .eq(&0u8)
        {
            return Err(CryptoError::InvalidStateToken(
                "invalid state signature".into(),
            ));
        }

        let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload_b64)
            .map_err(|_| CryptoError::InvalidStateToken("malformed state payload".into()))?;
        let claims: OauthStateClaims = serde_json::from_slice(&payload_bytes)
            .map_err(|_| CryptoError::InvalidStateToken("invalid state payload".into()))?;

        let now = chrono::Utc::now().timestamp();
        if now >= claims.expires_at {
            return Err(CryptoError::StateTokenExpired);
        }

        Ok(claims)
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<String> {
        if plaintext.is_empty() {
            return Ok(String::new());
        }

        let mut nonce_bytes = [0u8; NONCE_LENGTH];
        self.rng
            .fill(&mut nonce_bytes)
            .map_err(|_| CryptoError::EncryptionFailed)?;

        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let mut in_out = plaintext.as_bytes().to_vec();
        in_out.reserve(AES_256_GCM.tag_len());

        self.key
            .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
            .map_err(|_| CryptoError::EncryptionFailed)?;

        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&in_out);

        Ok(base64::engine::general_purpose::STANDARD.encode(&result))
    }

    pub fn decrypt(&self, ciphertext_b64: &str) -> Result<String> {
        if ciphertext_b64.is_empty() {
            return Ok(String::new());
        }

        let ciphertext = base64::engine::general_purpose::STANDARD
            .decode(ciphertext_b64)
            .map_err(|e| CryptoError::InvalidBase64(format!("ciphertext: {e}")))?;

        if ciphertext.len() < NONCE_LENGTH {
            return Err(CryptoError::DecryptionFailed);
        }

        let (nonce_bytes, encrypted) = ciphertext.split_at(NONCE_LENGTH);
        let mut nonce_array = [0u8; NONCE_LENGTH];
        nonce_array.copy_from_slice(nonce_bytes);
        let nonce = Nonce::assume_unique_for_key(nonce_array);

        let mut in_out = encrypted.to_vec();
        let plaintext = self
            .key
            .open_in_place(nonce, Aad::empty(), &mut in_out)
            .map_err(|_| CryptoError::DecryptionFailed)?;

        Ok(String::from_utf8(plaintext.to_vec())?)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// OAuth state claims
// ─────────────────────────────────────────────────────────────────────────────

/// Payload carried inside an OAuth state token. Signed (not encrypted) — the
/// values are not secret. The signature prevents forgery; the `expires_at`
/// caps the replay window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OauthStateClaims {
    /// Which source this in-flight flow belongs to.
    pub source_id: String,
    /// Non-null for re-auth / scope-change flows. `None` for fresh connects.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub existing_credential_id: Option<String>,
    /// Unix seconds. Verified to be in the future at decode time.
    pub expires_at: i64,
    /// Random per-token bytes. Hex-encoded for compactness.
    pub nonce: String,
}

impl OauthStateClaims {
    /// Build a fresh claims payload with a random nonce and a 10-minute
    /// expiry. The source_id and existing_credential_id are caller-supplied.
    pub fn new(source_id: String, existing_credential_id: Option<String>) -> Self {
        let mut nonce_bytes = [0u8; 16];
        SystemRandom::new()
            .fill(&mut nonce_bytes)
            .expect("SystemRandom should always produce bytes");
        Self {
            source_id,
            existing_credential_id,
            expires_at: chrono::Utc::now().timestamp() + 600, // 10 minutes
            nonce: hex::encode(nonce_bytes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_encryptor() -> TokenEncryptor {
        // 32 bytes of zero is fine for tests; real deployments use a random
        // master key from VIRTUES_ENCRYPTION_KEY.
        let key = base64::engine::general_purpose::STANDARD.encode([0u8; 32]);
        TokenEncryptor::from_base64_key(&key).expect("test key should parse")
    }

    #[test]
    fn round_trip() {
        let enc = test_encryptor();
        let claims = OauthStateClaims::new("google".into(), None);
        let token = enc.sign_oauth_state(&claims).expect("sign");
        let decoded = enc.verify_oauth_state(&token).expect("verify");
        assert_eq!(decoded, claims);
    }

    #[test]
    fn rejects_tampered_payload() {
        let enc = test_encryptor();
        let claims = OauthStateClaims::new("google".into(), None);
        let token = enc.sign_oauth_state(&claims).expect("sign");

        let (payload, sig) = token.split_once('.').unwrap();
        let mut tampered = payload.to_string();
        tampered.push('A');
        let bad = format!("{tampered}.{sig}");
        assert!(enc.verify_oauth_state(&bad).is_err());
    }

    #[test]
    fn rejects_tampered_signature() {
        let enc = test_encryptor();
        let claims = OauthStateClaims::new("google".into(), None);
        let token = enc.sign_oauth_state(&claims).expect("sign");

        let (payload, _) = token.split_once('.').unwrap();
        let bad = format!("{payload}.{}", "0".repeat(64));
        assert!(enc.verify_oauth_state(&bad).is_err());
    }

    #[test]
    fn rejects_expired() {
        let enc = test_encryptor();
        let mut claims = OauthStateClaims::new("google".into(), None);
        claims.expires_at = chrono::Utc::now().timestamp() - 1;
        let token = enc.sign_oauth_state(&claims).expect("sign");
        assert!(enc.verify_oauth_state(&token).is_err());
    }

    #[test]
    fn rejects_malformed() {
        let enc = test_encryptor();
        assert!(enc.verify_oauth_state("not-a-valid-token").is_err());
        assert!(enc.verify_oauth_state(".").is_err());
        assert!(enc.verify_oauth_state("payload.").is_err());
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let enc = test_encryptor();
        let plaintext = "hello world, this is a secret token";
        let ciphertext = enc.encrypt(plaintext).unwrap();
        assert_ne!(ciphertext, plaintext);
        let decrypted = enc.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn lookup_hash_is_stable() {
        let enc = test_encryptor();
        let h1 = enc.lookup_hash("device-token-abc").unwrap();
        let h2 = enc.lookup_hash("device-token-abc").unwrap();
        let h3 = enc.lookup_hash("different-token").unwrap();
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 64);
    }
}
