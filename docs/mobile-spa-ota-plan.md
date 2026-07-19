# Mobile SPA OTA updates — plan (not started)

Status: **parked** (2026-07-13). Ship when mobile UI churn outpaces app-store releases.

## Premise

The iOS/Android app is the **bundled SPA** (Option A — see `src-tauri/src/lib.rs`);
today updating the UI means shipping a new app build. But the box already carries
the newest web build (it serves the SPA on-disk for desktop), so the box can be
the phone's update server too: pull new SPA bundles **over the iroh loopback** —
no CDN, no cloud, consistent with the no-cloud doctrine.

Apple-compliant: the "no downloaded executable code" rule carves out JS/HTML/CSS
executed by WebKit (same basis as Expo Updates / CodePush / Capacitor Live
Updates). Native code (plugins, shell) still rides the App Store only.

## Design

**Two-layer asset resolver.** Register a custom URI scheme handler
(`register_uri_scheme_protocol`) that serves each request from
`<app-data>/web-bundles/active/` if present, else falls back to the baked-in
bundle. Bundled assets are read-only; we never touch them.

**Update flow.** On connect over the loopback: compare box's web-build version
(new box endpoint, e.g. `GET /api/web-bundle/version`) to the local active one →
download tarball over the same private link → unpack to
`web-bundles/<version>/` → atomically flip the `active` pointer → reload
webview. Offline = boot last-synced copy, unchanged behavior.

**Box side.** Version + tarball endpoints over the existing on-disk web build.
Box updates then propagate the UI to every paired phone automatically.

## Safety rules (non-negotiable)

1. **API compatibility gating.** OTA'd JS calling a plugin command the installed
   binary lacks = broken app. Shell exposes its API/build version; each SPA
   bundle declares `minShellVersion`; incompatible bundles are not applied.
   Plugin/native changes always ride app-store releases.
2. **Boot shell stays native-bundled.** `mobile-pair.html` + the initialization
   script are never OTA'd — that's the recovery surface if a bundle is bad.
3. **Rollback on failed boot.** Keep the previous bundle; new bundle must emit a
   "boot ok" beacon within a few seconds or the shell flips back (same spirit as
   the box's `virtues.bak` swap).

## Bonus

Dev loop: push a dev SPA build to the phone over the box link instead of
`tauri ios dev`'s Mac-hosted server (which requires phone + Mac on the same
network — bit us 2026-07-13).

## Work items

- [ ] Shell: custom-protocol resolver with active-dir override + fallback
- [ ] Shell: download/verify(checksum)/unpack/flip/rollback state machine
- [ ] Shell: API-version export + `minShellVersion` gate
- [ ] Box: web-bundle version + tarball endpoints
- [ ] SPA build: emit version + minShellVersion manifest in `apps/web` build
- [ ] Boot-ok beacon + rollback timer
- [ ] (later) Android parity pass
