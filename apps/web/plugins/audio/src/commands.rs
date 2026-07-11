use tauri::{command, AppHandle, Runtime};

use crate::models::*;
use crate::AudioExt;
use crate::Result;

/// Explicit "Enable" opt-in: prompts for microphone access, then starts
/// recording ambient audio in fixed-length chunks.
#[command]
pub(crate) async fn enable<R: Runtime>(app: AppHandle<R>) -> Result<AudioStatus> {
  app.audio().enable()
}

/// Turn the collector off (the This-device toggle / pause): finalize the current
/// chunk and stop recording. Persisted — a relaunch won't auto-resume.
#[command]
pub(crate) async fn disable<R: Runtime>(app: AppHandle<R>) -> Result<AudioStatus> {
  app.audio().disable()
}

/// Launch auto-resume: start recording only if already authorized AND left
/// enabled; never prompts.
#[command]
pub(crate) async fn resume<R: Runtime>(app: AppHandle<R>) -> Result<AudioStatus> {
  app.audio().resume()
}

#[command]
pub(crate) async fn status<R: Runtime>(app: AppHandle<R>) -> Result<AudioStatus> {
  app.audio().status()
}

/// Toggle the "notify me if recording stops" gap-nudge (default on).
#[command]
pub(crate) async fn set_notify<R: Runtime>(
  app: AppHandle<R>,
  enabled: bool,
) -> Result<AudioStatus> {
  app.audio().set_notify(enabled)
}
