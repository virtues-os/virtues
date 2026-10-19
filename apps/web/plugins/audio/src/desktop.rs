use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
  app: &AppHandle<R>,
  _api: PluginApi<R, C>,
) -> crate::Result<Audio<R>> {
  Ok(Audio(app.clone()))
}

/// What every desktop call resolves: nothing recording, and a microphone
/// permission that does not exist here rather than one that was refused.
fn unavailable() -> AudioStatus {
  AudioStatus { mic: Some("unavailable".into()), ..AudioStatus::default() }
}

/// Desktop no-op stub — the microphone collector only exists on iOS.
pub struct Audio<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Audio<R> {
  pub fn enable(&self) -> crate::Result<AudioStatus> {
    Ok(unavailable())
  }
  pub fn disable(&self) -> crate::Result<AudioStatus> {
    Ok(unavailable())
  }
  pub fn resume(&self) -> crate::Result<AudioStatus> {
    Ok(unavailable())
  }
  pub fn status(&self) -> crate::Result<AudioStatus> {
    Ok(unavailable())
  }
  pub fn set_notify(&self, _enabled: bool) -> crate::Result<AudioStatus> {
    Ok(unavailable())
  }
  pub fn set_quiet_hours(&self, _start: i32, _end: i32) -> crate::Result<AudioStatus> {
    Ok(unavailable())
  }
  pub fn set_schedule(&self, _schedule: serde_json::Value) -> crate::Result<AudioStatus> {
    Ok(unavailable())
  }
  pub fn set_places(&self, _places: Vec<MutedPlace>) -> crate::Result<AudioStatus> {
    Ok(unavailable())
  }
  pub fn set_override(&self, _mode: Option<String>, _minutes: Option<u32>) -> crate::Result<AudioStatus> {
    Ok(unavailable())
  }
}
