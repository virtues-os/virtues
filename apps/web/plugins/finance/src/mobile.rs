use serde::de::DeserializeOwned;
use tauri::{
  plugin::{PluginApi, PluginHandle},
  AppHandle, Runtime,
};

use crate::models::*;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_finance);

pub fn init<R: Runtime, C: DeserializeOwned>(
  _app: &AppHandle<R>,
  api: PluginApi<R, C>,
) -> crate::Result<Finance<R>> {
  #[cfg(target_os = "android")]
  let handle = api.register_android_plugin("com.virtues.finance", "FinancePlugin")?;
  #[cfg(target_os = "ios")]
  let handle = api.register_ios_plugin(init_plugin_finance)?;
  Ok(Finance(handle))
}

pub struct Finance<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Finance<R> {
  pub fn enable(&self) -> crate::Result<FinanceStatus> {
    self.0.run_mobile_plugin("enable", EmptyRequest {}).map_err(Into::into)
  }
  pub fn resume(&self) -> crate::Result<FinanceStatus> {
    self.0.run_mobile_plugin("resume", EmptyRequest {}).map_err(Into::into)
  }
  pub fn status(&self) -> crate::Result<FinanceStatus> {
    self.0.run_mobile_plugin("status", EmptyRequest {}).map_err(Into::into)
  }
  pub fn collect(&self) -> crate::Result<FinanceStatus> {
    self.0.run_mobile_plugin("collect", EmptyRequest {}).map_err(Into::into)
  }
}
