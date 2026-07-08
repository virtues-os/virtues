use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
  app: &AppHandle<R>,
  _api: PluginApi<R, C>,
) -> crate::Result<LocationProbe<R>> {
  Ok(LocationProbe(app.clone()))
}

/// Access to the location-probe APIs.
///
/// Desktop is a no-op stub: the probe only means something on iOS/Android where
/// the OS drives background location callbacks. These let the app compile and
/// run on the desktop target without cfg noise at the call sites.
pub struct LocationProbe<R: Runtime>(AppHandle<R>);

impl<R: Runtime> LocationProbe<R> {
  pub fn start_probe(&self) -> crate::Result<StartResponse> {
    Ok(StartResponse { started: false })
  }

  pub fn resume_probe(&self) -> crate::Result<StartResponse> {
    Ok(StartResponse { started: false })
  }

  pub fn read_rows(&self, _payload: RowsRequest) -> crate::Result<RowsResponse> {
    Ok(RowsResponse { rows: vec![] })
  }
}
