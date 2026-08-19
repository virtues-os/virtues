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

## The origin problem — read before writing the resolver

Found 2026-08-05, and it invalidates part of what is written above.

**Web storage is partitioned by origin, and the desktop app changes origin
between its online and offline states.**

| state | origin |
|---|---|
| box reachable | `http://localhost:7117` (`WebviewUrl::External`) |
| box unreachable | `tauri://localhost` (`WebviewUrl::App`, the baked bundle) |

`lib/config/backend.ts` documents both, so this was always visible; nobody had
put them side by side. IndexedDB, localStorage, and everything else the SPA
persists live under the first origin during normal use and are **invisible**
from the second.

So the offline fallback shipped in `a9db926f` boots the interface but opens it
against an empty IndexedDB. The Yjs documents that were the entire reason for
building it are on the other origin. **The plane case does not work**, and the
empty surfaces observed while testing were partly this, not only the absent API.

This also constrains OTA directly: an overlay served from a *new* scheme (the
obvious `register_uri_scheme_protocol` design) introduces a **third** origin, so
applying a bundle would silently empty the user's local documents. A resolver
written that way is worse than no resolver.

**The requirement this creates:** one stable origin across box-reachable,
box-unreachable, baked-bundle, and OTA-bundle. Not four states with three
origins.

The candidate that satisfies it is the loopback: serve the SPA from
`http://127.0.0.1:7117` always — from the box when it answers, from the local
bundle cache when it does not — which means the reach loopback, not the webview,
decides where assets come from. Mobile already has a stable origin
(`tauri://`), so its constraint is narrower: OTA must overlay *within* that
origin rather than beside it.

Deciding this is a prerequisite for item 5, not a detail of it.

## Safety rules (non-negotiable)

Carried from the parked plan, unchanged — they were right.

1. **API compatibility gating.** OTA'd JS calling a plugin command the installed
   binary lacks is a broken app. The shell exposes its API/build version; each
   SPA bundle declares `minShellVersion`; incompatible bundles are never
   applied. Plugin and native changes always ride store releases.
2. **The boot shell stays natively bundled.** `connect.html` and the
   initialization script are never OTA'd — that is the recovery surface when a
   bundle is bad.
3. **Rollback on failed boot.** Keep the previous bundle; a new bundle must emit
   a boot-ok beacon within a few seconds or the shell flips back (same spirit as
   the box's `virtues.bak` swap).

**Rule 1 is already owed, today, with no OTA anywhere.** `mac-plan.md` §3 names
it the *undeclared coupling*: the Mac bundles only `pair.html` and shells to the
box, so **the box already serves the JavaScript that calls the app's Tauri
commands**. `bridge.ts` ships with the box and `invoke()`s a surface compiled
into a separately-versioned binary — no negotiation, no feature detection, no
error path. A box newer than the app calls a command that does not exist and
fails at runtime, inside whatever feature needed it. It is latent only because
that surface has been stable.

So JS-ahead-of-native is not a risk this plan introduces. It is a live,
unguarded defect that this plan would inherit, and the fix is the same object
under two names: `minShellVersion` here, "command-surface version" in
`mac-plan.md` §4 (its Phase 5). One mechanism, built once, load-bearing for both.
Whoever gets there first should name it for both documents.

## Apple's position

The "no downloaded executable code" rule carves out JS/HTML/CSS executed by
WebKit — the same basis as Expo Updates, CodePush, and Capacitor Live Updates.
Native code still rides the App Store only.

Note this is also why the phone must *not* simply point a webview at the box the
way the Mac does today: a pure remote-URL wrapper invites guideline 4.2
(minimum functionality), on top of costing a cold load over the loopback on
cellular and making a collector's UI hostage to connectivity.

## Relationship to `mac-plan.md`

That document owns the Mac end to end — onboarding, permissions, and all three
update paths — written after a six-day silent outage. Two points of contact:

**It does not block this, and this does not block it.** Its Phases 0–2 (version
monotonicity, a CI gate, three-state permission truth) are about an outage that
already happened and touch none of the UI-delivery machinery here. They go
first. This plan is strictly downstream.

**One live disagreement.** `mac-plan.md` §3 records "Web UI delivery — genuinely
OTA from the box" as a *working* surface, and builds on the Mac continuing to
shell to `localhost:7117`. This plan proposes retiring that shell for the
resolver. Both are defensible; the tiebreaker is offline, which `mac-plan.md`
does not address for the UI. Until someone decides, do not let either document
be read as settling it.

**A refinement this forces on `mac-plan.md`'s identity model.** Its §4 has the
Mac display the paired box's release. Once the Mac caches a bundle, the box can
be upgraded while the client still runs the previous bundle, and reporting the
box's number would assert something the client has not actually loaded —
contrary to its own invariant 4 ("no component asserts a fact it didn't
observe"). The displayed release must come from the **active bundle**, not the
box's claim. They converge on the next pull; the number should not lie in the
gap.

## Offline is cheap; OTA is the expensive half

Decided 2026-08-05: offline matters, and it should not wait on the machinery.

These are two separable projects and this plan originally conflated them:

**Offline fallback — small.** Bundle the real SPA build into the desktop app
(`frontendDist` today points at `ui/`, a four-file connect shell) and load it
when the box does not answer, instead of `pair.html#unreachable`. No resolver,
no tarball endpoints, no version manifest, no atomic flip. Yjs and IndexedDB
already do the hard part, so this is the whole plane case: bundle, plus a launch
branch that already exists in another form.

**OTA overlay — large.** The resolver, `web-bundles/<version>/`, checksum,
flip, rollback beacon, box endpoints, build manifest. This buys UI *freshness*,
not offline, and freshness is a convenience until mobile UI churn actually
outpaces store releases.

So build the fallback now and defer the overlay. The Mac keeps
`WebviewUrl::External` while the box is reachable — which preserves the
skew-impossibility `mac-plan.md` values — and falls back to the baked bundle
only when it is not. That is a smaller change than the retirement this plan
first proposed, and it settles the §7 disagreement by not needing it resolved.

**The real cost is not plumbing, it is copy.** Offline, every Postgres-backed
surface has nothing to show. Deciding what each one says — and making "no box"
read as a state rather than as data loss — is the actual work, and it is design
work. Budget it there, not in the resolver.

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
