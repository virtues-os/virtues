//! Native HealthKit collector plugin.
//!
//! Reads health samples on iOS (incremental anchored queries + a 3-year
//! backfill) and enqueues box-shaped records into the shared outbox
//! (`virtues_enqueue`, the same FFI the location collector uses). The Rust side
//! is thin: it just relays enable/resume/status to the Swift collector.

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
use desktop::Health;
#[cfg(mobile)]
use mobile::Health;

/// Access the HealthKit collector from `App`/`AppHandle`/`Window`.
pub trait HealthExt<R: Runtime> {
  fn health(&self) -> &Health<R>;
}

impl<R: Runtime, T: Manager<R>> crate::HealthExt<R> for T {
  fn health(&self) -> &Health<R> {
    self.state::<Health<R>>().inner()
  }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("health")
    .invoke_handler(tauri::generate_handler![
      commands::enable,
      commands::resume,
      commands::status,
      commands::collect
    ])
    .setup(|app, api| {
      #[cfg(mobile)]
      let health = mobile::init(app, api)?;
      #[cfg(desktop)]
      let health = desktop::init(app, api)?;
      app.manage(health);
      Ok(())
    })
    .build()
}
