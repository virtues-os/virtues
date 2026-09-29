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

/// Whether the server can reach this phone. Never prompts.
#[command]
pub(crate) async fn push_status<R: Runtime>(app: AppHandle<R>) -> Result<PushStatusResponse> {
  app.location_probe().push_status()
}

/// The owner asked to let the server reach this phone: show the OS prompt if it
/// has never been shown, then register and report. Only ever called from a
/// button the owner pressed — asking cold, at launch, is how an app earns a
/// permanent "Don't Allow".
#[command]
pub(crate) async fn request_push<R: Runtime>(app: AppHandle<R>) -> Result<PushStatusResponse> {
  app.location_probe().request_push()
}

/// The current location authorization (see `LocationStatusResponse`). Never
/// prompts.
#[command]
pub(crate) async fn status<R: Runtime>(app: AppHandle<R>) -> Result<LocationStatusResponse> {
  app.location_probe().status()
}

/// The opt-in, answered: prompts if undetermined, starts collecting on a grant,
/// and resolves with the person's answer. `start_probe` resolves before the
/// sheet is even answered; this is the one a screen that shows the outcome
/// calls.
#[command]
pub(crate) async fn request_location<R: Runtime>(
  app: AppHandle<R>,
) -> Result<LocationStatusResponse> {
  app.location_probe().request_location()
}

/// Open this app's page in the Settings app, the only way back from a denied
/// permission. Hosted here because this is the plugin that holds shell-level
/// code; it is not about location.
#[command]
pub(crate) async fn open_settings<R: Runtime>(app: AppHandle<R>) -> Result<OpenSettingsResponse> {
  app.location_probe().open_settings()
}

#[command]
pub(crate) async fn read_rows<R: Runtime>(
  app: AppHandle<R>,
  payload: RowsRequest,
) -> Result<RowsResponse> {
  app.location_probe().read_rows(payload)
}
