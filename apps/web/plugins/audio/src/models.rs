use serde::{Deserialize, Serialize};

/// Empty payload for the mobile-plugin calls.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EmptyRequest {}

/// Toggle the "notify me if recording stops" gap-nudge.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SetNotifyRequest {
  pub enabled: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStatus {
  /// Microphone permission has been granted.
  pub authorized: bool,
  /// The recorder is running (a chunk is actively being captured).
  pub recording: bool,
  /// The gap-nudge notification is enabled (default true).
  #[serde(default)]
  pub notify: bool,
}
