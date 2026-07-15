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

So the hourly agent produces **no durable data**. Its only possible value is
showing the user something live during the day — and that does not require an
LLM at all.

> **Nightly is the authority. There is no hourly agent.**

Two clean pieces, no reconciliation between them:

1. **The nightly timeline** — the official events. One pass per completed day,
   complete evidence, gapless, frozen.
2. **The live "today so far" view** — deterministic, zero-LLM, disposable. Just
   renders what is already known as the day happens.

## The live view (intra-day, zero-AI)

During the day, the user sees a timeline built **only from source rows that bound
themselves** — a real start, a real end, and a self-evident label, with no model
in the loop:

- **location visits** — labelled by place name
- **calendar events** — labelled by title
- **sleep** — labelled by itself

That is the whole list. Workouts, app-sessions, messages, audio, heart rate are
**texture, not blocks**: they colour an event but cannot bound one on their own,
and inferring a block from them needs the detective — which is the nightly pass's
job. The live view never guesses. It shows the skeleton it can prove ("Home,
00:04–03:07 · Blue Bottle, 09:00–now") and leaves the rest blank until morning.

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

## Ordering: novelty comes after the summary

There are two distinct kinds of novelty, and both sit **after** the events have
summaries:

1. **Computed novelty** (`novelty_z`, LOF, topic/entity novelty — the z-scores
   that drive the charts) **embeds `event_summary` as its vector**. It therefore
   *cannot* run until segmentation has produced those summaries. Hard dependency.
2. **Narrative novelty** ("today broke the routine") is **discovered
   agentically** by the nightly agent, because it holds the last 14 days in
   context and notices the deviation itself.

The nightly chain, in order:

```
segment events        (dossier + 14-day context → the gapless timeline)
  → annotate          (stamp avg_hr, entities, source_ontologies per event window)
  → compute scores    (novelty / autonomic / topic-entity — embeds event summaries)
  → narrate           (prose; notices narrative novelty from the week it can see)
```

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

> **Nightly is the timeline** — one pass over the complete day, reading a
> compressed dossier plus the last 14 days, fusing candidate changepoints into a
> gapless sequence (with `Unknown` for real gaps), drilling into raw evidence on
> demand, and showing its reasoning. **The live view is a deterministic, zero-LLM
> "today so far"** built only from visits, calendar, and sleep. **Clean session
> rollups feed both**, and are the one thing that must be fixed first.
