use tauri::{command, AppHandle, Runtime};

use crate::models::*;
use crate::LocationProbeExt;
use crate::Result;

/// Explicit user opt-in (device-screen "Enable"): prompts for permission if
/// undetermined, then starts collecting.
#[command]
pub(crate) async fn start_probe<R: Runtime>(app: AppHandle<R>) -> Result<StartResponse> {
  app.location_probe().start_probe()
}

/// Launch-time auto-resume: start collecting **only if already authorized** —
/// never prompts. This is what the app calls on every launch (incl. cold
/// background relaunch), so a fresh/unauthorized install isn't cold-slapped
/// with a permission dialog before onboarding.
#[command]
pub(crate) async fn resume_probe<R: Runtime>(app: AppHandle<R>) -> Result<StartResponse> {
  app.location_probe().resume_probe()
}

#[command]
pub(crate) async fn read_rows<R: Runtime>(
  app: AppHandle<R>,
  payload: RowsRequest,
) -> Result<RowsResponse> {
  app.location_probe().read_rows(payload)
}
