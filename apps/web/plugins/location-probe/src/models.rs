use serde::{Deserialize, Serialize};

/// Empty payload for `start_probe` (kept so run_mobile_plugin has something to
/// serialize).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct StartRequest {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartResponse {
  pub started: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowsRequest {
  pub limit: Option<u32>,
}

/// One recorded location callback. `source` distinguishes an ordinary
/// `didUpdateLocations` from a marker (auth change, start, error). `app_state`
/// and `launch_reason` are the whole point of the probe: they prove the row was
/// written while the app was in the background / cold-relaunched with no webview.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeRow {
  pub ts: String,
  pub lat: f64,
  pub lon: f64,
  pub source: String,
  pub app_state: String,
  pub launch_reason: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowsResponse {
  pub rows: Vec<ProbeRow>,
}

/// Whether this phone lets the server reach it, as the OS reports it.
///
/// `status` is one of `authorized`, `denied`, `not_determined`, or
/// `unavailable` (desktop, where there is no push). Lives on this plugin, not a
/// new one, because this is the always-on plugin that owns the app's launch and
/// foreground hooks — and a push token has to be re-registered on exactly
/// those. See `PushRegistrar.swift`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushStatusResponse {
  pub status: String,
}
