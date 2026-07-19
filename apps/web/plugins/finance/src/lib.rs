//! Native Finance collector plugin.
//!
//! Reads financial accounts and transactions on iOS and enqueues box-shaped records into the
//! shared outbox via `virtues_enqueue` (id = the contact's stable identifier, so
//! re-scans dedup). Finance have no time dimension — it's a full snapshot on
//! enable/launch/"Sync now". The Rust side is thin; collection is Swift.

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
use desktop::Finance;
#[cfg(mobile)]
use mobile::Finance;

pub trait FinanceExt<R: Runtime> {
  fn finance(&self) -> &Finance<R>;
}

impl<R: Runtime, T: Manager<R>> crate::FinanceExt<R> for T {
  fn finance(&self) -> &Finance<R> {
    self.state::<Finance<R>>().inner()
  }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("finance")
    .invoke_handler(tauri::generate_handler![
      commands::enable,
      commands::resume,
      commands::status,
      commands::collect
    ])
    .setup(|app, api| {
      #[cfg(mobile)]
      let finance = mobile::init(app, api)?;
      #[cfg(desktop)]
      let finance = desktop::init(app, api)?;
      app.manage(finance);
      Ok(())
    })
    .build()
}
