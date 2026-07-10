use serde::{Deserialize, Serialize};

/// Empty payload for the mobile-plugin calls.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EmptyRequest {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStatus {
  /// Microphone permission has been granted.
  pub authorized: bool,
  /// The recorder is running (a chunk is actively being captured).
  pub recording: bool,
}
