# Getting started — one room

**Built 2026-09-08/16, unreleased.** After the founder's letter, getting
started is a single chat rather than a page of steps. Supersedes
`agents/plan/getting-started-plan.md` (deleted) and the seven-step Home page
before it ([archived](../archive/getting-started-page-plan.md)).

Setup — pairing, the airlock, claiming the box — is a different subject and
stays with [onboarding-paradigm](onboarding-paradigm.md). This begins where
the app opens.

---

## The thesis

**The page paradigm regrew the thing it replaced.** A first-run Home state
shipped as sections that retire, was struck the same day as "Home with getting
started slapped on", and came back as a numbered seven-step page — a wizard
with a serif, whose list had already grown from four asks to seven rows. The
replacement is not a better page. It is a room: one seeded chat that opens
already speaking.

## What holds it up

- **One room, seeded at boot.** `chat_getting_started`, undeletable and
  un-retitled by id, the same way the interview's room is.
- **Four steps, and that is the ceiling.** Connect AI, introductions, connect
  your world, in your own words. One promise card after them that is not a
  step. Applets and the manual are the app's business once the door opens.
- **Steps are derived, never stored.** Done means a row exists. Skips are the
  only stored state. There is no progress pointer and no `current_step` — so
  there is nothing for a model to advance, and nothing to migrate. Beta
  testers needed no migration when this landed: derivation simply re-reads
  them.
- **The model is a guest who arrives after step 1.** Until the box can call a
  model the room is written entirely by the box — synthetic turns and cards,
  no model call. After that the model speaks in a narrow mode with a short
  allowlist.
- **Secrets never enter the transcript.** Sign-in, the BYO key, OAuth and
  pairing are cards talking to their own endpoints. No tool takes a credential
  as an argument.
- **The interview stays its own room.** Step 4 hands off rather than absorbing
  it: the interviewer is a witness with zero tools, and an operator chat with
  tools cannot share that transcript.

## The rule the design kept failing and relearning

**The column is the product. Nothing lives outside it but the door.** Five
separate attempts to show progress in the margins — corner marks, rails,
shapes beside the column — were all rejected. What survived is a numbered
eyebrow inside the thread, in the flow of the conversation itself. If a future
change wants a progress indicator somewhere other than the column, that is the
same idea for the sixth time.

## What was built and then deliberately removed

**The lock is gone.** The plan specified that the app stayed shut until the
box could call a model: a route guard, a `locked` flag on the state and its
client store, a suppressed sidebar, a bare tab strip. It shipped, and it was
deleted on 2026-09-15 (`c0b5dc35`) — the app is not closed off any more.

This is recorded because the deleted design was load-bearing in the plan's
prose and a reader could easily rebuild it by accident. The room is still the
first thing the app opens; it is no longer the only thing reachable.
`/dangerously-skip-onboarding` survives as the explicit exit.

## Open

- The lifeline drawing in the interview hand-off.
- The letter seam: the two notes handed over at the end are the founder's own
  copy, not generated.
