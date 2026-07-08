use tauri::{command, AppHandle, Runtime};

use crate::models::*;
use crate::LocationProbeExt;
use crate::Result;

#[command]
pub(crate) async fn start_probe<R: Runtime>(app: AppHandle<R>) -> Result<StartResponse> {
  app.location_probe().start_probe()
}

#[command]
pub(crate) async fn read_rows<R: Runtime>(
  app: AppHandle<R>,
  payload: RowsRequest,
) -> Result<RowsResponse> {
  app.location_probe().read_rows(payload)
}
