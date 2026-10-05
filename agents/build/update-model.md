# The fleet update model (north star)

How a solo dev keeps a growing pile of apps versioned and in-control without
drowning. One mental model; every new app slots into it. How the web payload
actually reaches each client is [spa-delivery.md](../record/spa-delivery.md);
the app↔box floor is [version-compat-plan.md](../plan/version-compat-plan.md).

## The one idea

> **A thin native shell you ship rarely + a fast web payload you push freely,
> with a version contract between them.** Maximize the surface you can update on
> your own; shrink the surface a store gates. Push complexity into the tier you
> control — the box is the escape hatch for everything else.

Tauri is what makes this possible: one `apps/web` codebase becomes the Mac,
Windows and Linux desktop apps and the iPhone app. Lean on it.

## Two layers, per client

| Layer | What | Changes | Ships via |
|-------|------|---------|-----------|
| **Native shell** | Tauri Rust + platform bits (permissions, plugins) | *rarely* | store / notarization |
| **Web payload** | `apps/web` SPA — ~all product logic | *constantly* | box-serve / OTA |

Keep the shell dumb and stable → 95% of changes never touch a store: one
codebase, one version, push anytime.

## Three tiers, by update mechanism (not by app)

| Tier | How you ship | Version = | Rollback |
|------|--------------|-----------|----------|
| **Continuous** | you push anytime — git tag, box-serve, OTA, self-hosted updater feed | git tag | redeploy prior tag |
| **Store-gated native** | store review / notarization, own cadence, needs signing | store version | new submission |
| **Manual infra** | ssh, rarely | pinned | ssh |

## Where each thing lands

| App / artifact | Tier | Notes |
|----------------|------|-------|
| Box (`virtues` + inference daemons) | Continuous | `virtues.com/sh` + `virtues upgrade`; a nightly `virtues auto-update` follows the box's channel unless the owner turns it off |
| Cloud (atlas / api / oauth-proxy) | Continuous | image tag + redeploy |
| Web SPA (desktop **and** mobile payload) | Continuous | box-serve / OTA — the fast layer |
| **Mac desktop shell** | Continuous-ish | **our own Tauri updater feed, not the Mac App Store** — `mac-latest` (stable) and `mac-edge` (prerelease) on GitHub Releases |
| Windows / Linux desktop shells | Continuous-ish | their own release workflows; viewers and setup instruments, no collector |
| **iPhone app** (Tauri shell + native plugins) | Store-gated | App Store — but its web payload rides OTA from the box |
| Mac collector | rides the mac shell | bundled in the DMG |
| Relay | Manual infra | ssh + pinned version |

**The liberating fact:** the *only* hard store gate in the whole fleet is **iOS
App Store**. The Mac ships through our own updater feed; everything else we
push. So "store-gated native" is essentially just iOS.

## The iOS exception

The iPhone app is the Tauri shell, and it fits the model. Apple *allows*
OTA-updating **web content** in a webview (guideline 3.3.2 — the same basis as
Capacitor Live Updates / CodePush / Expo), so the SPA payload updates from the
box; only **native shell or new-capability** changes need a store release, and
`minShellVersion` in the bundle manifest is the gate between the two.

The 24/7 collectors (HealthKit, location, audio) cannot run in a webview, so
they are native Tauri plugins (`apps/web/plugins/`) inside that same shell and
ship with it. **Keep them dumb sensor pipes; move logic into the box** so they
almost never ship.

**What's OTA-able:** the UI/web payload, everywhere (desktop + mobile).
**What's not:** the native plugins, and any native-shell / new-capability change.

---

## The one contract

You don't unify *delivery* — each tier keeps its native transport. You unify:

1. **Identity** — every artifact reports `{version, sha, channel}` the same way,
   and devices report theirs to the box, so the Devices page answers "what's
   running where."
2. **A compatibility floor** — one min-version check at the one edge that matters
   (**app ↔ box**), plus `minShellVersion` for the web-payload↔shell edge.

That's the whole discipline. Everything else is per-tier native mechanism.

## Solo-dev rules of thumb

- **Web-first.** New feature? Put it in `apps/web`. It ships everywhere, free.
- **Keep native shells thin & stable.** Touch them only for permissions, plugins,
  Tauri bumps — the things that *require* a store release anyway.
- **Keep the collectors dumb.** Every bit of logic you push into the box is a bit
  you can fix without an App Store round-trip.
- **Automate only the iOS build.** `make ios-release` builds and signs the IPA;
  upload stays a manual Transporter drag because it publishes under a personal
  Apple ID. Automating metadata/screenshots/review-submission is not worth it.
- **The box is the escape hatch.** It can tell an old client "you're too old,
  here's what to do" — so a stale store app is never a dead end.

## What is still open, in leverage order

1. **The app ↔ box floor.** A shell runs ahead of its box as the ordinary
   case — phones update themselves, boxes when their owner (or the nightly
   pass) does — and OTA is forward-only, so a newer shell keeps UI newer than
   its box's API. The version coordinates already travel both ways; nothing
   acts on them, and no bundle declares a minimum box. See
   [version-compat-plan.md](../plan/version-compat-plan.md).
2. **One origin on the Mac.** The Mac bakes the SPA for setup and recovery but
   runs the box's live copy once paired, so with the box unreachable it can
   show the reconnect screen and none of the owner's documents. See
   `local-ui-plan.md`.
3. **Keep shrinking the collectors into the box.**
