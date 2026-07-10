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

use virtues_reach_client::{outbox, BoxStore};

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

/// Drain the outbox to the box, blocking until done or `timeout_secs` elapses.
///
/// Called by the Swift background paths (significant-location wake / BGTask)
/// while holding an OS background-task assertion, so the drain runs before iOS
/// suspends. Reuses the process-global warm iroh client (builds one if absent).
/// Returns the number of records delivered, or a negative error code. A timeout
/// (`-4`) is not data loss — partial progress is persisted and unsent rows stay
/// queued for the next wake.
///
/// # Safety
/// Safe to call from any non-tokio thread (it drives its own block_on).
#[no_mangle]
pub extern "C" fn virtues_drain_blocking(timeout_secs: i32) -> i32 {
  let store = crate::FileStore::new();
  let Some(rec) = store.load().ok().flatten() else {
    return -1;
  };
  let timeout = std::time::Duration::from_secs(timeout_secs.clamp(1, 60) as u64);

  tauri::async_runtime::block_on(async move {
    let client = match crate::warm_client() {
      Some(c) => c,
      None => match virtues_reach_client::build_client(&rec).await {
        Ok(c) => {
          crate::set_warm_client(c.clone());
          c
        }
        Err(_) => return -2,
      },
    };
    match tokio::time::timeout(timeout, crate::upload::drain(&client, &rec)).await {
      Ok(Ok(n)) => n as i32,
      Ok(Err(_)) => -3,
      Err(_) => -4,
    }
  })
}

/// Recover the box connection after an iOS network change / app foreground.
/// Pokes iroh (`network_change`), and if a bounded probe still fails, rebuilds the
/// whole endpoint (same identity) — the escape from the iOS socket wedge
/// (iroh#4289). Called from Swift's `NWPathMonitor` + `didBecomeActive`. Blocks
/// briefly (~1–5s), so call off the main thread. Returns 0 healed / 1 rebuilt /
/// negative on error. Never panics across the boundary.
#[no_mangle]
pub extern "C" fn virtues_recover_connection() -> i32 {
  tauri::async_runtime::block_on(crate::recover_connection())
}

/// Force the linker to keep the C ABI symbols in the app's static lib. Call once
/// from the plugin `init` (which is in the link graph); referencing the function
/// pointer pulls this object out of the archive so Swift can find the symbol.
pub(crate) fn keep_symbols() {
  let enqueue: extern "C" fn(*const c_char, *const c_char) -> i32 = virtues_enqueue;
  let drain: extern "C" fn(i32) -> i32 = virtues_drain_blocking;
  let recover: extern "C" fn() -> i32 = virtues_recover_connection;
  std::hint::black_box(enqueue as *const ());
  std::hint::black_box(drain as *const ());
  std::hint::black_box(recover as *const ());
}
