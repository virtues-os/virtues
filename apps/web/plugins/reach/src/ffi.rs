//! C ABI for native collectors to enqueue records into the shared outbox.
//!
//! The normal Tauri channel is JS → Rust → Swift; this is the **reverse** — a
//! Swift collector (e.g. the `CLLocationManager` delegate, which fires on a cold
//! background relaunch before any webview/JS exists) calls straight into Rust to
//! persist a reading. Enqueue is synchronous and cheap, so it's safe there.
//!
//! Symbol retention: these are only ever called from Swift, so nothing in the
//! Rust graph references them and the linker would drop the object from the
//! app's static lib. [`keep_symbols`] (called from the plugin `init`, which *is*
//! linked) takes their addresses to force the object to be included.

use std::ffi::CStr;
use std::os::raw::c_char;

use virtues_reach_client::outbox;

/// Enqueue one collector record into the shared outbox.
///
/// `stream` and `record_json` are NUL-terminated UTF-8 C strings; `record_json`
/// is the box-shaped record (its `id`, if present, is the dedup key). Returns 0
/// on success, negative on error. Never panics across the FFI boundary.
///
/// # Safety
/// Both pointers must be valid NUL-terminated C strings for the call's duration.
#[no_mangle]
pub extern "C" fn virtues_enqueue(stream: *const c_char, record_json: *const c_char) -> i32 {
  if stream.is_null() || record_json.is_null() {
    return -1;
  }
  let (stream, json) = unsafe { (CStr::from_ptr(stream), CStr::from_ptr(record_json)) };
  let (Ok(stream), Ok(json)) = (stream.to_str(), json.to_str()) else {
    return -2;
  };
  let record: serde_json::Value = match serde_json::from_str(json) {
    Ok(v) => v,
    Err(_) => return -3,
  };
  match outbox::enqueue(stream, record) {
    Ok(()) => 0,
    Err(_) => -4,
  }
}

/// Force the linker to keep the C ABI symbols in the app's static lib. Call once
/// from the plugin `init` (which is in the link graph); referencing the function
/// pointer pulls this object out of the archive so Swift can find the symbol.
pub(crate) fn keep_symbols() {
  let f: extern "C" fn(*const c_char, *const c_char) -> i32 = virtues_enqueue;
  std::hint::black_box(f as *const ());
}
