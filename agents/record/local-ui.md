# The Mac shows its own copy of the app

**Written 2026-10-20.** Designed and decided 2026-09-29, built on `wave`
2026-09-29 to 10-02, released in the clean-start line (`v0.1.1` and Mac
`mac-v1.0.31`). It finishes the
Mac half of what [spa-delivery.md](spa-delivery.md) left open and builds the
box floor that [version-compat-plan.md](../plan/version-compat-plan.md)
proposed.

## What was decided

Every native app shows its own copy of the SPA and treats the box as a source
of data. The iPhone already worked this way. The Mac now uses the same scheme,
resolver, update rule, reachability check and recovery screens. LAN browsers
and the box's own panel still load the box-served SPA. Windows and Linux keep
the old model; there is no date for moving them.

Before this, the Mac loaded the app from the box, so every state in which the
box was gone needed its own patch: a launch probe, `connect.html`, a baked copy
only for Setup and `/reconnect`, `prePair.handOff`, and nothing at all for an
outage mid-session. Closing the window only hides it on a Mac, so launch
routing almost never ran. Making the Mac hold its own copy removed the reason
for all of those patches.

## What was built

| Slice | Commit (pre-clean-start) | What |
|---|---|---|
| 0 | spike, 2026-09-29 | A native WKWebView test app with a `virtues://` scheme handler on macOS 26 kept localStorage and IndexedDB across two relaunches; `location.origin` was `virtues://localhost`; a `fetch` to `http://127.0.0.1` succeeded with `Origin: virtues://localhost`. WebSocket was not tested (the phone already does it from the same scheme on the same engine). |
| 1 | `85a3664a`, `0d59b8e1` | The box floor (`api_version` in `/health`, `MIN_BOX_API` and the "Your server needs an update" screen) and OTA integrity (the manifest read from inside the tarball, the unpacked bundle re-hashed). |
| 2 | `8a632ce7` | One resolver and `web_bundle` shared by both binaries; staged bundles apply by a reload while the window is hidden; rollback tracked per page load; the Mac's `minShellVersion` gate. |
| 3 | `e6a60405` | The Mac always opens `virtues://localhost/`; desktop injections; the summon shortcut moved into shell settings; the reachability store lost its phone-only guard; capabilities split by platform (`capabilities/mac.json` has no `remote` block). |
| — | `70fbf9e3` | Dev-profile isolation: a profile's OTA bundles live under `app_data/profiles/<name>`, and a profiled window gets its own WebKit data store. |

### One origin

The Mac registers the iPhone's `virtues://` scheme and opens
`virtues://localhost/` whether or not it is paired and whether or not the box
is up. The desktop shell injects the same globals the phone does
(`__VIRTUES_BACKEND_ORIGIN__`, `__VIRTUES_PAIRED__`, `__VIRTUES_MOBILE__ =
false`), so `prePair`, the fetch rewrite and `/reconnect` run one code path.
Data calls go over the same loopback as before. The fetch rewrite also gained
`/face/*` and `/oauth/callback` on both platforms.

Rejected: having the loopback serve the app files. That would have kept
`http://localhost:7117` and its storage, but it would have turned the loopback,
a raw byte splice onto one iroh stream that already needed a watchdog, into an
HTTP server parsing keep-alive, WebSocket upgrades and 260 MB uploads, and it
would have left the Mac on a second architecture.

Existing Macs started with fresh browser storage once (decided 2026-09-29).
Storage is per origin, and migrating it was the most fragile part of the first
draft for little value: open tabs, pinned and recent pages, a rare unsent
draft. The theme comes back from the box, the summon shortcut moved into the
shell, and the offline document cache (y-indexeddb) rebuilds from the box.

### Updates

Both apps pull the box's web build (`/api/web-bundle/version`, `/tarball`),
forward only, and refuse a bundle that needs a newer shell. A staged bundle
applies by reloading the page while the window is hidden, on both platforms,
because "next launch" never happens on a Mac. With in-process reloads the
booted bundle can no longer be captured once per process (`capture_booted`
was a `OnceLock`), so the resolver records which bundle served each top-level
load and `bundle_boot_ok` confirms that one. A Mac release with a newer bake
overtakes an older overlay (`drop_stale_overlay`).

### Versions

Three version lines remain: the box's `vX.Y.Z`, the UI's stamped source tag,
and the shell's app version. Two integers gate them:

1. Shell to UI: `COMMAND_SURFACE_VERSION`, now enforced on the Mac's OTA as on
   the phone's.
2. UI to box: `api_version`, an integer the box serves in `/health`
   (`virtues-core/src/api_version.rs`) and raises when it gains something a UI
   might depend on. A box that predates it reports nothing, which counts as 0.
   The SPA's floor is `MIN_BOX_API` (`apps/web/src/lib/boxApi.ts`); below it
   the app shows one screen telling the owner to update the server. The floor
   is a constant in code because a bundle pushed from a box always matches
   that box; only a baked copy can be ahead of it.

The rule that keeps version skew safe is that the box's API only grows:
new request fields are optional, response fields are added and never removed
or retyped, and renames keep an alias. It is enforced in review; the CI
ratchet is in [version-compat-plan.md](../plan/version-compat-plan.md).

### Deleted

Mac launch routing in `main.rs` (the probe, `External(localhost:7117)`, the
`setup` and `reconnect` branches), `prePair.handOff()`, `bakedDesktop()`, the
Mac branch of `/reconnect`'s `openApp`, `connect.html` on the Mac (Windows,
Linux and Android keep it), and on the Mac only, command grants for remote
origins.

## Edge cases as designed

| Case | Behavior |
|---|---|
| Existing Mac, first launch of the new shell | Fresh storage, theme from the box, summon shortcut kept natively |
| A box older than `api_version` | Reads as 0 |
| Box below the UI's `MIN_BOX_API` | "Your server needs an update" |
| Box newer than the app's UI | The app pulls the box's bundle and applies it on the next hidden reload |
| Bad OTA bundle | It never confirms; the watchdog reloads it after 20 s, the second unconfirmed load is its last, the next load rolls back |
| Bundle needs a newer shell | Refused; the current UI stays until the app updates |
| Sleep at home, wake at work | "Can't reach your server" in the sidebar footer after the grace period; clicking it opens `/reconnect` |
| LAN browser, the panel | Unchanged: box-served |
| `make mac-dev` | Loads Vite at its own origin; the resolver and OTA run only in a real build |

## Traps

- The navigation guard (`is_own_page`) must list `virtues://`.
- `make mac-dev` with Bluetooth aborts on TCC (the binary is unbundled).

## Left open

- Whether This Mac should show the UI version now that it can differ from the
  box's. The phone's `shell_identity` already carries it.
- Signing bundles in CI so a box relays UI it cannot forge.
- Windows and Linux adopting this. WebView2 serves custom schemes as
  `http://virtues.localhost`; pick `useHttpsScheme` once and never change it.
