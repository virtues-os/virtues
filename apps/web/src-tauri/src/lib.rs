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

      // Decide where to land (mirrors desktop main.rs is_paired → pick-URL):
      //   paired    → bind the loopback, serve the box over iroh, load it
      //   not paired → the bundled connect shell (pair.html)
      // ensure_serving() binds the port before we point the webview at it, so
      // the first request queues rather than gets refused.
      let reach = app.reach();
      let url = if reach.is_paired() {
        match tauri::async_runtime::block_on(reach.ensure_serving()) {
          Ok(()) => WebviewUrl::External(reach.loopback_url().parse().expect("loopback url")),
          Err(e) => {
            eprintln!("[reach] serve failed: {e}");
            WebviewUrl::App("mobile-pair.html".into())
          }
        }
      } else {
        WebviewUrl::App("mobile-pair.html".into())
      };

      WebviewWindowBuilder::new(app, "main", url)
        .title("Virtues")
        .build()?;
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
