# The update manifold & compatibility lattice

The first 0.3 feature. How the whole Virtues fleet — not just the box — moves
between builds coherently, and how components that **never update atomically**
stay compatible across the gaps.

This generalizes [the box update paradigm](update-paradigm.md) (5 pillars for
`virtues upgrade`) from one node to the whole system, and adds the piece the box
doc never needed: cross-component version negotiation.

## Why now

v0.2.0 is the first stable release. Every release from here touches **nine
independently-versioned artifacts** with almost no coordination between them.
The audit (2026-07-19) found the fleet has:

- **Five unsynchronized client version numbers** — Tauri app `1.0.x`, native iOS
  `1.1`, collector `1.0.0`, desktop client `0.1.0`, SPA `0.0.1` — and **no git-SHA
  on any client** (the collector's `gitCommit` is the literal string `"unknown"`).
- **Exactly two enforced compatibility gates in the entire system**: iOS honoring
  the box's `min_ios_version` (`apps/ios/.../DeviceManager.swift:195`), and restore
  refusing a newer backup schema against a **hardcoded** `KNOWN_MAX_MIGRATION = 9`
  (`cli/restore.rs:159`) that must be hand-bumped and already drifts.
- **No cloud version discipline** — atlas + virtues-api ship as `:latest` via manual
  `docker pull`+restart over SSM; the one "notify Atlas to roll a version" hook
  (`register-version` in `docker-build.yml`) POSTs to an endpoint that **does not
  exist** and is `continue-on-error`.
- **A contradiction in the box itself**: the paradigm doc's Pillar 2 wants migration
  lineage to *refuse* a mismatched upgrade, but `database/mod.rs:293` boots with
  `set_ignore_missing(true)` — the guard is off at runtime, live only on restore.

There is no single artifact that says "release R = {box vX, api vY, mac vZ, …} and
these are the versions that are known to work together." Building that — and the
negotiation that enforces it at each boundary — is this project.

## Two problems, named

- **The manifold** — the *delivery* surface. One spoke per node-type (box, cloud,
  mac desktop, mobile shell, mobile SPA, iOS collector, relay, model bucket), each
  with its own transport (systemd swap, docker pull, Tauri updater, App Store, iroh
  OTA), unified under a **common build identity** and a **single fleet release
  manifest**.
- **The lattice** — the *compatibility* graph. Every edge where two
  non-atomically-updated components talk, carrying a declared, negotiated
  version/capability contract instead of an implicit one.

The manifold makes a release coherent; the lattice makes a partial rollout safe.

## The fleet inventory (grounded, 2026-07-19)

| Node | Artifact | Delivery today | Version identity | Gap |
|------|----------|----------------|------------------|-----|
| **Box** | `virtues` + sidecars | `virtues upgrade` (naive subset of 5 pillars); installer | `build.rs` bakes SHA/describe/time; `/health` | migration guard off at boot; in-place refresh, partial rollback |
| **Cloud: virtues-api** (= oauth proxy) | Docker image | CI→GHCR **and** make→ECR (conflicting); manual SSM restart | per-svc `/health` version | `:latest`, two registries, no pin/rollback, dead `set-version` |
| **Cloud: atlas** | Docker image | `make deploy-atlas` from a laptop, **no CI** | `/health` version | no automated build at all |
| **Relay** | upstream `iroh-relay` | manual `ssh` + `cargo install`/scp + `systemctl restart` | `^1` semver range | unit-in-git ≠ unit-on-host; manual; unpinned |
| **Mac desktop** | Tauri v2 app | `tauri-plugin-updater`, minisign, `mac-latest/latest.json` | stamped `tauri.conf.json` | **edge has no feed**; bundled sidecars reconciled by byte-diff, not version |
| **Mobile shell** | Tauri iOS/Android | App Store (manual) | `CFBundle*` = `1.0.15` | **no updater compiled for mobile** |
| **Mobile SPA** | SvelteKit bundle | — | **none emitted** | OTA is a *parked plan*, not built |
| **iOS collector** | native `apps/ios` **or** Tauri mobile (ambiguous) | manual Xcode → TestFlight | `MARKETING_VERSION 1.1` | **no CI/fastlane**; two codebases, unclear which ships |
| **Models** | GGUF bucket | manual `workflow_dispatch` → GH Release `models-1` | fixed tag | decoupled from every release tag; hand-triggered |

## Design principles

1. **Identity before orchestration.** You cannot coordinate what you cannot name.
   Every artifact must carry the same shape of build identity before anything
   negotiates on it.
2. **Declare compatibility once, enforce at every edge.** The compatibility matrix
   lives in one signed manifest; each boundary reads it, rather than each pair
   inventing its own check.
3. **Refuse, don't brick.** Every boundary and every swap fails *before* committing,
   with a specific reason — never half-applied. (The box paradigm's Pillar 2/3,
   generalized.)
4. **Non-atomic by assumption.** Never require two components to update together.
   Every edge must degrade or refuse gracefully across a version skew window.
5. **One spine, many spokes.** Keep each node's native transport (Tauri updater,
   App Store, systemd, docker) — unify the *identity, channel, and manifest*, not
   the delivery mechanism.

## Part 1 — The manifold

### 1.1 A common build identity (`BUILD` manifest)

Extend the box's `build.rs` pattern to **every** artifact. Each build emits a
`BUILD` manifest (baked constant + a `BUILD.json` shipped *inside* the release
artifact — today it exists only inside backup tarballs, not distribution ones):

```json
{ "component": "box|virtues-api|atlas|relay|mac|mobile-shell|mobile-spa|ios",
  "version": "0.3.0", "sha": "13cfd9c", "channel": "stable|staging|edge",
  "built_at": "2026-07-19T18:06:00Z",
  "min_peers": { "box": "0.3.0", "virtues-api": "0.2.0" } }
```

Concrete first fixes (all cheap, all unblock the rest):
- **SPA**: emit a version + `minShellVersion` constant at build (the OTA plan already
  requires this; today `apps/web/package.json` is a placeholder `0.0.1`).
- **Collector**: actually stamp `gitCommit`/`buildDate` in CI (currently the literal
  `"unknown"` in `apps/mac-source/Sources/Version.swift`).
- **Cloud services**: bake SHA/channel the way core does; surface in each `/health`.
- **Channel becomes first-class** (baked), not inferred from the tag string.

### 1.2 One channel model, fleet-wide

Reuse the box's proven scheme everywhere: `stable` (`v*` / `mac-v*`), `staging.N` /
`edge` (any `-` identifier → prerelease). Cloud images get **version tags**, not
`:latest`; the relay pins an `iroh-relay` version instead of `^1`.

### 1.3 The fleet release manifest (the keystone)

A single signed JSON, published per fleet release, that pins every component and
declares the matrix. This is the artifact that does not exist today.

```json
{ "release": "0.3.0", "channel": "stable", "cut_at": "…",
  "components": {
    "box":         { "tag": "v0.3.0",       "sha": "…" },
    "virtues-api": { "image": "…@sha256:…", "tag": "v0.3.0" },
    "atlas":       { "image": "…@sha256:…", "tag": "v0.3.0" },
    "relay":       { "iroh_relay": "1.4.2" },
    "mac":         { "tag": "mac-v1.1.0" },
    "mobile-shell":{ "tag": "ios-v1.1.0", "spa_min": "0.3.0" },
    "mobile-spa":  { "tag": "spa-v0.3.0", "min_shell": "1.1.0" },
    "models":      { "tag": "models-2" } },
  "compat": { /* the lattice — see Part 2 */ } }
```

Published to a well-known URL (GH Release `fleet-latest`, like `mac-latest`). Every
node's updater reads *its own row*; the box's model-set drift check already reads a
model tag this way. Cloud deploy pins from `components.*.image` by digest, giving the
rollback target the cloud lacks today.

### 1.4 Per-spoke delivery work

- **Box** — implement the 5 pillars (`update-paradigm.md`), reference node. Highest
  value: Pillar 2 preflight + Pillar 3 atomic release slots.
- **Cloud** — collapse GHCR-vs-ECR to **one** registry; tag images by version + pin
  by digest; a real deploy action that SSM-runs `pull <pinned> && restart`; either
  implement the missing `set-version` endpoint or delete the dead hook; rollback =
  redeploy prior digest. Build atlas in CI (today: laptop-only).
- **Relay** — commit the *actually-deployed* unit to git (reconcile
  `virtues-iroh-relay.service` vs the live `iroh-relay.service`); pin the version; a
  one-line deploy script.
- **Mac** — keep the Tauri updater; add an **edge feed** so the test channel updates;
  reconcile bundled sidecars by **version**, not byte-diff.
- **SPA delivery** — build `spa-delivery-plan.md`: two-layer resolver,
  `minShellVersion` gate, rollback beacon, over iroh loopback. No longer mobile-only —
  the Mac drops `WebviewUrl::External` and adopts the same resolver, which is what
  unlocks the offline editing Yjs already supports. Native shells ride their stores.
- **iOS** — **decide the one codebase** (native `apps/ios` vs Tauri mobile) and add CI
  (fastlane → TestFlight). Manual Archive is the current single point of failure.
- **Models** — reference the model tag from the fleet manifest so a release declares
  its expected model set (box already detects drift, just isn't fed a target).

## Part 2 — The lattice

The edges where non-atomic components meet, and the contract each must carry.

| Edge | Today | Target |
|------|-------|--------|
| mobile/iOS → box (iroh HTTP) | one-way `min_ios_version` in `/health` | bidirectional `min_peer` handshake |
| SPA → box | nothing | `min_box` / `min_spa` check on load |
| desktop helper → box | nothing | same handshake |
| box → atlas / virtues-api | auth/telemetry headers only | `min_box` gate + capability flags |
| box binary ↔ DB migrations | guard **off** at boot | preflight refusal (Pillar 2) |
| app ↔ SPA bundle | — | `minShellVersion` (OTA plan) |
| box ↔ sidecars/models | topology + model-drift | version-stamped reconcile |

### 2.1 Version the transport, not just the payload

The iroh ALPN `virtues/http/1` is a single monotonic string with no negotiation —
a wire break fails opaquely at QUIC. Add an **in-band hello frame** right after
connect: each side sends its `BUILD` identity + `min_peer`; either side may refuse
with a *typed* reason ("box 0.3 requires app ≥ 1.1") instead of a dead socket. Keep
the ALPN as the coarse gate; the hello frame carries the range.

### 2.2 Make the min-version gate bidirectional and universal

Generalize `min_ios_version` → a `min_peers` map in `/health` and the hello frame,
consumed by **every** client (SPA + desktop today honor nothing), and add the reverse
`min_box_version` so a client can refuse a too-old box. Replace the naive
dotted-integer `compareVersions` with real semver (pre-release aware).

### 2.3 Resolve the migration-lineage contradiction

Pillar 2 says *refuse on divergence*; the runtime says `set_ignore_missing(true)`.
Resolve deliberately:
- Keep `ignore_missing` for the **dev multi-branch boot** convenience it was added for.
- Add the explicit `virtues migrate --check` **preflight** in the *upgrade* path
  (applies nothing, exits non-zero with the precise lineage reason) — this is where
  the invariant belongs, per the paradigm doc.
- Derive `KNOWN_MAX_MIGRATION` from the embedded migration set instead of the
  hardcoded `9` (`restore.rs:159`) so restore and preflight can't silently drift.

### 2.4 Declare the matrix in the manifest

The `compat` block of the fleet manifest is the single source of truth for every
`min_peer` value; nodes read it rather than hardcoding. This is what lets a release
say "0.3 box needs 0.2+ api but 1.1+ app" in one reviewed place.

## Phasing

1. **Identity spine** — `BUILD` manifest + version/SHA/channel baked into *every*
   artifact; surface in `/health`. Cheap, unblocks all of it.
2. **Box 5 pillars** — per `update-paradigm.md` order (preflight + slots first).
3. **Lattice core** — hello-frame handshake, bidirectional `min_peers`, fix the
   migration preflight + `KNOWN_MAX` drift.
4. **Cloud discipline** — one registry, digest-pinned deploy action, rollback.
5. **Fleet release manifest** — the keystone; ties nodes + `compat` together.
6. **Mobile** — SPA OTA (build the parked plan) + iOS CI/TestFlight + pick one iOS
   codebase.
7. **Relay hygiene** — unit-in-git = unit-on-host, pinned version, deploy script.

Steps 1 and (per-node) 4/7 can proceed in parallel once the manifest shape is fixed;
the lattice (3) depends on the identity spine (1).

## Open decisions

- **iOS codebase**: native `apps/ios` (@1.1, honors `min_ios_version`) vs Tauri
  mobile (@1.0.15) — which one ships? The manifold can't stamp an ambiguous node.
- **Cloud registry**: GHCR (CI today) or ECR (make/docs today) as the single one?
- **Manifest signing**: reuse the mac minisign key, or a dedicated fleet key?
- **Cloud deploy trigger**: does a `v*` fleet tag auto-roll the cloud, or stay
  operator-gated (blast radius)?
- **Auto-update posture**: box is deliberately opt-in today — does the manifold keep
  every node pull-only, or introduce atlas-scheduled windows (the `system_update.rs`
  comment already claims this, unbuilt)?
