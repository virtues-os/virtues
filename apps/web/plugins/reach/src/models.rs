use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairRequest {
  /// Box address as typed by the user: `10.0.0.5`, `10.0.0.5:8000`,
  /// `adam.local`, or a full `http://…:8000`. Normalized before use.
  pub server: String,
  /// The 6-digit pair code (spaces allowed).
  pub code: String,
}

/// A Virtues box found on the LAN via Bonjour.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundServer {
  pub name: String,
  pub origin: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverResponse {
  pub servers: Vec<FoundServer>,
  /// Diagnostic: what the scan saw (local IPs / subnets), shown small in the UI.
  #[serde(default)]
  pub debug: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReachStatus {
  pub paired: bool,
  /// "authed" | "rejected" | "unknown" | "unpaired" — mirrors the desktop
  /// connect-screen diagnosis.
  pub session: String,
  /// The loopback URL the webview should load once paired + reachable.
  pub loopback_url: String,
}
