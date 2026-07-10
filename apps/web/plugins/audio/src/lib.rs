//! Native microphone collector plugin.
//!
//! Records ambient audio on iOS in fixed-length `.m4a` chunks (16 kHz mono AAC)
//! and enqueues box-shaped `microphone` records into the shared outbox
//! (`virtues_enqueue`, the same FFI the location/health collectors use). The box
//! already ingests these into `data_audio_recording` and transcribes them via
//! Gemini (`transcription_resolution` action) — this plugin is the device half.
//!
//! The Rust side is thin: it relays enable/disable/resume/status to the Swift
//! `AudioRecorder`. All the audio-session finicky-ness (mix strategy, AirPods
//! A2DP routing, interruption/route-change handling) lives in `Audio.swift`.

use tauri::{
  plugin::{Builder, TauriPlugin},
  Manager, Runtime,
};

pub use models::*;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod commands;
mod error;
mod models;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::Audio;
#[cfg(mobile)]
use mobile::Audio;

/// Access the microphone collector from `App`/`AppHandle`/`Window`.
pub trait AudioExt<R: Runtime> {
  fn audio(&self) -> &Audio<R>;
}

impl<R: Runtime, T: Manager<R>> crate::AudioExt<R> for T {
  fn audio(&self) -> &Audio<R> {
    self.state::<Audio<R>>().inner()
  }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("audio")
    .invoke_handler(tauri::generate_handler![
      commands::enable,
      commands::disable,
      commands::resume,
      commands::status
    ])
    .setup(|app, api| {
      #[cfg(mobile)]
      let audio = mobile::init(app, api)?;
      #[cfg(desktop)]
      let audio = desktop::init(app, api)?;
      app.manage(audio);
      Ok(())
    })
    .build()
}
