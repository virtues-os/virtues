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

## Central decision: one reach path for ALL platforms (incl. macOS)

Run the reach proxy **in-process everywhere** — macOS included. The reason to
special-case macOS turned out to be **obsolete**:

- The macOS collector daemon was migrated to reach the box **directly over iroh**
  (`apps/mac-source`, July 5 — `BoxTransport` via `VirtuesIrohMac.xcframework`).
  `Config.swift`: `VIRTUES_API_URL`/`:7117` is used **only for the one-time
  pair/consume**; "everything after goes over iroh."
- So `:7117` now serves **only the app's own webview** — which only exists while
  the app is open. Nothing needs the proxy to outlive the app.
- The `install_helpers` doc comment claiming "the collector uploads through
  `:7117`" (and "tunnels over WireGuard") is **stale** — it predates the iroh
  collector migration.

Therefore in-process is correct on every platform:

- **Android** — mandatory (no sidecars). Already the mobile `lib.rs` path.
- **Windows / Linux** — in-process; no service install, no `reconcile_helpers`,
  no Windows rename-over-running-exe problem.
- **macOS** — **migrate to in-process too (see Phase 0)** and retire the proxy
  sidecar. Note there are *two* macOS sidecars: `virtues-client` (the proxy —
  retired here) and `virtues-collector` (the collector daemon — **kept**; it
  still needs FDA/Accessibility and 24/7 background, unchanged and out of scope).

## Phase 0 — retire the macOS proxy sidecar (do first)

Lay the unified foundation before porting anything. This is the highest-leverage
cleanup and it de-risks every downstream workstream, because after it there is
**one** reach path, not a macOS fork.

- Serve `:7117` **in-process** in the desktop app (`reach.ensure_serving()` at
  launch, *before* the box-session probe — mirrors the mobile `lib.rs` order).
- **Delete:** `install_helpers` / `uninstall_helpers` (the `virtues-client
  install` LaunchAgent — note the current app installs the proxy as a **durable**
  `com.virtues.client` agent, `RunAtLoad`+`KeepAlive`, not an app-session child),
  the `virtues-client` half of `reconcile_helpers` (+ its `launchctl kickstart` /
  `copy_executable` / exe-swap machinery), the `virtues-client` entry in
  `externalBin`, and the sidecar-probe assumptions in the launch flow.
- **Migration teardown (must-do, or it bites silently):** on first run of the
  in-process build, actively `launchctl bootout` + remove the already-installed
  `com.virtues.client` LaunchAgent. Existing users have it running durably; if
  left, it (a) orphans forever and (b) squats `:7117`, colliding with the app's
  new in-process bind on the same port. This is the one step that fails quietly
  if skipped.
- **Keep:** `virtues-collector` (the collector daemon) and *its* half of
  `reconcile_helpers` + `install_collector`. Point `install_collector`'s
  one-time consume at the in-process `:7117` (up during "Turn on this Mac" since
  the app is open) or directly at the box origin.
- **Why retiring the proxy is safe for background collection:** the collector is
  its **own** iroh peer — it embeds `VirtuesIrohMac.xcframework` and uploads via
  `BoxTransport.send` → `IrohTransport.dial` (`Uploader.swift:25`), and runs as an
  independent LaunchAgent (`RunAtLoad`+`KeepAlive`). It does **not** route uploads
  through `:7117`; that env var is used only for the one-time pairing consume. So
  the collector keeps sending over iroh with the app quit — it never depended on
  the app's proxy. (The collector and app are deliberately *separate paired
  devices*; they should not share one reach client.)
- **Risk to respect:** this touches the **shipping** macOS app. It's the correct
  foundation but a real migration, not a config tweak — test the full launch
  matrix `pair.html` already handles (paired / `#reset` / `#unreachable` /
  `#repair`) plus the silent-reconnect path before shipping.

The `virtues-client` CLI crate (`apps/desktop`) itself can stay for now (dev/
debugging), just no longer installed or shelled-to by the app.

**Leaving room for collectors:** keep the `reach` plugin as the single reach
layer (already true), keep the shared `virtues_enqueue` outbox in place (already
cross-platform Rust), and keep the collector plugins linked but dormant. When
collectors return, the durable-service question is revisited per platform then —
nothing here forecloses it.

---

## Workstream A — Desktop reach (folds into Phase 0 for macOS)

With Phase 0, **all** desktop platforms serve `:7117` in-process — no per-target
fork. macOS just goes through Phase 0 first; Windows/Linux inherit the same path.

1. **Widen the `reach` dep to all desktop targets.** It is currently gated
   `[target.'cfg(any(target_os = "ios", target_os = "android"))'.dependencies]`
   in `src-tauri/Cargo.toml` — **not** a desktop dependency today. Extend the gate
   to macOS + Windows + Linux, then `.plugin(tauri_plugin_reach::init())` in the
   desktop `Builder`.
2. Replace the sidecar calls (`install_helpers`, `virtues_client_command`) with
   `reach.ensure_serving()` in-process — the same on every desktop OS (this is
   the Phase 0 deletion, not a `cfg` fork).
3. **Command-surface parity (the real frontend work).** The desktop connect
   shell `ui/pair.html` speaks the sidecar vocabulary — `discover_servers`,
   `pair_with_code`, `install_helpers`, `recheck_box`, `diagnose_box`,
   `forget_pairing`. The reach plugin exposes a *different, smaller* set —
   `plugin:reach|{pair, discover, reach_status, forget, drain_now}` — because
   `status`/`ensure_serving` collapse what the sidecar split across
   diagnose/recheck/install. Two options: (a) point Windows/Linux at a
   **desktop-chrome variant of `mobile-pair.html`** (which already speaks reach
   commands — cheapest), or (b) keep `pair.html` and add thin `#[cfg(not(macos))]`
   wrapper commands in `main.rs` that forward the old names to `app.reach()`.
   Recommend (a). Note the reach `discover` uses a **subnet scan, not mDNS**
   (deliberate — Bonjour is flaky); behavior-equivalent on desktop LANs.
4. **Frontend-loading model:** desktop loads the box-served UI directly
   (`WebviewUrl::External("http://localhost:7117")`); mobile bundles the SPA and
   hits the loopback as an API. Windows/Linux should follow the **desktop model**
   (External loopback) — it works unchanged over the in-process reach and avoids
   bundling. Inject `__VIRTUES_BACKEND_ORIGIN__` from `reach.loopback_url()` only
   for the pre-paired shell.

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
- **WebView runtime:** Windows needs **WebView2** (evergreen; NSIS bootstraps
  it — a non-issue). Linux needs **`webkit2gtk` 4.1** present at runtime — a real
  distro-version pain; the deb should declare it, the AppImage should bundle or
  document it.
- **Credential store:** the reach `FileStore` (`dirs::data_dir()`) already works
  on Windows/Linux and is the store the in-process path uses — so `keyring` is
  not on the critical path for views. (The sidecar's `keyring` use stays on
  macOS.)
- **Icons:** generate the `.ico` (Windows) and PNG set (Linux) with
  `tauri icon <source.png>` — none exist yet.
- **Updater:** Windows NSIS + `latest.json` + (ideally) Authenticode signing;
  Linux AppImage updater + its own `latest.json`. Acceptable to defer self-update
  to v1 and ship manual/store updates first.
- **CI:** add a Windows job (`tauri build` → NSIS) and a Linux desktop job
  (`tauri build` → AppImage/deb). These are **new** — `release-linux.yml` today
  builds the server, not the app.

## Workstream C — Android (view) — SCOPE

Reuse the mobile `lib.rs` (already the iOS entry point), boot it **reach-only**,
and add the Android shell. The current `lib.rs` registers `reach` + 6 collectors
and calls `resume()` on all 6 in `setup()`; the collectors have no Android Kotlin
halves, so the whole job is: compile+run reach on Android, gate collectors off,
and add the Android-specific storage/manifest bits.

**Android builds run on macOS** (Android SDK/NDK cross-compile — unlike the
Win/Linux Tauri builds), so this is locally validatable *if* the toolchain is
installed. Otherwise a Linux/macOS CI runner with the Android SDK.

### C0 — De-risk FIRST (cheap spike, do before anything else)

**The load-bearing unknown: does `virtues-reach-client` (iroh + QUIC + its dep
tree) cross-compile and *run* on Android?** iroh generally supports Android, but
this is unverified for our exact deps. Spike it before investing:
`cargo build --target aarch64-linux-android -p virtues-reach-client` (needs the
NDK + `cargo-ndk` or the linker configured). If reach doesn't build/run on
Android, the whole Android app is blocked and we learn it in an hour, not a week.

### C1 — Re-gate deps so Android compiles reach-only (code, no toolchain)

- **Cargo.toml:** move the 6 collector plugin deps from
  `cfg(any(ios, android))` to `cfg(target_os = "ios")`; keep `reach` on
  `cfg(any(ios, android))`. (`objc2` is already ios-only.) Android then compiles
  **only reach** — sidesteps "do the collector crates build on Android?" entirely.
- **lib.rs:** gate the 6 collector `.plugin()` registrations **and** their
  `resume()` calls in `setup()` to `#[cfg(target_os = "ios")]` (use the
  conditional-`let builder` pattern for the plugin chain, as `main.rs` does for
  single-instance). Android boots reach + the webview only.
- Verifiable now: iOS/desktop still compile. Android compile needs C0's toolchain.

### C2 — Android storage (reach-plugin code)

The reach `FileStore` uses `dirs::data_dir()` (`virtues_dir()`), which is
unreliable on Android. Resolve `box.json`'s dir from the Tauri app-data dir
(`app.path().app_data_dir()`) on Android — likely refactor `FileStore` to accept
an injected base dir set from the plugin's `setup(app)` (iOS keeps its Keychain
bridge; desktop keeps `dirs::data_dir()`). App-private storage is sandboxed;
Android Keystore hardening is a later follow-up (parallels the iOS Keychain).

### C3 — `tauri android init` + Android project config (needs toolchain)

- `tauri android init` → generates `gen/android` (Gradle project).
- **Cleartext loopback:** the SPA calls `http://127.0.0.1:7117` (the in-process
  reach). Android blocks cleartext by default (API 28+) → add a
  `network_security_config.xml` permitting cleartext to `127.0.0.1`, referenced
  from `AndroidManifest.xml`.
- **Manifest:** `INTERNET` only (no collector permissions for views).
- **Capabilities:** confirm `capabilities/mobile.json` targets `android` for the
  reach commands (`plugin:reach|pair`/`discover`/…).
- **Icons:** `tauri icon` already emits the Android `mipmap` set — reuse.

### C4 — Build + run (needs toolchain)

- `tauri android dev` (emulator/device) → confirm it boots reach-only, loads
  `mobile-pair.html`, pairs by code, and loads the box UI over the loopback.
- `tauri android build` → APK/AAB.

### C5 — Signing + CI

- Android keystore; a `release-android.yml` (APK/AAB). Debug/unsigned for the
  first validation, signed release after.

### Deferred (not this workstream)

- **Collectors** — per-plugin Android Kotlin halves, un-gating C1 as each lands:
  `health`→Health Connect, `location-probe`→FusedLocationProvider,
  `audio`→AudioRecord + typed foreground service, `contacts`→ContactsContract,
  `eventkit`→CalendarContract; **`finance` has no Android equivalent** (iOS-only).
- **Network monitoring** — the reach plugin's iOS `ReachMonitor` (reconnect on
  network change) has no Android half; rely on Rust-side reconnect for v1, add a
  `ConnectivityManager` shim later.
- **`set_appearance`** already has an Android no-op branch — nothing to do.

### What's doable now vs needs the Android toolchain

- **Now (pure code):** C1 (re-gate) + C2 (storage) — leaves iOS/desktop building,
  sets Android up to compile reach-only.
- **Needs SDK/NDK/Gradle (local on macOS or CI):** C0 spike, C3 init/config, C4
  build/run, C5 signing/CI.

---

## Sequencing / status

0. **Phase 0 — retire the macOS proxy sidecar** — ✅ DONE (commit 45e13811,
   branch `phase0-in-process-reach`). `:7117` in-process; compiles on macOS.
1. **A — desktop reach** — ✅ DONE (folded into Phase 0). reach dep widened to all
   desktop targets; pairing shell unchanged.
2. **B — Windows + Linux (views)** — ✅ CODE DONE, ⏳ CI-UNVERIFIED. macOS-only
   code (`tray`/updater/reconcile/collector/`Reopen`/close-to-hide) gated;
   `tauri.windows.conf.json` (nsis) + `tauri.linux.conf.json` (appimage+deb),
   `externalBin` cleared, `createUpdaterArtifacts:false`; `icons/icon.ico`
   generated; `release-windows.yml` + `release-linux-desktop.yml` added. macOS
   `cargo check` still green. **Windows/Linux compile + bundle can only be
   verified in CI** (can't cross-compile Tauri's native webview from macOS) —
   push a `win-edge` / `linux-desktop-edge` tag (or run the workflow) to validate.
   Minor wart: the `tray-icon` feature stays compiled on Linux, so the build
   pulls `libayatana-appindicator3-dev` even though no tray is shown; a future
   cleanup can gate that feature off non-macOS for a leaner binary.
3. **C — Android (view)** — 📋 SCOPED (see Workstream C above). Sequence: C0
   iroh-on-Android spike → C1 re-gate deps reach-only → C2 Android storage → C3
   `tauri android init` + cleartext/manifest → C4 build/run → C5 signing/CI.
   C1/C2 are code-now; C0/C3–C5 need the Android SDK/NDK (local on macOS or CI).

(Lower-risk alternative if you'd rather not touch the shipping app first: run
B and C on in-process, leave macOS on its sidecar, and do Phase 0 last. Costs you
the two-path fork in the interim — see Open decision 4.)

Each collector, later, is additive: Android gets `android/` Kotlin halves +
un-gating; Windows/Linux get a desktop collector story (daemon or plugin halves)
against the same `virtues_enqueue` outbox that already exists.

## Open decisions (need a call before/at implementation)

1. **Tray on Windows/Linux?** The macOS tray exists largely to show *collector*
   status. A view-only app has only box status + updater to show. Recommend
   **shipping windowed-only for v1** (skip the tray, its
   `libayatana-appindicator` dep, and the GNOME no-tray fallback) — add it back
   with the desktop collector.
2. **Self-update now or later?** Per-platform updaters (Windows NSIS + latest.json
   + Authenticode; Linux AppImage-only) are real work. Recommend **deferring to
   manual/download for v1**, keeping the macOS updater as-is.
3. **Pairing shell approach** — (a) desktop-chrome variant of `mobile-pair.html`
   on reach commands (recommended, cheapest) vs (b) sidecar-name wrapper commands
   in `main.rs`. See Workstream A.3.
4. **Phase 0 timing / risk appetite** — retiring the macOS proxy sidecar is the
   right foundation (one reach path, deletes the sidecar/reconcile/launchctl
   machinery) but it modifies the **shipping** macOS app. Decision: do Phase 0
   first (recommended — everything downstream gets simpler and proven), or ship
   Windows/Linux/Android on in-process while leaving macOS on its working sidecar
   and migrate macOS last (lower blast radius, but carries the two-path fork
   longer). ~~macOS *must* keep the sidecar~~ — corrected: it needn't, the
   collector is iroh-direct.
5. **Unverified build assumptions** to settle by actually building, not planning:
   iroh / `virtues-reach-client` on the Android target; `webkit2gtk` 4.1
   availability on the Linux distros you care about.
