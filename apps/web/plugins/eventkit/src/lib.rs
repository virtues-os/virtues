//! Native EventKit collector plugin.
//!
//! Reads calendar events on iOS (3 years back + 3 years forward, chunked by year
//! because `predicateForEvents` caps at a 4-year span) and enqueues box-shaped
//! records into the shared outbox via `virtues_enqueue`. The Rust side is thin;
//! collection is Swift.

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
use desktop::EventKit;
#[cfg(mobile)]
use mobile::EventKit;

pub trait EventKitExt<R: Runtime> {
  fn eventkit(&self) -> &EventKit<R>;
}

impl<R: Runtime, T: Manager<R>> crate::EventKitExt<R> for T {
  fn eventkit(&self) -> &EventKit<R> {
    self.state::<EventKit<R>>().inner()
  }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("eventkit")
    .invoke_handler(tauri::generate_handler![
      commands::enable,
      commands::resume,
      commands::status,
      commands::collect
    ])
    .setup(|app, api| {
      #[cfg(mobile)]
      let eventkit = mobile::init(app, api)?;
      #[cfg(desktop)]
      let eventkit = desktop::init(app, api)?;
      app.manage(eventkit);
      Ok(())
    })
    .build()
}
