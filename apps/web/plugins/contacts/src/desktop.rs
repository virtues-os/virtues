use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
  app: &AppHandle<R>,
  _api: PluginApi<R, C>,
) -> crate::Result<Contacts<R>> {
  Ok(Contacts(app.clone()))
}

pub struct Contacts<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Contacts<R> {
  pub fn enable(&self) -> crate::Result<ContactsStatus> {
    Ok(ContactsStatus::default())
  }
  pub fn resume(&self) -> crate::Result<ContactsStatus> {
    Ok(ContactsStatus::default())
  }
  pub fn status(&self) -> crate::Result<ContactsStatus> {
    Ok(ContactsStatus::default())
  }
  pub fn collect(&self) -> crate::Result<ContactsStatus> {
    Ok(ContactsStatus::default())
  }
}
