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

/// The weekly mute schedule. `default_muted` is what happens outside every
/// window; a window inverts it. `false` + 22:00→07:00 is quiet hours; `true` +
/// 09:00→17:00 on weekdays is record-at-work-only. Windows are `[start, end]`
/// in minutes since local midnight, `start > end` wraps midnight. Days are
/// `mon`..`sun`. Carried as an opaque JSON document: the phone owns the shape
/// and evaluates it; Rust only relays it.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SetScheduleRequest {
  pub schedule: serde_json::Value,
}

/// The muted places, copied from the box's `wiki_places` rows with
/// `is_audio_muted`. The phone caches them so the gate runs offline; this is
/// a copy, never the authority. Each entry: `{id, name, lat, lon, radius_m}`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SetPlacesRequest {
  pub places: Vec<MutedPlace>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MutedPlace {
  pub id: String,
  pub name: String,
  pub lat: f64,
  pub lon: f64,
  pub radius_m: f64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStatus {
  /// Microphone permission has been granted. Deliberately required — no serde
  /// default: a Swift resolve missing it must fail loudly, not read back as a
  /// de-authorized mic. Every command resolves the full shape.
  pub authorized: bool,
  /// The recorder is running (a chunk is actively being captured). Required
  /// for the same reason as `authorized`.
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
  /// Capture is paused for a reason the user did not choose — "carplay" while
  /// a car audio route is present (the session is released so the car keeps
  /// its audio). Absent when recording, off, or paused by the user.
  #[serde(default)]
  pub paused_reason: Option<String>,
}
