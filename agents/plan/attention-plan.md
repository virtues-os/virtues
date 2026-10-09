# Attention, and the Tense of a Record

**Status:** Open. What is left is surfacing coverage, composing the novelty
input, deterministic anticipation refs, and revision on new refs. See "Next".

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

Three separate failures with three different causes:

1. **The occasion was split across two events and neither was named.** The
   detective had no evidence that anything had been arranged. Messages now reach
   the segmenter as time-placed per-thread bursts, which addresses this for the
   timeline; the day article no longer narrates from event summaries at all (see
   `day-article-plan.md`).
2. **The most routine stretch of the day won the salience badge.** `novelty_z`
   embeds the *event summary*, so it measures the lexical rarity and topic
   breadth of a paragraph the model itself wrote. A long, many-topic summary is
   far from any centroid by construction. Meanwhile a milestone occasion at a
   café is *maximally* important and *minimally* novel — coffee is bought daily.
   **The system's only ranking signal is anti-correlated with importance.**
3. **The aftermath could not have registered in any case.** See the next section.

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

## Why the timeline cannot name an arranged occasion

An **event summary** is a claim scoped to one bounded interval, warranted only by
the dossier lines inside it. **Segmentation is window-local**: no event claim is
ever warranted by evidence outside its own boundaries. Naming an arranged
occasion is a claim whose warrant lies partly outside its window — in the
arrangement beforehand and the reaction afterward — and no prompt wording creates
a place for it. Only a ref pointing across time can.

Events are immutable-by-replacement (deleted and re-minted with content-addressed
ids on every re-cut; `delete_auto_events_for_day` guards `is_user_edited` and
`user_hidden`). `event_summary` is reader prose in the Record view *and* the
novelty embedding input — see "The impoverished vector".

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

### Importance is not knowable at 4am

The nightly chain runs once and freezes `novelty_z` forever. But a day's
importance *grows*: aftermath and persistence arrive after the article was
written. A wiki article is revised as new sources appear, and the pipeline does
not yet deliver that for a day.

## What may be asserted, linked, and noted

This is the accuracy core of the document. Three different acts, three different
bars, and conflating them is how fabrication ships.

| act | writes | bar | who may |
|---|---|---|---|
| **link** (a ref) | `wiki_refs` | deterministic, or a human | never the writer, at any confidence |
| **note** (a proposal) | `wiki_notes` | must cite; capped at 3/pass | a pass that held a complete session in context |
| **assert** (prose) | `event_summary`, the article | evidence in the window; observe-never-infer | the detective and the narrator |

### Linking

`wiki_refs` is the citation edge: *record R refers to subject S at time T*.
Subject types are `person`, `place`, `organization`, `thing`, `event`, `day`,
`thread`.

`thread` is the important one. A conversation has an identity from its first
message, months before the person on the other end is ever resolved into the
graph. **A first meeting is, by definition, with someone new**, so any attention
measure anchored on resolved people scores exactly the motivating case at zero,
while a workday thick with resolved colleagues scores high. Anchoring on threads
fixes both that and the frequency-saturation problem, provided the measure is
*deviation from that thread's own baseline* rather than raw volume — a brand-new
thread has no baseline and is therefore maximal signal, while a family group
chat is quiet on an ordinary day.

**Nothing writes `event`, `day` or `thread` refs yet.** Deliberately: producing
them is a doctrine question, not a schema one.

The doctrine, from `entity_resolution/mod.rs`: semantic ER was deleted, and the
numbers were decisive — of 130,777 entity refs, the semantic path produced
**189 (0.14%)**, and even those linked only via a human-written alias. Meanwhile
it accrued **11,113 permanently-floating mentions**, a review queue never
cleared, 172k log rows, and a per-sweep LLM call. Handle matching, merchant
resolution and place clustering produced the other 99.86% for free.

So: **a model may not write a ref.** The two sanctioned paths are

1. **Deterministic** — an explicit date/time string in the text, a thread
   continuing across the event, a calendar title match, a merchant match. Writes
   a ref directly. No model, no floating mentions possible.
2. **Proposed** — a pass leaves a *note* citing both records; a human, or the
   article editor, promotes it.

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
  `wiki_notes_machine_must_cite` is a DB CHECK, not a prompt instruction. A cited
  note is useful *even when wrong*, because it is checkable in seconds; a bare
  claim with a confidence score is worthless when wrong.
- **The writer never touches the graph.** Notes are the machine's only channel.
- **Notes never age out.** Three exits, all events — `accepted`, `dismissed`,
  `absorbed`.

`write_machine_notes` has **zero production callers**.

**What may NOT be a note.** [`wiki-editor.md`](../build/wiki-editor.md) already
legislates it: a machine note comes only from a pass that held a complete session
in context, never a sweep over isolated rows, and silence is the default. So:

- ✅ the detective or the narrator noticing something it cannot fit in prose
- ❌ **attention telemetry** — *"the record returned to 18:45–20:30 six times."*
  That is a sweep over isolated rows, it has no reachable exit (no human will
  ever "accept" a count), and `wiki_notes` has **no unique constraint**, so an
  automated writer re-inserts it every run forever.

If anything automated is ever to write a note, a uniqueness constraint comes
first. `wiki_refs` already has one — `(entity_id, source_table, source_id, role)
NULLS NOT DISTINCT` — which is exactly the idempotency `wiki_notes` lacks.

### Asserting

`SEGMENT_PROMPT` carries `MESSAGES ARE PLANS TOO` (a message arranging something
is a plan exactly as a calendar entry is) and `DO NOT QUOTE PEOPLE` (both
directions may be *read*; another person's words may not be *reproduced*).

**A hard limit on any "name the occasion" license.** If anything is ever allowed
to name an event by quoting its evidence, the quotable corpus must be
**owner-authored only** — messages the owner sent, titles on an owned (not
subscribed) calendar, user notes. Never `data_audio_session.content`: that field
is *already model prose* (the stitched summaries), and ambient audio mixes
podcasts, TV and strangers. A podcast line could otherwise become the headline
occasion — with a citation making the fabrication look checked.

## The impoverished vector

With the day article reading the day directly, `event_summary`'s embedding only
matters for **novelty** — and that is where it does harm.

| path | input | serves |
|---|---|---|
| search index — `EmbeddingConfig.embed_text_sql` | `label` + `event_summary` + `user_notes`, NULL for unknown/hidden | retrieval |
| novelty — `embed_input_for_event` (`dayline/embedding_ops.rs`) | `summary.trim()` — the entire function body | the day line and its "Most unlike your usual" badge |

The *weaker* input feeds the *stronger* claim. Two faults:

- **Input.** The vector describes prose *about* the event. The same morning
  written in 12 words and in 60 gets a different vector.
- **Geometry.** `score_global` is distance from a kernel-weighted **centroid**. A
  summary spanning many topics embeds *between* those regions and is therefore
  far from any single-topic mean. Multi-topic → high z.

**The fix for the geometry is wired.** `local_novelty_z` / `lof_raw` is a Local
Outlier Factor — density-relative, no centroid, "off-pattern for its *kind*".
The day page reads it first: `usualScore` (`lib/wiki/dayLine.ts`) takes
`local_novelty_z`, else `novelty_z`, for the day line's heights and for the
event timeline's "Most unlike your usual" badge (the highest at 1.0 or
above, in Data).

**The fix for the input is to compose.** Prose stays as it is; the vector is
built from the event's *evidence* — label, entities, source ontologies,
merchants, places, dossier slice.

The governing principle: **an embedding that serves retrieval may be prose; an
embedding that serves a verdict must be evidence.** The day article's vector
feeds search and Similar days, where prose is a fair proxy. The event vector
feeds a score rendered as a judgement about a life, and must not.

## Revision: dirty vs clean

| mechanism | state |
|---|---|
| `wiki_days.sources_fingerprint` | written and read — gates re-segmentation. Works. |
| `wiki_days.segmented_at` | written, read by nobody |
| `wiki_days.narrated_at` | written, read by the catch-up queue |
| `wiki_events.dirty_at`, `wiki_days.dirty_at` | **0 writers, 0 readers** |
| `wiki_articles.dirty_at`, `refresh_after_new_refs` | superseded by article resolution's `maintenance` setting; not dropped yet |

The revision trigger should be **fingerprint drift, not a counter** — the
predicate already exists and works; it is wired only to *skip* work, never to
*trigger* it.

**But two fingerprints, not one.** Re-segmentation is destructive — new
content-addressed ids, stranded search chunks, discarded scores — while
re-narration is prose regeneration and safe to repeat.

| fingerprint | covers | gates | destructive |
|---|---|---|---|
| `sources_fingerprint` | the day's own rows | re-segmentation | yes — leave exactly as is |
| `refs_fingerprint` | + inbound refs | re-narration only | no |

A single fingerprint over both cannot exist: a Saturday text about Thursday must
be able to trigger Thursday's *rewrite* without being able to trigger Thursday's
*re-cut*.

Preconditions before any revision queue is switched on:

1. **A narrate-only path the applet uses.** `--narrate-only` exists as a CLI flag
   (`cli/mod.rs`); the catch-up applet runs the whole chain.
2. **The loop must converge.** Catch-up runs hourly and takes one day per tick.
   With no note dedup and no worth gate, a nightly pass that re-dirties days
   yields a stable oscillator at up to 24 Chat-slot narrations per day against 1
   today.
3. **A worth gate with an actual writer.**

A human edit is the highest-weight attention signal in the system, and it is
already recorded in the article's history; `save_day_article`'s guards
(maintenance off, human edit, open in the editor) protect it.

## Refuted — do not re-propose these

| proposal | why it fails |
|---|---|
| attention anchored on `entity_id` | entity co-occurrence ≠ event reference; monotone in contact frequency, so a family thread outranks a singular occasion — and it scores a first meeting with a new correspondent at **zero** |
| attention telemetry as machine notes | violates the note covenant (sweep over isolated rows), no reachable exit, no uniqueness constraint |
| a model writing refs | semantic ER, with numbers: 0.14% yield, 11,113 floating mentions |
| hashing the dossier text as `sources_fingerprint` | moves the gate behind ~27 queries; with a cross-day window, a neighbouring day's data triggers a *destructive* re-cut |
| naming an occasion from any quotable span | the quote corpus contains model prose and ambient TV; only owner-authored text is admissible |
| "the revision loop is free" | up to 24× understated |
| deleting `local_novelty_z` / `lof_raw` as unread columns | LOF is the *better* novelty statistic, immune to the centroid effect that produced the bad badge. Wire them up. |
| deleting the "Most Novel" badge outright | its fault is the word and the input. Surprise is honest and worth surfacing; rename and re-point it. |
| renaming `entity_id`/`entity_type` → `subject_*` | the identifier is overloaded across three unrelated concepts (this table, the ref-*route* addressing scheme, dead `er_mentions`) — 166 occurrences, 27 files, and it breaks the ref picker |

## Next

In dependency order:

1. **Surface `wiki_events.confidence` as coverage** — add to the event SELECTs
   and render it in the Record view. Pure surfacing, zero risk.
2. **The badge is fixed, not deleted**: it reads "Most unlike your usual"
   and takes `local_novelty_z`, else `novelty_z`. Importance, when it
   exists, becomes a *second* badge.
3. **Compose the novelty embed input from evidence** (`embed_input_for_event`),
   prose unchanged. Re-embed the corpus together afterwards.
4. **Correction surfacing.** `UpdateTemporalEventRequest` carries only
   `user_label`, `user_location`, `user_notes` — there is **no way to hide an
   event** from the API, and deleting is a hard `DELETE FROM wiki_events`. Add
   `user_hidden` to the request before wiring any "hide" in the UI.
5. **Unjam the catch-up queue** — its floor counts all events while narration's
   floor counts `NOT is_unknown AND NOT user_hidden`, so empty days can block
   real failures behind them. Export the predicate rather than mirroring it.
6. **Deterministic anticipation** — write `event`/`thread` refs where the link is
   literal (explicit date/time in owner-sent text, thread continuing across the
   event, calendar title match). Feed them to the dossier as **intention** lines,
   under the calendar rules.
7. **`refs_fingerprint` + narrate-only in the applet**, with a worth gate and a
   per-horizon call ceiling. Only after 6 produces refs worth revising for.

Open, and genuinely undecided:

- Whether a bounded closed-question model resolver is ever admissible for
  temporal reference.
- Whether the **activation gate** should change. It refuses to segment a day with
  no *span* source, and messages are discrete — so a message-heavy day still gets
  no events. Its reasoning survives bursts ("a thousand text messages never say
  when anything started"; a burst's span is derived from clustering, not
  measured), so changing it carries real fabrication risk.

## Related

- `day-article-plan.md` — the article written from the day itself
- [`the-day.md`](../build/the-day.md) — the day page and its data model
- [`event-timeline.md`](../build/event-timeline.md) — segmentation as evidence fusion
- [`wiki-editor.md`](../build/wiki-editor.md) — the note covenant; [`wiki-notes-plan.md`](wiki-notes-plan.md) — the unbuilt note writer
- [`article-resolution.md`](../record/article-resolution.md) — article maintenance
- [`privacy-boundary.md`](../build/privacy-boundary.md) — egress; message bodies reach the Chat slot, which under BYO AI
  is whatever endpoint the user configured
