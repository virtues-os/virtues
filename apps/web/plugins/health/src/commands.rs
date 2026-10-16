use tauri::{command, AppHandle, Runtime};

use crate::models::*;
use crate::HealthExt;
use crate::Result;

/// Explicit "Enable" opt-in: prompts for HealthKit access, then backfills +
/// starts collecting.
#[command]
pub(crate) async fn enable<R: Runtime>(app: AppHandle<R>) -> Result<HealthStatus> {
  app.health().enable()
}

/// Cycle tracking's own opt-in: its own Health sheet, then a full-history
/// backfill of the cycle, symptom and wrist-temperature types.
#[command]
pub(crate) async fn enable_cycle<R: Runtime>(app: AppHandle<R>) -> Result<HealthStatus> {
  app.health().enable_cycle()
}

/// Launch auto-resume: collect only if already authorized; never prompts.
#[command]
pub(crate) async fn resume<R: Runtime>(app: AppHandle<R>) -> Result<HealthStatus> {
  app.health().resume()
}

#[command]
pub(crate) async fn status<R: Runtime>(app: AppHandle<R>) -> Result<HealthStatus> {
  app.health().status()
}

/// Fetch new samples now (the "Sync now" button pairs this with a drain).
#[command]
pub(crate) async fn collect<R: Runtime>(app: AppHandle<R>) -> Result<HealthStatus> {
  app.health().collect()
}
