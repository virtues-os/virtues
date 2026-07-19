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
// Standalone AES-256-GCM under an explicit key (not the env master key)
// ─────────────────────────────────────────────────────────────────────────────

/// AES-256-GCM seal of raw bytes under an explicit 32-byte key — NOT the env
/// master key. Layout: `nonce(12) || ciphertext || tag(16)`, matching
/// `TokenEncryptor::encrypt`. A general-purpose seal for bytes under a
/// caller-supplied key that is never an environment secret. A fresh random
/// nonce per call is safe: the box is the sole writer and seals rarely.
pub fn seal_aes_256_gcm(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let unbound = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|_| CryptoError::InvalidKey("failed to create AES key".to_string()))?;
    let sealing = LessSafeKey::new(unbound);

    let mut nonce_bytes = [0u8; NONCE_LENGTH];
    SystemRandom::new()
        .fill(&mut nonce_bytes)
        .map_err(|_| CryptoError::EncryptionFailed)?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = plaintext.to_vec();
    in_out.reserve(AES_256_GCM.tag_len());
    sealing
        .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| CryptoError::EncryptionFailed)?;

    let mut out = nonce_bytes.to_vec();
    out.extend_from_slice(&in_out);
    Ok(out)
}

/// Inverse of [`seal_aes_256_gcm`]. Returns the plaintext, or
/// `DecryptionFailed` on a wrong key / tampered data / too-short input.
pub fn open_aes_256_gcm(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < NONCE_LENGTH + AES_256_GCM.tag_len() {
        return Err(CryptoError::DecryptionFailed);
    }
    let unbound = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|_| CryptoError::InvalidKey("failed to create AES key".to_string()))?;
    let opening = LessSafeKey::new(unbound);

    let (nonce_bytes, encrypted) = data.split_at(NONCE_LENGTH);
    let mut nonce_array = [0u8; NONCE_LENGTH];
    nonce_array.copy_from_slice(nonce_bytes);
    let nonce = Nonce::assume_unique_for_key(nonce_array);

    let mut in_out = encrypted.to_vec();
    let plaintext = opening
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| CryptoError::DecryptionFailed)?;
    Ok(plaintext.to_vec())
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

// ─────────────────────────────────────────────────────────────────────────
// OAuth proxy exchange tokens
// ─────────────────────────────────────────────────────────────────────────
//
// After a provider callback (Google/Notion/Strava/Plaid), the OAuth proxy
// signs the normalized `{secrets, metadata, expires_in, scopes}` payload as a
// short-lived HMAC token and redirects the browser back with
// `?exchange_token=...`. The home server then POSTs to
// `{proxy}/{source}/exchange/{token}` to pull the payload server-side. Keyed by
// an explicit `secret` (OAUTH_PROXY_EXCHANGE_SECRET): the proxy signs *and*
// verifies its own tokens, so it's self-consistent — no master-key dependency,
// which is why these are standalone fns rather than `TokenEncryptor` methods.

const EXCHANGE_TOKEN_TTL_SECS: i64 = 5 * 60;

/// Claims carried in an OAuth-proxy exchange token. `secrets`/`metadata` are
/// opaque provider payloads; `iat`/`exp` bound the 5-minute lifetime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeTokenClaims {
    pub source_id: String,
    pub secrets: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub expires_in: Option<i64>,
    pub scopes: Option<Vec<String>>,
    pub iat: i64,
    pub exp: i64,
}

/// Sign an exchange token (HMAC-SHA256 over base64url(claims); 5-min TTL).
/// `secret` must be at least 32 chars.
pub fn sign_exchange_token(
    secret: &str,
    source_id: &str,
    secrets: serde_json::Value,
    metadata: serde_json::Value,
    expires_in: Option<i64>,
    scopes: Option<Vec<String>>,
) -> Result<String> {
    if secret.len() < 32 {
        return Err(CryptoError::InvalidKey(
            "exchange secret must be >= 32 chars".into(),
        ));
    }
    let now = chrono::Utc::now().timestamp();
    let claims = ExchangeTokenClaims {
        source_id: source_id.to_string(),
        secrets,
        metadata,
        expires_in,
        scopes,
        iat: now,
        exp: now + EXCHANGE_TOKEN_TTL_SECS,
    };
    let body = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims)?);
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes())
        .map_err(|_| CryptoError::Hmac("exchange hmac key".into()))?;
    mac.update(body.as_bytes());
    let sig = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    Ok(format!("{body}.{sig}"))
}

/// Verify an exchange token: HMAC (constant-time) + expiry + source match.
pub fn verify_exchange_token(
    secret: &str,
    token: &str,
    expected_source_id: &str,
) -> Result<ExchangeTokenClaims> {
    let (body, sig_b64) = token
        .split_once('.')
        .ok_or_else(|| CryptoError::InvalidStateToken("malformed exchange_token".into()))?;

    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes())
        .map_err(|_| CryptoError::Hmac("exchange hmac key".into()))?;
    mac.update(body.as_bytes());
    let expected = mac.finalize().into_bytes();
    let provided = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|_| CryptoError::InvalidStateToken("malformed exchange signature".into()))?;
    if provided.len() != expected.len()
        || provided
            .iter()
            .zip(expected.iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            != 0
    {
        return Err(CryptoError::InvalidStateToken(
            "exchange_token signature mismatch".into(),
        ));
    }

    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(body)
        .map_err(|_| CryptoError::InvalidStateToken("malformed exchange body".into()))?;
    let claims: ExchangeTokenClaims = serde_json::from_slice(&raw)
        .map_err(|_| CryptoError::InvalidStateToken("invalid exchange claims".into()))?;

    if claims.exp < chrono::Utc::now().timestamp() {
        return Err(CryptoError::StateTokenExpired);
    }
    if claims.source_id != expected_source_id {
        return Err(CryptoError::InvalidStateToken(format!(
            "exchange_token source mismatch: token={} expected={expected_source_id}",
            claims.source_id
        )));
    }
    Ok(claims)
}

// ─────────────────────────────────────────────────────────────────────────
// Stripe webhook signature verification
// ─────────────────────────────────────────────────────────────────────────
//
// Stripe sends a `Stripe-Signature` header on every webhook delivery:
//
//     Stripe-Signature: t=1614266341,v1=68fbc40b3b6fcf1a9b1f1...
//
// Verification:
//   1. Parse the header — extract `t` (unix timestamp) and `v1` (signature).
//   2. Build `signed_payload = "<t>.<raw_body>"`.
//   3. Compute HMAC-SHA256 with the webhook secret.
//   4. Compare against `v1` in constant time.
//   5. Reject if the timestamp is outside `tolerance_seconds` of now.
//
// Spec: https://docs.stripe.com/webhooks/signatures

/// Errors from Stripe webhook signature verification.
#[derive(Debug, Error)]
pub enum StripeWebhookError {
    #[error("missing or malformed Stripe-Signature header")]
    MalformedHeader,
    #[error("signature timestamp outside tolerance ({0}s)")]
    TimestampOutsideTolerance(i64),
    #[error("signature mismatch")]
    SignatureMismatch,
}

/// Verify a Stripe webhook delivery.
///
/// - `payload`: the **raw** request body bytes (any reformatting breaks the HMAC)
/// - `signature_header`: the value of the `Stripe-Signature` HTTP header
/// - `secret`: the webhook signing secret (`whsec_...`) configured for this endpoint
/// - `tolerance_seconds`: how stale a delivery may be (Stripe recommends 300)
///
/// Returns `Ok(())` if the signature is valid; an error variant otherwise.
pub fn verify_stripe_signature(
    payload: &[u8],
    signature_header: &str,
    secret: &str,
    tolerance_seconds: i64,
) -> std::result::Result<(), StripeWebhookError> {
    // Parse `t=<ts>,v1=<sig>[,v1=<sig>...]`. Multiple v1 entries can occur during
    // signing-secret rotation — any one matching is acceptable.
    let mut timestamp: Option<i64> = None;
    let mut sigs: Vec<&str> = Vec::new();
    for part in signature_header.split(',') {
        let mut kv = part.splitn(2, '=');
        match (kv.next(), kv.next()) {
            (Some("t"), Some(v)) => {
                timestamp = v.parse::<i64>().ok();
            }
            (Some("v1"), Some(v)) => {
                sigs.push(v);
            }
            _ => {}
        }
    }

    let timestamp = timestamp.ok_or(StripeWebhookError::MalformedHeader)?;
    if sigs.is_empty() {
        return Err(StripeWebhookError::MalformedHeader);
    }

    // Tolerance check (replay protection).
    let now = chrono::Utc::now().timestamp();
    if (now - timestamp).abs() > tolerance_seconds {
        return Err(StripeWebhookError::TimestampOutsideTolerance(
            tolerance_seconds,
        ));
    }

    // Compute expected signature.
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| StripeWebhookError::MalformedHeader)?;
    mac.update(format!("{}.", timestamp).as_bytes());
    mac.update(payload);
    let expected = mac.finalize().into_bytes();
    let expected_hex = hex::encode(expected);

    // Constant-time compare against each provided signature.
    for sig in &sigs {
        if constant_time_eq(expected_hex.as_bytes(), sig.as_bytes()) {
            return Ok(());
        }
    }
    Err(StripeWebhookError::SignatureMismatch)
}

/// HMAC-SHA256(key, msg) as lowercase hex. Used to authenticate a value with a
/// shared secret across services/devices (e.g. the link-a-device MAC that binds a
/// device EndpointId to the one-time code). Lives here because Lint 3 forbids HMAC
/// primitives outside this module.
pub fn hmac_sha256_hex(key: &[u8], msg: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = <Hmac<Sha256>>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(msg);
    hex::encode(mac.finalize().into_bytes())
}

/// Length-independent-of-content byte comparison: returns `true` iff `a == b`,
/// taking time proportional to the (equal) length rather than short-circuiting
/// at the first differing byte. Use for comparing secrets/MACs/bearers so a
/// caller can't recover them via a timing side-channel. (Length itself is not
/// hidden — an early `false` on length mismatch is fine for fixed-size tokens.)
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
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
    fn exchange_token_round_trip() {
        let secret = "0123456789abcdef0123456789abcdef"; // 32 chars
        let tok = sign_exchange_token(
            secret,
            "google",
            serde_json::json!({ "access_token": "x", "refresh_token": "r" }),
            serde_json::json!({ "granted_scopes": "a b" }),
            Some(3600),
            Some(vec!["a".to_string(), "b".to_string()]),
        )
        .unwrap();

        let claims = verify_exchange_token(secret, &tok, "google").unwrap();
        assert_eq!(claims.source_id, "google");
        assert_eq!(claims.secrets["access_token"], "x");
        assert_eq!(claims.expires_in, Some(3600));

        // wrong expected source → reject
        assert!(verify_exchange_token(secret, &tok, "notion").is_err());
        // tampered signature → reject
        let (body, _) = tok.split_once('.').unwrap();
        assert!(verify_exchange_token(secret, &format!("{body}.AAAA"), "google").is_err());
        // wrong secret → reject
        assert!(verify_exchange_token("ffffffffffffffffffffffffffffffff", &tok, "google").is_err());
        // too-short secret on sign → error
        assert!(sign_exchange_token("short", "google", serde_json::json!({}), serde_json::json!({}), None, None).is_err());
    }

    #[test]
    fn aes_256_gcm_explicit_key_round_trip() {
        let key = [7u8; 32];
        let msg = b"rendezvous endpoint blob";
        let sealed = seal_aes_256_gcm(&key, msg).unwrap();
        assert_ne!(sealed.as_slice(), msg);
        let opened = open_aes_256_gcm(&key, &sealed).unwrap();
        assert_eq!(opened.as_slice(), msg);
    }

    #[test]
    fn aes_256_gcm_wrong_key_fails() {
        let sealed = seal_aes_256_gcm(&[1u8; 32], b"secret").unwrap();
        assert!(open_aes_256_gcm(&[2u8; 32], &sealed).is_err());
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
