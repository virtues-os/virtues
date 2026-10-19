# SPA delivery: over-the-air UI for the phone

Written 2026-09-29, corrected 2026-10-05. Built August to September 2026. The
mobile app runs a UI bundle baked into its binary and is built to upgrade it
over the air from the box it is paired with, forward only, with rollback on a
failed boot. **No device ever completed that loop before 2026-10-05**: see
"It never worked end to end" below. The Mac half (the Mac taking its own copy
of the SPA) is built on `wave`, unreleased, and owned by
`agents/plan/local-ui-plan.md`.

The code:

| Piece | Where |
|---|---|
| Box: version + tarball endpoints | `virtues-core/src/api/web_bundle.rs` |
| Client: store, gate, flip, rollback | `apps/web/src-tauri/src/web_bundle.rs` |
| The `virtues://` resolver, the OTA check | `apps/web/src-tauri/src/lib.rs` (mobile entry) |
| The shell's command contract | `COMMAND_SURFACE_VERSION`, `apps/web/src-tauri/src/lib.rs` |
| The bundle's self-description | `apps/web/scripts/write-bundle-manifest.mjs`, `apps/web/bundle-contract.json` |
| Boot-ok beacon | `reportBootOk` in `apps/web/src/lib/tauri/bridge.ts` → `bundle_boot_ok` → `mark_boot_ok` |

## Why

On mobile the baked bundle can only change through an App Store release, so a
UI fix that is ready today waits days. The box already has the newer build.
OTA is the path from one to the other, over the link the app already has to
the box: no CDN, no cloud.

The box is the only source because it is the only source guaranteed to carry
the API the bundle it serves was built against.

## The premise that died

The design began from a stronger claim: if the box is the only place a bundle
can come from, the UI can never be ahead of the box, and a class of bug
becomes impossible. The motivating case was a phone build calling a day
endpoint with a `tz` parameter the box had no handler for. The box ignored the
unknown parameter, and the phone showed the wrong midnight with no error.

That invariant was retired on 2026-09-14, knowingly. A phone updates on
Apple's cadence and a box when its owner runs `virtues upgrade`, so a shell
newer than its box is the ordinary state. "Take whatever the box serves" then
meant a fresh App Store build OTAs itself *backwards* on first launch, and
since the airlock (`connect.html`) is baked into the binary while the rest of
the UI rides the bundle, a downgrade splits one launch into two halves that
were never tested together. So the bundle only moves forward: an offer must
be provably newer than anything the device can already serve, and an App
Store update that overtakes the overlay puts the device back on its baked
build. The client runs whichever of {baked, box-served} is newer, and the
UI-ahead-of-box class is possible again. The structural answer, if it
returns, is a bundle declaring a minimum *box* version, mirroring
`minShellVersion`. Nothing enforces that today.

## The safety rules

1. **Command-surface gating.** A bundle's JavaScript calls Tauri commands
   compiled into a separately versioned binary. Each bundle declares
   `minShellVersion`; the shell exports `COMMAND_SURFACE_VERSION`; a bundle
   needing more than the shell has is never applied. The same constant backs
   `shellSupports()` in `bridge.ts`, which features use to degrade
   deliberately instead of failing inside an unknown command. This defect was
   live before OTA existed: the Mac already rendered box-served JS against its
   own binary with no negotiation. `minShellVersion` is 9: no shell below
   surface 9 can run a box bundle at all (it served new chunks as
   `text/html`), so those shells keep the UI they shipped with.
2. **The recovery surface is never OTA'd.** The resolver serves the airlock
   pages from the binary unconditionally, checked before the overlay rather
   than as its fallback. Learned on 2026-08-11, when a stale pair page left
   in the SPA build output shadowed the compiled copy and a day of fixes never
   reached the phone.
3. **Rollback on failed boot.** A flipped bundle stays pending until a page
   loaded from it reports boot-ok, which the SPA sends from
   `hooks.client.ts` `init`, once its code runs and before any page data
   loads. Each page load serving a pending bundle is an attempt; after
   `BOOT_ATTEMPTS` (2) unconfirmed attempts it is abandoned and the pointer
   reverts. On the desktop a watchdog reloads a visible page that has not
   confirmed within 20 seconds, which is the next attempt. A rolled-back hash
   and a download refused after unpacking are both recorded, so neither is
   fetched again until the box offers a different build.
4. **Fail-safe resolution.** Every lookup answers "use the baked bundle"
   unless an overlay is provably good: pointer present, directory there,
   `index.html` inside, manifest parses. Corrupt state is the default path,
   not an error path. The worst case is a white screen for up to two
   watchdog periods on the Mac (on a phone, until the app is next opened),
   then the version the app shipped with.

Identity is the manifest's `contentHash`, computed over the tree; there is no
separate checksum step. A bundle is staged in the background and takes effect
at the next launch, and every outcome is recorded, because a shell silently
refusing every bundle otherwise looks exactly like OTA never having been set
up.

## The origin problem

Found 2026-08-05: web storage is partitioned by origin. The Mac served the UI
from `http://localhost:7117` when the box answered and from `tauri://localhost`
(the baked build) when it did not, so an offline fallback booted against an
empty IndexedDB and showed none of the documents that were its reason to
exist. It was reverted the same day, and the comment at the Mac's launch
branch in `main.rs` says not to re-add it alone.

The same trap applied to OTA: serving an overlay from a new scheme adds a
third origin and would silently empty local state on the first apply. Mobile
therefore moved once, before OTA shipped, from Tauri's `tauri://localhost` to
its own `virtues://localhost` (Tauri gives no hook to intercept its own
scheme). That cost one IndexedDB reset, which was a cache: pages persist on
the box and re-sync. From then on the origin never moves, so applying a
bundle cannot cost local state. **The rule: one origin across every state a
client can be in.** `local-ui-plan.md` applies it to the Mac.

## Apple's position

App Store rules on downloaded executable code carve out JavaScript, HTML and
CSS run by WebKit, the same basis as Expo Updates, CodePush and Capacitor Live
Updates. Native code still ships only through the store. The same reasoning
rules out the phone simply pointing a webview at the box the way the Mac did:
a remote-URL wrapper invites the minimum-functionality guideline, pays a cold
load over the loopback on cellular, and makes a collector's UI hostage to
connectivity.

## What was built, against the original checklist

- Box version and tarball endpoints over the web build it already serves.
- The `virtues://` resolver: airlock from the binary, then overlay, then baked.
- Download, unpack, flip, prune, and the forward-only gate.
- `COMMAND_SURFACE_VERSION` and the `minShellVersion` gate.
- The build-time manifest, stamped after vite from `bundle-contract.json`.
  The bundle describes itself rather than borrowing the box's version, so a
  client never displays a version it has not loaded.
- The boot-ok beacon and rollback.

Owned by `local-ui-plan.md`: the Mac adopting the resolver (built on `wave`,
unreleased) and the offline copy pass. The mobile path is compiled for
`cfg(mobile)`, so it covers Android builds too; everything above was written
against iOS.

## It never worked end to end

Found 2026-10-05, when a Mac running the own-copy build stayed on its baked UI
through a box upgrade. Three defects, each enough on its own:

1. **The box build had no `index.html`.** `pnpm build` writes only SvelteKit's
   `200.html`; the iOS and Mac bakes each copied it to `index.html` for their
   own copy, the box build never did. `is_usable` requires both, so every
   device threw away every bundle any box offered, and recorded it as
   `no_bundle_on_box`, the same state a headless box reports. Fixed in the
   build (`scripts/index-html.mjs`, 718ca34e).
2. **An update's code was served as `text/html`.** `serve_ui` gave an overlay
   file the Content-Type of the baked file at the same path, defaulting to
   `text/html`. A newer build's chunks have new hashed names and no baked
   twin, so WebKit refused them as module scripts and the bundle booted white
   (025f06dd). Every iOS build through 1.2.19 has this, which is why box
   bundles now require surface 9.
3. **Any extension-less request counted as a page load**, re-pinning a running
   page to a just-applied bundle (d9b7c472).

Nobody noticed because the `src-tauri` tests never ran in CI, the test
fixtures wrote `index.html` by hand, nothing fed a real `pnpm build` through
the box's tarball and the device's apply, and the one real-build check
(85a3664a) verified the hash of a build that had no `index.html`. CI now runs
`real_box_bundle_applies` over a real build packed by the box's own code.

The same audit fixed the rollback rules described above (attempts, watchdog,
the boot-ok signal moved before page data loads), the shell's outcome
reporting, the SPA's routing of absolute box URLs, and a race that could
disarm rollback (57444fe6, 940052aa).
