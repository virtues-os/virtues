use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
  app: &AppHandle<R>,
  _api: PluginApi<R, C>,
) -> crate::Result<EventKit<R>> {
  Ok(EventKit(app.clone()))
}

pub struct EventKit<R: Runtime>(AppHandle<R>);

impl<R: Runtime> EventKit<R> {
  pub fn enable(&self) -> crate::Result<EventKitStatus> {
    Ok(EventKitStatus::default())
  }
  pub fn resume(&self) -> crate::Result<EventKitStatus> {
    Ok(EventKitStatus::default())
  }
  pub fn status(&self) -> crate::Result<EventKitStatus> {
    Ok(EventKitStatus::default())
  }
  pub fn collect(&self) -> crate::Result<EventKitStatus> {
    Ok(EventKitStatus::default())
  }
}
