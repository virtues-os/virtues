# The Event Timeline

How a person's day becomes a clean, gapless sequence of events — and why the
architecture is far simpler than it first appears.

## The problem, stated honestly

A day has to be reconstructed from exhaust: GPS, ambient audio, messages,
calendar, heart rate. That evidence is **incomplete, out-of-order, and noisy**.
Every source lies a little, in its own direction:

- the calendar says 2:00; you joined at 2:07 and it ran to 3:15
- GPS drifts, marks you "home" 200m early
- a transcription is timestamped when it *uploaded*, not when you spoke
- the phone was off from 1–4pm, so that window has nothing at all

There is no table with "events" in it. An event boundary is **latent** — it
exists only where several weak, lying signals happen to agree that something
changed. So the job is not to *read* boundaries; it is to *infer* them by
**fusion**, the way a detective triangulates a time of death from four
contradictory witnesses.

This doc is the design for doing that cleanly.

## The one decision that collapses the complexity

The tempting architecture is an hourly agent that incrementally detects
boundaries as the day happens, plus a nightly agent that finishes the job. That
path drags in a swamp of hard problems: settle horizons, dirty-window
re-derivation, boundary-anchored windows, retroactive re-labeling versus
re-cutting, event-ID churn.

**All of it evaporates once you notice the raw evidence persists.**

Because the raw rows are immutable and kept, a **nightly** pass — running over
the *complete* day, with late uploads settled and sleep resolved — can always
reconstruct the timeline better than any provisional intra-day pass. Anything an
hourly agent writes, the nightly pass would overwrite with a more-informed cut.

And there is a second, deeper reason the synthesis must wait for the whole day:
**meaning is relative, so it is retrospective.** You cannot know whether the last
hour was novel, calm, weird, or ordinary without the rest of the day to measure it
against — a z-score needs the whole distribution. An hour has no *relative* meaning
until the day it belongs to exists. So the nightly pass waits not only for complete
*evidence* but for the complete *distribution*.

So the hourly agent produces **no durable data**. Its only possible value is
showing the user something live during the day — and that does not require an
LLM at all.

> **Nightly is the authority. There is no hourly agent.**

Two clean pieces, no reconciliation between them:

1. **The nightly timeline** — the official events. One pass per completed day,
   complete evidence, gapless, frozen.
2. **The live "today so far" view** — deterministic, zero-LLM, disposable. Just
   renders what is already known as the day happens.

## Two clocks, and that is the whole orchestration

It looks like a fragile pipeline — "step A must finish before step B or we are
broken." It is not. Every transformation is **idempotent, backlog-driven, and
self-healing** (the visit rollup was proven so: run it three times, get 13/13/13).
So nothing waits on anything. Each step runs on its own clock, does whatever work
is ready, and if its inputs are not there yet it does nothing and picks them up the
next tick. The lattice **converges**; it is not orchestrated.

**Fast clock (~15 min, all day): resolve raw → derived.**
Transcription as recordings land; visits as points land. Independent, idempotent,
order-free. This feeds the live view.

**Slow clock (nightly): make meaning, on complete and comparable data.**
Sessionize audio → label sessions → the detective → score → narrate. One
authoritative pass.

You do not build a precise sequence. You run a couple of cheap self-healing
refreshers frequently, and one smart synthesis nightly. The latency between them is
not mess — it is the honest shape of meaning accreting: a visit is not a visit
until you leave, a session is not done until it ends, a day is not a day until it
is over. Each layer settles only once the one beneath it has.

## The live view (intra-day, zero-AI)

During the day, the user sees a timeline built **only from source rows that bound
themselves** — a real start, a real end, and a self-evident label, with no model
in the loop:

- **location visits** — labelled by place name
- **calendar events** — labelled by title
- **sleep** — labelled by itself

That is the whole list. Conversation sessions are **not** here, deliberately:
audio sessionization is a *nightly* step (it needs closed sessions and resolved
visits, both complete only by nightfall), so conversation-blocks land in the
settled day, not the live strip. Workouts, app-sessions, messages, heart rate
remain **texture, not blocks** — they colour an event but cannot bound one on
their own, and inferring a block from them needs the detective. The live view
never guesses. It shows the skeleton it can prove ("Home, 00:04–03:07 · Blue
Bottle, 09:00–now") and leaves the rest blank until morning.

This view cannot hallucinate, cannot drift, and costs nothing, because there is
no model in it. The polished, fused, narrated day arrives the next morning — the
photo, developed overnight.

## The nightly pass (the detective)

Once per completed day, one agent reconstructs the authoritative timeline.

### Gapless, with Unknown as a first-class event

The timeline covers 24 hours with no holes. Where the evidence genuinely does not
support a classification, the block is an **`Unknown` event** covering that span —
not a gap, not a fabrication. A day that is mostly `Unknown` with three clear
events is more truthful than one padded with five speculative ones. Dead zones
(phone off, driving in silence) are `Unknown`, and no architecture changes that:
absence of evidence is not evidence.

### Candidate changepoints, then fusion

Boundaries are not read from one signal. Every source contributes **candidate
changepoints** — a visit edge, a calendar edge, a sleep transition, an
audio-topic shift, a long idle gap, an HR regime change. None is a boundary; each
is a *"something might have shifted here"* flag. Where flags **cluster in time**,
the boundary is strong; where one stands alone, it is weak.

The agent fuses the local evidence around each cluster and decides the *actual*
boundary and label, reconciling the lies (calendar 2:00, arrival 2:05, meeting-talk
2:08 → a boundary around 2:05). **How many liars make a truth is not
deterministic and must not be** — it is a judgement over a rich tapestry, which is
exactly the thing an LLM is for and a threshold is not.

### Features by default, raw on demand

A 22-hour recording day is ~300k tokens of raw transcript. It technically fits a
1M window, but it should never be sent: it is wasteful, and model attention
degrades across a haystack that large.

So the agent reads a **compressed dossier**, not a data dump. Each source arrives
as its *feature*, not its raw text:

- a transcription → its Gemini-made title + summary (not the 5-minute transcript)
- a visit → its place + duration (not the GPS points)
- messages → counts and threads (not bodies)

The whole day compresses to ~10–20k tokens. When a boundary is ambiguous, the
agent **drills down agentically** — pulling the raw evidence for that one window.
Detective with an indexed case file, requesting specific documents only when the
summary is not enough.

### The last 14 days, as context

The agent is given the **previous ~14 days of event summaries and day timelines**
(~140 sentences — trivially cheap). This is the case history, and it does three
things at once:

1. **Disambiguates** — "Unknown, 2–4pm, a Tuesday" reads differently when the last
   fortnight shows a standing Tuesday pattern.
2. **Grounds narrative novelty** — "you did *not* go to the office today" is only
   a remark against a visible routine.
3. **Compounds** — the detective gets better the longer it works the case.

This replaces any separate persisted "memory file": it is just retrieval of
derived data already on hand, so there is nothing to maintain or let drift.

### Show your work

The inherent uncertainty is turned into the feature. When the user taps an event,
the timeline explains its reasoning: *"Meeting, ~2–3pm — your calendar, your
arrival at the office, and the audio all agree."* The user sees *why*, and can
correct it — and a user's correction is the highest-value evidence there is.

## Audio sessions: the detective's ears

Raw audio arrives as 5-minute recorder chunks — hundreds a day, each transcribed
and titled in isolation. That granularity is a recorder artifact, not a unit of
life, and it must roll up into **sessions** (`data_communication_transcription`
becomes the session, mirroring `location_point → location_visit`) before the
detective reads it. This is a **nightly** step.

**Boundaries come from acoustic context, never topic.** The tempting signal —
embedding/topic distance between chunks — is exactly wrong: topic drifts wildly
*within* a single context ("HDMI screens → shipping → lunch → your date", all at
one desk with one colleague, is one session). What actually marks a context change
is **who is around and how loud it is**, and both are already measured:

- **`average_db_level`** (on `data_audio_recording`) — a car, a quiet room, a loud
  restaurant, a lull all read differently.
- **`speaker_count`** (on the transcription) — bucketed to {silent 0, solo 1, dyad
  2, group 3+}, because raw diarization is noisy (it will claim 40 speakers).

Run **PELT changepoint detection** (offline, deterministic, O(n), no LLM) over the
`(db, speaker-bucket)` series, speaker-weighted. Verified on a real day: it cut 271
chunks into ~24 coherent context blocks — writing, a restaurant, a car with music,
~10 hours of sleep-with-a-fan as *one* block, a bus ride, a meal, distinct
conversations. Topic drift stayed inside its session; a real shift got its own. No
embeddings, no rerank, no topics — just db + speakers + PELT.

**Bias toward MORE sessions, not fewer.** These are *clues for the detective, not
verdicts*. The detective can merge boundaries it was handed; it can never split one
it was never told about. So over-segmentation is recoverable and under-segmentation
is lost information — tune the penalty toward recall, stopping only where a boundary
becomes noise (a single-chunk diarization blip).

**Labels are context-aware, not per-chunk.** A 5-minute fan recording is correctly
titled "Steady Engine Hum" — with no context. No better transcription model fixes
that; it still just *hears a fan*. The fix is to label at the **session** level with
the facts the audio cannot contain: local time, duration, speaker profile, and
**place from the (now clean) visits**. `[quiet, no speakers, 10h, at home,
overnight]` becomes "Sleeping", not "Engine Hum". Give the AI the facts it cannot
hear. The raw chunk titles stay as detective *clues* (it interprets them); the
context-aware session label is what a human sees.

## Ordering: novelty comes after the summary

There are two distinct kinds of novelty, and both sit **after** the events have
summaries:

1. **Computed novelty** (`novelty_z`, LOF, topic/entity novelty — the z-scores
   that drive the charts) **embeds `event_summary` as its vector**. It therefore
   *cannot* run until segmentation has produced those summaries. Hard dependency.
2. **Narrative novelty** ("today broke the routine") is **discovered
   agentically** by the nightly agent, because it holds the last 14 days in
   context and notices the deviation itself.

The full shape, both clocks:

```
FAST CLOCK (all day, ~15 min):  transcribe chunks · resolve visits
                                → feeds the live view (visits + calendar + sleep)

NIGHTLY (the authority):
  1  sessionize    PELT over db + speaker-count (audio) · gap-based (iMessage).
                   Purely MECHANICAL — boundaries + stitched chunk summaries +
                   speaker profile. No LLM. No titles, no per-session summary:
                   all labelling is the detective's job in step 2.
  2  detective     a dossier of FEATURES (session summaries, visit labels, message
                   counts) + agentic drill-down into raw text only where a boundary
                   is ambiguous → 8-16 gapless events, Unknown for real gaps.
                   This is where meaning is made: "quiet, no speakers, 10h, at home,
                   overnight" becomes "Sleeping".
  3  score         novelty / autonomic / topic-entity. Embeds event summaries, so
                   it runs after step 2. Baseline is an 84-DAY (12-week) rolling
                   window — relative measures need statistical power.
  4  narrate       events + scores + a 14-DAY case file → prose.
```

**Two windows, two jobs — do not conflate them.** Scoring (step 3) looks back
**84 days** for a statistical baseline. The narrative case file (step 4) is
**14 days** of recent event summaries for pattern-awareness. Long history for the
z-scores; recent history for the story.

## The one hard dependency: clean rollups

None of this works on forged witnesses. The nightly detective is only as good as
the evidence it fuses, and today two of the richest signals arrive as **sensor
cadence, not sessions**:

- **audio** is 271 five-minute transcription rows per day — the recorder's
  chunking, not conversations. There is no sessionizer merging consecutive
  recordings into activities.
- **location** is emitted as overlapping per-batch "visits" — the visit id hashes
  `start_time`, so a re-run on a sliding window mints a new row instead of
  extending the open stay. One 3-hour stay becomes a dozen overlapping rows.

These are the same doctrine violated twice: **raw sensor samples must roll up
into sessions before anything reasons over them** (the signal-vs-artifact rule).
Until the rollups produce real visits and real audio sessions, the timeline is a
detective reading echoes.

**Fix the rollups first.** They are the prerequisite for everything above.

## What this model does and does not solve

- **Gaps** are not an architecture problem; they are an evidence problem. Where
  there is no evidence, the event is `Unknown`. Where evidence conflicts, the
  detective shows its work. This is the honest shape of reconstructing a life from
  exhaust — not a flaw to engineer away.
- **The convergence judgement** ("how many liars make a truth") is deliberately
  non-deterministic and lives in the agent, tuned against real days, never a fixed
  threshold.
- **Provisional-versus-settled** disappears: the live view is deterministic and
  disposable; the nightly timeline is authoritative and frozen. There is no
  in-between to reconcile.
- **Late evidence** that arrives after the nightly pass (rare — more than a day
  late) marks the window for re-derivation as an *offer*, never a silent rewrite
  of prose the user may have read.

## Summary

> **Two clocks.** All day, a fast clock resolves raw → derived (transcription,
> visits) — idempotent, self-healing, order-free — and feeds a deterministic,
> zero-LLM live view (visits + calendar + sleep). At night, one authoritative pass
> makes meaning on complete and comparable data: **sessionize** (mechanical PELT +
> stitch, no LLM) → **the detective** (a dossier of features + drill-down → gapless
> 8-16 events, `Unknown` for real gaps, all labelling here) → **score** (84-day
> relative baseline) → **narrate** (with a 14-day case file). Nightly is
> authoritative because evidence *and the distribution it is measured against* are
> only complete once the day is over. **Clean session rollups are the one
> prerequisite** — the detective is only as good as its witnesses.
