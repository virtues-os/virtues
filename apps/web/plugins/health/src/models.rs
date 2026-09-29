use serde::{Deserialize, Serialize};

/// Empty payload for the mobile-plugin calls.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EmptyRequest {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
  /// The person opted in through `enable`. Not a grant — HealthKit never
  /// reports whether reads were allowed; see `permission`.
  pub authorized: bool,
  /// The collector is running (observers/timer active).
  pub collecting: bool,
  /// Whether the Health sheet has been shown: `requested`, `not_requested`,
  /// `unknown`, or `unavailable` (no HealthKit, or desktop). `requested` says
  /// nothing about what the person allowed. Absent on a native build that
  /// predates it.
  #[serde(default)]
  pub permission: Option<String>,
}
