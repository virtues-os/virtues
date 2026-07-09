use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
  app: &AppHandle<R>,
  _api: PluginApi<R, C>,
) -> crate::Result<Health<R>> {
  Ok(Health(app.clone()))
}

/// Desktop no-op stub — HealthKit only exists on iOS.
pub struct Health<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Health<R> {
  pub fn enable(&self) -> crate::Result<HealthStatus> {
    Ok(HealthStatus::default())
  }
  pub fn resume(&self) -> crate::Result<HealthStatus> {
    Ok(HealthStatus::default())
  }
  pub fn status(&self) -> crate::Result<HealthStatus> {
    Ok(HealthStatus::default())
  }
  pub fn collect(&self) -> crate::Result<HealthStatus> {
    Ok(HealthStatus::default())
  }
}
