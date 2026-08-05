# SPA delivery — plan (not started)

Status: **ready to schedule** (2026-08-05). Supersedes `mobile-spa-ota-plan.md`
(parked 2026-07-13), which scoped this to mobile only.

How the web UI reaches every client — phone, Mac, and whatever comes next — and
what remains true when the box cannot be reached.

## Why this changed from "nice to have"

The parked plan justified itself on convenience: ship when mobile UI churn
outpaces app-store releases. Two better reasons have since appeared, and both
are about correctness.

**1. Offline editing is already built and currently unreachable.**
`lib/yjs/document.ts` wires `IndexeddbPersistence` next to the websocket
provider, loads from IndexedDB first, and explicitly permits editing when the
connection fails. The CRDT layer — the genuinely hard part — is done and paid
for. But on the Mac, an unreachable box sends the window to
`pair.html#unreachable` (`src-tauri/src/main.rs`) and the SPA never loads at
all, so none of that offline capability can be reached. A person on a plane has
a working editor and no way into it.

**2. The box as sole source makes version skew impossible.**
Today the UI can be *newer* than the box, and silently wrong as a result. On
2026-08-05 a TestFlight build shipped a phone UI calling

    GET /api/wiki/day/{date}/heart-rate?tz=America/Chicago

against a box with no `tz` handler — the parameter was written on the client
before the server half existed. The box ignored the unknown query param and fell
back to the recorded zone. No crash, no error, just the wrong midnight for
today's heart rate. That class of bug is unfindable from the UI.

If the box is the only place a bundle can come from, the UI can never be ahead
of the box, because they are the same artifact. The skew stops being a bug to
catch and becomes a state that cannot exist.

Neither reason is about shipping speed. Shipping speed is a side effect.

## Where each client stands today

| | UI source | Offline | Version skew |
|---|---|---|---|
| **Mac** | `WebviewUrl::External("http://localhost:7117")` — streams the box's build | none; unreachable box = no UI | impossible (renders the box's own build) |
| **iOS/Android** | baked SvelteKit build; box is REST/WS API over the iroh loopback | UI boots, data does not | possible, and has happened |
| **Box** | serves the build from disk (`ServeDir`, `server/mod.rs`) | n/a | source of truth |

Each platform is half-right. The Mac cannot be ahead of the box but dies without
it. The phone survives without the box but can drift ahead of it. Neither
property should be platform-specific.

## Target: one architecture, both platforms

    baked bundle  →  OTA overlay from the box  →  local data (Yjs/IndexedDB)

Every client bakes a bundle, overlays a newer one pulled from the box, and holds
its own document state. The Mac's `WebviewUrl::External` special case goes away;
both platforms use the same resolver and the same launch path.

**Two-layer asset resolver.** A custom URI scheme handler
(`register_uri_scheme_protocol`) serves each request from
`<app-data>/web-bundles/active/` if present, else from the baked-in bundle.
Baked assets are read-only; we never write over them.

**Update flow.** On connect: compare the box's web-build version
(`GET /api/web-bundle/version`) against the local active one → download the
tarball over the same private link → unpack to `web-bundles/<version>/` →
atomically flip the `active` pointer → reload the webview.

**Box side.** Version and tarball endpoints over the web build already on disk.
Upgrading a box then propagates its UI to every paired client on their next
connect — one artifact, one version, no fan-out to coordinate.

**Reachability stops being a dead end.** An unreachable box currently means "no
UI." It should mean "load the SPA, serve what is cached, queue writes."
`pair.html` goes back to meaning only *unpaired*, which is what its name says.

## What is actually offline, and what is not

Be precise here, because promising more than this makes the product look broken.

**Offline: Pages.** Yjs gives CRDT merge for free, `y-indexeddb` is already a
dependency and already wired. Documents open, edit, and reconcile cleanly on
reconnect. This is the plane case, and it is nearly free.

**Not offline: everything Postgres-backed.** Wiki, day view, search, records,
notes-as-data are live reads against the box and will be empty without it. A
read cache for those is a much larger project with worse staleness properties,
and is explicitly out of scope.

**So the UI must say so.** An empty day view with no explanation reads as data
loss — the worst possible impression for something holding a person's life. The
honest line is: *your writing is always with you; your record lives on your
box.* That is a defensible product boundary, but only when stated. Offline
states need copy, not just empty components.

This copy is the one real design question left in this plan. Everything else is
mechanical.

## Safety rules (non-negotiable)

Carried from the parked plan, unchanged — they were right.

1. **API compatibility gating.** OTA'd JS calling a plugin command the installed
   binary lacks is a broken app. The shell exposes its API/build version; each
   SPA bundle declares `minShellVersion`; incompatible bundles are never
   applied. Plugin and native changes always ride store releases.
2. **The boot shell stays natively bundled.** `mobile-pair.html` and the
   initialization script are never OTA'd — that is the recovery surface when a
   bundle is bad.
3. **Rollback on failed boot.** Keep the previous bundle; a new bundle must emit
   a boot-ok beacon within a few seconds or the shell flips back (same spirit as
   the box's `virtues.bak` swap).

Rule 1 matters more once the Mac joins: killing UI-ahead-of-box exposes the
mirror risk, JS ahead of native. Without the floor, OTA converts a store
round-trip into a white screen in the field.

## Apple's position

The "no downloaded executable code" rule carves out JS/HTML/CSS executed by
WebKit — the same basis as Expo Updates, CodePush, and Capacitor Live Updates.
Native code still rides the App Store only.

Note this is also why the phone must *not* simply point a webview at the box the
way the Mac does today: a pure remote-URL wrapper invites guideline 4.2
(minimum functionality), on top of costing a cold load over the loopback on
cellular and making a collector's UI hostage to connectivity.

## Sequencing

**First, and standalone: version legibility.** Nothing on screen today says
which box, which UI bundle, and which native app you are running. The 2026-08-05
confusion — a phone visibly newer than the Mac, for reasons invisible from
either — was unfalsifiable without `ssh`. There is a Phase-1 identity spine
already (`version()`/`channel()`, `X-Virtues-Client` → DevicesView); extend it to
carry the UI bundle version. This is small, it pays off immediately, and OTA
cannot be debugged without it.

Then the rest, as one chunk:

- [ ] Box: web-bundle version + tarball endpoints over the existing `ServeDir`
- [ ] Shell: custom-protocol resolver with active-dir override + baked fallback
- [ ] Shell: download / verify checksum / unpack / flip / rollback state machine
- [ ] Shell: API-version export + `minShellVersion` gate
- [ ] SPA build: emit version + `minShellVersion` manifest in `apps/web`
- [ ] Boot-ok beacon + rollback timer
- [ ] Mac: drop `WebviewUrl::External`, adopt the resolver, keep the self-updater
      (`tauri_plugin_updater`) for native
- [ ] Offline copy pass: what each Postgres-backed surface says with no box
- [ ] (later) Android parity

## Not the most urgent iOS work

Worth stating so this plan does not crowd out measurement. As of 1.2.5 the
radio-hygiene A/B counters are on real hardware for the first time — the battery
pass has been reasoned about but never measured. On-device verification, quiet
hours, and the location off-switch all outrank this. OTA makes *future* UI
iteration cheaper; verification tells you whether what already shipped works.

## Bonus

Dev loop: push a dev SPA build to the phone over the box link instead of
`tauri ios dev`'s Mac-hosted server, which requires phone and Mac on the same
network — that bit us on 2026-07-13.
