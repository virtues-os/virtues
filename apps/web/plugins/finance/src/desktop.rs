use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
  app: &AppHandle<R>,
  _api: PluginApi<R, C>,
) -> crate::Result<Finance<R>> {
  Ok(Finance(app.clone()))
}

pub struct Finance<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Finance<R> {
  pub fn enable(&self) -> crate::Result<FinanceStatus> {
    Ok(FinanceStatus::default())
  }
  pub fn resume(&self) -> crate::Result<FinanceStatus> {
    Ok(FinanceStatus::default())
  }
  pub fn status(&self) -> crate::Result<FinanceStatus> {
    Ok(FinanceStatus::default())
  }
  pub fn collect(&self) -> crate::Result<FinanceStatus> {
    Ok(FinanceStatus::default())
  }
}
