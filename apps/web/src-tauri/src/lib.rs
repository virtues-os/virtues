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
/// | 2 | `ota_check_now` — lets the UI trigger an update check on foreground |
/// | 3 | `update_state_cmd` / `apply_update_cmd` — the app updater's state and
/// |   | apply, for the sidebar's "Relaunch to X" chip (desktop-only commands;
/// |   | mobile at 3 still rejects them and the UI treats that as silence) |
/// | 4 | `check_app_update_cmd` — manual check trigger for This Mac's ledger |
///
/// Note `bundle-contract.json` stays at `minShellVersion: 1`: every addition
/// so far is called best-effort and the UI works fine without it, so requiring
/// more would strand clients on an older app for no gain.
///
/// Lives here rather than in main.rs so mobile can see it: main.rs is the
/// desktop bin and is never compiled for iOS/Android.
pub const COMMAND_SURFACE_VERSION: u32 = 4;

/// What the native shell knows about itself.
///
/// Three artifacts carry three version lines — the box, the UI bundle, and this
/// binary — and until now only the first two were visible anywhere. On
/// 2026-08-05 a phone was running visibly newer UI than the Mac beside it and
/// the reason was not discoverable from either screen; it took `ssh` and a git
/// log. An update mechanism whose state cannot be read is one you cannot debug
/// when it misbehaves, so this ships before OTA is trusted, not after.
#[derive(serde::Serialize)]
pub struct ShellIdentity {
  /// This binary's version — `tauri.conf.json > version`.
  pub app_version: String,
  /// The command contract this binary exposes; see [`COMMAND_SURFACE_VERSION`].
  pub command_surface: u32,
  /// Content hash of the active OTA bundle, or `None` when running the build
  /// baked into the app. This is the bit the SPA cannot know about itself.
  pub active_bundle: Option<String>,
  /// What the last update check concluded, or `None` if none has run.
  ///
  /// Carries the refusal case especially: a shell too old for the bundle the
  /// box offers stays on its baked build *correctly*, but with nothing on
  /// screen that is indistinguishable from OTA being broken or unconfigured.
  pub last_check: Option<serde_json::Value>,
}

/// Collect [`ShellIdentity`]. Shared so the desktop bin and the mobile entry
/// answer identically — a diagnostic that differs by platform is worse than
/// none.
pub fn shell_identity<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> ShellIdentity {
  use tauri::Manager;
  ShellIdentity {
    app_version: app.package_info().version.to_string(),
    command_surface: COMMAND_SURFACE_VERSION,
    active_bundle: app
      .path()
      .app_data_dir()
      .ok()
      .and_then(|d| web_bundle::active_bundle_id(&d)),
    last_check: app
      .path()
      .app_data_dir()
      .ok()
      .and_then(|d| web_bundle::last_outcome(&d)),
  }
}

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

/// See `shell_identity_cmd` in main.rs — same command, same shape, both
/// platforms, so a diagnostic never differs by where it is read.
#[cfg(mobile)]
#[tauri::command]
fn shell_identity_cmd(app: tauri::AppHandle) -> ShellIdentity {
  shell_identity(&app)
}

/// See `bundle_boot_ok` in main.rs: a staged bundle is only kept once the UI it
/// contains has actually rendered.
#[cfg(mobile)]
#[tauri::command]
fn bundle_boot_ok(app: tauri::AppHandle) {
  use tauri::Manager;
  if let Ok(dir) = app.path().app_data_dir() {
    // The shell's own record of what this process booted — never the page's
    // claim, and never the (mutable) active pointer, which the background
    // check may already have moved to a bundle this session never ran.
    web_bundle::mark_boot_ok(&dir, web_bundle::booted_bundle_id().as_deref());
  }
}

/// Check for a new bundle now, at the UI's request.
///
/// The launch-time check is not enough on its own: this app stays alive for
/// days (the mic session is also the background keepalive), so a phone that is
/// never cold-started would never check again. The UI calls this when it comes
/// back to the foreground.
///
/// Returns immediately; the work runs on its own thread so a slow or
/// unreachable box cannot block the webview.
#[cfg(mobile)]
#[tauri::command]
fn ota_check_now(app: tauri::AppHandle) {
  let handle = app.clone();
  std::thread::spawn(move || ota_check(&handle));
}

/// Ask the box for a newer bundle and apply it if this shell can run it.
///
/// Runs off the launch path and never swaps the bundle the current session is
/// already serving — an applied bundle takes effect at the NEXT launch, where
/// `resolve_pending_at_startup` is watching it.
///
/// Every outcome is recorded (`record_outcome`) because this runs on a
/// background thread: by the time anyone looks at a screen the result is
/// otherwise gone, and a shell silently refusing every bundle looks exactly
/// like OTA never being configured.
#[cfg(mobile)]
fn ota_check(app: &tauri::AppHandle) {
  use tauri::Manager;
  let Ok(dir) = app.path().app_data_dir() else { return };
  match web_bundle::check_and_apply(&dir, COMMAND_SURFACE_VERSION) {
    Ok(outcome) => {
      match &outcome {
        web_bundle::Outcome::Applied { content_hash } => {
          eprintln!("[ota] staged bundle {content_hash}; active next launch")
        }
        web_bundle::Outcome::ShellTooOld { needs, have } => eprintln!(
          "[ota] box bundle needs shell surface {needs}, this app has {have} — \
           staying on the bundled build (update the app from the App Store)"
        ),
        _ => {}
      }
      web_bundle::record_outcome(&dir, &outcome);
    }
    Err(e) => eprintln!("[ota] check failed (harmless, will retry): {e}"),
  }
}

#[cfg(mobile)]
#[tauri::mobile_entry_point]
pub fn run() {
  // `Manager` for `app.path()` — the OTA bundle store needs the app-data dir.
  use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
  use tauri_plugin_reach::ReachExt;

  // OTA asset protocol. Every request for the UI comes through here, and the
  // handler decides per-file: the overlay bundle the box handed us, else the
  // build baked into this binary.
  //
  // A custom scheme rather than Tauri's own `tauri://` because Tauri owns that
  // one and gives no hook to intercept it. The cost is a one-time origin change
  // (`tauri://localhost` → `virtues://localhost`), which empties this app's
  // IndexedDB once. That is a cache, not data: pages persist server-side in
  // `app_pages.yjs_state` and re-sync on connect. Doing it now, before OTA
  // ships, is deliberate — from here the origin never moves again, so applying
  // a bundle can never cost a user their local state.
  //
  // Fail-safe: every path out of the handler that is not a confirmed overlay
  // hit falls through to the baked asset. A corrupt or half-written bundle
  // costs freshness, never the UI.
  let builder = tauri::Builder::default()
    .plugin(tauri_plugin_reach::init())
    .register_uri_scheme_protocol("virtues", |ctx, request| {
      use tauri::Manager;

      let path = request.uri().path().to_string();
      let baked = |p: &str| ctx.app_handle().asset_resolver().get(p.to_string());

      let Some(resolved) = web_bundle::resolve_request_path(&path) else {
        return tauri::http::Response::builder()
          .status(400)
          .body(Vec::new())
          .unwrap();
      };

      // The AIRLOCK pages are served from the binary, unconditionally, and
      // checked BEFORE the overlay/baked chain — not just as its fallback.
      // These pages gate pairing and setup; they must version with the binary
      // that runs them, never with a web bundle. Lived the alternative on
      // 2026-08-11: a stale mobile-pair.html inside the SPA build output (an
      // Aug 7 fossil in apps/web/build/) shadowed the compiled-in copy, and
      // every connect-screen fix that day silently never reached the phone —
      // five rebuilds of whack-a-mole against a file nobody was serving on
      // purpose. Same doctrine as the include_bytes fallback below ("an
      // airlock must not depend on packaging"), completed: it must not be
      // OVERRIDABLE by packaging either.
      let airlock: Option<&'static [u8]> = match resolved.as_str() {
        "connect.html" => Some(include_bytes!("../ui/connect.html")),
        "probe.html" => Some(include_bytes!("../ui/probe.html")),
        _ => None,
      };
      if let Some(bytes) = airlock {
        return tauri::http::Response::builder()
          .status(200)
          .header("Content-Type", "text/html")
          .body(bytes.to_vec())
          .unwrap();
      }

      // Overlay first, baked second. `mime_guess` is not a dependency here, so
      // the baked asset's own mime type is reused when the overlay serves the
      // same path — which it does for every file, both being the same build
      // shape.
      let overlay = ctx
        .app_handle()
        .path()
        .app_data_dir()
        .ok()
        .and_then(|d| web_bundle::read_from_overlay(&d, &resolved));

      match (overlay, baked(&resolved)) {
        (Some(bytes), asset) => tauri::http::Response::builder()
          .status(200)
          .header(
            "Content-Type",
            asset.map(|a| a.mime_type).unwrap_or_else(|| "text/html".into()),
          )
          .body(bytes)
          .unwrap(),
        (None, Some(asset)) => tauri::http::Response::builder()
          .status(200)
          .header("Content-Type", asset.mime_type)
          .body(asset.bytes)
          .unwrap(),
        // The airlock pages are answered before this match ever runs (see
        // above), so a miss here is a genuine 404.
        (None, None) => tauri::http::Response::builder()
          .status(404)
          .body(Vec::new())
          .unwrap(),
      }
    });

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
      bundle_boot_ok,
      shell_identity_cmd,
      ota_check_now
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
        // Freeze this process's boot identity NOW, while the active pointer
        // still names what this launch will serve — the check thread below
        // can move the pointer mid-session, and boot-ok is judged against
        // what actually booted, not against wherever the pointer points.
        web_bundle::capture_booted(&dir);
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
        std::thread::spawn(move || ota_check(&handle));
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
      // Loaded through the `virtues://` scheme registered above, not Tauri's
      // built-in asset protocol — that is what lets an OTA bundle answer these
      // requests. The URL is otherwise identical to what `WebviewUrl::App`
      // produced, and the handler falls back to the baked asset, so with no
      // overlay present this behaves exactly as before.
      let start = "virtues://localhost/connect.html"
        .parse()
        .expect("static url");
      WebviewWindowBuilder::new(app, "main", WebviewUrl::CustomProtocol(start))
        .title("Virtues")
        .initialization_script(&init)
        .build()?;
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
