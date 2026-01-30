//! Semantic ID generation utilities
//!
//! This module provides collision-resistant ID generation using a universal
//! `prefix_hash` paradigm. All IDs follow the pattern: `{prefix}_{hash16}`
//!
//! The hash is derived from "uniqueness components" - a set of values that together
//! define what makes this entity unique.
//!
//! # Example
//! ```ignore
//! // All IDs use the same function
//! let session_id = generate_id("session", &["My Chat", "2024-01-15T10:30:00Z"]);
//! let archive_id = generate_id("archive", &[&storage_key]);
//! let msg_id = generate_id("msg", &[&session_id, &uuid::Uuid::new_v4().to_string()]);
//! ```

use sha2::{Digest, Sha256};

// ============================================================================
// Prefix Constants (organized by layer)
// ============================================================================

// Wiki Layer (Personal Encyclopedia - WHO/WHAT/WHERE/WHEN)
pub const WIKI_PERSON_PREFIX: &str = "person";
pub const WIKI_PLACE_PREFIX: &str = "place";
pub const WIKI_ORG_PREFIX: &str = "org";
pub const WIKI_CONNECTION_PREFIX: &str = "conn";
pub const WIKI_CITATION_PREFIX: &str = "cite";
pub const WIKI_DAY_PREFIX: &str = "day";
pub const WIKI_EVENT_PREFIX: &str = "event";

// Narrative Layer (Life Meaning - WHY)
pub const NARRATIVE_VISION_PREFIX: &str = "vision";
pub const NARRATIVE_ERA_PREFIX: &str = "era";
pub const NARRATIVE_PHASE_PREFIX: &str = "phase";
pub const NARRATIVE_VALUE_PREFIX: &str = "value";

// Data Layer (Machine-Generated Records)
pub const HEALTH_HEART_RATE_PREFIX: &str = "hr";
pub const HEALTH_HRV_PREFIX: &str = "hrv";
pub const HEALTH_STEPS_PREFIX: &str = "steps";
pub const HEALTH_SLEEP_PREFIX: &str = "sleep";
pub const HEALTH_WORKOUT_PREFIX: &str = "workout";
pub const LOCATION_POINT_PREFIX: &str = "loc";
pub const LOCATION_VISIT_PREFIX: &str = "visit";
pub const MESSAGES_EMAIL_PREFIX: &str = "email";
pub const MESSAGES_TEXT_PREFIX: &str = "text";
pub const ACTIVITY_APP_PREFIX: &str = "app";
pub const ACTIVITY_BROWSING_PREFIX: &str = "browse";
pub const AUDIO_TRANSCRIPTION_PREFIX: &str = "transcript";
pub const MONEY_ACCOUNT_PREFIX: &str = "account";
pub const MONEY_TRANSACTION_PREFIX: &str = "txn";
pub const MONEY_ASSET_PREFIX: &str = "asset";
pub const MONEY_LIABILITY_PREFIX: &str = "liability";
pub const KNOWLEDGE_DOCUMENT_PREFIX: &str = "doc";
pub const KNOWLEDGE_AI_CHAT_PREFIX: &str = "aichat";

// System Layer
pub const SOURCE_PREFIX: &str = "source";
pub const STREAM_PREFIX: &str = "stream";
pub const STREAM_OBJECT_PREFIX: &str = "streamobj";
pub const JOB_PREFIX: &str = "job";
pub const ARCHIVE_JOB_PREFIX: &str = "archive";
pub const CHECKPOINT_PREFIX: &str = "checkpoint";
pub const PROFILE_PREFIX: &str = "profile";
pub const USER_PREFIX: &str = "user";
pub const SESSION_PREFIX: &str = "session";
pub const MESSAGE_PREFIX: &str = "msg";
pub const AGENT_PREFIX: &str = "agent";
pub const MODEL_PREFIX: &str = "model";
pub const TOOL_PREFIX: &str = "tool";
pub const AUTH_SESSION_PREFIX: &str = "authsession";
pub const AUTH_TOKEN_PREFIX: &str = "authtoken";

// Drive Layer
pub const DRIVE_FILE_PREFIX: &str = "file";

// Pages Layer (User-authored knowledge documents)
pub const PAGE_PREFIX: &str = "page";
pub const PAGE_VERSION_PREFIX: &str = "ver";

// Space Layer (Organization system)
pub const SPACE_PREFIX: &str = "space";
pub const WORKSPACE_PREFIX: &str = "ws";  // Deprecated: use SPACE_PREFIX

// Chat Layer (Conversations)
pub const CHAT_PREFIX: &str = "chat";
pub const SESSION_PREFIX_DEPRECATED: &str = "session";  // Kept for backwards compat

// ============================================================================
// Universal ID Generation - ONE FUNCTION FOR EVERYTHING
// ============================================================================

/// Generate a collision-resistant ID from components.
/// Format: `{prefix}_{hash16}`
///
/// This is the ONLY ID generation function. Use it for everything.
/// The hash is deterministic - same components always produce the same ID.
///
/// # Arguments
/// * `prefix` - The entity type prefix (e.g., "session", "archive", "msg")
/// * `components` - Slice of strings that together define uniqueness
///
/// # Examples
/// ```ignore
/// // Session: unique by title + creation time
/// let id = generate_id("session", &[title, &timestamp]);
///
/// // Archive job: unique by storage key
/// let id = generate_id("archive", &[&storage_key]);
///
/// // Message: unique by session + random UUID
/// let id = generate_id("msg", &[&session_id, &uuid::Uuid::new_v4().to_string()]);
///
/// // Checkpoint: unique by source + stream + key
/// let id = generate_id("checkpoint", &[source_id, stream_name, checkpoint_key]);
///
/// // Day: unique by date
/// let id = generate_id("day", &[date]);
/// ```
pub fn generate_id(prefix: &str, components: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for component in components {
        hasher.update(component.as_bytes());
        hasher.update(b"|"); // Separator to avoid collisions like ["ab", "c"] vs ["a", "bc"]
    }
    let hash = hasher.finalize();
    let hash_str = hex::encode(&hash[..8]); // 16 hex chars from 8 bytes
    format!("{}_{}", prefix, hash_str)
}

// ============================================================================
// ID Parsing and Validation
// ============================================================================

/// Extract prefix from a semantic ID
/// Example: `session_a1b2c3d4e5f6g7h8` → `session`
pub fn extract_prefix(id: &str) -> Option<&str> {
    id.split('_').next()
}

/// Extract hash from a semantic ID
/// Example: `session_a1b2c3d4e5f6g7h8` → `a1b2c3d4e5f6g7h8`
pub fn extract_hash(id: &str) -> Option<&str> {
    id.splitn(2, '_').nth(1)
}

/// Validate if an ID matches a specific prefix
pub fn validate_prefix(id: &str, expected_prefix: &str) -> bool {
    extract_prefix(id) == Some(expected_prefix)
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_id_deterministic() {
        // Same components should always produce the same ID
        let id1 = generate_id("archive", &["calendar", "job_123"]);
        let id2 = generate_id("archive", &["calendar", "job_123"]);
        assert_eq!(id1, id2);

        // Different components should produce different IDs
        let id3 = generate_id("archive", &["calendar", "job_456"]);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_generate_id_format() {
        let id = generate_id("checkpoint", &["source_1", "stream_x"]);
        assert!(id.starts_with("checkpoint_"));
        // prefix + underscore + 16 hex chars
        assert_eq!(id.len(), "checkpoint_".len() + 16);
    }

    #[test]
    fn test_generate_id_separator_prevents_collisions() {
        // ["ab", "c"] should differ from ["a", "bc"]
        let id1 = generate_id("test", &["ab", "c"]);
        let id2 = generate_id("test", &["a", "bc"]);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_generate_id_empty_components() {
        // Even with empty components, should produce valid ID
        let id = generate_id("test", &[]);
        assert!(id.starts_with("test_"));
        assert_eq!(id.len(), "test_".len() + 16);
    }

    #[test]
    fn test_extract_prefix() {
        assert_eq!(extract_prefix("session_a1b2c3d4e5f6g7h8"), Some("session"));
        assert_eq!(extract_prefix("day_f9e8d7c6b5a43210"), Some("day"));
        assert_eq!(extract_prefix("nounderscore"), Some("nounderscore"));
    }

    #[test]
    fn test_extract_hash() {
        assert_eq!(extract_hash("session_a1b2c3d4e5f6g7h8"), Some("a1b2c3d4e5f6g7h8"));
        assert_eq!(extract_hash("nounderscore"), None);
    }

    #[test]
    fn test_validate_prefix() {
        assert!(validate_prefix("session_a1b2c3d4e5f6g7h8", "session"));
        assert!(!validate_prefix("session_a1b2c3d4e5f6g7h8", "archive"));
        assert!(validate_prefix("day_f9e8d7c6b5a43210", "day"));
    }
}
