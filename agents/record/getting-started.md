# Getting started as a single chat

**Built 2026-09-08/16, unreleased.** After the founder's letter, getting
started is a single chat instead of a page of steps. This supersedes
`agents/plan/getting-started-plan.md` (deleted) and the seven-step Home page
before it ([archived](../archive/getting-started-page-plan.md)).

Setup — pairing, the airlock, claiming the box — is a different subject and
stays with [onboarding-paradigm](../build/onboarding-paradigm.md). This document
starts where the app opens.

---

## Why a chat replaced the page

Each page-based design grew back into the thing it replaced. A first-run Home
state shipped as sections that retire. It was rejected the same day as "Home
with getting started slapped on", and came back as a numbered seven-step page:
a wizard set in a serif, whose list had already grown from four asks to seven
rows. The replacement is one seeded chat that already contains the first
messages when it opens.

## Design

- **One chat, seeded at boot.** `chat_getting_started` cannot be deleted or
  retitled, keyed by id, the same way the interview's chat is.
- **Four steps at most.** The steps are: connect AI, introductions, connect
  your world, in your own words. One promise card follows them and is not a
  step. Applets and the manual are left to the rest of the app once setup is
  done.
- **Steps are derived from data, never stored.** A step is done when its row
  exists. Skips are the only stored state. There is no progress pointer and no
  `current_step`, so there is nothing for a model to advance and nothing to
  migrate. Beta testers needed no migration when this landed, because
  derivation re-reads their existing rows.
- **The model only takes part after step 1.** Until the box can call a model,
  the box writes every message in the chat itself — synthetic turns and cards,
  with no model call. After that the model speaks in a narrow mode with a short
  allowlist of tools.
- **Secrets never enter the transcript.** Sign-in, the BYO key, OAuth and
  pairing are cards that call their own endpoints. No tool takes a credential
  as an argument.
- **The interview stays a separate chat.** Step 4 hands off to it instead of
  absorbing it: the interviewer has zero tools, and an operator chat with
  tools cannot share that transcript.

## Progress stays inside the chat column

The chat column holds all of the UI; the only thing outside it is the exit
from setup. Five separate attempts to show progress in the margins — corner
marks, rails, shapes beside the column — were all rejected. The design that
was kept is a numbered eyebrow inside the thread, in the flow of the
conversation. If a future change wants a progress indicator somewhere other
than the column, it is the same idea a sixth time and should be rejected for
the same reasons.

## The lock was built and then removed

The plan specified that the app stayed shut until the box could call a model:
a route guard, a `locked` flag on the state and its client store, a suppressed
sidebar, a bare tab strip. It shipped, and it was deleted on 2026-09-15
(`c0b5dc35`). The app is no longer closed off.

This is written down because the deleted design was central to the plan's
prose, and a reader could easily rebuild it by accident. The getting-started
chat is still the first thing the app opens, but it is no longer the only
thing reachable. `/dangerously-skip-onboarding` remains as the explicit exit.

## Open questions

- The lifeline drawing in the interview hand-off.
- The letter seam: the two notes handed over at the end are the founder's own
  copy, not generated.
