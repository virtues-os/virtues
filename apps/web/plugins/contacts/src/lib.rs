//! Native Contacts collector plugin.
//!
//! Snapshots the address book on iOS and enqueues box-shaped records into the
//! shared outbox via `virtues_enqueue` (id = the contact's stable identifier, so
//! re-scans dedup). Contacts have no time dimension — it's a full snapshot on
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
use desktop::Contacts;
#[cfg(mobile)]
use mobile::Contacts;

pub trait ContactsExt<R: Runtime> {
  fn contacts(&self) -> &Contacts<R>;
}

impl<R: Runtime, T: Manager<R>> crate::ContactsExt<R> for T {
  fn contacts(&self) -> &Contacts<R> {
    self.state::<Contacts<R>>().inner()
  }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("contacts")
    .invoke_handler(tauri::generate_handler![
      commands::enable,
      commands::resume,
      commands::status,
      commands::collect
    ])
    .setup(|app, api| {
      #[cfg(mobile)]
      let contacts = mobile::init(app, api)?;
      #[cfg(desktop)]
      let contacts = desktop::init(app, api)?;
      app.manage(contacts);
      Ok(())
    })
    .build()
}
