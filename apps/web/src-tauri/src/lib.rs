// Mobile (iOS/Android) entry point.
//
// Desktop builds the bin from src/main.rs and never compiles this file. This
// deliberately does NOT reuse the desktop `main()` (tray, self-updater,
// localhost proxy probing — all desktop-only).
//
// The mobile app is: an in-process iroh reach loopback + a webview pointed at it
// + native collectors feeding a shared upload queue. This entry wires the
// reach + location plugins and picks the launch URL the same way desktop does:
// paired → the box UI over the loopback; not paired → the bundled connect shell.

#[cfg(mobile)]
#[tauri::mobile_entry_point]
pub fn run() {
  use tauri::{WebviewUrl, WebviewWindowBuilder};
  use tauri_plugin_location_probe::LocationProbeExt;
  use tauri_plugin_reach::ReachExt;

  tauri::Builder::default()
    .plugin(tauri_plugin_reach::init())
    .plugin(tauri_plugin_location_probe::init())
    .setup(|app| {
      // Background location: install the CLLocationManager delegate as early as
      // Tauri lets us (runs on every launch, incl. cold background relaunch).
      if let Err(e) = app.location_probe().start_probe() {
        eprintln!("[location-probe] start failed: {e}");
      }

      // Bundled-SPA architecture (Option A): the app IS the bundled SvelteKit
      // build; the box is a REST/WS API reached over the in-process iroh
      // loopback. We inject the loopback origin so the SPA's /api + /ws route
      // there (see lib/config/backend.ts), and bind the loopback before load so
      // the first request queues rather than gets refused.
      let reach = app.reach();
      let paired = reach.is_paired();
      if paired {
        if let Err(e) = tauri::async_runtime::block_on(reach.ensure_serving()) {
          eprintln!("[reach] serve failed: {e}");
        }
      }

      // Always launch the connect shell; when paired it immediately redirects to
      // the SPA root ("/"), which guarantees SvelteKit boots at "/" rather than
      // "/index.html". Pre-pair it shows discovery + pairing.
      let init = format!(
        "window.__VIRTUES_BACKEND_ORIGIN__ = '{}'; window.__VIRTUES_PAIRED__ = {};",
        reach.loopback_url(),
        paired
      );
      WebviewWindowBuilder::new(app, "main", WebviewUrl::App("mobile-pair.html".into()))
        .title("Virtues")
        .initialization_script(&init)
        .build()?;
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
