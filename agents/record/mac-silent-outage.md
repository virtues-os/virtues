# The Mac's six-day silent outage

Written 2026-09-29 from `agents/plan/mac-plan.md`, whose built half this
replaces. From 2026-07-30 to 2026-08-05 a Mac stopped sending Messages, and
the app, the collector and the box all failed to report it; the one banner shown
named the wrong permission.

## What went wrong

- **A macOS permission grant is keyed to the binary's signature.** An update
  signed differently left the old Full Disk Access entry in System Settings,
  looking granted and doing nothing.
- **Rust discarded the permission state.** Swift's `StatusCommand` emitted
  `permissionsReportedByDaemon` and `permissionsCheckedAt`; `CollectorStatus`
  dropped both, so "haven't heard" and "denied" looked the same on screen.
- **The stored permission state stopped updating once granted.** `recordFromDaemon()` ran only
  inside the `!hasFullDiskAccess` branch, so granting made it unreachable: a
  later revocation was never noticed, and `isStale` went true fifteen minutes
  into healthy running, so everyone learned to ignore it.
- **Versions were inverted.** The tree built 1.0.15 while production published
  1.0.20, so a fresh build was offered a downgrade; nothing enforced
  monotonicity.

## What was built

- The app version is a counter that only goes up, and `release-mac.yml`
  refuses a tag that is not strictly greater than the published `latest.json`.
- Permissions reach the screen as granted, denied or unknown, each with when
  it was observed; the collector republishes every tick, ahead of the pause
  check (pausing stops data collection and does not affect permission reporting). The UI text states the
  consequence first (`PERMISSION_COPY` in `apps/web/src/lib/devices/shared.ts`).
- The app never asks before updating: checked at launch and every 6h, downloaded and
  staged silently, applied on the next launch; two consecutive failures of
  check or download send one notification (`check_for_update`, `main.rs`).
- One update channel, `mac-latest`. Routing by the box's channel pointed Macs
  at an endpoint that had stopped publishing, and the box's channel names never
  matched the router's ([device-version-update-audit.md](device-version-update-audit.md), U1–U5).
- The shell declares `COMMAND_SURFACE_VERSION`; `bridge.ts` reads it and
  disables a feature instead of failing inside it
  ([spa-delivery.md](spa-delivery.md)).

## Rules adopted after the outage

1. No component asserts a fact it didn't observe. "Unknown" is a real state
   and reaches the screen as one.
2. Every reported state carries the time it was observed.
3. A version only goes up, and CI enforces this. Relying on people to remember
   is not enough.
4. An update never prompts the user and never forces a relaunch.
5. Failure is described by consequence first, cause second.
