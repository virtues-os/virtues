use serde::de::DeserializeOwned;
use tauri::{
  plugin::{PluginApi, PluginHandle},
  AppHandle, Runtime,
};

use crate::models::*;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_audio);

pub fn init<R: Runtime, C: DeserializeOwned>(
  _app: &AppHandle<R>,
  api: PluginApi<R, C>,
) -> crate::Result<Audio<R>> {
  #[cfg(target_os = "android")]
  let handle = api.register_android_plugin("com.virtues.audio", "AudioPlugin")?;
  #[cfg(target_os = "ios")]
  let handle = api.register_ios_plugin(init_plugin_audio)?;
  Ok(Audio(handle))
}

/// Access to the microphone collector.
pub struct Audio<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Audio<R> {
  /// Explicit opt-in: prompt for mic access, then start recording.
  pub fn enable(&self) -> crate::Result<AudioStatus> {
    self
      .0
      .run_mobile_plugin("enable", EmptyRequest {})
      .map_err(Into::into)
  }

  /// Toggle off: finalize the current chunk + stop; persisted.
  pub fn disable(&self) -> crate::Result<AudioStatus> {
    self
      .0
      .run_mobile_plugin("disable", EmptyRequest {})
      .map_err(Into::into)
  }

  /// Launch auto-resume: record only if already authorized + left enabled.
  pub fn resume(&self) -> crate::Result<AudioStatus> {
    self
      .0
      .run_mobile_plugin("resume", EmptyRequest {})
      .map_err(Into::into)
  }

  pub fn status(&self) -> crate::Result<AudioStatus> {
    self
      .0
      .run_mobile_plugin("status", EmptyRequest {})
      .map_err(Into::into)
  }
}
