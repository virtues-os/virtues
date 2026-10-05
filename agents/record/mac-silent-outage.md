# The Mac's six-day silent outage

Written 2026-09-29 from `agents/plan/mac-plan.md`, whose built half this
replaces. From 2026-07-30 to 2026-08-05 a Mac stopped sending Messages, and
the app, the collector and the box all failed to say so; the one banner shown
named the wrong permission.

## What went wrong

- **A macOS permission grant is keyed to the binary's signature.** An update
  signed differently left the old Full Disk Access entry in System Settings,
  looking granted and doing nothing.
- **Permission truth was discarded in Rust.** Swift's `StatusCommand` emitted
  `permissionsReportedByDaemon` and `permissionsCheckedAt`; `CollectorStatus`
  dropped both, so "haven't heard" and "denied" looked the same on screen.
- **The permission record froze once granted.** `recordFromDaemon()` ran only
  inside the `!hasFullDiskAccess` branch, so granting made it unreachable: a
  later revocation was never noticed, and `isStale` went true fifteen minutes
  into healthy running, which taught every reader to ignore it.
- **Versions were inverted.** The tree built 1.0.15 while production published
  1.0.20, so a fresh build was offered a downgrade; nothing enforced
  monotonicity.

## What was built

- The app version is a counter that only goes up, and `release-mac.yml`
  refuses a tag that is not strictly greater than the published `latest.json`.
- Permissions reach the screen as granted, denied or unknown, each with when
  it was observed; the collector republishes every tick, ahead of the pause
  check (pausing is about data, not permissions). Copy leads with the
  consequence (`PERMISSION_COPY` in `apps/web/src/lib/devices/shared.ts`).
- Updates are never a question: checked at launch and every 6h, downloaded and
  staged silently, applied on the next launch; two consecutive failures of
  check or download send one notification (`check_for_update`, `main.rs`).
- One update channel, `mac-latest`. Routing by the box's channel pointed Macs
  at an endpoint that had stopped publishing, and the box's vocabulary never
  matched the router's ([device-version-update-audit.md](device-version-update-audit.md), U1–U5).
- The shell declares `COMMAND_SURFACE_VERSION`; `bridge.ts` reads it and
  degrades a feature rather than failing inside it
  ([spa-delivery.md](spa-delivery.md)).

## The rules it left

1. No component asserts a fact it didn't observe. "Unknown" is a real state
   and reaches the screen as one.
2. Every claim carries when it was observed.
3. A version only goes up, enforced mechanically. Care is not a mechanism.
4. An update is never a question, and never a forced relaunch.
5. Failure is described by consequence first, cause second.
