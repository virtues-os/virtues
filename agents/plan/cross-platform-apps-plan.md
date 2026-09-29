# Cross-platform apps — Android (views), and what is left on Windows/Linux

Ship the Virtues **viewer** app on Android, and finish the Windows/Linux
viewers. Collectors are **out of scope** here; the seams stay in place so the
Kotlin/native collector halves can land later without rework.

**Where it stands.** Every desktop target serves `:7117` in-process through the
`reach` plugin (the macOS proxy sidecar is retired; `virtues-collector` stays).
Windows and Linux viewers build in CI (`release-windows.yml`,
`release-linux-desktop.yml`) and have one release each, `win-edge` and
`linux-desktop-edge`, both from 2026-07-20. **That single build predates Setup
as one flow** (2026-09-25), so neither has been run against the current
pairing/setup path; rebuild and walk it before pointing anyone at them.

Android: collectors are gated to iOS in `src-tauri/Cargo.toml` and `lib.rs`,
the capabilities file is split (`capabilities/android.json`), and the reach
plugin takes its storage base from `app_data_dir()` on Android. What remains
needs the Android toolchain.

## Android (view)

Reuse the mobile `lib.rs` (the iOS entry point), which on Android boots
**reach + the webview only**.

**Android builds run on macOS** (SDK/NDK cross-compile — unlike the Win/Linux
Tauri builds), so this is locally validatable once the toolchain is installed.

### C0 — De-risk FIRST (the go/no-go)

**The load-bearing unknown: does `virtues-reach-client` (iroh + QUIC + its dep
tree) cross-compile and *run* on Android?** iroh generally supports Android, but
this is unverified for our exact deps:
`cargo ndk -t arm64-v8a build -p virtues-reach-client`, then push a smoke bin to
the device over `adb` — compiling does **not** prove QUIC works under Android's
network stack. If reach does not run on Android, the whole Android app is
blocked, and we learn it in an hour.

Toolchain, in order: `brew install --cask android-studio` (its bundled JBR is
the JDK), SDK Manager → Platform-Tools + Build-Tools + Command-line Tools + NDK
(Side by side) + API 35, then `JAVA_HOME` / `ANDROID_HOME` / `NDK_HOME` per
Tauri's prerequisites, `rustup target add aarch64-linux-android`,
`cargo install cargo-ndk`. Gate: `adb devices` sees the phone and
`pnpm tauri info` finds the SDK/NDK.

### C3 — `tauri android init` + project config

- `tauri android init` → generates `gen/android` (Gradle project). Track it, as
  `gen/apple` is tracked.
- **Cleartext loopback:** the SPA calls `http://127.0.0.1:7117` (the in-process
  reach). Android blocks cleartext by default (API 28+) → add a
  `network_security_config.xml` permitting cleartext to `127.0.0.1`, referenced
  from `AndroidManifest.xml`.
- **Manifest:** `INTERNET` only (no collector permissions for views).
- **`tauri.android.conf.json`:** mirror `tauri.ios.conf.json` — same
  `beforeBuildCommand`, `frontendDist: "../build"`, `externalBin: []`, mobile
  CSP. Identifier stays `com.virtues.app`.
- **Icons:** `tauri icon` already emits the Android `mipmap` set — reuse.

### C4 — Build + run

- `tauri android dev --target aarch64` (device) → confirm it boots reach-only,
  loads the connect shell, pairs, and loads the box UI over the loopback. Then a
  background/foreground cycle.
- `tauri android build --apk --target aarch64` → sideloadable APK.

### C5 — Signing + CI

- **ABI: `arm64-v8a` only** (decided). The app carries native Rust
  (iroh/QUIC/bundled SQLite), so each extra ABI is a full extra compile.
  `armeabi-v7a` and `x86_64` are a one-line matrix addition if they come up; an
  arm64 emulator runs natively on Apple Silicon.
- **Distribution: sideloaded APK first**, Play Store later. Emit `--apk` now;
  add `--aab` when Play is live — Play requires AAB + Play App Signing, where
  Google holds the real key and the keystore below becomes the *upload* key.
- `release-android.yml` on `ubuntu-latest`, tags `android-v*` + rolling
  `android-edge` (mirrors `release-mac.yml`): setup-java 17 → setup-android →
  `rustup target add aarch64-linux-android` → rust-cache → `pnpm install` →
  `tauri android build --apk --target aarch64` → sign → gh-release. Ship an
  unsigned debug APK first to prove Android compiles reach in CI, then sign.
- Keystore + four repo secrets (`ANDROID_KEYSTORE_BASE64`,
  `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`) are
  an **owner action** — the key must be generated and stored by a human.

### Known debt — the ingest action name

`init_outbox` passes `"ios_ingest"` as the ingest key on every platform. It names
a box-side action whose concrete id the box hands back at pairing; the device
POSTs to `/webhook/{action_id}` (`plugins/reach/src/upload.rs`). Left alone
because Android is a viewer (nothing enqueues), and a client-side
`"android_ingest"` without a box action would still post to the iOS action. The
right shape is one platform-neutral `mobile_ingest`, but the rename breaks
already-paired iPhones (template GC deletes renamed rows). Do it when the first
Android collector lands, with an alias window.

### Deferred

- **Collectors** — per-plugin Kotlin halves, un-gating each as it lands:
  `health`→Health Connect, `location-probe`→FusedLocationProvider,
  `audio`→AudioRecord + typed foreground service, `contacts`→ContactsContract,
  `eventkit`→CalendarContract; **`finance` has no Android equivalent**.
- **Network monitoring** — the iOS `ReachMonitor` has no Android half; rely on
  Rust-side reconnect for v1, add a `ConnectivityManager` shim later.
- **Keystore hardening** of the reach store (parallels the iOS Keychain).

## Windows / Linux leftovers

- **Linux tray wart.** The `tray-icon` feature is on for every desktop target
  (`src-tauri/Cargo.toml`), so the Linux build pulls
  `libayatana-appindicator3-dev` even though no tray is shown. Gate the feature
  to macOS for a leaner binary.
- **Rebuild against Setup-as-one-flow** (see "Where it stands").

## Open decisions

1. **Tray on Windows/Linux?** The macOS tray exists largely to show *collector*
   status. Recommend **windowed-only for v1** — add it back with the desktop
   collector.
2. **Self-update now or later?** Per-platform updaters (Windows NSIS +
   `latest.json` + Authenticode; Linux AppImage-only) are real work; the builds
   ship with `createUpdaterArtifacts: false`. Recommend **manual download for
   v1**.
3. **Unverified build assumptions** to settle by building: iroh /
   `virtues-reach-client` on Android (C0); `webkit2gtk` 4.1 availability on the
   Linux distros that matter.
