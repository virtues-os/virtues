use serde::de::DeserializeOwned;
use tauri::{
  plugin::{PluginApi, PluginHandle},
  AppHandle, Runtime,
};

use crate::models::*;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_location_probe);

// initializes the Swift plugin class
pub fn init<R: Runtime, C: DeserializeOwned>(
  _app: &AppHandle<R>,
  api: PluginApi<R, C>,
) -> crate::Result<LocationProbe<R>> {
  #[cfg(target_os = "android")]
  let handle = api.register_android_plugin("com.virtues.locationprobe", "LocationProbePlugin")?;
  #[cfg(target_os = "ios")]
  let handle = api.register_ios_plugin(init_plugin_location_probe)?;
  Ok(LocationProbe(handle))
}

/// Access to the location-probe APIs.
pub struct LocationProbe<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> LocationProbe<R> {
  pub fn start_probe(&self) -> crate::Result<StartResponse> {
    self
      .0
      .run_mobile_plugin("startProbe", StartRequest {})
      .map_err(Into::into)
  }

  pub fn read_rows(&self, payload: RowsRequest) -> crate::Result<RowsResponse> {
    self
      .0
      .run_mobile_plugin("readRows", payload)
      .map_err(Into::into)
  }
}
