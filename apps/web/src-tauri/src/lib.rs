// Mobile (iOS/Android) entry point.
//
// Desktop builds the bin from src/main.rs and never compiles this file. This
// deliberately does NOT reuse the desktop `main()` (tray, self-updater,
// localhost proxy probing — all desktop-only).
//
// The mobile app is: an in-process iroh reach loopback + a webview pointed at it
// + native collectors feeding a shared upload queue. This entry wires the
// reach + location plugins and picks the launch URL.
//
// NOTE: mobile does NOT resolve its URL the way desktop does, despite what this
// comment claimed until 2026-08-05. Desktop shells to the box and renders the
// build the box serves; mobile IS the bundled SvelteKit build and uses the box
// only as a REST/WS API. That difference is the whole reason `web_bundle`
// exists — see docs/spa-delivery-plan.md.

/// OTA web-bundle overlay. Lives in the lib so BOTH shells can reach it: the
/// mobile entry below, and the desktop bin via `virtues_lib::web_bundle`.
pub mod web_bundle;

/// Version of the Tauri command surface this binary exposes.
///
/// **Why this exists.** The UI and the shell are separate artifacts with
/// separate version lines. On desktop the box literally serves the JavaScript
/// that `invoke()`s commands compiled into a different binary; with OTA, the
/// box hands mobile a bundle that does the same. Nothing negotiated between
/// them, so a UI newer than its shell called a command that did not exist and
/// threw inside whatever feature needed it.
///
/// **The contract.** A bundle declares the lowest surface it can run against as
/// `minShellVersion` (apps/web/bundle-contract.json). A shell reporting less
/// than that refuses the bundle rather than loading it and failing somewhere
/// unpredictable — see `web_bundle::check_and_apply`. Within a bundle that does
/// load, `bridge.ts`'s `shellSupports()` gates individual features so a missing
/// command degrades visibly instead of throwing.
///
/// **Bump this** when you add a command the UI may require, or change an
/// existing command's arguments or return shape. Do NOT bump for internal
/// changes that leave the surface identical — the number tracks the contract,
/// not the code. Raising `minShellVersion` to match strands every client that
/// has not updated its native app, so raise that only when the UI genuinely
/// cannot run on the older surface.
///
/// | v | change |
/// |---|---|
/// | 1 | baseline: the surface as of 2026-08-05 |
///
/// Lives here rather than in main.rs so mobile can see it: main.rs is the
/// desktop bin and is never compiled for iOS/Android.
pub const COMMAND_SURFACE_VERSION: u32 = 1;

// Appearance bridge: the SPA's themes are user-picked (not system-driven), so
// the iOS status bar can't ride the system light/dark mode — a dark theme on a
// light-mode phone gets an invisible clock. tao's window.set_theme() is a no-op
// on iOS, so flip UIWindow.overrideUserInterfaceStyle ourselves; the status
// bar, keyboard, and native sheets then all resolve from the app theme's
// darkness. Called by the SPA on startup and on every theme change.
#[cfg(target_os = "ios")]
#[tauri::command]
fn set_appearance(app: tauri::AppHandle, dark: bool) {
  let _ = app.run_on_main_thread(move || unsafe {
    use objc2::{class, msg_send, runtime::AnyObject};
    let ui_app: *mut AnyObject = msg_send![class!(UIApplication), sharedApplication];
    let windows: *mut AnyObject = msg_send![ui_app, windows];
    let count: usize = msg_send![windows, count];
    // UIUserInterfaceStyle: 1 = light, 2 = dark.
    let style: isize = if dark { 2 } else { 1 };
    for i in 0..count {
      let w: *mut AnyObject = msg_send![windows, objectAtIndex: i];
      let _: () = msg_send![w, setOverrideUserInterfaceStyle: style];
    }
  });
}

// Android resolves the theme through the webview alone; accept and ignore so
// the SPA can call unconditionally on mobile.
#[cfg(all(mobile, not(target_os = "ios")))]
#[tauri::command]
fn set_appearance(_dark: bool) {}

/// Mobile's half of the OTA contract. Mirrors the desktop commands of the same
/// names in main.rs — the SPA calls these without knowing which shell it is in,
/// so both must exist and agree.
#[cfg(mobile)]
#[tauri::command]
fn command_surface_version() -> u32 {
  COMMAND_SURFACE_VERSION
}

/// See `bundle_boot_ok` in main.rs: a staged bundle is only kept once the UI it
/// contains has actually rendered.
#[cfg(mobile)]
#[tauri::command]
fn bundle_boot_ok(app: tauri::AppHandle) {
  use tauri::Manager;
  if let Ok(dir) = app.path().app_data_dir() {
    web_bundle::mark_boot_ok(&dir);
  }
}

#[cfg(mobile)]
#[tauri::mobile_entry_point]
pub fn run() {
  // `Manager` for `app.path()` — the OTA bundle store needs the app-data dir.
  use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
  use tauri_plugin_reach::ReachExt;

  let builder = tauri::Builder::default().plugin(tauri_plugin_reach::init());

  // The six collectors are iOS-only: Rust shims over Swift halves, with no
  // Android counterpart yet (see Cargo.toml). Android boots reach + the webview
  // alone — a viewer. Chained conditionally rather than `cfg`-ing each line, the
  // same pattern main.rs uses for the single-instance plugin.
  #[cfg(target_os = "ios")]
  let builder = builder
    .plugin(tauri_plugin_location_probe::init())
    .plugin(tauri_plugin_health::init())
    .plugin(tauri_plugin_eventkit::init())
    .plugin(tauri_plugin_contacts::init())
    .plugin(tauri_plugin_finance::init())
    .plugin(tauri_plugin_audio::init());

  builder
    .invoke_handler(tauri::generate_handler![
      set_appearance,
      command_surface_version,
      bundle_boot_ok
    ])
    .setup(|app| {
      // Collector resume — iOS only, mirroring the plugin registrations above.
      #[cfg(target_os = "ios")]
      {
        use tauri_plugin_audio::AudioExt;
        use tauri_plugin_contacts::ContactsExt;
        use tauri_plugin_eventkit::EventKitExt;
        use tauri_plugin_finance::FinanceExt;
        use tauri_plugin_health::HealthExt;
        use tauri_plugin_location_probe::LocationProbeExt;

        // Background location: install the CLLocationManager delegate as early as
        // Tauri lets us (runs on every launch, incl. cold background relaunch).
        // resume_probe only (re)starts if already authorized — it never prompts,
        // so a fresh/unauthorized install isn't cold-slapped before onboarding.
        // The explicit "Enable" opt-in calls start_probe (which prompts).
        if let Err(e) = app.location_probe().resume_probe() {
          eprintln!("[location-probe] resume failed: {e}");
        }
        // HealthKit: resume collecting only if already opted in (never prompts).
        if let Err(e) = app.health().resume() {
          eprintln!("[health] resume failed: {e}");
        }
        // Calendar: re-scan on launch if already authorized (never prompts).
        if let Err(e) = app.eventkit().resume() {
          eprintln!("[eventkit] resume failed: {e}");
        }
        // Contacts: re-snapshot on launch if already authorized (never prompts).
        if let Err(e) = app.contacts().resume() {
          eprintln!("[contacts] resume failed: {e}");
        }
        // Finance: re-sync on launch if already opted in (never prompts).
        if let Err(e) = app.finance().resume() {
          eprintln!("[finance] resume failed: {e}");
        }
        // Audio: resume recording only if already authorized + left enabled. The
        // recording session doubles as the background keepalive; a significant-
        // location wake also calls this path so it resurrects after suspension.
        if let Err(e) = app.audio().resume() {
          eprintln!("[audio] resume failed: {e}");
        }
      }

      // OTA rollback, FIRST — before anything can load a bundle. A pointer left
      // pending means the previous launch flipped to a bundle that never came
      // back to confirm it rendered, so that bundle does not boot: abandon it
      // and revert. Doing this before the window exists is the whole point;
      // afterwards we would be deciding while already showing the bad bundle.
      if let Ok(dir) = app.path().app_data_dir() {
        if web_bundle::resolve_pending_at_startup(&dir) {
          eprintln!("[ota] a staged bundle failed to confirm; rolled back");
        }
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

      // Ask the box whether it has newer UI, off the launch path entirely.
      //
      // Deliberately AFTER the window is decided and on its own thread: an
      // update must never delay a launch, and must never change the bundle the
      // current session is already running. A bundle applied now takes effect
      // on the NEXT launch, where `resolve_pending_at_startup` above is
      // watching it. That ordering is what makes a bad bundle survivable.
      if paired {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
          let Ok(dir) = handle.path().app_data_dir() else { return };
          match web_bundle::check_and_apply(&dir, COMMAND_SURFACE_VERSION) {
            Ok(web_bundle::Outcome::Applied { content_hash }) => {
              eprintln!("[ota] staged bundle {content_hash}; active next launch")
            }
            Ok(web_bundle::Outcome::ShellTooOld { needs, have }) => eprintln!(
              "[ota] box bundle needs shell surface {needs}, this app has {have} — \
               staying on the bundled build (update the app from the App Store)"
            ),
            Ok(_) => {}
            Err(e) => eprintln!("[ota] check failed (harmless, will retry next launch): {e}"),
          }
        });
      }

      // Always launch the connect shell; when paired it immediately redirects to
      // the SPA root ("/"), which guarantees SvelteKit boots at "/" rather than
      // "/index.html". Pre-pair it shows discovery + pairing.
      // __VIRTUES_MOBILE__ tells the SPA to render the bottom-tab phone chrome
      // (hide the desktop sidebar) — see lib/stores/mobileLayout.svelte.ts.
      let init = format!(
        "window.__VIRTUES_BACKEND_ORIGIN__ = '{}'; window.__VIRTUES_PAIRED__ = {}; window.__VIRTUES_MOBILE__ = true;",
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
