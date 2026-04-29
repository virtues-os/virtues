//! Subprocess contract types — single source of truth for the JSON envelope
//! piped between the runner and action binaries.
//!
//! # Wire format
//!
//! Stdin (runner → subprocess):
//! ```json
//! {
//!   "config": { ... },
//!   "credentials": { ... } | null,
//!   "payload": { ... } | null
//! }
//! ```
//!
//! Stdout (subprocess → runner):
//! ```json
//! {
//!   "result": "summary string",
//!   "config": { ... }
//! }
//! ```
//!
//! # Forward compatibility
//!
//! Optional fields use `#[serde(default)]`. Adding new optional fields is
//! backward-compatible: an old binary's output omits the field, a new runner
//! deserializes it as `None`. **Don't add `#[serde(deny_unknown_fields)]`**
//! anywhere — it would break this guarantee.

use serde::{Deserialize, Serialize};

/// Input piped to the action subprocess via stdin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionInput {
    /// Settings + code-managed state from `app_actions.config`.
    pub config: serde_json::Value,

    /// Decrypted credentials from the `credentials` Vault, resolved by the
    /// runner before spawn. `None` if the action has no `credential_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials: Option<serde_json::Value>,

    /// Trigger payload: webhook body for `webhook`, UI args for `manual`,
    /// LLM tool args for `tool`, `None` for `cron`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

impl ActionInput {
    pub fn new(config: serde_json::Value) -> Self {
        Self {
            config,
            credentials: None,
            payload: None,
        }
    }

    pub fn with_credentials(mut self, credentials: serde_json::Value) -> Self {
        self.credentials = Some(credentials);
        self
    }

    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = Some(payload);
        self
    }
}

/// Output written to stdout as a single JSON object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionOutput {
    /// One-line summary of what the action did.
    pub result: String,
    /// Updated config to persist back into `app_actions.config`.
    pub config: serde_json::Value,
}

impl ActionOutput {
    pub fn new(result: impl Into<String>, config: serde_json::Value) -> Self {
        Self {
            result: result.into(),
            config,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn input_round_trip() {
        let input = ActionInput::new(json!({"sync_token": "abc"}))
            .with_credentials(json!({"secrets": {"token": "x"}}))
            .with_payload(json!({"records": [1, 2, 3]}));

        let serialized = serde_json::to_string(&input).unwrap();
        let parsed: ActionInput = serde_json::from_str(&serialized).unwrap();

        assert_eq!(parsed.config["sync_token"], "abc");
        assert!(parsed.credentials.is_some());
        assert!(parsed.payload.is_some());
    }

    #[test]
    fn input_skips_none_fields() {
        let input = ActionInput::new(json!({"a": 1}));
        let serialized = serde_json::to_string(&input).unwrap();
        assert!(!serialized.contains("credentials"));
        assert!(!serialized.contains("payload"));
    }

    #[test]
    fn input_accepts_missing_optional_fields() {
        let json = r#"{"config": {"x": 1}}"#;
        let parsed: ActionInput = serde_json::from_str(json).unwrap();
        assert!(parsed.credentials.is_none());
        assert!(parsed.payload.is_none());
    }

    #[test]
    fn output_round_trip() {
        let out = ActionOutput::new("synced 5 events", json!({"sync_token": "xyz"}));
        let serialized = serde_json::to_string(&out).unwrap();
        let parsed: ActionOutput = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed.result, "synced 5 events");
        assert_eq!(parsed.config["sync_token"], "xyz");
    }
}
