use serde::{Deserialize, Serialize};

/// Empty payload for the mobile-plugin calls.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EmptyRequest {}

/// Toggle the "notify me if recording stops" gap-nudge.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SetNotifyRequest {
  pub enabled: bool,
}

/// Quiet-hours window, minutes since local midnight; -1/-1 = off. The window
/// mutes chunk writing while the capture graph stays armed (mute-don't-release).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SetQuietHoursRequest {
  pub start: i32,
  pub end: i32,
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
  /// Quiet-hours window in minutes since local midnight. Only the `status`
  /// command reports these; None elsewhere (and on desktop).
  #[serde(default)]
  pub quiet_start: Option<i32>,
  #[serde(default)]
  pub quiet_end: Option<i32>,
}
