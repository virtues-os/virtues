# agents/

Everything written for whoever is **building** Virtues — people and agents
alike. The documentation written for people **running** a box lives in
[`../docs/`](../docs/) and publishes at `virtues.com/docs`.

This directory used to be `docs/`, which meant the folder named "docs" held no
documentation and the actual manual had nowhere to go. Splitting by audience
fixed both.

## The three genres

Two questions decide where a document belongs, and they generate the whole
scheme:

**Am I describing or prescribing?** **Will this stop being true when we ship?**

| | Describes | Prescribes |
|---|---|---|
| **Survives shipping** | [`record/`](record/) | [`build/`](build/) |
| **Dies on shipping** | — | [`plan/`](plan/) |

The empty cell is empty for a reason: a description of something temporary is
just a record of it. Three genres, not four.

| Directory | What it holds | Edited? | Publishes? |
|---|---|---|---|
| [`build/`](build/) | Contracts, vocabularies, style, runbooks. How it must be done. | Yes — maintained to stay true | No |
| [`record/`](record/) | Audits, measured findings, design records of shipped work. | No — editing falsifies it | **Yes** |
| [`plan/`](plan/) | Designs for things being built. | Yes, until it ships | No |
| [`archive/`](archive/) | Superseded. Kept for the reasoning. | No | No |

[`archive/`](archive/) is not a fourth genre — it is where any of the three go
when they stop being true.

## The rules that keep it from rotting again

- **A plan is deleted when the thing ships.** What survives is a record plus a
  manual page. `docs/` grew to 63 files because nothing ever left it; a plan
  with no death condition eventually gets read as a description of the system.
- **Every doc is listed** in its directory's README. An unlisted doc is one
  nobody finds — and for `record/`, one that does not publish at all.
  `tools/check-manual.py` enforces this.
- **Write against the code, never against another doc.** Three separate audits
  on 2026-08-28 found the docs here wrong in ways that had already reached a
  user-facing page: a config path that does not exist on a real box, SQL
  against a dropped column, a relay privacy claim that was never true. Verify
  before you repeat.
- **Status belongs to `plan/` only.** A build doc does not need "Current" — it
  is maintained or it is deleted. A record does not need it either; it is dated
  and immutable.

## Where the public documentation is

[`../docs/`](../docs/) — the manual, for people running a box. It publishes
from `main`, so it describes what boxes actually run. Its contract is in
[`../docs/README.md`](../docs/README.md).
