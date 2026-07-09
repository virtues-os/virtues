use serde::de::DeserializeOwned;
use tauri::{
  plugin::{PluginApi, PluginHandle},
  AppHandle, Runtime,
};

use crate::models::*;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_health);

pub fn init<R: Runtime, C: DeserializeOwned>(
  _app: &AppHandle<R>,
  api: PluginApi<R, C>,
) -> crate::Result<Health<R>> {
  #[cfg(target_os = "android")]
  let handle = api.register_android_plugin("com.virtues.health", "HealthPlugin")?;
  #[cfg(target_os = "ios")]
  let handle = api.register_ios_plugin(init_plugin_health)?;
  Ok(Health(handle))
}

/// Access to the HealthKit collector.
pub struct Health<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Health<R> {
  /// Explicit opt-in: request authorization (prompts), then backfill + collect.
  pub fn enable(&self) -> crate::Result<HealthStatus> {
    self
      .0
      .run_mobile_plugin("enable", EmptyRequest {})
      .map_err(Into::into)
  }

  /// Launch auto-resume: start collecting only if already authorized (no prompt).
  pub fn resume(&self) -> crate::Result<HealthStatus> {
    self
      .0
      .run_mobile_plugin("resume", EmptyRequest {})
      .map_err(Into::into)
  }

  pub fn status(&self) -> crate::Result<HealthStatus> {
    self
      .0
      .run_mobile_plugin("status", EmptyRequest {})
      .map_err(Into::into)
  }
}
