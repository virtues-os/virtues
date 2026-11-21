use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EmptyRequest {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PhotosStatus {
  /// Full or limited library access.
  pub authorized: bool,
  /// Limited access: PhotoKit shows only the photos the person picked.
  pub limited: bool,
  /// A pass is reading the library right now.
  pub collecting: bool,
  /// Every photo's metadata has been sent once; only changes go now.
  pub first_pass_done: bool,
}
