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
// exists — see agents/record/spa-delivery.md.

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
/// | 5 | `reach|improv_owner_claim` — reopen a moved, offline server over
/// |   | Bluetooth as its owner (`$lib/tauri/boxRadio.ts` gates on this) |
/// | 6 | `reach|reach_rehome` — point the pairing at a moved server's new
/// |   | address after it joins over Bluetooth (the `/reconnect` screen) |
/// | 7 | `recheck_collector` — make the Mac collector re-check its permissions
/// |   | now (SIGUSR1); `get_collector_status` gains `safari_library` |
/// |   | On iOS, same release: `location-probe|status`, `|request_location`
/// |   | (resolves on the person's answer), `|open_settings`; audio and health
/// |   | `status` gain `mic`/`enabled` and `permission` (Setup's Connections) |
/// | 8 | `bundle_update_ready` on both shells, and `ota_check_now` on the Mac:
/// |   | a staged UI bundle applies by reloading while hidden, each page load
/// |   | pinned to one bundle (`checkForNewUi` in `routes/(app)/+layout.svelte`, local-ui-plan.md) |
///
/// Note `bundle-contract.json` stays at `minShellVersion: 1`: every addition
/// so far is called best-effort and the UI works fine without it, so requiring
/// more would strand clients on an older app for no gain.
///
/// Lives here rather than in main.rs so mobile can see it: main.rs is the
/// desktop bin and is never compiled for iOS/Android.
pub const COMMAND_SURFACE_VERSION: u32 = 8;

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
  ShellIdentity {
    app_version: app.package_info().version.to_string(),
    command_surface: COMMAND_SURFACE_VERSION,
    active_bundle: ui_data_dir(app)
      .and_then(|d| web_bundle::active_bundle_id(&d)),
    last_check: ui_data_dir(app)
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
  confirm_page_load(&app);
}

/// See `bundle_update_ready` in main.rs.
#[cfg(mobile)]
#[tauri::command]
fn bundle_update_ready(app: tauri::AppHandle) -> bool {
  update_ready(&app)
}

/// Where this app keeps its own copy of the UI (web_bundle.rs): the app's
/// data dir, or a folder of its own under a dev profile (`VIRTUES_PROFILE`).
/// A profile pairs with a bench box, and that box's UI bundles must never
/// become the real app's.
pub fn ui_data_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<std::path::PathBuf> {
  use tauri::Manager;
  let dir = app.path().app_data_dir().ok()?;
  Some(match tauri_plugin_reach::profile() {
    Some(p) => dir.join("profiles").join(p),
    None => dir,
  })
}

/// The page rendered: confirm the bundle it was pinned to at load. The shell's
/// own record (`web_bundle::serving_bundle_id`), never the page's claim, and
/// never the active pointer, which a background check may already have moved to
/// a bundle this page never ran. Shared by both shells' `bundle_boot_ok`.
pub fn confirm_page_load<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
  if let Some(dir) = ui_data_dir(app) {
    web_bundle::confirm_page_load(&dir);
  }
}

/// Is a newer UI bundle staged since this page loaded? The SPA asks when it is
/// hidden, and reloads if so (`checkForNewUi` in `routes/(app)/+layout.svelte`). Shared by both shells'
/// `bundle_update_ready`.
pub fn update_ready<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
  ui_data_dir(app)
    .map(|d| web_bundle::update_ready(&d))
    .unwrap_or(false)
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
/// Runs off the launch path and never swaps the bundle the open page is
/// serving: each page load is pinned to its bundle (web_bundle.rs, "The bundle
/// a page load serves from"), so an applied bundle takes effect at the NEXT
/// page load, where `resolve_pending` is watching it. The SPA reloads while
/// hidden to get there (`checkForNewUi` in `routes/(app)/+layout.svelte`). Shared by both shells.
///
/// Every outcome is recorded (`record_outcome`) because this runs on a
/// background thread: by the time anyone looks at a screen the result is
/// otherwise gone, and a shell silently refusing every bundle looks exactly
/// like OTA never being configured.
pub fn ota_check<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
  use std::sync::atomic::{AtomicBool, Ordering};
  // One check at a time. The launch check and a foreground `ota_check_now`
  // can overlap (a download takes up to 30s), and two would unpack into the
  // same staging directory and tear each other's files. The second one is
  // simply skipped: the first is already asking the same question.
  static CHECKING: AtomicBool = AtomicBool::new(false);
  if CHECKING.swap(true, Ordering::SeqCst) {
    return;
  }
  struct Done;
  impl Drop for Done {
    fn drop(&mut self) {
      CHECKING.store(false, Ordering::SeqCst);
    }
  }
  let _done = Done;
  let Some(dir) = ui_data_dir(app) else { return };
  let baked = baked_bundle_version(app);
  match web_bundle::check_and_apply(&dir, COMMAND_SURFACE_VERSION, baked.as_deref()) {
    Ok(outcome) => {
      match &outcome {
        web_bundle::Outcome::Applied { content_hash } => {
          eprintln!("[ota] staged bundle {content_hash}; active at the next page load")
        }
        web_bundle::Outcome::ShellTooOld { needs, have } => eprintln!(
          "[ota] box bundle needs shell surface {needs}, this app has {have} — \
           staying on the bundled build (update the app from the App Store)"
        ),
        web_bundle::Outcome::BoxBehind { box_version, have } => eprintln!(
          "[ota] box serves UI {box_version}, this app already runs {have} — \
           staying put (upgrade the box with `sudo virtues upgrade`)"
        ),
        // Loud, because this one refuses every bundle until it is fixed: it
        // means a build went out without its version stamped (see
        // tools/ios-release.sh) and OTA is inert on that binary.
        web_bundle::Outcome::VersionUnreadable { box_version, have } => eprintln!(
          "[ota] cannot order box UI {box_version} against this app's {} — \
           refusing rather than risk a downgrade",
          have.as_deref().unwrap_or("(unstamped)")
        ),
        _ => {}
      }
      web_bundle::record_outcome(&dir, &outcome);
    }
    Err(e) => eprintln!("[ota] check failed (harmless, will retry): {e}"),
  }
}

/// The `version` of the UI build compiled into THIS binary, read off the
/// manifest that build stamped for itself.
///
/// Read rather than asserted. `.virtues-bundle.json` is written by
/// `apps/web/scripts/write-bundle-manifest.mjs` into the same `build/` that
/// `tauri.ios.conf.json` bakes as `frontendDist`, so the asset resolver hands
/// back the binary's own copy of exactly the document the box serves at
/// `/api/web-bundle/version`. Baking the version into Rust separately would be
/// a second number to keep in step, and the delivery plan's fourth invariant is
/// that no component asserts a fact it did not observe.
///
/// `None` when the manifest is missing — a build whose SPA was never stamped.
/// The gate treats that as ambiguous and refuses, which is the safe direction.
pub fn baked_bundle_version<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<String> {
  let asset = app.asset_resolver().get(".virtues-bundle.json".into())?;
  web_bundle::Manifest::parse(&String::from_utf8_lossy(&asset.bytes)).map(|m| m.version)
}

/// Answer one request on the `virtues://` scheme: the app's own copy of the UI.
///
/// Shared by both shells (the phone since it was built; the Mac since
/// 2026-09-29, agents/plan/local-ui-plan.md), so the two can never serve the
/// app two different ways. Airlock pages from the binary; then the page's
/// pinned overlay bundle; then the build baked into the binary.
///
/// A custom scheme rather than Tauri's own `tauri://` because Tauri owns that
/// one and gives no hook to intercept it. On the phone the cost was a one-time
/// origin change (`tauri://localhost` → `virtues://localhost`), which emptied
/// its IndexedDB once. That is a cache, not data: pages persist server-side in
/// `app_pages.yjs_state` and re-sync on connect. From here the origin never
/// moves again, so applying a bundle can never cost a user their local state.
///
/// Fail-safe: every path out of the handler that is not a confirmed overlay hit
/// falls through to the baked asset. A corrupt or half-written bundle costs
/// freshness, never the UI.
pub fn serve_ui<R: tauri::Runtime>(
  app: &tauri::AppHandle<R>,
  request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {

  let path = request.uri().path().to_string();
  let baked = |p: &str| app.asset_resolver().get(p.to_string());

  let Some(resolved) = web_bundle::resolve_request_path(&path) else {
    return tauri::http::Response::builder()
      .status(400)
      .body(Vec::new())
      .unwrap();
  };

  // The box's paths are never the app's: 404, and never a page load. See
  // `web_bundle::is_backend_path`.
  if web_bundle::is_backend_path(&path) {
    return tauri::http::Response::builder()
      .status(404)
      .body(Vec::new())
      .unwrap();
  }

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
  let airlock: Option<(&'static [u8], &'static str)> = match resolved.as_str() {
    "connect.html" => Some((include_bytes!("../ui/connect.html"), "text/html")),
    "probe.html" => Some((include_bytes!("../ui/probe.html"), "text/html")),
    // jsQR (Apache-2.0), vendored because the airlock has no bundler and
    // must not depend on packaging. WebKit has NEVER shipped
    // `BarcodeDetector` — it is a Chrome API, and building the scanner on
    // it meant every iPhone reported itself "too old" while the camera
    // never even started. A real decoder is the only portable answer.
    "jsqr.js" => Some((include_bytes!("../ui/jsqr.js"), "text/javascript")),
    _ => None,
  };
  if let Some((bytes, mime)) = airlock {
    return tauri::http::Response::builder()
      .status(200)
      .header("Content-Type", mime)
      .body(bytes.to_vec())
      .unwrap();
  }

  // A navigation starts a page load: settle rollback and pin this load to
  // one bundle, so every asset it requests afterwards comes from the same
  // place (web_bundle.rs, "The bundle a page load serves from").
  let accept = request
    .headers()
    .get(tauri::http::header::ACCEPT)
    .and_then(|v| v.to_str().ok());
  if web_bundle::is_page_load(&resolved, accept) {
    if let Some(dir) = ui_data_dir(app) {
      if web_bundle::begin_page_load(&dir) {
        eprintln!("[ota] a staged bundle failed to confirm; rolled back");
      }
    }
  } else if web_bundle::is_page_document(&resolved) && resolved != "index.html" {
    // An extension-less path the page asked for as data: a call site that
    // should be reaching the box. Served the document as before, and named.
    eprintln!("[ui] {path} is not an app file; answered with 200.html (accept: {accept:?})");
  }

  // Overlay first, baked second. `mime_guess` is not a dependency here, so
  // the baked asset's own mime type is reused when the overlay serves the
  // same path — which it does for every file, both being the same build
  // shape.
  let overlay = ui_data_dir(app)
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
}

#[cfg(mobile)]
#[tauri::mobile_entry_point]
pub fn run() {
  use tauri::{WebviewUrl, WebviewWindowBuilder};
  use tauri_plugin_reach::ReachExt;

  // OTA asset protocol: every request for the UI comes through `serve_ui`.
  let builder = tauri::Builder::default()
    .plugin(tauri_plugin_reach::init())
    .register_uri_scheme_protocol("virtues", |ctx, request| serve_ui(ctx.app_handle(), &request));

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
      bundle_update_ready,
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

      // OTA rollback is settled per page load now, in `serve_ui` (web_bundle.rs,
      // "The bundle a page load serves from"), not once here.
      if let Some(dir) = ui_data_dir(app.handle()) {
        // Drop an overlay the App Store has overtaken. An app update keeps
        // the container, so a bundle applied weeks ago outlives the binary that
        // fetched it and goes on shadowing the newer build THIS binary ships
        // with. After the rollback above, so a revert to `previous` is judged
        // too; before the window, because this moves the pointer a live page
        // would be serving from.
        let baked = baked_bundle_version(app.handle());
        if let Some(dropped) = web_bundle::drop_stale_overlay(&dir, baked.as_deref()) {
          eprintln!(
            "[ota] overlay {dropped} is older than this app's own UI — \
             back to the build it shipped with"
          );
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
      // open page is already running. A bundle applied now takes effect at the
      // NEXT page load, where `resolve_pending` (in `serve_ui`) is watching
      // it. That ordering is what makes a bad bundle survivable.
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
      //
      // UNPAIRED, THE iPHONE OPENS SETUP (2026-09-27). Setup's first half
      // (sign in, find the server, its four words, Wi-Fi, pair) runs from
      // this app's own baked copy, over the same reach commands the connect
      // page used (apps/web/src/lib/components/setup/prepair.svelte.ts), so
      // the flow runs unbroken from Welcome to the app. A route path is
      // answered with the SPA's `200.html`. Android has no radio bridge in
      // the SPA and keeps the connect page; so does every recovery screen
      // (`connect.html#reset`, `#unreachable`), which the SPA links to.
      let start = if paired || cfg!(not(target_os = "ios")) {
        "virtues://localhost/connect.html"
      } else {
        "virtues://localhost/setup"
      }
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
