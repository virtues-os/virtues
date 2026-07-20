# Cross-platform apps — Windows, Linux, Android (views)

Ship the Virtues **viewer** app on Windows, Linux, and Android. Collectors are
**out of scope** here; the plan deliberately leaves the seams in place so the
Kotlin/native collector halves can land later without rework.

## State of play (audited, branch `feat/applets`)

- **The in-process reach proxy already exists and is shared.**
  `crates/virtues-reach-client` builds an iroh client and serves the box on a
  loopback port (`serve_loopback` / `serve_on_provider`). It is pure
  cross-platform Rust — no Apple-only deps.
- **The mobile app is already built and works on iOS.**
  `apps/web/src-tauri/src/lib.rs` is the `#[tauri::mobile_entry_point]`: it runs
  the `reach` plugin in-process (`:7117` loopback over iroh), loads the bundled
  SvelteKit SPA via `ui/mobile-pair.html`, and injects
  `__VIRTUES_BACKEND_ORIGIN__` / `__VIRTUES_MOBILE__`. The collector plugins are
  wired here too, but that is the deferred part.
- **The desktop app (macOS) still uses the OLD sidecar model.**
  `apps/web/src-tauri/src/main.rs` shells out to the `virtues-client` binary
  (`pair`, `discover`, `up`) via `tauri-plugin-shell`, installs it as a
  LaunchAgent, and reconciles it on update. `externalBin` ships
  `virtues-client` + `virtues-collector`.
- **Why the sidecar exists at all:** the `:7117` proxy must **outlive the app**
  so the separate collector daemon keeps uploading after the app is quit. That
  is a *collector* requirement. **View-only has no such requirement** — which is
  what makes this milestone small.
- **Config / CI today:** `tauri.conf.json` (macOS) + `tauri.ios.conf.json`.
  `gen/apple` only (no `gen/android`). `release-mac.yml` builds the Tauri app;
  `release-linux.yml` builds the **box** (server) and cross-compiles the
  `virtues-client` binary, but does **not** build a Linux desktop app bundle.
  No Windows or Android config/CI.
- **Storage:** the reach plugin's `FileStore` uses `dirs::data_dir()`
  (`virtues_dir()` in `plugins/reach/src/lib.rs`) — correct on
  macOS/Windows/Linux, **wrong on Android** (needs the Tauri path API). iOS uses
  a Keychain bridge; Android will fall back to the file store initially.

## Central decision: one reach path for all three new platforms

For the view-only milestone, run the reach proxy **in-process** everywhere:

- **Android** — mandatory (no sidecars on Android). Already the mobile `lib.rs`
  path.
- **Windows / Linux** — *choose* in-process over porting the sidecar. Since
  view-only doesn't need the proxy to outlive the app, in-process deletes an
  entire category of work: no per-platform service install, no
  `reconcile_helpers`, and no Windows rename-over-running-exe problem.
- **macOS** — leave the existing sidecar path untouched (its collector already
  depends on it). The desktop `main.rs` keeps macOS on the sidecar; Windows/Linux
  take the in-process branch.

**Leaving room for collectors:** keep the `reach` plugin as the single reach
layer (already true), keep the shared `virtues_enqueue` outbox in place (already
cross-platform Rust), and keep the collector plugins linked but dormant. When
collectors return, the durable-service question is revisited per platform then —
nothing here forecloses it.

---

## Workstream A — Desktop reach unification (shared prerequisite)

Make the desktop `main.rs` serve the box **in-process on Windows/Linux** while
leaving macOS on the sidecar.

1. Register the `reach` plugin in the desktop `Builder` (already a path dep in
   `src-tauri/Cargo.toml`; just add `.plugin(tauri_plugin_reach::init())`).
2. `#[cfg(target_os = "macos")]` → keep `install_helpers`/`virtues_client_command`
   (sidecar `up`). `#[cfg(not(target_os = "macos"))]` → replace those calls with
   `reach.ensure_serving()` and route pair/discover through the reach plugin's
   commands instead of shelling to `virtues-client`.
3. Confirm the connect shell: desktop uses `ui/pair.html` (not
   `mobile-pair.html`) and does **not** set `__VIRTUES_MOBILE__`, so the desktop
   sidebar chrome renders. Inject `__VIRTUES_BACKEND_ORIGIN__` from
   `reach.loopback_url()` the same way `lib.rs` does.

## Workstream B — Windows + Linux (one workstream)

Config + gating + packaging. No collector, no service, no sidecar.

- **Config overlays:**
  - `tauri.windows.conf.json` — `nsis` target, `icon.ico`, drop `externalBin`.
  - `tauri.linux.conf.json` — `appimage` + `deb` targets, PNG icon set, drop
    `externalBin`. (Updater only self-updates **AppImage**, not deb — AppImage is
    the primary channel.)
- **cfg-gate the macOS-isms in `main.rs`:**
  - `launchctl` / System-Settings deep-links (`open x-apple.systempreferences…`)
    — collector-perms plumbing, not needed for views. Gate to macOS.
  - `RunEvent::Reopen` — macOS-only enum variant; gate or it won't compile.
  - Tray: carries over; `icon_as_template` is a no-op off macOS. **Linux tray**
    needs `libayatana-appindicator` and degrades on stock GNOME — add a graceful
    "no tray" fallback.
  - `reconcile_helpers` / `copy_executable` — sidecar-only; compiled out on the
    in-process branch.
- **Credential store:** `keyring` covers Windows Credential Manager + Linux
  Secret Service; the reach `FileStore` fallback already covers headless/no-
  keyring Linux.
- **Updater:** Windows NSIS + `latest.json` + (ideally) Authenticode signing;
  Linux AppImage updater + its own `latest.json`. Acceptable to defer self-update
  to v1 and ship manual/store updates first.
- **CI:** add a Windows job (`tauri build` → NSIS) and a Linux desktop job
  (`tauri build` → AppImage/deb). These are **new** — `release-linux.yml` today
  builds the server, not the app.

## Workstream C — Android (view)

Reuse the mobile `lib.rs`; add the Android shell; gate collectors off.

- **Init:** `tauri android init` → generates `gen/android` (Gradle project) and
  the Android half of `tauri.conf`.
- **Gate collectors to iOS for now.** `lib.rs` currently calls
  `app.health().resume()`, `app.audio().resume()`, etc. unconditionally on
  mobile. On Android these call `register_android_plugin("com.virtues.health",
  "HealthPlugin")`, which **fails** — the Kotlin class doesn't exist. Wrap the
  collector plugin registration + `resume()` calls in `#[cfg(target_os = "ios")]`
  so Android boots with **only `reach`** active. This is the seam collectors slot
  back into (un-gate + add `android/` Kotlin halves).
- **Storage:** point the reach `FileStore` at the Tauri `app_data_dir()` on
  Android instead of `dirs::data_dir()` (which is unreliable on Android). App-
  private storage is already sandboxed; Android Keystore hardening is a later
  follow-up (parallels the iOS Keychain bridge).
- **Cleartext loopback:** add a `network_security_config.xml` permitting cleartext
  to `127.0.0.1` so the SPA→loopback HTTP isn't blocked by Android's default
  cleartext policy. Mirror the CSP/connect-src into the Android setup.
- **Network monitoring:** the reach plugin's iOS `ReachMonitor` (reconnect on
  network change) has no Android half. Rely on Rust-side reconnect for v1; a small
  `ConnectivityManager` Kotlin shim is a later nicety.
- **`set_appearance`:** already has an Android no-op branch — good.
- **Manifest:** `INTERNET` only (no collector perms for views).
- **Signing + CI:** Android keystore + a build workflow (APK/AAB).

---

## Sequencing

1. **A — desktop reach unification** (in-process for non-macOS). Unblocks B.
2. **B — Windows + Linux** config + gates + CI → two view apps ship.
3. **C — Android** init + collector-gating + shell + CI → view app ships.

Each collector, later, is additive: Android gets `android/` Kotlin halves +
un-gating; Windows/Linux get a desktop collector story (daemon or plugin halves)
against the same `virtues_enqueue` outbox that already exists.
