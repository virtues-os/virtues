use tauri::{command, AppHandle, Runtime};

use crate::models::*;
use crate::ContactsExt;
use crate::Result;

#[command]
pub(crate) async fn enable<R: Runtime>(app: AppHandle<R>) -> Result<ContactsStatus> {
  app.contacts().enable()
}

#[command]
pub(crate) async fn resume<R: Runtime>(app: AppHandle<R>) -> Result<ContactsStatus> {
  app.contacts().resume()
}

#[command]
pub(crate) async fn status<R: Runtime>(app: AppHandle<R>) -> Result<ContactsStatus> {
  app.contacts().status()
}

#[command]
pub(crate) async fn collect<R: Runtime>(app: AppHandle<R>) -> Result<ContactsStatus> {
  app.contacts().collect()
}
