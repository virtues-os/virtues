//! Native photo favorites collector plugin.
//!
//! On iOS it sends one record per favorite photo or video into the shared
//! outbox via `virtues_enqueue` (stream `photos`, `local_id` = PhotoKit's local
//! identifier, so a photo sent again updates its row on the server), with a
//! picture at most 2048 pixels on its longest side. A photo that stops being a
//! favorite is sent as removed. The Rust side is thin; collection is Swift.

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
use desktop::Photos;
#[cfg(mobile)]
use mobile::Photos;

pub trait PhotosExt<R: Runtime> {
  fn photos(&self) -> &Photos<R>;
}

impl<R: Runtime, T: Manager<R>> crate::PhotosExt<R> for T {
  fn photos(&self) -> &Photos<R> {
    self.state::<Photos<R>>().inner()
  }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("photos")
    .invoke_handler(tauri::generate_handler![
      commands::enable,
      commands::resume,
      commands::status,
      commands::collect
    ])
    .setup(|app, api| {
      #[cfg(mobile)]
      let photos = mobile::init(app, api)?;
      #[cfg(desktop)]
      let photos = desktop::init(app, api)?;
      app.manage(photos);
      Ok(())
    })
    .build()
}
