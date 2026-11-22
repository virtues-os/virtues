use tauri::{command, AppHandle, Runtime};

use crate::models::*;
use crate::PhotosExt;
use crate::Result;

#[command]
pub(crate) async fn enable<R: Runtime>(app: AppHandle<R>) -> Result<PhotosStatus> {
  app.photos().enable()
}

#[command]
pub(crate) async fn resume<R: Runtime>(app: AppHandle<R>) -> Result<PhotosStatus> {
  app.photos().resume()
}

#[command]
pub(crate) async fn status<R: Runtime>(app: AppHandle<R>) -> Result<PhotosStatus> {
  app.photos().status()
}

#[command]
pub(crate) async fn collect<R: Runtime>(app: AppHandle<R>) -> Result<PhotosStatus> {
  app.photos().collect()
}
