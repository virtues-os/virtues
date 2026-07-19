use serde::{Deserialize, Serialize};

/// Empty payload for the mobile-plugin calls.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EmptyRequest {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
  /// The user has granted (some) HealthKit read access.
  pub authorized: bool,
  /// The collector is running (observers/timer active).
  pub collecting: bool,
}
