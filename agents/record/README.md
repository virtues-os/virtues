# Record — what happened

**Descriptive and permanent.** Audits, measured findings, and the design
records of things that shipped. True when written, and dated for that reason:
editing one to match today's system would falsify an observation rather than
maintain a document. Supersede by writing a new record, never by revising.

**These do not publish.** The website stopped syncing `agents/` on 2026-08-28;
the repo is public, so a record is still readable on GitHub — write it as if a
stranger will. Every record has a row here; `tools/check-manual.py` enforces it.


| Doc | What it's for |
|---|---|
| [applets-surface-audit.md](applets-surface-audit.md) | What phases 1–3 of the applets overhaul actually landed as, read back against what the plan specified. Every item is a contract severed in implementation, a regression from the plan's own UI spec, or drift the plan predicted and the sweep never reached. |
| [article-resolution.md](article-resolution.md) | Every subject has an article and one editor writes them all — one constitution plus a brief per kind. A first draft is one-shot; a revision is agentic, in the applet runner's never-used agent phase. **Ownership never flips**: the server refuses a machine edit that loses a sentence the person wrote. Overrules wiki-plan §10. |
| [auth-model.md](auth-model.md) | Pair-only auth: no passwords, no email, no magic links. Devices are the auth surface; `virtues sudo` gates the dangerous verbs. |
| [data-durability.md](data-durability.md) | Three-pass audit of the iOS → box ingestion path against the stated "zero silent data loss" promise, split into a data-integrity track and a background-reliability track. |
| [device-version-update-audit.md](device-version-update-audit.md) | Four parallel sweeps of the Devices page, the version-identity inventory, every update mechanism, and the user-facing status surfaces. The diagnosis is that every surface renders a *different* truth honestly — nothing lies, and nothing is authoritative. |
| [display-hardware.md](display-hardware.md) | Measured behavior of the Dragon Q6A panel: the lying EDID, the ddcutil prohibition, the Q6A bootloader finding, and the only surviving copy of the captured EDID blob. |
| [editor-kernel-2026-09-14.md](editor-kernel-2026-09-14.md) | CodeMirror is the one editor kernel; the document is a markdown string on every surface. Measured ProseMirror and TipTap markdown round trips on 163 documents, the three silent failure classes, and why reformat-on-open disqualifies a tree editor for a product where a model edits the same string. |
| [entitlement-split.md](entitlement-split.md) | 0017 made linking identity rather than billing and no consumer was told: a free account read as Active for three days. The failure class (a semantic decoupling is a contract change), what was fixed 2026-09-03, and what is still open. |
| [getting-started.md](getting-started.md) | After the founder's letter, getting started is a single chat rather than a page of steps — four derived steps, no stored progress pointer, and the model arriving as a guest only once the box can call one. **The lock it originally specified was built and then deliberately deleted**; the room is the first thing the app opens, not the only thing reachable. |
| [ir-notes.md](ir-notes.md) | Grounded map of the retrieval stack as it actually is, the non-obvious truths a full read exposed, and a ranked set of improvements with spikes. |
| [map-tiles.md](map-tiles.md) | Why the maps have no basemap today: CARTO went key-only and its placeholder tiles were cached as real ones, and the OpenFreeMap stopgap leaked every viewed street, so it was removed. The box's own Protomaps files replace both. |
| [npu-hardware-findings.md](npu-hardware-findings.md) | Measured field report from running the embed/rerank stack on two edge NPUs. Every number from real silicon. Settles the board question. |
| [observability.md](observability.md) | How a box explains itself: one vocabulary, a key on every log line so a failed run can find its own lines, and one door a client reports its own failures through. Also the honest account of the crash beacon — the one thing a running box sends anywhere — which now records locally first and sends panics and errors rather than the last fifty lines whatever they were. Carries the traps, including the two that cost a deploy round each. |
| [one-wire-plan.md](one-wire-plan.md) | Setup end to end with **zero typed codes** — phrase, sign-in, Wi-Fi, grant, codeless pair, auto-update — rehearsed on a bench box over the same client-isolating network that produced the original freeze. Also the reason a prerelease prefix must track the next unreleased stable. |
| [privacy-model.md](privacy-model.md) | Describes the pre-iroh secret-ownership table. Its *inference boundary* section is current and is the honest account of where data leaves the box. |
| [resolution-audit.md](resolution-audit.md) | Six read-only sweeps across entity resolution, narrative identity, summarization, retrieval, the wiki schema and the docs. Prompted by finding a table written since a migration and read by nothing. Nothing here is fixed, and its line numbers are stale by construction — verify before acting. |
| [schema-audit-2026-08-28.md](schema-audit-2026-08-28.md) | Full-schema audit against the code, 2026-08-28: 6 dead tables, ~60 dead columns, 10 live bugs — several dead-since-rename. The FIX and GUARD tiers landed the same day; the do-list driving the rest is its checklist, now retired. |
| [timezone-model.md](timezone-model.md) | Two timezones: the box's stable `home_timezone` plus a per-day user-location timezone. Implemented 2026-06-25. |
| [update-paradigm.md](update-paradigm.md) | How one box moves between builds. Fully shipped; kept as the design record for the three real `virtues upgrade` failures that shaped it. Current behavior lives in `cli/upgrade.rs` + `api/updates.rs`. |
| [why-this-was-hard-to-debug.md](why-this-was-hard-to-debug.md) | Two bugs cost hours that should have cost minutes: applet subprocesses that could not write the lake, and the desktop app's own IPC refused by its ACL. Both are fixed; the document is about why neither was *visible*, which is the part that recurs. |
