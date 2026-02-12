//! Assistant profile defaults
//!
//! Default values for user's assistant profile preferences.
//! These are used when creating a new user profile.

use serde::{Deserialize, Serialize};

use crate::tools::default_enabled_tools;

/// The canonical default theme for the application.
/// This is the single source of truth — frontend and spaces.rs reference this.
pub const DEFAULT_THEME: &str = "pemberley";

/// Assistant profile defaults
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssistantProfileDefaults {
    /// Optional custom name for the assistant
    pub assistant_name: Option<String>,
    /// Default agent ID
    pub default_agent_id: String,
    /// Default model ID
    pub default_model_id: String,
    /// Which tools are enabled by default (JSON object: tool_id -> bool)
    pub enabled_tools: serde_json::Value,
    /// UI preferences (JSON object)
    pub ui_preferences: serde_json::Value,
}

/// Get assistant profile defaults
pub fn assistant_profile_defaults() -> AssistantProfileDefaults {
    AssistantProfileDefaults {
        assistant_name: None,
        default_agent_id: "agent".to_string(),
        default_model_id: "google/gemini-3-flash".to_string(),
        enabled_tools: default_enabled_tools(),
        ui_preferences: serde_json::json!({
            "theme": DEFAULT_THEME,
            "contextIndicator": {
                "alwaysVisible": false,
                "showThreshold": 70
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assistant_profile_defaults() {
        let defaults = assistant_profile_defaults();

        // Verify required fields
        assert!(
            !defaults.default_agent_id.is_empty(),
            "Default agent ID should not be empty"
        );
        assert!(
            !defaults.default_model_id.is_empty(),
            "Default model ID should not be empty"
        );
        assert!(
            defaults.enabled_tools.is_object(),
            "Enabled tools should be an object"
        );
        assert!(
            defaults.ui_preferences.is_object(),
            "UI preferences should be an object"
        );

        // Verify UI preferences structure
        let ui_prefs = &defaults.ui_preferences;
        assert!(
            ui_prefs.get("contextIndicator").is_some(),
            "Should have contextIndicator preferences"
        );
    }
}
