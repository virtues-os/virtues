# The app is its own copy — Mac joins the iPhone

**Status:** Built on `wave` 2026-09-29 → 10-02, unreleased, not yet run on
hardware: slice 1 `85a3664a` (floor + OTA integrity) with the `/health`
`api_version` line in `0d59b8e1`, slice 2 `8a632ce7` (one resolver, per-load
pin, hidden reload), slice 3 `e6a60405` (the switch), dev-profile isolation
`70fbf9e3`. Finishes the Mac half of what
[the spa-delivery record](../record/spa-delivery.md) left open and builds the
box floor [version-compat-plan.md](version-compat-plan.md) proposed. When this
ships, delete this plan and the Mac sections of those two, and write the
record.

## The decision

Every native app shows **its own copy** of the SPA and treats the box as a
source of data. The iPhone already works this way. The Mac becomes the same
program: same scheme, same resolver, same updates, same reachability, same
recovery. LAN browsers and the box's own panel keep loading the box-served SPA,
unchanged. Windows and Linux keep today's model; no date yet.

Why: on the Mac today the box is also *where the app comes from*, so every
state where the box is gone needs its own patch (launch probe, `connect.html`,
a baked copy just for Setup and `/reconnect`, `prePair.handOff`, and the
mid-session gap nothing covers). Each patch fixes one moment of one problem:
the app disappearing with its server. Remove the cause and they all go.

## What exists (swept 2026-09-29)

| | iPhone | Mac |
|---|---|---|
| Page origin | `virtues://localhost` (own scheme, `lib.rs:280`) | `http://localhost:7117` when the box answers; `tauri://localhost` (baked) for Setup and `/reconnect` |
| App files from | Overlay bundle (OTA) → baked (`web_bundle.rs`) | The box, through the loopback |
| Data from | Loopback `http://127.0.0.1:7117`, via the rewrite in `backend.ts:73-89` | Same loopback, same origin as the page |
| Box gone | App opens, banner, `/reconnect` | Nothing to show; the shell picks another copy at launch only |
| UI updates | OTA from the box, forward-only, next launch, rollback | Whenever the box updates |
| Shell ↔ UI gate | `minShellVersion`, in `decide()` | None (audit U13) |
| UI ↔ box gate | None | None (the box served it, so they matched) |

The loopback is a raw byte splice onto one iroh stream (`proxy.rs:115-153`),
not HTTP-aware. The box's CORS admits `virtues://` origins
(`server/mod.rs:1149-1154`), and the iPhone proves a custom-scheme page can
call the `http` loopback, fetch and WebSocket both, in production.

## Architecture

### One origin, once: `virtues://localhost`

The Mac registers the iPhone's `virtues://` scheme and opens
`virtues://localhost/` always, paired or not, box up or down. The scheme
handler and `web_bundle` move out of the mobile-only path into one module both
binaries use. The desktop shell injects what the phone's does
(`__VIRTUES_BACKEND_ORIGIN__`, `__VIRTUES_PAIRED__`, `__VIRTUES_MOBILE__ =
false`), so `prePair`, the fetch rewrite and `/reconnect` run one code path.

**Rejected: the loopback serves the app files** (spa-delivery-plan's
candidate). It keeps `http://localhost:7117` and so keeps storage, but turns
the most reliability-critical piece we have, a byte pipe that already needed an
iroh watchdog, into an HTTP server parsing keep-alive, WebSocket upgrades and
260 MB uploads, and leaves the Mac a second architecture.

**Existing Macs start fresh once** (decided 2026-09-29). Browser storage is
per origin, and moving it was the most fragile part of the first draft for the
least value: open tabs, pinned and recent pages, a rare unsent draft. The theme
comes back from the box. The one real setting, the summon shortcut, moves into
the shell's own settings, where a shell setting belongs. The offline document
cache (y-indexeddb) rebuilds from the box, which is authoritative; a Mac page
only ever existed while the box answered, so it never held edits the box
lacks.

Data calls are unchanged. One fix while here: the fetch rewrite omits
`/face/*` and `/oauth/callback`; add both, for both platforms.

### Updates: one rule for both platforms

The box publishes its web build (`/api/web-bundle/version`, `/tarball`,
exist). Both apps pull it, forward-only, refusing a bundle that needs a newer
shell (`minShellVersion`, now on the Mac too).

**A staged bundle applies by reloading the page while the window is hidden**,
on both platforms. The SPA already reloads while hidden when the box's build
changes (`build.ts`); this is the same move. It replaces "next launch," which
suited a phone and never happens on a Mac (closing the window only hides it),
and it gets phones their updates sooner too.

This changes rollback: today the booted bundle is captured once per process
(`capture_booted`, a `OnceLock`) and boot-ok is judged against it. With
in-process reloads that must become **per page load**: the resolver records
which bundle each top-level load was served from, and `bundle_boot_ok` confirms
that one. The pending/booting/rolled-back pointers keep their meaning.

A Mac app release with a newer bake overtakes an older overlay
(`drop_stale_overlay`, exists).

**Integrity:** the phone never re-hashes an unpacked bundle against
`contentHash`, and reads version and tarball in two requests, so a box upgrade
between them can file one build under another's hash. Read the manifest from
inside the tarball, recompute the hash after unpacking, refuse on mismatch.
Signing bundles is its own later plan.

### Versions: one number, one floor

Three version lines stay: box `vX.Y.Z`, the UI's stamped source tag, the
shell's app version. Two integers join them.

1. **Shell ↔ UI: `COMMAND_SURFACE_VERSION`** (exists, 7), now enforced on the
   Mac's OTA as on the phone's.
2. **UI ↔ box: `api_version`** (new). One integer on the box, raised whenever
   it gains an endpoint or behavior a UI might depend on (Tailscale's
   `CapabilityVersion`), served in `/health`. A box that predates it reports
   nothing, which reads as **0**.

The SPA holds one constant, `MIN_BOX_API`, the lowest `api_version` it runs
against. Below it, one screen: "Your server needs an update," with how. It
starts at 0, so no box released before this change is stranded, and it is
raised only deliberately, with a release note. It is a constant in code, not a
manifest field: a bundle pushed from the box always matches that box, so only a
baked copy can be ahead, and that is a runtime question.

Features gate on `api_version` one at a time, as each "a 404 means an older
box" guess is replaced (`gettingStarted`, `visits`, `IntroductionSection`,
`StepSubscription`). No framework on day one.

**The rule that keeps skew safe: the box's API only grows.** Optional new
request fields; response fields added, never removed or retyped; renames keep
an alias. Enforced in review for now; a CI ratchet is its own later plan. This
is what the v0.1.6 `model` incident needed. mac-plan.md's "the UI is
structurally incapable of leading the box" is retired: it was never true on the
phone, and the floor plus this rule replace it.

### Reachability: the phone's check, everywhere

The SPA's `reachability` store loses its phone-only guard, and that is all. It
works on the Mac now because the Mac's page is always alive; the only reason to
detect outages in the shell was that the page could be dead. No new native
event, which would also mean new background probing on the phone's radio. The
Mac gets "Can't reach your server · Fix it" at launch, mid-session and after
sleep. The tray keeps its own probe.

### What gets deleted

- Mac launch routing in `main.rs` (the probe, `External(localhost:7117)`, the
  `setup` and `reconnect` branches): the window opens `virtues://localhost/`.
- `prePair.handOff()` and `bakedDesktop()`: one origin.
- `/reconnect`'s Mac branch of `openApp`.
- `connect.html` on the Mac. (Windows, Linux and Android keep it.)
- On the Mac only, command grants for remote origins: split
  `capabilities/default.json` by platform, since Windows and Linux still load
  the box's pages.

## Edge cases

| Case | Behavior |
|---|---|
| Existing Mac, first launch of the new shell | Fresh storage, theme from the box, summon shortcut kept natively |
| A box older than `api_version` | Reads as 0; runs, since `MIN_BOX_API` starts at 0 |
| Box below the UI's `MIN_BOX_API` | "Your server needs an update," nothing half-broken |
| Box newer than the app's UI | App pulls the box's bundle, applies on the next hidden reload |
| Bad OTA bundle | That page load never confirms; the next load rolls back; the hash is never fetched again |
| Bundle needs a newer shell | Refused; current UI stays until the app updates |
| Sleep at home, wake at work | Banner after the grace period, "Fix it" → `/reconnect` |
| LAN browser, the panel | Unchanged: box-served |
| `make mac-dev` | Loads Vite at its own origin; the resolver and OTA only run in a real build, which testing uses |

## Build slices

0. **WebKit spike — done, 2026-09-29, passed.** A native WKWebView test app
   (default data store, `virtues://` scheme handler, macOS 26) wrote
   localStorage and IndexedDB, then two separate relaunches read both back
   intact; `location.origin` was `virtues://localhost`. A `fetch` from that page
   to `http://127.0.0.1` succeeded with `Origin: virtues://localhost`, the
   loopback pattern the Mac will use. Not tested: WebSocket, which the phone
   already does from the same scheme on the same engine.
1. **Floor and integrity** (a day). `api_version` in `/health`, `MIN_BOX_API`
   and its screen, the manifest read from inside the tarball and re-hashed.
   Helps the phone now.
2. **One resolver, one update rule** (1 to 1.5 days). The scheme handler and
   `web_bundle` shared by both binaries; apply-by-hidden-reload on both;
   rollback per page load; the Mac's `minShellVersion` gate. The Mac still
   opens the old way; this only keeps its own copy current.
3. **The switch** (a day). The Mac opens `virtues://localhost/` always; desktop
   injections; the summon shortcut into shell settings; reachability unguarded;
   the deletions; capabilities split by platform.

## Later, as their own plans

- Signing bundles in CI so a box relays UI it cannot forge.
- The CI ratchet for the grow-only API.
- Windows and Linux adopting this (WebView2 serves custom schemes as
  `http://virtues.localhost`; pick `useHttpsScheme` once and never change it).

## Open

- Should This Mac show the UI version, now that it can differ from the box's?
  The phone's `shell_identity` already carries it.
