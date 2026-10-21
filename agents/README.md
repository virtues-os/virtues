# agents/

Everything written for whoever is **building** Virtues — people and agents
alike. The documentation written for people **running** a box lives in
[`../docs/`](../docs/) and publishes at `virtues.com/docs`.

This directory used to be `docs/`, which meant the folder named "docs" held no
documentation and the actual manual had nowhere to go. Splitting by audience
fixed both.

## The three kinds of document

Two questions decide where a document belongs:

**Am I describing or prescribing?** **Will this stop being true when we ship?**

| | Describes | Prescribes |
|---|---|---|
| **Survives shipping** | [`record/`](record/) | [`build/`](build/) |
| **Dies on shipping** | — | [`plan/`](plan/) |

The empty cell is empty because a description of something temporary is just
a record of it, so it goes in `record/`. That leaves three kinds.

| Directory | What it holds | Edited? |
|---|---|---|
| [`build/`](build/) | Contracts, vocabularies, style, runbooks. How it must be done. | Yes — maintained to stay true |
| [`record/`](record/) | Audits, measured findings, design records of shipped work. | No — editing it would make it inaccurate |
| [`plan/`](plan/) | Designs for things being built. | Yes, until it ships |
| [`archive/`](archive/) | Superseded. Kept for the reasoning. | No |

None of it publishes: the website stopped syncing `agents/` on 2026-08-28. The
repo is public, so all of it is readable on GitHub.

[`archive/`](archive/) is not a fourth kind; documents of any of the three
kinds move there when they stop being true.

## Rules that keep these docs accurate

- **A plan is deleted when the thing ships.** What survives is a record plus a
  manual page. `docs/` grew to 63 files because nothing ever left it; a plan
  that is never deleted eventually gets read as a description of the system.
- **Every doc is listed** in its directory's README, and every relative link
  resolves. An unlisted doc is one nobody finds; a doc moved without a link
  sweep strands every citation of it. `tools/check-manual.py` enforces both.
- **Write against the code, never against another doc.** Three separate audits
  on 2026-08-28 found the docs here wrong in ways that had already reached a
  user-facing page: a config path that does not exist on a real box, SQL
  against a dropped column, a relay privacy claim that was never true. Verify
  before you repeat.
- **Only `plan/` docs carry a status.** A build doc does not need "Current": it
  is either kept up to date or deleted. A record does not need it either; it is dated
  and immutable.

## Where the public documentation is

[`../docs/`](../docs/) — the manual, for people running a box. It publishes
from `main`, so it describes what boxes actually run. Its contract is in
[`../docs/README.md`](../docs/README.md).
