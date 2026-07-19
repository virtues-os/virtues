use serde::de::DeserializeOwned;
use tauri::{
  plugin::{PluginApi, PluginHandle},
  AppHandle, Runtime,
};

use crate::models::*;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_eventkit);

pub fn init<R: Runtime, C: DeserializeOwned>(
  _app: &AppHandle<R>,
  api: PluginApi<R, C>,
) -> crate::Result<EventKit<R>> {
  #[cfg(target_os = "android")]
  let handle = api.register_android_plugin("com.virtues.eventkit", "EventKitPlugin")?;
  #[cfg(target_os = "ios")]
  let handle = api.register_ios_plugin(init_plugin_eventkit)?;
  Ok(EventKit(handle))
}

pub struct EventKit<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> EventKit<R> {
  pub fn enable(&self) -> crate::Result<EventKitStatus> {
    self.0.run_mobile_plugin("enable", EmptyRequest {}).map_err(Into::into)
  }
  pub fn resume(&self) -> crate::Result<EventKitStatus> {
    self.0.run_mobile_plugin("resume", EmptyRequest {}).map_err(Into::into)
  }
  pub fn status(&self) -> crate::Result<EventKitStatus> {
    self.0.run_mobile_plugin("status", EmptyRequest {}).map_err(Into::into)
  }
  pub fn collect(&self) -> crate::Result<EventKitStatus> {
    self.0.run_mobile_plugin("collect", EmptyRequest {}).map_err(Into::into)
  }
}
