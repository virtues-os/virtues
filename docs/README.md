# Virtues docs

Reference and design docs for the Virtues home-server appliance.

**Every doc is listed here.** The previous index covered ten of them, which meant
the other forty were invisible and rotted unread. If you add, rename, or retire a
doc, add or update its row — an unlisted doc is a doc nobody will find.

## How to read the Status column

Most of these are design documents, and a design document that doesn't say
whether it happened is worse than no document. Status is the first thing to check.

| Status | Means |
|---|---|
| **Current** | Describes the system as it is. Safe to act on. |
| **Partly built** | Some of it shipped. The doc says which parts; believe the doc over the plan. |
| **Planned** | Designed, agreed, not built. Nothing here exists yet. |
| **Parked** | Designed, deliberately not being worked on. |
| **Reference** | Measured findings or working notes, not a plan. True when written. |
| **History** | Superseded. Kept for the reasoning, not the content. |
| **⚠️** | Contains a claim that is not currently true. Read the banner before quoting it. |

Statuses come from each doc's own header where it has one. When work lands,
**fix the status in the doc and the row here in the same commit** — that is the
only discipline that keeps this file honest.

Docs whose subject was cut entirely live in [`archive/`](archive/) and are not
listed below.

---

## Architecture & system model

| Doc | Status | What it's for |
|---|---|---|
| [architecture.md](architecture.md) | Current | The implementation contract for the **applet** system: `function` / `view` runtimes, manifest↔SQL field ownership, dispatch + reconcile, the two applet roots. Authoring how-to is [`../applets/AUTHORING.md`](../applets/AUTHORING.md). |
| [applets-overhaul-plan.md](applets-overhaul-plan.md) | Planned | "A user-space systemd with an AI author." Design locked 2026-07-19. Supersedes architecture.md at the concept/UX layer; the execution engine stays. |
| [applet-authoring-plan.md](applet-authoring-plan.md) | Planned | Phase 3: chat intent → folder → check → reconcile → gate → enabled applet. The capability and param-schema contract. |
| [auth-model.md](auth-model.md) | Current | Pair-only auth: no passwords, no email, no magic links. Devices are the auth surface; `virtues sudo` gates the dangerous verbs. |
| [entitlement.md](entitlement.md) | Current | How a paying box gets AI and utility calls paid for: accounts, a rotatable device api_key, an append-only ledger, Stripe-webhook crediting. **§1 states the privacy posture plainly — read it before making any privacy claim.** |
| [virtues-api.md](virtues-api.md) | Current | The philosophy and the FAQ-ready copy, rewritten 2026-07-30 to claim only what ships: *we can see what your usage cost, we never keep what it was.* **Lift copy from here, not from its final section**, which preserves the retired voucher claims as history. |
| [deployment.md](deployment.md) | Current | The two shipping shapes — native Linux binary on the box, Docker on EC2 for atlas + api — and the systemd privilege split. |
| [design.md](design.md) | Current | Shell design constraints. Exists because the same visual mistakes kept recurring across sessions. |
| [composable-inference.md](composable-inference.md) | Current | Plan of record for inference composability: the two HTTP contracts (`/v1/embeddings` required, `/v1/rerank` optional) and how the index defends itself. |

## Networking & reach

| Doc | Status | What it's for |
|---|---|---|
| [../manual/operate/reach.md](../manual/operate/reach.md) | Current | **The accurate account of reach**, and the one to quote: paths, pairing, revocation, and exactly what the relay can and cannot see. Public docs page. |
| [relay-control-plane.md](relay-control-plane.md) | Design notes | Naming, auth, and revocation reasoning. Its SNI/HMAC mechanism was never built; the *iroh caveat* section is the honest statement of what the relay observes. |
| [networking-relay-tee.md](networking-relay-tee.md) | **Superseded** | The pre-iroh ADR: single-hop SNI relay, per-box ACME, browser-anywhere. Its privacy hardening — RAM-only, blinded tokens, unlinkability, audit — was **proposed and never built**. History only; do not quote. |
| [privacy-model.md](privacy-model.md) | **Superseded (transport half)** | Describes the pre-iroh secret-ownership table. Its *inference boundary* section is current and is the honest account of where data leaves the box. |
| [relay-walkthrough.html](relay-walkthrough.html) | **Superseded** | Visual lifecycle diagrams of the old SNI relay. Kept as a record. |
| [reach-reliability-plan.md](reach-reliability-plan.md) | Planned | "100% reachable whenever the box is up." Root cause is upstream iroh #4289; the fix is `network_change()` on `NWPathMonitor` plus a watchdog rebuild. |
| [map-atlas-plan.md](map-atlas-plan.md) | Current | The box serves and caches map tiles so the browser never talks to a tile provider — no location leak, and already-seen areas work offline. |

## Operations

| Doc | Status | What it's for |
|---|---|---|
| [recovery.md](recovery.md) | Current | **Workshop** operator runbook, rewritten against the code 2026-08-28 after an audit found ~19 stale claims (a config path no box has, three commands that don't exist, a dropped atlas column). Carries the operator-only material: the recovery command surface, the sudo gate, the diagnostic surface, and the described-but-not-implemented list. Owner-facing recovery is [`manual/operate/recovery.md`](../manual/operate/recovery.md), which wins on any disagreement. |
| [backup-plan.md](backup-plan.md) | Partly built | Surviving the loss of the box. Pillars 1–5 and the volume path landed; Phase A is the gate, and both ends of the pipeline are still missing. |
| [update-model.md](update-model.md) | Current | The north star for fleet updates: a thin native shell shipped rarely plus a fast web payload pushed freely, with a version contract between them. **Start here** — the three below are history and horizon. |
| [update-paradigm.md](update-paradigm.md) | Built | How one box moves between builds. Fully shipped; kept as the design record for the three real `virtues upgrade` failures that shaped it. Current behavior lives in `cli/upgrade.rs` + `api/updates.rs`. |
| [update-manifold-plan.md](update-manifold-plan.md) | Planned | Generalizes the box paradigm to the whole fleet, and adds the cross-component version negotiation the box doc never needed. |
| [update-identity-spine.md](update-identity-spine.md) | Built | Phase 1 of the manifold: every artifact states its `{version, sha, channel}` and the fleet shows on the Devices page. |
| [model-migration.md](model-migration.md) | Current | Runbook for changing the local embedding/rerank GGUFs. **Three places must agree** or the box serves one model while the runtime expects another. |
| [review-access-plan.md](review-access-plan.md) | Current | How an Apple reviewer who owns no box exercises the iOS app. Live coordinates and the pair code are deliberately **not** in this repo. |

## The day, the graph, and retrieval

| Doc | Status | What it's for |
|---|---|---|
| [the-day.md](the-day.md) | Partly built | Design spec for the Day Page — the life-mirror you read at night. The Four Questions, each with a different implementation maturity. |
| [event-timeline.md](event-timeline.md) | Current | How a day becomes a clean, gapless sequence of events out of incomplete, out-of-order, mutually contradictory evidence. The spine the day page renders. |
| [attention-plan.md](attention-plan.md) | Partly built | How the wiki learns which parts of a day mattered. A record has no tense — only calendar knows what time it is *about* — so aftermath is invisible by construction. Attention's four phases, what may be linked vs noted vs asserted, and the refuted list. |
| [timezone-model.md](timezone-model.md) | Current | Two timezones: the box's stable `home_timezone` plus a per-day user-location timezone. Implemented 2026-06-25. |
| [ir-notes.md](ir-notes.md) | Reference | Grounded map of the retrieval stack as it actually is, the non-obvious truths a full read exposed, and a ranked set of improvements with spikes. |
| [npu-hardware-findings.md](npu-hardware-findings.md) | Reference | Measured field report from running the embed/rerank stack on two edge NPUs. Every number from real silicon. Settles the board question. |

## Product & feature specs

| Doc | Status | What it's for |
|---|---|---|
| [notebooks-plan.md](notebooks-plan.md) | Partly built | Notebook as a workspace lens over the life-graph: a Library of materials, scoped grounded chat, source-level citations. |
| [researcher-plan.md](researcher-plan.md) | Planned | The researcher/PhD archetype, built complete in v1 — corpus, annotation, scholarly metadata, synthesis bridge. Extends notebooks-plan Phase D. |
| [bookmarks-plan.md](bookmarks-plan.md) | Planned | Capture, enrichment, and retrieval for `data_content_bookmark` — a table with a complete ontology descriptor and zero producers. |
| [references.md](references.md) | Planned | One primitive for `@`, peek, and open, so a person / place / file / page stops being rendered by a different code path each time. |
| [codemirror.md](codemirror.md) | Current | The page editor's document model. The document **is** markdown in a Yjs `Y.Text`; decorations provide live preview. No intermediate AST. |
| [chat-ux-roadmap.md](chat-ux-roadmap.md) | Planned | The chat surface overhaul, in independently shippable tracks A–L, each ending in a verification gate. |
| [ui-overhaul-plan.md](ui-overhaul-plan.md) | Planned | Nineteen layout/settings/design items triaged against the codebase, each marked ship / spike / defer / drop. |
| [onboarding.md](onboarding.md) | Current | "Box in hand" → "Virtues earning its keep," as built — rewritten against the code 2026-08-28. **The one Bluetooth wire (the RPC table), the phrase vs the pair code, the display's seven states, the four onboarding screens, and the DIY handoff.** It previously stacked three generations of doctrine behind a corrections preamble; the corrections are folded in and the superseded generations are kept only as failure history. Intent is [onboarding-paradigm.md](onboarding-paradigm.md). |
| [appliance-image.md](appliance-image.md) | Partly built | How a Dragon becomes a shippable unit: the card-boots/NVMe-roots truth read off real hardware, the OS-on-card/data-on-NVMe split and why, `tools/build-dragon.sh`, the deprovision → `image-check` → `dd` gate, and the three questions that need the bench. |
| [plaid-plan.md](plaid-plan.md) | Partly built | Plaid as the *general* finance collector — FinanceKit reaches only Apple's own products, so Plaid is the path that has to work for everyone else. |

## Clients & collection

| Doc | Status | What it's for |
|---|---|---|
| [cross-platform-apps-plan.md](cross-platform-apps-plan.md) | Planned | Shipping the viewer on Windows, Linux, and Android. Collectors deliberately out of scope, with the seams left in place for them. |
| [mobile-ux-plan.md](mobile-ux-plan.md) | Partly built | Pragmatic mobile next steps: keep the tab strip on phone, block split view, keyboard work gets a spike first. Edge-to-edge safe-area theming already shipped. |
| [spa-delivery-plan.md](spa-delivery-plan.md) | Ready to schedule | One UI-delivery architecture for phone and Mac: baked bundle, OTA overlay pulled from the box, local data. Unlocks the offline editing already built into Yjs, and makes UI-ahead-of-box skew impossible. |
| [mac-presence-plan.md](mac-presence-plan.md) | Planned | `data_activity_app_usage` is close to an inversion of the truth — real focused work is invisible while artifacts are the headline numbers. |
| [audio-collector-plan.md](audio-collector-plan.md) | Partly built | Porting continuous mic capture to Tauri. The hardest collector: iOS audio-session lifecycle, and the only continuous, high-volume, binary stream. |
| [data-durability.md](data-durability.md) | Partly built | Three-pass audit of the iOS → box ingestion path against the stated "zero silent data loss" promise, split into a data-integrity track and a background-reliability track. |
| [ios-overhaul-plan.md](ios-overhaul-plan.md) | History | Full audit of the **native Swift iOS app**, which has since been deleted — the live iOS app is the Tauri build of `apps/web`. The delivery, durability, and narrative findings still transfer; anything about Swift views does not. |

## Legal

**The operative policies live in the `website` repo** (`src/legal/virtues-os/`)
and are published at `virtues.com/privacy` and `/terms`. They are not mirrored
here — an unreviewed second copy in a public repo is quotable as posted policy,
which is exactly the trap the drafts that used to sit in `docs/legal/` created.

Use of *this repository's source* is governed solely by [`LICENSE`](../LICENSE)
(BUSL-1.1 + MIT), which is independent of those policies.
