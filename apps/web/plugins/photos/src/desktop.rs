use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
  app: &AppHandle<R>,
  _api: PluginApi<R, C>,
) -> crate::Result<Photos<R>> {
  Ok(Photos(app.clone()))
}

pub struct Photos<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Photos<R> {
  pub fn enable(&self) -> crate::Result<PhotosStatus> {
    Ok(PhotosStatus::default())
  }
  pub fn resume(&self) -> crate::Result<PhotosStatus> {
    Ok(PhotosStatus::default())
  }
  pub fn status(&self) -> crate::Result<PhotosStatus> {
    Ok(PhotosStatus::default())
  }
  pub fn collect(&self) -> crate::Result<PhotosStatus> {
    Ok(PhotosStatus::default())
  }
}
