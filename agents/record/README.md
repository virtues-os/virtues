# Record — what happened

**Descriptive and permanent.** Audits, measured findings, and the design
records of things that shipped. True when written, and dated for that reason:
editing one to match today's system would falsify an observation rather than
maintain a document. Supersede by writing a new record, never by revising.

**These are what publish** to `virtues.com/docs/notes`. Every row here becomes
a page, indexed from this table, so a record missing a row does not publish.


| Doc | Status | What it's for |
|---|---|---|
| [applets-surface-audit.md](applets-surface-audit.md) | — | _Needs a line._ |
| [auth-model.md](auth-model.md) | Current | Pair-only auth: no passwords, no email, no magic links. Devices are the auth surface; `virtues sudo` gates the dangerous verbs. |
| [data-durability.md](data-durability.md) | Partly built | Three-pass audit of the iOS → box ingestion path against the stated "zero silent data loss" promise, split into a data-integrity track and a background-reliability track. |
| [device-version-update-audit.md](device-version-update-audit.md) | — | _Needs a line._ |
| [event-timeline.md](event-timeline.md) | Current | How a day becomes a clean, gapless sequence of events out of incomplete, out-of-order, mutually contradictory evidence. The spine the day page renders. |
| [ir-notes.md](ir-notes.md) | Reference | Grounded map of the retrieval stack as it actually is, the non-obvious truths a full read exposed, and a ranked set of improvements with spikes. |
| [lsi-plan.md](lsi-plan.md) | — | _Needs a line._ |
| [map-atlas-plan.md](map-atlas-plan.md) | Current | The box serves and caches map tiles so the browser never talks to a tile provider — no location leak, and already-seen areas work offline. |
| [npu-hardware-findings.md](npu-hardware-findings.md) | Reference | Measured field report from running the embed/rerank stack on two edge NPUs. Every number from real silicon. Settles the board question. |
| [onboarding-paradigm.md](onboarding-paradigm.md) | Current | **The settled model, and the doc the other two defer to** — one phrase that is both the Bluetooth setup key and the recovery key, reset as the only recovery, three relationships in one session, and *atlas may carry, never grant*. Audited against the code 2026-08-28. Read it before changing anything in this area; a change that contradicts it needs a deliberate revision, not an exception. |
| [one-wire-plan.md](one-wire-plan.md) | — | _Needs a line._ |
| [privacy-model.md](privacy-model.md) | Superseded (transport half) | Describes the pre-iroh secret-ownership table. Its *inference boundary* section is current and is the honest account of where data leaves the box. |
| [resolution-audit.md](resolution-audit.md) | — | _Needs a line._ |
| [the-day.md](the-day.md) | Partly built | Design spec for the Day Page — the life-mirror you read at night. The Four Questions, each with a different implementation maturity. |
| [timezone-model.md](timezone-model.md) | Current | Two timezones: the box's stable `home_timezone` plus a per-day user-location timezone. Implemented 2026-06-25. |
| [update-identity-spine.md](update-identity-spine.md) | Built | Phase 1 of the manifold: every artifact states its `{version, sha, channel}` and the fleet shows on the Devices page. |
| [update-model.md](update-model.md) | Current | The north star for fleet updates: a thin native shell shipped rarely plus a fast web payload pushed freely, with a version contract between them. **Start here** — the three below are history and horizon. |
| [update-paradigm.md](update-paradigm.md) | Built | How one box moves between builds. Fully shipped; kept as the design record for the three real `virtues upgrade` failures that shaped it. Current behavior lives in `cli/upgrade.rs` + `api/updates.rs`. |
| [virtues-api.md](virtues-api.md) | Current | The philosophy and the FAQ-ready copy, rewritten 2026-07-30 to claim only what ships: *we can see what your usage cost, we never keep what it was.* **Lift copy from here, not from its final section**, which preserves the retired voucher claims as history. |
| [why-this-was-hard-to-debug.md](why-this-was-hard-to-debug.md) | — | _Needs a line._ |
