use tauri::{command, AppHandle, Runtime};

use crate::models::*;
use crate::FinanceExt;
use crate::Result;

#[command]
pub(crate) async fn enable<R: Runtime>(app: AppHandle<R>) -> Result<FinanceStatus> {
  app.finance().enable()
}

#[command]
pub(crate) async fn resume<R: Runtime>(app: AppHandle<R>) -> Result<FinanceStatus> {
  app.finance().resume()
}

#[command]
pub(crate) async fn status<R: Runtime>(app: AppHandle<R>) -> Result<FinanceStatus> {
  app.finance().status()
}

#[command]
pub(crate) async fn collect<R: Runtime>(app: AppHandle<R>) -> Result<FinanceStatus> {
  app.finance().collect()
}
