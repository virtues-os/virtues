# Attention, and the Tense of a Record

**Status:** Partly built — 2026-08-07. Migration 0099 and message bursts have
landed; everything under "Next" has not. Believe this header over the prose.

How the life wiki learns which parts of a day mattered, without being allowed to
guess how anyone felt.

## The failure this exists to fix

Take the failure class in its general form, since the shape recurs and the
specifics do not matter.

**A singular personal occasion — the kind of morning a person would still
remember a year later — rendered as two unnamed fragments, while three hours of
routine desk work took the day's salience badge.**

The record held everything needed: messages arranging the occasion the evening
before, a card transaction at the venue, a long two-person conversation in audio,
messages about it afterward. The day page produced roughly this:

| window | label | summary |
|---|---|---|
| 07:24–07:44 | "Desk stint and \<venue\> purchase" | a device stint, then a small transaction |
| 07:44–09:27 | "Long conversation and walk" | *none* |
| 09:27–12:10 | "Hardware setup and desk work" | **badged "Most Novel"** |

Three separate failures with three different causes. Keeping them apart matters,
because the fixes do not overlap:

1. **The occasion was split across two events and neither was named.** The
   detective had no evidence that anything had been arranged: messages reached it
   as `- 14 with <name>` — no text, no timestamp, off the time spine entirely.
2. **The most routine stretch of the day won the salience badge.** `novelty_z`
   embeds the *event summary*, so it measures the lexical rarity and topic
   breadth of a paragraph the model itself wrote. A long, many-topic summary is
   far from any centroid by construction. Meanwhile a milestone occasion at a
   café is *maximally* important and *minimally* novel — coffee is bought daily.
   **The system's only ranking signal is anti-correlated with importance.**
3. **The aftermath could not have registered in any case.** See the next section.

The second and third failures are the general ones. The first is the one a single
change fixes.

## The keystone: a record has no tense

**Every record in this system carries exactly one time — when it was recorded.
Nothing anywhere models what time it is *about*.**

| | stores | cannot express |
|---|---|---|
| `data_communication_message.timestamp` | when sent | that Tuesday's text is about Thursday |
| `wiki_notes.created_at` | when written | which time the note concerns |
| `wiki_refs.timestamp` | when the referring record was made | — |
| `data_calendar_event.start_time` | **an intended future time** | — |

The calendar is the single exception in the entire schema: the one record type
that carries a time it is *about* rather than a time it *happened*. And the
codebase's hardest-won prompt rules — the whole `CALENDAR EVENTS ARE INTENTIONS`
block, born of a real fabrication bug — exist precisely because of it.

So the general shape of the problem is: **anticipation is asking for the general
case of what calendar is the special case of.** Every hazard the calendar rules
already name is a hazard the general case inherits, and worse — a plan written in
a person's own voice reads far more like a memory than a calendar row does.

Mapped to the three tenses:

- **Future-referring** — calendar only.
- **Present-referring** — the implicit default. A record's timestamp is *assumed*
  to be what it is about. Usually right for sensors, usually wrong for text.
- **Past-referring** — **nothing at all.** A message at 11:20 reacting to
  something that happened at 07:44 is filed as an 11:20 event, never as a
  reference back to 07:44.

That last row is the sharpest single finding here. **Aftermath is invisible by
construction**, because a past-referring message is silently misfiled as
present-referring. So is persistence. Two of the four phases below cannot fire at
all until a record can point at a time other than its own.

## What the pipeline is, abstractly

Both summarizing stages are **claims scoped to a time span, warranted only by
evidence inside that span.**

- An **event summary** asserts something about *one bounded interval*. The window
  is the unit of claim; the dossier lines inside it are the entire warrant.
- The **day article** asserts something about *relations among intervals*. Its
  job is defined negatively — it may not re-list the timeline, so its only
  legitimate content is what spans or connects events.

The structural consequence: **the pipeline is window-local at every stage.** No
claim is ever warranted by evidence outside its own boundaries.

That is why an arranged occasion is hard. Naming it is a claim whose warrant lies
partly outside its own window — in the arrangement beforehand and the reaction
afterward. There is no place in the architecture to put such a claim, and no
amount of prompt wording creates one.

Two asymmetries fall out of the same observation:

- **Events are immutable-by-replacement; articles are editable.** Events are
  deleted and re-minted with content-addressed ids on every re-cut. Only the
  article has `narrated_at`, `dirty_at`, `auto_update`, CRDT state. Until
  2026-08-07 that asymmetry was silently destroying user labels on every re-cut
  (fixed — `delete_auto_events_for_day` now also guards `is_user_edited` and
  `user_hidden`).
- **`event_summary` serves two masters** — reader prose *and* the novelty
  embedding input. See "The impoverished vector".

## Attention: four phases, and their temporal staging

Importance is measurable without inference: **it is how much the record itself
returns to a thing.** Counting is observation. Naming a feeling is not.

| phase | what it counts | known when | what it may change |
|---|---|---|---|
| **anticipation** | references *before* | at cut time — it is already past | **the boundary, the label, the summary** |
| **convergence** | independent sources agreeing *during* | at cut time | confidence, event class |
| **aftermath** | references within days *after* | +days | rank, re-narration |
| **persistence** | references *weeks* after | +weeks | long-term weight |

The staging column is the load-bearing part. **Anticipation is the only phase
available before the cut**, so it is the only one that can fix a boundary or a
name. The other three can only re-rank or trigger a rewrite.

**Convergence already exists — but it is misnamed.** `wiki_events.confidence` is
computed nightly from witness agreement: how many distinct ontologies have rows
inside the event's own window (3+ high, 2 medium, else low), with per-kind
overrides. Deterministic, registry-driven, so a new ontology is covered the day
it lands. It is absent from the API select and has zero references in `apps/web`.

It measures **corroboration breadth — coverage, not correctness.** Three sources
agreeing you were somewhere does not make the *label* right; and the inverse
holds too — a `[visit]` plus a `[purchase]` at the same merchant is two sources
warranting a confident name, while heart rate + steps + device is three sources
warranting almost nothing. **Surface it as coverage.** Calling it confidence
invites reading it as a probability that the claim is true, which it is not.

Surfacing it is still the highest value-to-risk change available, and needs no
new machinery.

### Importance is not knowable at 4am

The nightly chain runs once and freezes `novelty_z` forever. But a day's
importance *grows*: aftermath and persistence arrive after the article was
written. A wiki article is revised as new sources appear — the metaphor the
codebase already commits to ("the ARTICLE OF THE DAY … in the sense a wikipedia
gives that word") already promises this, and the pipeline does not deliver it.

## What may be asserted, linked, and noted

This is the accuracy core of the document. Three different acts, three different
bars, and conflating them is how fabrication ships.

| act | writes | bar | who may |
|---|---|---|---|
| **link** (a ref) | `wiki_refs` | deterministic, or a human | never the writer, at any confidence |
| **note** (a proposal) | `wiki_notes` | must cite; capped at 3/pass | a pass that held a complete session in context |
| **assert** (prose) | `event_summary`, the article | evidence in the window; observe-never-infer | the detective and the narrator |

### Linking

`wiki_refs` (renamed from `wiki_entity_refs` in 0099) is the citation edge:
*record R refers to subject S at time T*. Subject types are now `person`,
`place`, `organization`, `thing` — and, new in 0099, `event`, `day`, `thread`.

`thread` is the important addition. A conversation has an identity from its first
message, months before the person on the other end is ever resolved into the
graph. **A first meeting is, by definition, with someone new**, so any attention measure anchored on
resolved people scores exactly the motivating case at zero, while a workday
thick with resolved colleagues scores high. Anchoring on threads fixes both that
and the frequency-saturation problem, provided the measure is *deviation from
that thread's own baseline* rather than raw volume — a brand-new thread has no
baseline and is therefore maximal signal, while a family group chat is quiet on
an ordinary day.

**Nothing writes `event`, `day` or `thread` refs yet.** Deliberately: 0099 is a
rename plus a widened CHECK, and producing the new types is a doctrine question,
not a schema one.

The doctrine, from `entity_resolution/mod.rs` and migration 0061: semantic ER was
deleted, and the numbers were decisive — of 130,777 entity refs, the semantic
path produced **189 (0.14%)**, and even those linked only via a human-written
alias. Meanwhile it accrued **11,113 permanently-floating mentions**, a review
queue never cleared, 172k log rows, and a per-sweep LLM call. Handle matching,
merchant resolution and place clustering produced the other 99.86% for free.

So: **a model may not write a ref.** The two sanctioned paths are

1. **Deterministic** — an explicit date/time string in the text, a thread
   continuing across the event, a calendar title match, a merchant match. Writes
   a ref directly. No model, no floating mentions possible.
2. **Proposed** — a pass leaves a *note* citing both records; a human, or an
   editor pass gated on `auto_update`, promotes it.

The distinction that might make a bounded model resolver admissible later: the ER
failure was **open-ended extraction** ("find the entities in this prose"), which
can always emit something unresolvable. A **closed** question — *"does this
message refer to one of these 12 events from the last 3 days, or none?"* — has a
bounded candidate set and cannot produce a floating mention by construction. That
is a real difference, but it is the same family as the thing that was deleted, so
it needs the constraint written into the design up front.

### Noting

`wiki_notes` is **the system's only representation of a claim that is not yet
part of the record** — the proposal type, the one tier where a claim can be
*pending*. Its covenant, all of it enforced structurally rather than by prompt:

- **Point, don't decide.** A machine note must cite —
  `wiki_notes_machine_must_cite` is a DB CHECK, not a prompt instruction. The
  asymmetry is the whole design: a cited note is useful *even when wrong*,
  because it is checkable in seconds; a bare claim with a confidence score is
  worthless when wrong, because there is nothing to check.
- **The writer never touches the graph.** Notes are the machine's only channel.
- **Notes never age out.** Three exits, all events — `accepted`, `dismissed`,
  `absorbed`. A note whose purpose is "for later" that deletes itself before
  later arrives has defeated itself, and silently.

Current state: `write_machine_notes` has **zero production callers**, and the
`NotesRail` that renders notes sits inside an `{#if hasAnyContent}` guard — so
the correction channel disappears on exactly the thin days most likely to be
wrong.

**What may NOT be a note.** `wiki-plan.md` already legislates it: *"A machine
note may only be written by a pass that held a complete session in context …
Never a sweep over isolated rows,"* and *"Silence is the default."* So:

- ✅ the detective or the narrator noticing something it cannot fit in prose
- ❌ **attention telemetry** — *"the record returned to 18:45–20:30 six times."*
  That is a sweep over isolated rows, it has no reachable exit (all three
  resolutions are human or editor events, and no human will ever "accept" a
  count), and `wiki_notes` has **no unique constraint**, so an automated writer
  re-inserts it every run forever.

If anything automated is ever to write a note, a uniqueness constraint comes
first. `wiki_refs` already has one — `(entity_id, source_table, source_id, role)
NULLS NOT DISTINCT` — which is exactly the idempotency `wiki_notes` lacks.

### Asserting

Two rules now live in `SEGMENT_PROMPT`, added alongside message bursts because
shipping the evidence without them would have re-opened a fixed bug:

- **`MESSAGES ARE PLANS TOO`** — a message arranging something is a plan exactly
  as a calendar entry is, and every calendar rule applies to it unchanged. A plan
  for a later day is not evidence about this day at all.
- **`DO NOT QUOTE PEOPLE`** — both directions may be *read*; another person's
  words may not be *reproduced* in a summary. Reading their text to cut the day
  correctly is a different act from printing it back.

**And a hard limit on any future "name the occasion" license.** If the article is
ever allowed to name an event by quoting its evidence, the quotable corpus must
be **owner-authored only** — messages the owner sent, titles on an owned (not
subscribed) calendar, user notes. Never `data_audio_session.content`, for two
compounding reasons: that field is *already model prose* (the code calls it "the
stitched summaries"), and ambient audio deliberately mixes podcasts, TV and
strangers, with no owner-voice signal anywhere in the system. A podcast line
could otherwise become the headline occasion — with a citation making the fabrication
look checked, which is strictly worse than the current failure.

## The impoverished vector

`event_summary` does two incompatible jobs. `NARRATE` reads it as prose;
`embed_input_for_event` embeds it as the novelty signal. The prompt admits it:
*"the single most load-bearing field: the user reads it AND it is embedded."*

**The codebase already has the right pattern, one file away, and novelty does not
use it.** There are two embedding paths for an event:

| path | input | serves |
|---|---|---|
| search index — `EmbeddingConfig.embed_text_sql` | `label` + `event_summary` + `user_notes`, NULL for unknown/hidden | retrieval |
| novelty — `embed_input_for_event` | `summary.trim()` — that is the entire function body | a score shown as a badge |

So the *weaker* input feeds the *stronger* claim. Two faults follow, and only one
is about the statistic:

- **Input.** The vector describes prose *about* the event. The same morning
  written in 12 words and in 60 gets a different vector.
- **Geometry.** `score_global` is distance from a kernel-weighted **centroid**. A
  summary spanning many topics embeds *between* those regions and is therefore
  far from any single-topic mean. Multi-topic → high z. This is inherent to
  comparing against a mean; normalization does not touch it.

**The fix for the geometry already exists and is unread.** `local_novelty_z` /
`lof_raw` is a Local Outlier Factor — density-relative, no centroid, "off-pattern
for its *kind*", and immune to the centroid effect by construction. It is
computed every night and read by nobody, while the badge uses the global score.

**The fix for the input is to compose.** Prose stays exactly as it is; the vector
is built from the event's *evidence* — label, entities, source ontologies,
merchants, places, dossier slice — at a length chosen for embedding rather than
for reading.

The governing principle, which also answers "should the day article get this
treatment": **an embedding that serves retrieval may be prose; an embedding that
serves a verdict must be evidence.** The day article's vector feeds search, where
prose is a fair proxy for what a day was about. The event vector feeds a score
rendered as a judgement about a life, and must not.

## Revision: dirty vs clean

The machinery mostly exists and is mostly unread.

| mechanism | state |
|---|---|
| `wiki_days.sources_fingerprint` | written and read — gates re-segmentation. Works. |
| `wiki_days.segmented_at` | written, read by nobody |
| `wiki_days.narrated_at` | written, read by the catch-up queue |
| `wiki_articles.dirty_at` | 3 writers, 1 clear, **0 production readers** |
| `wiki_events.dirty_at`, `wiki_days.dirty_at` (0033) | **0 writers, 0 readers** |
| `wiki_articles.refresh_after_new_refs` | 1 reader, **no writer, no UI** |

The revision trigger should be **fingerprint drift, not a counter** — the
predicate already exists and already works; it is simply wired only to *skip*
work, never to *trigger* it.

**But two fingerprints, not one.** Migration 0044 warns that re-segmentation is
destructive — new content-addressed ids, stranded search chunks, discarded
scores — while re-narration is pure prose regeneration and safe to repeat.

| fingerprint | covers | gates | destructive |
|---|---|---|---|
| `sources_fingerprint` | the day's own rows | re-segmentation | yes — leave exactly as is |
| `refs_fingerprint` | + inbound refs | re-narration only | no |

A single fingerprint over both cannot exist: a Saturday text about Thursday must
be able to trigger Thursday's *rewrite* without being able to trigger Thursday's
*re-cut*.

Three further preconditions before any revision queue is switched on, all of them
found by adversarial review of the first draft of this plan:

1. **The applet has no narrate-only branch.** The catch-up date is fed to
   `main()`, which runs the whole chain from step 1. `--narrate-only` exists only
   as a CLI flag the applet never invokes.
2. **The loop must converge.** Catch-up runs hourly and takes one day per tick.
   With no note dedup and no worth gate, a nightly pass that re-dirties days
   yields a stable oscillator at up to 24 Chat-slot narrations per day against 1
   today — not a converging process.
3. **A worth gate with an actual writer.** `refresh_after_new_refs` has a reader
   and no writer; "reuse what exists" is not accurate for it.

`last_edited_by = 'human'` already vetoes the AI writer, so the revision pass
inherits that protection for free — and it answers the significance doctrine
cleanly: **a human edit is the highest-weight attention signal in the system, and
it is already recorded.**

## Refuted — do not re-propose these

Recorded so they are not rediscovered as good ideas.

| proposal | why it fails |
|---|---|
| attention anchored on `entity_id` | entity co-occurrence ≠ event reference; monotone in contact frequency, so a family thread outranks a singular occasion — and it scores a first meeting with a new correspondent at **zero** |
| attention telemetry as machine notes | violates the note covenant (sweep over isolated rows), no reachable exit, no uniqueness constraint |
| a model writing refs | 0061, with numbers: 0.14% yield, 11,113 floating mentions |
| hashing the dossier text as `sources_fingerprint` | moves the gate behind ~27 queries; with a cross-day window, a neighbouring day's data triggers a *destructive* re-cut |
| naming an occasion from any quotable span | the quote corpus contains model prose and ambient TV; only owner-authored text is admissible |
| "the revision loop is free" | up to 24× understated |
| deleting `local_novelty_z` / `lof_raw` as unread columns | they are unread because nothing reads them, not because they are worthless — LOF is the *better* novelty statistic, immune to the centroid effect that produced the bad badge. Wire them up, do not delete them. |
| deleting the "Most Novel" badge outright | its fault is the word and the input. Surprise is honest and worth surfacing; rename and re-point it. |
| renaming `entity_id`/`entity_type` → `subject_*` | the identifier is overloaded across three unrelated concepts (this table, the ref-*route* addressing scheme, dead `er_mentions`) — 166 occurrences, 27 files, and it breaks the ref picker. Cosmetics do not get to carry that risk. |

## Next

Landed 2026-08-07:

- **A1** — `delete_auto_events_for_day` guards `is_user_edited` and `user_hidden`.
  Was live data loss on every re-cut.
- **0099** — `wiki_entity_refs` → `wiki_refs`, subject types widened with
  `event`, `day`, `thread`.
- **B1** — messages onto the dossier spine as time-placed per-thread bursts with
  bounded excerpts and direction counts, plus the two prompt rules above and a
  regression test encoding the failure class above.

Next, in dependency order:

1. **Surface `wiki_events.confidence`** — add to both SELECT lists and render it.
   Convergence is already computed; this is pure surfacing, zero risk, highest
   value-to-risk in the whole review.
2. **Fix the "Most Novel" badge; do not delete it.** Surprise is a real,
   honest thing to surface, and an earlier draft of this plan was wrong to call
   for deletion. The badge's fault is its *word* and its *input*, not its
   existence — "Most Novel" reads as a claim about importance. Rename it to what
   it measures ("Most Unusual" / "Off-pattern"), and point it at
   `local_novelty_z`, the better statistic already being computed and thrown
   away. Importance, when it exists, becomes a *second* badge rather than a
   replacement. (The two thresholds also disagree today: prose uses `z > 0.5`,
   the badge `z >= 1.0`.)
3. **Split `event_summary`'s two jobs** — evidence-derived embed text, prose
   unchanged.
4. **Correction surfacing.** `UpdateTemporalEventRequest` carries only
   `user_label`, `user_location`, `user_notes` — there is **no way to hide an
   event**, and `delete_temporal_event` is a hard `DELETE FROM wiki_events`. So
   wiring the UI before adding `user_hidden` ships "hide" as "destroy". Order:
   add the field, then wire the three zero-caller client functions
   (`createTemporalEvent` / `updateTemporalEvent` / `deleteTemporalEvent`), then
   move `NotesRail` out of the `hasAnyContent` guard so the correction channel
   stops vanishing on thin days.
5. **Unjam the catch-up queue** — its floor counts all events while narration's
   floor counts `NOT is_unknown AND NOT user_hidden`, so empty days can block
   real failures behind them. Export the predicate rather than mirroring the
   constant.
6. **Deterministic anticipation** — write `event`/`thread` refs where the link is
   literal (explicit date/time in owner-sent text, thread continuing across the
   event, calendar title match). Feed them to the dossier as **intention** lines,
   textually under the calendar rules.
7. **`refs_fingerprint` + a narrate-only branch**, with a worth gate and a
   per-horizon call ceiling. Only after 6 produces refs worth revising for.

Open, and genuinely undecided:

- Whether a bounded closed-question model resolver is ever admissible for
  temporal reference, given 0061.
- Whether the **activation gate** should change. It refuses to segment a day with
  no *span* source, and messages are discrete — so a message-heavy day still gets
  no events regardless of what the dossier now contains. Its reasoning survives
  bursts ("a thousand text messages never say when anything started"; a burst's
  span is derived from clustering, not measured), so changing it carries real
  fabrication risk.

## Related

- [`the-day.md`](the-day.md) — the day page and its data model
- [`event-timeline.md`](event-timeline.md) — segmentation as evidence fusion
- [`wiki-plan.md`](wiki-plan.md) — the note covenant, in full
- [`privacy-model.md`](privacy-model.md) — egress; message bodies now reach the
  Chat slot, which under BYO AI is whatever endpoint the user configured
