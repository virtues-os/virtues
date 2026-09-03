# Build — how it must be done

**Normative and permanent.** These say what must be true when you write code
here: contracts, vocabularies, style, and the runbooks for operations we
perform ourselves. They are maintained rather than dated — when the system
changes, these change with it.

Read the ones touching your area BEFORE you write. They exist because the same
mistakes kept recurring across sessions.

**These never publish.** They are instructions to whoever is building, only
meaningful with the codebase open. `virtues.com/docs` is the manual, written
for people running a box; see [`../docs/`](../docs/).


| Doc | Status | What it's for |
|---|---|---|
| [appliance-image.md](appliance-image.md) | Partly built | How a Dragon becomes a shippable unit: the card-boots/NVMe-roots truth read off real hardware, the OS-on-card/data-on-NVMe split and why, `tools/build-dragon.sh`, the deprovision → `image-check` → `dd` gate, and the three questions that need the bench. |
| [architecture.md](architecture.md) | Current | The implementation contract for the **applet** system: `function` / `view` runtimes, manifest↔SQL field ownership, dispatch + reconcile, the two applet roots. Authoring how-to is [`../applets/AUTHORING.md`](../applets/AUTHORING.md). |
| [codemirror.md](codemirror.md) | Current | The page editor's document model. The document **is** markdown in a Yjs `Y.Text`; decorations provide live preview. No intermediate AST. |
| [composable-inference.md](composable-inference.md) | Current | Plan of record for inference composability: the two HTTP contracts (`/v1/embeddings` required, `/v1/rerank` optional) and how the index defends itself. |
| [deployment.md](deployment.md) | Current | The two shipping shapes — native Linux binary on the box, Docker on EC2 for atlas + api — and the systemd privilege split. |
| [design.md](design.md) | Current | Shell design constraints. Exists because the same visual mistakes kept recurring across sessions. |
| [entitlement.md](entitlement.md) | Current | How a paying box gets AI and utility calls paid for: accounts, a rotatable device api_key, an append-only ledger, Stripe-webhook crediting. **§1 states the privacy posture plainly — read it before making any privacy claim.** |
| [glossary.md](glossary.md) | — | _Needs a line._ |
| [mac-plan.md](mac-plan.md) | — | _Needs a line._ |
| [model-migration.md](model-migration.md) | Current | Runbook for changing the local embedding/rerank GGUFs. **Three places must agree** or the box serves one model while the runtime expects another. |
| [narrative-identity.md](narrative-identity.md) | — | _Needs a line._ |
| [onboarding.md](onboarding.md) | Current | "Box in hand" → "Virtues earning its keep," as built — rewritten against the code 2026-08-28. **The one Bluetooth wire (the RPC table), the phrase vs the pair code, the display's seven states, the four onboarding screens, and the DIY handoff.** It previously stacked three generations of doctrine behind a corrections preamble; the corrections are folded in and the superseded generations are kept only as failure history. Intent is [onboarding-paradigm.md](onboarding-paradigm.md). |
| [recovery.md](recovery.md) | Current | **Workshop** operator runbook, rewritten against the code 2026-08-28 after an audit found ~19 stale claims (a config path no box has, three commands that don't exist, a dropped atlas column). Carries the operator-only material: the recovery command surface, the sudo gate, the diagnostic surface, and the described-but-not-implemented list. Owner-facing recovery is [`manual/operate/recovery.md`](../manual/operate/recovery.md), which wins on any disagreement. |
| [review-access-plan.md](review-access-plan.md) | Current | How an Apple reviewer who owns no box exercises the iOS app. Live coordinates and the pair code are deliberately **not** in this repo. |
| [virtues-api.md](virtues-api.md) | — | The philosophy and the FAQ-ready copy for the paid API — what may and may not be claimed. Lift wording from here; it is maintained, not dated. |
| [voice.md](voice.md) | — | _Needs a line._ |
