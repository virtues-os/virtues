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
use desktop::LocationProbe;
#[cfg(mobile)]
use mobile::LocationProbe;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the location-probe APIs.
pub trait LocationProbeExt<R: Runtime> {
  fn location_probe(&self) -> &LocationProbe<R>;
}

impl<R: Runtime, T: Manager<R>> crate::LocationProbeExt<R> for T {
  fn location_probe(&self) -> &LocationProbe<R> {
    self.state::<LocationProbe<R>>().inner()
  }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("location-probe")
    .invoke_handler(tauri::generate_handler![
      commands::start_probe,
      commands::resume_probe,
      commands::read_rows
    ])
    .setup(|app, api| {
      #[cfg(mobile)]
      let location_probe = mobile::init(app, api)?;
      #[cfg(desktop)]
      let location_probe = desktop::init(app, api)?;
      app.manage(location_probe);
      Ok(())
    })
    .build()
}
