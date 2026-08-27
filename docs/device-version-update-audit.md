# Devices, versions, and updatability — audit (2026-08-27)

Four parallel investigations: the Devices page data model, the version-identity
inventory, every update mechanism, and the map of user-facing status surfaces.
This records what is true today, with the defects numbered for fixing. The
proposal at the end is the consolidation argument.

## The one-line diagnosis

Every surface renders a *different* truth honestly — nothing is lying, and
nothing is authoritative. And every update mechanism except the box's fails
**silently** in its most likely failure mode.

## Why the Devices page reads the way it does

The screenshot that started this: "Virtues Desktop (This device)" showing
`0.1.5-staging.65 · staging`, `phf-virtues.local` showing "version unknown",
"Local console" showing "Never".

1. **The Version column answers "which UI bundle does this device's traffic
   come from" — never "which app binary."** The only writer is
   `record_client_build` (`middleware/auth.rs:230`), fed by the
   `X-Virtues-Client` header, and the only *sender* of that header is the SPA's
   fetch monkey-patch (`apps/web/src/lib/build.ts:49`). No native client — not
   the Tauri shell, not the iOS plugin, not the collector — sends it.
2. **The desktop row shows the box's version because the paired desktop window
   loads the box-served SPA** (`WebviewUrl::External(localhost:7117)`,
   `main.rs:1583`), and that SPA's baked build identity is byte-for-byte the
   box's release identity (same `GIT_DESCRIBE`, same tarball,
   `release-linux.yml:277-279`). A hall of mirrors, not a bug in the column.
3. **The collector row is "version unknown" permanently.** The collector HAS a
   CI-stamped version (`apps/mac-source/Sources/Version.swift`,
   `release-mac.yml:172-183`) but `Version.userAgent` has **zero call sites**;
   its uploads carry only `Content-Type` and its pair payload has no build
   field. The identity exists and is transmitted to nobody.
4. **"Local console: Never · from 127.0.0.1" is fiction.** The row is inserted
   at server startup with a hardcoded `'127.0.0.1'` literal
   (`auth.rs:249-258`), and the loopback auth branch never bumps
   `last_seen_at` — no amount of CLI use changes it. The same branch discards
   `X-Virtues-Client`, so the box's own screen also reports nothing.
5. **One Mac is two rows by construction**: the Tauri app ("Virtues Desktop",
   hardcoded name, `reach/src/lib.rs:663`) and the collector daemon
   (`hostName`, `Config.swift:266-277`) are two separate pairings with two iroh
   keys. The box has no "machine" joining them — which is why permissions live
   only on the collector row and version only on the app row
   (acknowledged gap: `devices/shared.ts:145-160`).
6. Dead key: desktop pairing writes `device_info.version` =
   the reach *plugin crate* version, a never-bumped `0.1.0`
   (`reach/src/lib.rs:669`) — written, never read. The real app version
   (`ShellIdentity.app_version`) is one IPC call away and never sent.

## Version identities — the inventory (condensed)

Seventeen distinct identities exist. The ones that matter for legibility:

| Identity | Defined | Shown to a human | Reported to the box |
|---|---|---|---|
| Box release (`v0.1.5-staging.65`) | baked `GIT_DESCRIBE` (`codename.rs`) | `--version`, Software → Box, tray version line | is the box |
| Box build channel (`stable/staging/edge/dev`) | derived from the tag string, in **three** places (codename.rs, build.ts, write-bundle-manifest.mjs) | Software, tray suffix | `/health` |
| Box update *preference* (`stable/prerelease`) | `/var/lib/virtues/channel` | Software channel select ("Main/Nightly") | `/api/system/update` |
| SPA bundle (version·sha·channel) | Vite defines | Software → Interface | `X-Virtues-Client` → `device_info.build` — **the Devices column** |
| Mac app (`1.0.23`) | tauri.conf.json | Software → App row ONLY (shell-only render) | **never** |
| iOS app (`1.2.7`) | tauri.ios.conf.json | iOS This-device About | **never** (the phone's row shows the SPA's identity) |
| Collector | Version.swift, CI-stamped | `virtues-collector --version` ONLY | **never** |
| Command surface (`2`) | lib.rs const | Software App row `· surface 2` | gates OTA (mobile only) |

Notable rot: `min_ios_version` published in `/health` with zero consumers;
`app_device.named_at` read but never written (the "owner named a device"
onboarding tier is permanently false); desktop shell's Cargo.toml says `1.0.0`
(inert decoy — Tauri reads the conf); `virtues device ls` doesn't select the
version data sitting in the same row.

## Update mechanisms — confirmed defects

Full detail lives in the sweep transcripts; numbered here for fixing.

### Mac app self-update (`main.rs`)

- **U1 (critical).** A box on the literal `edge` tag routes the updater to
  `mac-edge/latest.json`, which **does not exist** — `updater.check()` errors
  are collapsed into `_ => return` (`main.rs:817-820`): that Mac never updates
  again, with zero log, zero notification.
- **U2 (high).** Channel vocabulary mismatch: the updater matches
  `"prerelease"|"pre"|"edge"|"nightly"` (`main.rs:782-791`) but `/health` can
  only say `stable|staging|edge|dev`. `"staging"` — the entire real prerelease
  line — falls through to stable. Box-owns-channel never engages. (Today this
  is accidentally benign: mac-latest is the only maintained channel.)
- **U3 (high).** The failure counter counts only *download* failures; check
  failures (404, DNS, TLS, bad signature) are invisible to it, so the "could
  not update itself" notification structurally cannot fire for U1.
- **U4 (high).** Manual "Check for Updates…" shows **"Up to date ✓" on a
  failed check** (`main.rs:1196-1223` reads only `ready.is_some()`). The
  box-side UI explicitly refuses this lie (`UpdateSection.svelte:249`); the
  tray commits it.
- **U5 (medium).** Edge mac releases are structurally unpublishable: edge takes
  its version from the same tauri.conf.json as stable and the workflow requires
  strictly-greater — the channel is dead by design, not neglect.
- **U6.** 6h poll with no jitter, no wake/network-up/focus recheck.
- **U7.** Windows/Linux: no tray, no self-update, updater plugin still
  registered, no surface saying the binary is frozen while its box-served UI
  advances.

### Collector reconcile (`main.rs:1272-1355`)

- **U8 (medium-high).** Copy-then-kickstart, kickstart result discarded: a
  failed kickstart is *unrecoverable* — the byte-compare now says "in sync" so
  every later launch no-ops while the old process runs from its held inode
  until logout.
- **U9.** Reconcile is launch-only and the app is designed never to relaunch
  (close = hide; staged updates framed as "installs on next launch"). Weeks can
  pass with a staged app update and a stale collector.

### SPA OTA overlay (`web_bundle.rs`, mobile)

- **U10 (high).** `mark_boot_ok` clears PENDING without knowing *which* bundle
  booted; it races the launch-time apply. Either rollback protection is
  silently destroyed or a good bundle is rolled back and re-downloaded in a
  loop.
- **U11.** A bundle that fails to boot is re-downloaded forever — no poison
  list.
- **U12 (medium-high).** The check flips `PTR_ACTIVE` mid-session despite its
  own "never swaps the running session" contract; un-fetched lazy chunks then
  404 on navigation.
- **U13.** Desktop has **no OTA path and no `ShellTooOld` gate at all** — the
  box-served SPA is loaded unconditionally against whatever shell is installed;
  `bundle_boot_ok` on desktop writes into a store nothing populates.
- **U14.** `shellSupports()` — the documented per-feature degradation gate —
  has zero call sites.
- **U15.** No shipped bundle has ever raised `minShellVersion` above 1 while
  the surface is 2, so the entire shell-too-old path (and its "update from the
  App Store" copy) is unexercised.

### Box upgrade

- **U16 (high).** `/health` is not in the mobile fetch-proxy prefix list, so on
  iOS it resolves to the OTA scheme handler and returns `index.html` with
  HTTP 200: the post-upgrade watcher never sees the box go down and reports
  **every successful phone-initiated upgrade as a 10-minute failure**; the
  Software page's Box row renders `—` on every iPhone for the same reason.
- **U17.** `UpdateStatus.current` is the frozen workspace `0.1.0`; on GitHub
  rate-limit the fallback comparison says "update available" on a box already
  on that tag.
- **U18 (medium-high).** No reload/cache-bust signal to any connected client
  after a box upgrade — only the tab that pressed the button reloads; every
  other browser, the Mac webview, and phones keep a page whose content-hashed
  chunks no longer exist (the kiosk restart fix covers only the kiosk).
- **U19.** The downgrade guard is bypassed on any box whose version string
  isn't semver (`edge`).

The load-bearing pattern: **the box's update path is the only one that
distinguishes "couldn't check" from "up to date." The mac path, the OTA
confirm, and the collector reconcile all report success or say nothing.**

## The surfaces (nine of them)

Tray · Devices · Software · System · This Mac (`/virtues/devices/this` —
already exists, with collector card, permissions, pause/resume, Turn on this
Mac) · iOS This device · Device detail · Sources · Display mirror.

Vocabulary schisms: **four** vocabularies for channel, **three** for collector
state, **four** for sync, **three** for box reachability, and "Up to date"
means the mac app in the tray but the box in Software — both can be on screen,
correctly disagreeing.

**Only-in-tray capabilities** (what would be orphaned by deleting it): apply a
staged mac-app update; manual update check; knowing an update is staged at all
(nothing in the SPA reads `UpdateState`); box-unreachable status while the
window is closed (the SPA can't report on the box because the box serves it);
not-installed vs stopped collector distinction; quit (close only hides);
re-reveal the hidden window; the sole push notification.

## Proposal

Three principles, then slices.

1. **Report versions to the box; read them from the box.** Native shells and
   the collector each stamp their own identity onto traffic they already send;
   the box records it; Devices/Software render it. Remote legibility — "what is
   my cofounder's Mac running" — falls out.
2. **One machine, one row.** Introduce the machine grouping (or fold the
   collector under the app's device) so version, permissions, and last-seen
   stop being sharded across two rows nobody can join by eye.
3. **A check that fails must say so.** Every mechanism gets the box path's
   couldn't-check/up-to-date distinction.

### Slice 1 — truth (small, high yield)

- Shell adds `app=<version>` to the client header (or its own
  `X-Virtues-Shell`); collector sends its already-stamped `Version.userAgent`
  on uploads and pair; box records both (and on the loopback branch too, or
  drop the console row's fiction: kill the fake IP, hide "last seen" for it).
- Devices column splits into what it actually knows: App / UI / Collector.
- Delete the dead `device_info.version` write; select version in
  `virtues device ls`.

### Slice 2 — update honesty

- U1–U4: surface check failures (counter + tray line "couldn't check"),
  route `edge` to mac-latest until mac-edge really exists, match `"staging"`.
- U8: verify the kickstarted process (or compare against a recorded hash, not
  the copied file).
- U10: `mark_boot_ok(hash)` — confirm the bundle that actually booted.
- U16: add `/health` to the mobile proxy prefixes.
- U18: version-stamp the SPA's API responses (server already knows its build)
  and have the SPA soft-reload on mismatch — closes kiosk-style staleness for
  every surface at once.

### Slice 3 — surface diet

- Tray shrinks to presence: status dot + one status line (door to the app),
  "Show Virtues", the staged-update line, Quit. Pause/resume, last-sync
  detail, check-now move to This Mac / Software — which requires first
  mirroring updater state over IPC (`shell_identity` already crosses that
  bridge) so Software can show "Mac app 1.0.22 → 1.0.23 staged · Restart to
  apply" and offer Check now.
- One vocabulary per fact: channel (pick the baked words; the preference
  select maps onto them), collector state, sync. The overlap matrix in the
  sweep is the checklist.
- Wire or delete: `min_ios_version`, `shellSupports`, `named_at`, desktop
  `bundle_boot_ok`.

The tray survives — slimmer — because three of its jobs cannot move into a
webview the box serves: box-unreachable when the window is closed, quit, and
applying a staged update.
