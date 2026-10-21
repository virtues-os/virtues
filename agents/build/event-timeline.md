# The Event Timeline

How a person's day becomes a clean, gapless sequence of events, and why the
architecture is simpler than it first looks.

## The problem

A day has to be reconstructed from side-effect data: GPS, ambient audio,
messages, calendar, heart rate. That evidence is **incomplete, out-of-order, and
noisy**. Every source is a little wrong, each in its own direction:

- the calendar says 2:00; you joined at 2:07 and it ran to 3:15
- GPS drifts and marks you "home" 200m early
- a transcription is timestamped when it *uploaded*, not when you spoke
- the phone was off from 1–4pm, so that window has no data at all

No table contains "events". An event boundary is **latent**: it exists only
where several weak, inaccurate signals agree that something changed. The job is
therefore to *infer* boundaries by **fusing** those signals, because no single
source records them directly.

This doc describes the design for doing that.

## The decision that removes most of the complexity

The obvious architecture is an hourly agent that detects boundaries as the day
happens, plus a nightly agent that finishes the job. That design brings in a set
of hard problems: settle horizons, dirty-window re-derivation, boundary-anchored
windows, retroactive re-labeling versus re-cutting, and event-ID churn.

**None of those problems apply once you notice that the raw evidence is kept.**

The raw rows are immutable and retained, so a **nightly** pass over the
*complete* day, with late uploads settled and sleep resolved, can always
reconstruct the timeline better than a provisional intra-day pass. The nightly
pass would overwrite anything an hourly agent wrote with a better-informed cut.

There is a second reason the synthesis must wait for the whole day: whether an
hour was novel, calm, unusual, or ordinary can only be judged relative to the rest
of the day. A z-score needs the whole distribution, and an hour has no relative
meaning until the rest of its day exists. So the nightly pass waits for both the
complete *evidence* and the complete *distribution*.

The hourly agent would therefore produce **no durable data**. Its only possible
use is showing the user something live during the day, and that does not need an
LLM.

> **The nightly pass is the source of truth. There is no hourly agent.**

That leaves two independent pieces with no reconciliation between them:

1. **The nightly timeline** — the official events. One pass per completed day,
   complete evidence, gapless, frozen.
2. **The live "today so far" view** — deterministic, no LLM, disposable. It
   renders what is already known as the day happens.

## Two clocks: the whole orchestration

This can look like a fragile pipeline where step A must finish before step B.
It is not one. Every transformation is **idempotent, backlog-driven, and
self-healing** (the visit rollup was tested this way: run it three times, get
13/13/13). Nothing waits on anything else. Each step runs on its own schedule,
does whatever work is ready, and if its inputs are not there yet it does nothing
and picks them up on the next tick. The steps **converge** on the right result
without a coordinator.

**Fast clock (~15 min, all day): resolve raw → derived.**
Transcription as recordings land; visits as points land. Independent, idempotent,
order-free. This feeds the live view.

**Slow clock (nightly): derive meaning from complete, comparable data.**
Sessionize audio → label sessions → the detective → score → narrate. One
authoritative pass.

So there is no precise sequence to build. A couple of cheap self-healing
refreshers run frequently, and one expensive synthesis runs nightly. The delay
between them is expected, because each layer depends on the one below it being
finished: a visit is not complete until you leave, a session is not complete
until it ends, and a day is not complete until it is over.

## The live view (intra-day, no AI)

During the day, the user sees a timeline built **only from source rows that carry
their own boundaries** — a real start, a real end, and an obvious label, with no
model involved:

- **location visits** — labelled by place name
- **calendar events** — labelled by title
- **sleep** — labelled as sleep

That is the whole list. Conversation sessions are **not** included: audio
sessionization is a *nightly* step (it needs closed sessions and resolved visits,
which are only complete at the end of the day), so conversation blocks appear in
the settled day, not the live strip. Workouts, app sessions, messages, and heart
rate are **supporting detail, not blocks**: they add information to an event but
cannot define its boundaries on their own, and inferring a block from them needs
the detective. The live view never guesses. It shows only what the source rows
establish ("Home, 00:04–03:07 · Blue Bottle, 09:00–now") and leaves the rest
blank until the next morning.

This view cannot hallucinate or drift, and it costs nothing, because there is no
model in it. The fused, narrated version of the day arrives the next morning.

## The nightly pass (the detective)

Once per completed day, one agent reconstructs the authoritative timeline. The
code calls this segmentation step "the detective" (`SEGMENT_PROMPT` in
`virtues-core/src/api/day_summary.rs`, step 1 of
`applets/day_summary_eod/main.rs`), and this doc uses the same name.

### Gapless, with Unknown as a first-class event

The timeline covers 24 hours with no holes. Where the evidence does not support a
classification, that span is an **`Unknown` event** — neither a gap nor an
invented event. A day that is mostly `Unknown` with three clear events is more
accurate than one padded with five speculative ones. Dead zones (phone off,
driving in silence) are `Unknown`, and no architecture changes that: missing
evidence cannot be turned into evidence.

### Candidate changepoints, then fusion

Boundaries are not taken from a single signal. Every source contributes
**candidate changepoints** — a visit edge, a calendar edge, a sleep transition,
an audio-topic shift, a long idle gap, an HR regime change. None of these is a
boundary by itself; each one flags that something might have changed there.
Where flags **cluster in time**, the boundary is strong; where one flag stands
alone, it is weak.

The agent combines the local evidence around each cluster and decides the
*actual* boundary and label, reconciling the conflicts (calendar 2:00, arrival
2:05, meeting audio 2:08 → a boundary around 2:05). **How many agreeing signals
are enough is not a fixed rule, and should not be.** It is a judgement over many
kinds of evidence, which is a task an LLM handles well and a threshold does not.

### Features by default, raw data on demand

A 22-hour recording day is ~300k tokens of raw transcript. It technically fits a
1M window, but it should never be sent: it is wasteful, and model attention
degrades over input that large.

So the agent reads a **compressed dossier** instead of the raw data. Each source
arrives as a summary feature instead of raw text:

- a transcription → its Gemini-made title + summary (not the 5-minute transcript)
- a visit → its place + duration (not the GPS points)
- messages → counts and threads (not bodies)

The whole day compresses to ~10–20k tokens. When a boundary is ambiguous, the
agent **drills down with tool calls**, pulling the raw evidence for that one
window. It works from the summary index and requests specific raw records only
when the summary is not enough.

### The last 14 days as context

The agent is given the **previous ~14 days of event summaries and day
timelines** (~140 sentences, which is cheap). This history does three things:

1. **Disambiguates** — "Unknown, 2–4pm, a Tuesday" is easier to interpret when
   the last two weeks show a regular Tuesday pattern.
2. **Grounds narrative novelty** — "you did *not* go to the office today" is only
   worth saying when there is a visible routine to compare against.
3. **Improves over time** — the detective has more context the longer the box
   has been running.

This replaces a separate persisted "memory file". It is retrieval of derived data
that already exists, so there is nothing extra to maintain or to drift.

### Show the reasoning

Because the timeline is uncertain, it explains itself. When the user taps an
event, the timeline shows its reasoning: *"Meeting, ~2–3pm — your calendar, your
arrival at the office, and the audio all agree."* The user sees *why*, and can
correct it. A user's correction is the most valuable evidence available.

## The structure of a settled day: stays, transit, and unknowns

The detective emits a gapless sequence of blocks, and each block has exactly one
**kind**. It is a single enum, so a block is exactly one thing and invalid
combinations (`unknown AND transit`) cannot be represented:

- **`stay`** — you were somewhere, doing something (a work session, lunch, reading).
- **`transit`** — you were moving between places (a drive, a walk, a flight).
- **`sleep`** — the overnight block, owned by the deterministic sleep resolver
  and not guessed by the model.
- **`unknown`** — the evidence does not support a classification.

That is the entire vocabulary. The day is made of these four, end to end, 00:00 →
24:00. The two core kinds are **stay** (a node: you stopped) and **transit** (an
edge: you moved between nodes), so the timeline is a graph of places and
movements rather than a flat log. `kind` replaces the old set of `is_unknown` /
`is_transit` / `is_sleep` booleans. The *provenance* flags (`is_user_added`,
`user_hidden`, …) are a separate axis — who touched the block, independent of
what kind it is — and they stay.

### Kind describes the block; the score decides its importance

Stay versus transit is a *physical mode* and says nothing about importance. Every
block is scored the **same way** (novelty / autonomic / topic), and the score
decides what becomes the day's headline and what shrinks to a thin connector. The
mode and the duration do not. A silent commute and a two-hour conversation in the
car are both transit; only the score separates them. Transit acts as a
**container**: its importance comes from what happened during it, so a quiet
drive scores low and a phone call about the budget during the drive scores high,
with no special cases. **Never exclude a block from scoring for being "just
transit" or "just short"** — that is how a day's most important moments get
suppressed.

### Minimum-duration rules

Duration filters out *noise*, and it should not be used to judge *importance*, so
the minimum differs by kind:

- **Stay / Unknown → 15-minute minimum.** A stay or Unknown block must last at
  least a quarter hour or it is absorbed into its neighbour. This removes the
  "insufficient data" **slivers** — the 4–6 minute Unknowns that come from the
  detective drawing boundaries at exact data timestamps. *Sliver absorption*
  means: do not emit the tiny block; extend the adjacent event to cover the gap.
- **Transit → 3-minute minimum.** Movement between two different places is real
  at any length, so transit is exempt from the 15-minute rule. But below ~3
  minutes a "transit" is almost always visit-boundary noise or GPS drift rather
  than a real move, and it is absorbed like any sliver. (The stronger check is
  *"are these two actually different places?"*, which the visit match-and-extend
  rollup already enforces; the 3-minute minimum is a backstop.)
- **Important moments shorter than a block → no row at all.** A three-minute
  important phone call at your desk does not get its own event. It is a
  **highlight inside** the work-session block that contains it, surfaced by the
  summary and the scores. No information is lost; the timeline just isn't split
  into fragments for it.

> **Summary: a stay or Unknown block needs 15 minutes, a transit needs 3, and a
> shorter important moment is mentioned inside its block instead of getting a row.**

### Label by the strongest evidence

A block's title describes its most important content, with mode and place as
context — "settling the budget with Corvin, on the drive home", not "Transit".
Movement is the headline only when movement is all that happened. A
location-change span that also contains a substantial audio session is titled by
the *conversation*, and the drive becomes the context.

### The structured fields: kind, confidence, importance score

Apart from its content (label, summary, place, people) and provenance (who
touched it), a block carries three independent signals. Two are **enums**
instead of arbitrary numbers: a 1–5 scale suggests more precision than exists
and, for anything a model judges, tends to cluster and drift across models. Named
buckets anchor themselves and stay stable.

- **`kind`** = `{ stay, transit, sleep, unknown }` — *what it was*. Deterministic.
- **`confidence`** = `{ low, medium, high }` — *how sure we are*. The timeline
  today presents every block as equally certain, but events are **inferred by
  fusing imperfect sources**, so some are certain and some are guesses, and that
  information is currently discarded. Confidence lets the timeline show which
  blocks are guesses, which enables two things this doc already calls for:
  **showing the reasoning** (a low-confidence block renders softly — "*probably*
  gym"; a high one is stated plainly) and the **correction loop** (show the
  *uncertain* blocks first, since a user's correction is the most valuable
  evidence available). Where possible, confidence is **deterministic**, which
  avoids cross-model variance entirely:
  - **event confidence** = *source agreement* (a count, independent of the
    model): **high** = three or more independent sources support the window
    (calendar *and* location *and* audio); **medium** = two agree; **low** = a
    single signal or pure inference.
- **importance score** (`novelty_z` / `autonomic_z` / topic) — *how much it
  mattered*. It shapes the timeline: prominence follows the score, not
  chronology. Per-event.

**Confidence uses the same word and the same 3-bucket scale for events and
days.** The day has a confidence too: the existing `data_quality.overall`,
reinterpreted as *how much to trust this day's account*. It is the only one a
model judges (a model reads coverage), so it uses the W6H `coverage` breakdown as
supporting detail and maps to the same three buckets: **high** = continuous,
multi-source coverage across the waking day; **medium** = a typical weekday with
some gaps; **low** = sparse, with hours missing. (low ≈ old 1–2, medium ≈ 3, high
≈ 4–5 — a re-bucketing, not a redesign.) A **high**-confidence day is presented
as *the* narrative; a **low** one is presented as partial notes — captured and
searchable, but not presented as the definitive account.

So **kind = what, confidence = how sure, importance score = how much it
mattered**. The first two are deterministic where possible, and all three use the
same vocabulary for a single event and for the whole day.

### Rendering follows the importance score, not equal-weight chronology

The timeline should vary in visual weight: the most important block is a full
card, routine stays are quieter, and empty transits are thin connectors between
them. The result is a day with visible high and low points instead of a uniform
stack of rows. In the UI, `kind` is used for this: `transit` drives *connector*
styling and accurate "time at place X" accounting. But it is a **styling hint
and never a filter on scoring**. A transit with something important in it (the
conversation during a drive) is still a full card, because the score sets
prominence, not the kind.

### Status

Built: the gapless block sequence, stays, transit, deterministic sleep,
`Unknown`, per-event importance scores, per-day completeness,
narrative-with-standout, and the **minimum-duration rules + gap-classification
pass** — a deterministic pass over the detective's output that absorbs sub-15-min
Unknown slivers, labels location-change gaps as **Transit (A → B)**, and sets
`is_transit` (on both its own conversions and movement the detective already
named). Not yet built, in increasing size:

1. **The `kind` + `confidence` model refactor** — collapse the mutually exclusive
   `is_unknown` / `is_transit` / `is_sleep` booleans into a single **`kind`** enum
   (`stay` / `transit` / `sleep` / `unknown`), and add **`confidence`**
   (`low` / `medium` / `high`) on both event (source agreement, deterministic)
   and day (reinterpreting `data_quality.overall`). This is a cross-cutting
   change: a migration plus every read/write site and the frontend converters, so
   it should be its own focused piece of work.
2. **Score-driven rendering** — cards vs. connectors by score + `kind` (depends
   on the novelty baseline having enough history; it has none on a fresh box).
3. **Place/route and time-of-day novelty** — today's scoring embeds the summary
   *text*, so "first walk downtown in months" is caught only if the words say so.
   Real *where/when* novelty is the next layer of the novelty scoring engine.

## Audio sessions: the detective's audio input

Raw audio arrives as 5-minute recorder chunks — hundreds a day, each transcribed
and titled in isolation. That chunk size comes from the recorder and does not
correspond to any real activity, so chunks must roll up into **sessions**
(`data_communication_transcription` becomes the session, mirroring
`location_point → location_visit`) before the detective reads them. This is a
**nightly** step.

**Session boundaries come from acoustic context, never from topic.** The obvious
signal — embedding/topic distance between chunks — gives the wrong answer: topic
changes constantly *within* a single context ("a bug → the budget → lunch →
weekend plans", all at one desk with one colleague, is one session). What
actually marks a context change is **who is around and how loud it is**, and both
are already measured:

- **`average_db_level`** (on `data_audio_recording`) — a car, a quiet room, a
  loud restaurant, and a lull all have different levels.
- **`speaker_count`** (on the transcription) — bucketed to {silent 0, solo 1,
  dyad 2, group 3+}, because raw diarization is noisy (it will claim 40 speakers).

Run **PELT changepoint detection** (offline, deterministic, O(n), no LLM) over
the `(db, speaker-bucket)` series, weighted toward speakers. Tested on a real
day, it cut a few hundred chunks into a couple of dozen coherent context blocks,
with a night's sleep as *one* block and each distinct conversation as its own.
Topic changes stayed inside their session, and a real context change got its own
session. It uses no embeddings, no reranking, and no topics — just db + speakers
+ PELT.

**Prefer more sessions over fewer.** Session boundaries are *hints for the
detective*, not final decisions. The detective can merge boundaries it was
given, but it can never split at a boundary it was not given. Over-segmentation
is therefore recoverable and under-segmentation loses information. Tune the
penalty toward recall, stopping only where a boundary becomes noise (a
single-chunk diarization blip).

**Labels use session context, not individual chunks.** A 5-minute recording of a
fan is correctly titled "Steady Engine Hum" when it has no context. A better
transcription model would not fix that, because the audio really is just a fan.
The fix is to label at the **session** level using facts the audio does not
contain: local time, duration, speaker profile, and **place from the (now clean)
visits**. `[quiet, no speakers, 10h, at home, overnight]` becomes "Sleeping", not
"Engine Hum". The model needs the facts it cannot get from the audio. The raw
chunk titles stay as input for the detective (it interprets them); the
context-aware session label is what a person sees.

## Ordering: novelty comes after the summary

There are two kinds of novelty, and both run **after** the events have
summaries:

1. **Computed novelty** (`novelty_z`, LOF, topic/entity novelty — the z-scores
   that drive the charts) **embeds `event_summary` as its vector**. It therefore
   *cannot* run until segmentation has produced those summaries. Hard dependency.
2. **Narrative novelty** ("today broke the routine") is **found by the nightly
   agent itself**, because it has the last 14 days in context and notices the
   deviation.

The full pipeline, both clocks:

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

**The two lookback windows do different jobs; do not merge them.** Scoring
(step 3) looks back **84 days** for a statistical baseline. The narrative
context in step 4 (the "case file" in the diagram) is **14 days** of recent event
summaries, used to recognise patterns. The z-scores use long history; the
narrative uses recent history.

## The hard dependency: clean rollups

None of this works if the input data is wrong. The nightly detective is only as
good as the evidence it combines, and today two of the most informative signals
arrive at **sensor cadence instead of as sessions**:

- **audio** is a few hundred five-minute transcription rows per day — the
  recorder's chunking, not conversations. There is no sessionizer merging
  consecutive recordings into activities.
- **location** is emitted as overlapping per-batch "visits" — the visit id hashes
  `start_time`, so a re-run on a sliding window creates a new row instead of
  extending the open stay. One 3-hour stay becomes a dozen overlapping rows.

Both break the same rule: **raw sensor samples must roll up into sessions before
anything reasons over them** (the signal-vs-artifact rule). Until the rollups
produce real visits and real audio sessions, the detective is working from
duplicated, fragmented input.

**Fix the rollups first.** They are the prerequisite for everything above.

## What this model does and does not solve

- **Gaps** are an evidence problem, not an architecture problem. Where there is
  no evidence, the event is `Unknown`. Where evidence conflicts, the detective
  shows its reasoning. This is an unavoidable limit of reconstructing a day from
  side-effect data, and it should not be engineered away.
- **The convergence judgement** (how many agreeing signals are enough) is
  intentionally non-deterministic and lives in the agent, tuned against real
  days, never a fixed threshold.
- **Provisional versus settled** state no longer exists: the live view is
  deterministic and disposable; the nightly timeline is authoritative and frozen.
  There is no intermediate state to reconcile.
- **Late evidence** that arrives after the nightly pass (rare — more than a day
  late) marks the window for re-derivation as an *offer* to the user, never a
  silent rewrite of prose the user may already have read.

## Summary

> **Two clocks.** All day, a fast clock resolves raw → derived (transcription,
> visits) — idempotent, self-healing, order-free — and feeds a deterministic,
> no-LLM live view (visits + calendar + sleep). At night, one authoritative pass
> derives meaning from complete, comparable data: **sessionize** (mechanical PELT
> + stitch, no LLM) → **the detective** (a dossier of features + drill-down →
> gapless 8-16 events, `Unknown` for real gaps, all labelling here) → **score**
> (84-day relative baseline) → **narrate** (with 14 days of recent context). The
> nightly pass is authoritative because the evidence *and the distribution it is
> measured against* are only complete once the day is over. **Clean session
> rollups are the one prerequisite** — the detective is only as good as its input.
