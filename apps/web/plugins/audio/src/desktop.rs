use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
  app: &AppHandle<R>,
  _api: PluginApi<R, C>,
) -> crate::Result<Audio<R>> {
  Ok(Audio(app.clone()))
}

/// Desktop no-op stub — the microphone collector only exists on iOS.
pub struct Audio<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Audio<R> {
  pub fn enable(&self) -> crate::Result<AudioStatus> {
    Ok(AudioStatus::default())
  }
  pub fn disable(&self) -> crate::Result<AudioStatus> {
    Ok(AudioStatus::default())
  }
  pub fn resume(&self) -> crate::Result<AudioStatus> {
    Ok(AudioStatus::default())
  }
  pub fn status(&self) -> crate::Result<AudioStatus> {
    Ok(AudioStatus::default())
  }
  pub fn set_notify(&self, _enabled: bool) -> crate::Result<AudioStatus> {
    Ok(AudioStatus::default())
  }
}
