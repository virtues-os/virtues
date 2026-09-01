# Phase 1 — the identity spine + the Devices version panel

The first, cheapest, zero-risk step of [the update manifold](update-manifold-plan.md).
No behavior change, no gating — just make **every artifact state its identity**, get
each device to **report it to the box**, and **show the fleet on the Devices page**.

This is deliberately the invisible-plumbing step given a visible destination: when
it's done, the Devices page shows every paired thing's version at a glance.

## Scope

**In:** a canonical `{version, sha, channel}` baked into every artifact; each client
reporting it to the box; the box storing it per-device; the Devices page rendering it.

**Out (later phases, don't build here):**
- The *verdict* — "update required" / "supported" badges. That's a min-version compare
  (Phase 3, the lattice); this phase only *displays* versions.
- "Update available" nudges — needs a latest-version source (App Store feed / fleet
  manifest); deferred.
- The signed fleet release manifest.

## The canonical identity

One shape, everywhere. Reuse what the box already bakes.

```
version   semver "0.3.0"        from the git tag (GIT_DESCRIBE)
sha       "13cfd9c"             git rev-parse HEAD — the unambiguous one
channel   stable|staging|edge   currently inferred from the tag; make it FIRST-CLASS (baked)
```

`built_at` is nice-to-have (box already bakes it). `channel` is the one genuinely-new
field — today it's derived ad hoc from the tag string; bake it as its own constant so
every artifact reports it uniformly.

## Per-artifact stamping

The box is the reference — already correct. The work is the five stragglers.

| Artifact | Where | State today | Do |
|----------|-------|-------------|----|
| **Box** `virtues` | `virtues-core/build.rs`, `/health` (`server/mod.rs:945`) | ✅ bakes SHA/describe/time | add `channel` as a first-class baked const |
| **SPA** | `apps/web/vite.config.ts:13`, `app.d.ts:5` | `__BUILD_COMMIT__` only (sha), falls back `'dev'` | extend to `__BUILD__ = {version, sha, channel}`; surface in Settings |
| **Desktop client** (Rust) | `apps/desktop/Cargo.toml` | Cargo version only, no SHA | add a `build.rs` mirroring core |
| **Mac collector** (Swift) | `apps/mac-source/Sources/Version.swift`; CI `release-mac.yml` | `gitCommit = "unknown"` (never set) | stamp `gitCommit`/`buildDate` in CI at build |
| **Cloud** virtues-api / atlas | `services/*/src/routes/health.rs` | `/health` has `version` only | bake sha/channel, add to `/health` |

CI already passes `GIT_COMMIT` to the SPA build and `VIRTUES_BUILD_VERSION` to core —
so the values exist in the pipeline; this is mostly wiring them into each build's
constants, not new infrastructure.

## Device → box reporting (how the Devices page gets data)

Two touchpoints, both mostly-existing:

1. **At pair time** — the client includes its identity in the `device_info` blob the
   pairing upsert already writes (`api/pair.rs:908`). No migration: `device_info` and
   `last_seen_at` already exist (`migrations/0002_auth.sql`).
2. **On the wire** — clients send a lightweight `X-Virtues-Client:
   version=…; sha=…; channel=…` header on box requests; the box refreshes that device
   row's `device_info` + `last_seen_at`. This keeps the version fresh as the app
   auto-updates, not frozen at pair time.

Box side:
- `api/devices.rs` `DeviceListItem` (line 31) gains `version` / `sha` / `channel`,
  read out of `device_info`; add them to the `SELECT` at line 46.
- Everything else (the row, `last_seen_at`, `kind`, `label`) already exists.

## The Devices page panel

`apps/web/.../tabs/views/DevicesView.svelte` already enumerates every paired thing —
just add columns. Per row:

```
Label            Kind        Version         Channel   Last seen
This browser     web         0.3.0 (13cfd9c) stable    now
iPhone           ios         1.0.15          stable    2m ago
MacBook          mac         1.1.0           stable    1h ago
Sensor / box     box         0.3.0           stable    —
```

v1 = **display only**: `version · channel · last seen`. That alone answers "is the
phone way behind the browser?" at a glance. The **badge** (🟢 supported / 🔴 update
required) is the first thing Phase 3 lights up — it slots into this same row when the
min-version compare exists. Don't build the verdict here; leave room for it.

Optional, cheap add: the box already reads cloud `/health`; surface atlas/api versions
in `SystemInfoView` so the box's own "upstream" is visible too.

## Acceptance criteria

- `virtues --version`, the SPA Settings screen, the mac app, the collector, and each
  cloud `/health` all report `{version, sha, channel}` in the same shape.
- A freshly paired device shows its version on the Devices page; it updates (not just
  at pair time) after the client sends a request post-upgrade.
- No gating, no refusal, no new migration. Pure additive display.

## Why first

It's a day or two, risk-free, and it's the prerequisite for *everything* downstream:
the lattice's min-version checks (Phase 3) and every future bug report both need "what
is this thing?" to have an answer. This step makes that answer exist — and puts it on
a screen.
