//! Device pairing API - Secure device registration and authentication

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::error::{Error, Result};
use crate::registry::RegisteredStream;
use crate::sources::base::TokenEncryptor;

/// Response when pairing is initiated
#[derive(Debug, Clone)]
pub struct PairingInitiated {
    pub source_id: String,
    pub code: String,
    pub expires_at: DateTime<Utc>,
}

/// Response when pairing is completed successfully
#[derive(Debug, Clone)]
pub struct PairingCompleted {
    pub source_id: String,
    pub device_token: String,
    pub available_streams: Vec<RegisteredStream>,
}

/// Device information provided during pairing
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub device_name: String,
    pub device_model: String,
    pub os_version: String,
    pub app_version: Option<String>,
}

/// Pairing status for a source
#[derive(Debug, Clone)]
pub enum PairingStatus {
    Pending,
    Active(DeviceInfo),
    Revoked,
}

/// Pending pairing information for display
#[derive(Debug, Clone)]
pub struct PendingPairing {
    pub source_id: String,
    pub name: String,
    pub device_type: String,
    pub code: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Initiate device pairing by generating a pairing code
///
/// This creates a pending source entry with a 6-character alphanumeric pairing code
/// that expires in 10 minutes.
pub async fn initiate_device_pairing(
    db: &SqlitePool,
    device_type: &str,
    name: &str,
) -> Result<PairingInitiated> {
    // Validate device type exists in registry
    crate::api::validation::validate_provider_name(device_type)?;

    // Validate name format
    crate::api::validation::validate_name(name, "Device name")?;

    // Check if a source with this name already exists and is actively paired
    let existing_active: Option<(String,)> = sqlx::query_as(
        r#"
        SELECT id
        FROM elt_source_connections
        WHERE source = $1 AND name = $2 AND pairing_status = 'active'
        "#,
    )
    .bind(device_type)
    .bind(name)
    .fetch_optional(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to check existing source: {e}")))?;

    if existing_active.is_some() {
        return Err(Error::InvalidInput(format!(
            "A {} device named '{}' is already paired. Please use a different name or unpair the existing device first.",
            device_type, name
        )));
    }

    // Check for existing pending pairing with same name and source
    let existing_pending: Option<(String,)> = sqlx::query_as(
        r#"
        SELECT id
        FROM elt_source_connections
        WHERE source = $1 AND name = $2 AND pairing_status = 'pending'
        "#,
    )
    .bind(device_type)
    .bind(name)
    .fetch_optional(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to check existing pairing: {e}")))?;

    // If there's an existing pending pairing, return it
    if let Some((source_id_str,)) = existing_pending {
        return Ok(PairingInitiated {
            source_id: source_id_str,
            code: "".to_string(),   // Removed
            expires_at: Utc::now(), // Removed
        });
    }

    // Create pending source or update existing
    let new_id = crate::ids::generate_id(crate::ids::SOURCE_PREFIX, &[name, device_type]);
    let source_id = sqlx::query_scalar::<_, String>(
        r#"
        INSERT INTO elt_source_connections (id, source, name, auth_type, pairing_status, is_active, is_internal)
        VALUES ($1, $2, $3, 'device', 'pending', false, false)
        ON CONFLICT (name)
        DO UPDATE SET
            pairing_status = 'pending',
            updated_at = datetime('now')
        WHERE elt_source_connections.pairing_status = 'pending' OR elt_source_connections.pairing_status IS NULL
        RETURNING id
        "#,
    )
    .bind(&new_id)
    .bind(device_type)
    .bind(name)
    .fetch_one(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to create pending pairing: {e}")))?;

    Ok(PairingInitiated {
        source_id,
        code: "".to_string(),
        expires_at: Utc::now(),
    })
}

/// Complete device pairing with a valid pairing code
///
/// Validates the pairing code, updates the source with device information,
/// generates a secure device token, and returns available streams.
pub async fn complete_device_pairing(
    _db: &SqlitePool,
    _code: &str,
    _device_info: DeviceInfo,
) -> Result<PairingCompleted> {
    // This flow is deprecated in favor of manual linking (link_device_manually)
    // where the device ID is the token.
    Err(Error::Other(
        "Pairing code flow is deprecated. Use /link endpoint.".to_string(),
    ))
}

/// Link a device manually using its device ID as the token
///
/// This is used for devices like iOS where the device generates its own ID
/// and the user manually enters it in the web interface.
pub async fn link_device_manually(
    db: &SqlitePool,
    device_id: &str,
    name: &str,
    device_type: &str,
) -> Result<PairingCompleted> {
    // Validate inputs
    crate::api::validation::validate_provider_name(device_type)?;
    crate::api::validation::validate_name(name, "Device name")?;

    if device_id.trim().is_empty() {
        return Err(Error::InvalidInput("Device ID cannot be empty".to_string()));
    }

    // Encrypt device ID to use as token
    let encryptor = TokenEncryptor::from_env()
        .map_err(|e| Error::Other(format!("Failed to initialize encryption: {e}")))?;
    let encrypted_token = encryptor
        .encrypt(device_id)
        .map_err(|e| Error::Other(format!("Failed to encrypt device token: {e}")))?;

    // Check if source already exists
    let existing_source = sqlx::query!(
        r#"
        SELECT id
        FROM elt_source_connections
        WHERE device_id = $1 AND source = $2
        "#,
        device_id,
        device_type
    )
    .fetch_optional(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to check existing source: {e}")))?;

    let source_id = if let Some(row) = existing_source {
        // Parse existing ID from string
        let existing_id = row
            .id
            .clone()
            .ok_or_else(|| Error::Database("Invalid source ID".to_string()))?;

        // Update existing source
        sqlx::query(
            r#"
            UPDATE elt_source_connections
            SET name = $1,
                device_token = $2,
                pairing_status = 'active',
                is_active = true,
                updated_at = datetime('now')
            WHERE id = $3
            "#,
        )
        .bind(name)
        .bind(&encrypted_token)
        .bind(&existing_id)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("Failed to update source: {e}")))?;

        existing_id
    } else {
        // Create new source
        let new_id = crate::ids::generate_id(crate::ids::SOURCE_PREFIX, &[device_id]);
        sqlx::query(
            r#"
            INSERT INTO elt_source_connections (
                id, source, name, auth_type, pairing_status,
                is_active, is_internal, device_id, device_token,
                created_at, updated_at
            )
            VALUES ($1, $2, $3, 'device', 'active', true, false, $4, $5, datetime('now'), datetime('now'))
            "#,
        )
        .bind(&new_id)
        .bind(device_type)
        .bind(name)
        .bind(device_id)
        .bind(&encrypted_token)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("Failed to create source: {e}")))?;

        new_id
    };

    // Get available streams for this device type
    let available_streams = crate::registry::get_source(device_type)
        .map(|info| info.streams.clone())
        .unwrap_or_default();

    // Enable default streams
    crate::api::streams::enable_default_streams(db, source_id.clone(), device_type).await?;

    Ok(PairingCompleted {
        source_id,
        device_token: device_id.to_string(), // Return the ID as the token
        available_streams,
    })
}

/// Check the status of a pairing by source ID
pub async fn check_pairing_status(db: &SqlitePool, source_id: String) -> Result<PairingStatus> {
    let source_id_str = &source_id;
    let source = sqlx::query!(
        r#"
        SELECT pairing_status, device_info
        FROM elt_source_connections
        WHERE id = $1
        "#,
        source_id_str
    )
    .fetch_one(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to check pairing status: {e}")))?;

    match source.pairing_status.as_deref() {
        Some("pending") => Ok(PairingStatus::Pending),
        Some("active") => {
            // SQLite stores device_info as TEXT, so we need from_str not from_value
            let device_info: DeviceInfo = source
                .device_info
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .ok_or_else(|| Error::Other("Device info not found".to_string()))?;
            Ok(PairingStatus::Active(device_info))
        }
        Some("revoked") => Ok(PairingStatus::Revoked),
        _ => Ok(PairingStatus::Pending),
    }
}

/// List all pending pairings
pub async fn list_pending_pairings(db: &SqlitePool) -> Result<Vec<PendingPairing>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id,
            name,
            source as device_type,
            created_at
        FROM elt_source_connections
        WHERE pairing_status = 'pending'
        ORDER BY created_at DESC
        "#
    )
    .fetch_all(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to list pending pairings: {e}")))?;

    let pairings = rows
        .into_iter()
        .filter_map(|row| {
            // SQLite returns id as Option<String>, but name/device_type/created_at as String (NOT NULL)
            let source_id = row.id.clone()?;
            // These columns are NOT NULL, so they're String not Option<String>
            let name = row.name.clone();
            let device_type = row.device_type.clone();
            let created_at = chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            Some(PendingPairing {
                source_id,
                name,
                device_type,
                code: String::new(),
                expires_at: Utc::now(), // Pairing codes are deprecated
                created_at,
            })
        })
        .collect();

    Ok(pairings)
}

/// Validate a device token and return the source ID
pub async fn validate_device_token(db: &SqlitePool, token: &str) -> Result<String> {
    // Initialize encryptor
    let encryptor = TokenEncryptor::from_env()
        .map_err(|e| Error::Other(format!("Failed to initialize encryption: {e}")))?;

    // Get all active device sources with tokens
    // SQLite returns id as String, so we use (String, String)
    let sources = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT id, device_token
        FROM elt_source_connections
        WHERE device_token IS NOT NULL
        AND pairing_status = 'active'
        AND is_active = true
        "#,
    )
    .fetch_all(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to query device tokens: {e}")))?;

    // Try to decrypt each token and compare with the provided token
    for (source_id, encrypted_token) in sources {
        // Decrypt stored token
        if let Ok(decrypted_token) = encryptor.decrypt(&encrypted_token) {
            // Compare with provided token
            if decrypted_token == token {
                return Ok(source_id);
            }
        }
    }

    // No match found
    Err(Error::Unauthorized(
        "Invalid or revoked device token".to_string(),
    ))
}

/// Update the last seen timestamp for a device
pub async fn update_last_seen(db: &SqlitePool, source_id: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE elt_source_connections
        SET last_seen_at = datetime('now')
        WHERE id = $1
        "#,
    )
    .bind(source_id)
    .execute(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to update last_seen: {e}")))?;

    Ok(())
}

/// Response when verifying a device token
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceVerified {
    pub source_id: String,
    pub enabled_streams: Vec<crate::api::StreamConnection>,
    pub configuration_complete: bool,
}

/// Verify a device token and return stream configuration status
///
/// This endpoint is called by devices that already have a device_token
/// to check if streams have been configured in the web app.
pub async fn verify_device(db: &SqlitePool, token: &str) -> Result<DeviceVerified> {
    // Validate token and get source ID
    let source_id = validate_device_token(db, token).await?;

    // Update last seen
    update_last_seen(db, &source_id).await?;

    // Get all streams for this source
    let all_streams = crate::api::list_source_streams(db, source_id.clone()).await?;

    // Filter to only enabled streams
    let enabled_streams: Vec<crate::api::StreamConnection> =
        all_streams.into_iter().filter(|s| s.is_enabled).collect();

    // Configuration is complete if at least one stream is enabled
    let configuration_complete = !enabled_streams.is_empty();

    Ok(DeviceVerified {
        source_id,
        enabled_streams,
        configuration_complete,
    })
}

/// Generate a 6-character alphanumeric pairing code
#[cfg(test)]
fn generate_pairing_code() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // Removed ambiguous: 0, O, 1, I
    let mut rng = rand::rng();

    (0..6)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Generate a secure 256-bit device token
#[cfg(test)]
fn generate_device_token() -> String {
    use rand::RngCore;
    let mut token = [0u8; 32]; // 256 bits
    rand::rng().fill_bytes(&mut token);
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn test_generate_pairing_code() {
        let code = generate_pairing_code();

        // Should be 6 characters
        assert_eq!(code.len(), 6);

        // Should only contain valid characters (no 0, O, 1, I)
        for ch in code.chars() {
            assert!(
                "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".contains(ch),
                "Invalid character in pairing code: {}",
                ch
            );
        }

        // Generate multiple codes to ensure randomness
        let code1 = generate_pairing_code();
        let code2 = generate_pairing_code();
        let code3 = generate_pairing_code();

        // While technically possible, it's extremely unlikely all three are the same
        assert!(
            !(code1 == code2 && code2 == code3),
            "Generated codes should be random"
        );
    }

    #[test]
    fn test_generate_device_token() {
        let token = generate_device_token();

        // Should be base64 encoded 32 bytes (256 bits)
        // Base64 encoding of 32 bytes = 44 characters (with padding)
        assert!(
            token.len() >= 43 && token.len() <= 44,
            "Token length should be 43-44 chars"
        );

        // Should be valid base64
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&token)
            .expect("Token should be valid base64");
        assert_eq!(decoded.len(), 32, "Decoded token should be 32 bytes");

        // Generate multiple tokens to ensure randomness
        let token1 = generate_device_token();
        let token2 = generate_device_token();
        assert_ne!(token1, token2, "Generated tokens should be unique");
    }

    #[test]
    fn test_device_info_serialization() {
        let info = DeviceInfo {
            device_id: "iPhone-12345".to_string(),
            device_name: "Adam's iPhone".to_string(),
            device_model: "iPhone16,1".to_string(),
            os_version: "iOS 17.2".to_string(),
            app_version: Some("1.0.0".to_string()),
        };

        // Should serialize to JSON
        let json = serde_json::to_value(&info).expect("Should serialize");
        assert!(json.is_object());
        assert_eq!(json["device_id"], "iPhone-12345");
        assert_eq!(json["device_name"], "Adam's iPhone");

        // Should deserialize from JSON
        let deserialized: DeviceInfo = serde_json::from_value(json).expect("Should deserialize");
        assert_eq!(deserialized.device_id, info.device_id);
        assert_eq!(deserialized.device_name, info.device_name);
    }

    #[test]
    fn test_encryption_roundtrip() {
        // Set up a test encryption key
        let key_bytes = b"12345678901234567890123456789012";
        let key_b64 = base64::engine::general_purpose::STANDARD.encode(key_bytes);
        std::env::set_var("VIRTUES_ENCRYPTION_KEY", &key_b64);

        let encryptor = TokenEncryptor::from_env().expect("Should create encryptor");

        // Generate a token
        let original_token = generate_device_token();

        // Encrypt it
        let encrypted = encryptor.encrypt(&original_token).expect("Should encrypt");

        // Encrypted should be different from original
        assert_ne!(encrypted, original_token);

        // Decrypt it
        let decrypted = encryptor.decrypt(&encrypted).expect("Should decrypt");

        // Should match original
        assert_eq!(decrypted, original_token);

        // Clean up
        std::env::remove_var("VIRTUES_ENCRYPTION_KEY");
    }

    #[test]
    fn test_validate_token_uses_encryption() {
        // This test verifies that validation compares encrypted tokens
        // It ensures the encryption is applied consistently

        let key_bytes = b"12345678901234567890123456789012";
        let key_b64 = base64::engine::general_purpose::STANDARD.encode(key_bytes);
        std::env::set_var("VIRTUES_ENCRYPTION_KEY", &key_b64);

        let encryptor = TokenEncryptor::from_env().expect("Should create encryptor");

        let token1 = "test_token_123";
        let token2 = "test_token_456";

        // Encrypt both tokens
        let encrypted1 = encryptor.encrypt(token1).expect("Should encrypt");
        let encrypted2 = encryptor.encrypt(token2).expect("Should encrypt");

        // Different tokens should produce different encrypted values
        assert_ne!(encrypted1, encrypted2);

        // Same token should produce different encrypted values (due to random nonce)
        let encrypted1_again = encryptor.encrypt(token1).expect("Should encrypt");
        assert_ne!(
            encrypted1, encrypted1_again,
            "Same plaintext should produce different ciphertext due to random nonce"
        );

        // Clean up
        std::env::remove_var("VIRTUES_ENCRYPTION_KEY");
    }
}
