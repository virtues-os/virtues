use tauri::{command, AppHandle, Runtime};

use crate::models::*;
use crate::EventKitExt;
use crate::Result;

#[command]
pub(crate) async fn enable<R: Runtime>(app: AppHandle<R>) -> Result<EventKitStatus> {
  app.eventkit().enable()
}

#[command]
pub(crate) async fn resume<R: Runtime>(app: AppHandle<R>) -> Result<EventKitStatus> {
  app.eventkit().resume()
}

#[command]
pub(crate) async fn status<R: Runtime>(app: AppHandle<R>) -> Result<EventKitStatus> {
  app.eventkit().status()
}

#[command]
pub(crate) async fn collect<R: Runtime>(app: AppHandle<R>) -> Result<EventKitStatus> {
  app.eventkit().collect()
}
