# The fleet update model (north star)

How a solo dev keeps a growing pile of apps versioned and in-control without
drowning. One mental model; every new app slots into it. The detailed specs
([update-manifold-plan.md](update-manifold-plan.md),
[update-identity-spine.md](update-identity-spine.md)) *implement* this — read
this first.

## The one idea

> **A thin native shell you ship rarely + a fast web payload you push freely,
> with a version contract between them.** Maximize the surface you can update on
> your own; shrink the surface a store gates. Push complexity into the tier you
> control — the box is the escape hatch for everything else.

Tauri is what makes this possible: one `apps/web` codebase becomes the mac
desktop app and the (future) mobile shell. Lean on it.

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
| Box (`virtues` + sidecars) | Continuous | `virtues.com/sh` + `virtues upgrade` |
| Cloud (atlas / api / oauth-proxy) | Continuous | image tag + redeploy |
| Web SPA (desktop **and** mobile payload) | Continuous | box-serve / OTA — the fast layer |
| **Mac desktop shell** | Continuous-ish | **your own Tauri updater feed, not the Mac App Store** — you control releases, no review |
| **iOS Tauri shell** (future) | Store-gated | App Store — but its web payload rides OTA |
| **Native iOS collector** (`apps/ios`) | Store-gated | the one genuine exception (below) |
| mac collector / desktop-client | rides the mac shell | bundled in the DMG |
| Relay | Manual infra | ssh + pinned version |

**The liberating fact:** the *only* hard store gate in the whole fleet is **iOS
App Store**. Mac ships through your own updater feed; everything else you push.
So "store-gated native" is essentially just iOS.

## The iOS exception

Two separate iOS things — don't conflate them:

- **Tauri mobile shell (future)** — fits the model. Apple *allows* OTA-updating
  **web content** in a webview (guideline 3.3.2 — same basis as Capacitor Live
  Updates / CodePush / Expo). So the SPA payload updates OTA; only **native shell
  or new-capability** changes need a store release. `minShellVersion` is the gate
  between the two.
- **Native collector (`apps/ios`)** — *not* Tauri and can't be (24/7 background
  HealthKit/location/audio can't run in a webview). Stays native, store-gated.
  Fine, because it changes rarely — **keep it a dumb sensor pipe; move logic into
  the box** so it almost never ships.

**What's OTA-able:** the UI/web payload, everywhere (desktop + mobile).
**What's not:** the native collector, and any native-shell / new-capability change.

## The one contract

You don't unify *delivery* — each tier keeps its native transport. You unify:

1. **Identity** — every artifact reports `{version, sha, channel}` the same way
   (the identity spine). You can always answer "what's running where."
2. **A compatibility floor** — one min-version check at the one edge that matters
   (**app ↔ box**), plus `minShellVersion` for the web-payload↔shell edge.

That's the whole discipline. Everything else is per-tier native mechanism.

## Solo-dev rules of thumb

- **Web-first.** New feature? Put it in `apps/web`. It ships everywhere, free.
- **Keep native shells thin & stable.** Touch them only for permissions, plugins,
  Tauri bumps — the things that *require* a store release anyway.
- **Keep the collector dumb.** Every bit of logic you push into the box is a bit
  you can fix without an App Store round-trip.
- **Automate only the iOS build.** `fastlane → TestFlight` (build/sign/upload) is
  worth ~a day; automating metadata/screenshots/review-submission is not. You need
  exactly **one** native pipeline — it serves the collector now and the Tauri iOS
  shell later.
- **The box is the escape hatch.** It can tell an old client "you're too old,
  here's what to do" — so a stale store app is never a dead end.

## What to build, in leverage order

1. **SPA delivery** — so the web layer updates freely on every client, not just
   mobile (turns the whole model on). Spec: `spa-delivery-plan.md`.
2. **One `fastlane → TestFlight` lane** — the only native pipeline worth having.
3. **Keep shrinking the collector into the box.**
