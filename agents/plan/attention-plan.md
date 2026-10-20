# Attention and Time References

**Status:** Open. What is left is surfacing coverage, composing the novelty
input, deterministic anticipation refs, and revision on new refs. See "Next".

Goal: the life wiki works out which parts of a day mattered by counting how
often other data refers to them, without guessing how anyone felt.

## Problem this fixes

**A planned occasion is split into unnamed fragments, while a routine stretch of
the same day gets the novelty badge.**

An invented example shows the problem. Nick arranges by message to meet David
Okafor and sign a lease. The day contains that arrangement, a payment at the
shop where they meet, a recorded conversation, and a few messages about it that
evening. A day page built from it could show:

| window | label | summary |
|---|---|---|
| afternoon, ~4h | "Spreadsheet cleanup and inbox" | **badged "Most Novel"** |
| early evening, ~10 min | "Quick stop and \<shop\> payment" | a short stop, then a transaction |
| next ~50 min | "Conversation" | *none* |

There are three separate failures with three different causes:

1. **The occasion is split across two events and neither is named.** The
   detective sees no evidence that anything was arranged. Messages now reach
   the segmenter as time-placed per-thread bursts, which fixes this for the
   timeline; the day article no longer writes from event summaries at all (see
   `day-article-plan.md`).
2. **The most routine stretch of the day gets the novelty badge.** `novelty_z`
   embeds the *event summary*, so it measures the lexical rarity and topic
   breadth of a paragraph the model itself wrote. A long summary covering many
   topics always lands far from any centroid. Meanwhile an important occasion
   in a routine place, such as a shop visited every week, scores *very low* on
   novelty.
   **The system's only ranking signal is anti-correlated with importance.**
3. **The system cannot detect the aftermath at all.** See the next section.

## Root cause: records store only when they were recorded

**Every record in this system carries exactly one time — when it was recorded.
Nothing stores what time a record is *about*.**

| | stores | cannot express |
|---|---|---|
| `data_communication_message.timestamp` | when sent | that Tuesday's text is about Thursday |
| `wiki_notes.created_at` | when written | which time the note concerns |
| `wiki_refs.timestamp` | when the referring record was made | — |
| `data_calendar_event.start_time` | **an intended future time** | — |

The calendar is the only exception in the schema: it is the one record type
that stores a time it is *about* rather than a time it *happened*. The
codebase's most important prompt rules — the whole `CALENDAR EVENTS ARE
INTENTIONS` block, written after a real fabrication bug — exist because of this.

So anticipation is the general case, and the calendar is one special case of
it. Every hazard the calendar rules already cover applies to the general case,
and more strongly — a plan written in a person's own voice is much easier to
mistake for a memory than a calendar row is.

By time direction:

- **Records about the future** — calendar only.
- **Records about the present** — the implicit default. A record's timestamp is
  *assumed* to be what it is about. Usually right for sensors, usually wrong for
  text.
- **Records about the past** — **not represented at all.** A message at 18:00
  reacting to something that happened at 09:30 is filed as an 18:00 event, never
  as a reference back to 09:30.

That last point is the most important finding here. **The system cannot see
aftermath**, because a message about the past is silently filed as if it were
about the present. The same applies to persistence. Two of the four phases below
cannot work at all until a record can point at a time other than its own.

## Why the timeline cannot name a planned occasion

An **event summary** is a claim about one bounded interval, supported only by
the dossier lines inside it. **Segmentation only looks inside the window**: no
event claim ever uses evidence from outside its own boundaries. Naming a
planned occasion needs evidence partly outside its window — the arrangement
beforehand and the reaction afterward — and no prompt wording can supply that.
Only a ref pointing across time can.

Events are replaced rather than updated (deleted and re-created with
content-addressed ids on every re-cut; `delete_auto_events_for_day` guards
`is_user_edited` and `user_hidden`). `event_summary` is both reader-facing prose
in the Record view *and* the novelty embedding input — see "The novelty vector
uses the wrong input".

## Attention: four phases and when each is available

Importance can be measured without inference: **it is how often later data
refers back to a thing.** Counting references is observation; naming a feeling
is inference.

| phase | what it counts | known when | what it may change |
|---|---|---|---|
| **anticipation** | references *before* | at cut time — it is already past | **the boundary, the label, the summary** |
| **convergence** | independent sources agreeing *during* | at cut time | confidence, event class |
| **aftermath** | references within days *after* | +days | rank, re-narration |
| **persistence** | references *weeks* after | +weeks | long-term weight |

The "known when" column is what matters. **Anticipation is the only phase
available before the cut**, so it is the only one that can fix a boundary or a
name. The other three can only re-rank or trigger a rewrite.

**Convergence already exists, under a misleading name.** `wiki_events.confidence`
is computed nightly from source agreement: how many distinct ontologies have
rows inside the event's own window (3+ high, 2 medium, else low), with per-kind
overrides. It is deterministic and registry-driven, so a new ontology is
covered the day it lands. It is absent from the API select and has zero
references in `apps/web`.

It measures **how many sources agree — coverage, not correctness.** Three
sources agreeing you were somewhere does not make the *label* right. The
reverse also holds: a `[visit]` plus a `[purchase]` at the same merchant is two
sources that support a confident name, while heart rate + steps + device is
three sources that support almost nothing. **Surface it as coverage.** Calling
it confidence invites reading it as a probability that the claim is true, which
it is not.

### Importance is not knowable at 4am

The nightly chain runs once and freezes `novelty_z` permanently. But a day's
importance *grows*: aftermath and persistence arrive after the article was
written. A wiki article should be revised as new sources appear, and the
pipeline does not do that for a day yet.

## What may be asserted, linked, and noted

This section covers accuracy. There are three different actions with three
different requirements, and mixing them up is how fabrication ships.

| act | writes | bar | who may |
|---|---|---|---|
| **link** (a ref) | `wiki_refs` | deterministic, or a human | never the writer, at any confidence |
| **note** (a proposal) | `wiki_notes` | must cite; capped at 3/pass | a pass that had a complete session in context |
| **assert** (prose) | `event_summary`, the article | evidence in the window; observe-never-infer | the detective and the narrator |

### Linking

`wiki_refs` is the citation edge: *record R refers to subject S at time T*.
Subject types are `person`, `place`, `organization`, `thing`, `event`, `day`,
`thread`.

`thread` is the important one. A conversation has an identity from its first
message, months before the person on the other end is resolved into the graph.
**A first meeting is always with someone new**, so any attention measure based
on resolved people scores exactly the case we care about at zero, while a
workday full of resolved colleagues scores high. Basing it on threads fixes both
that and the frequency-saturation problem, as long as the measure is *deviation
from that thread's own baseline* rather than raw volume — a brand-new thread has
no baseline and is therefore maximal signal, while a family group chat is quiet
on an ordinary day.

**Nothing writes `event`, `day` or `thread` refs yet.** That is intentional:
the open question is which writers are allowed to produce them, not what the
schema looks like.

The rule comes from `entity_resolution/mod.rs`: semantic ER was deleted because
the numbers were clear — of 130,777 entity refs, the semantic path produced
**189 (0.14%)**, and even those linked only via a human-written alias.
Meanwhile it built up **11,113 permanently-floating mentions**, a review queue
that was never cleared, 172k log rows, and a per-sweep LLM call. Handle
matching, merchant resolution and place clustering produced the other 99.86% at
no cost.

So: **a model may not write a ref.** There are two allowed paths:

1. **Deterministic** — an explicit date/time string in the text, a thread
   continuing across the event, a calendar title match, a merchant match. Writes
   a ref directly. No model, so no floating mentions are possible.
2. **Proposed** — a pass leaves a *note* citing both records; a human, or the
   article editor, promotes it.

One difference might make a bounded model resolver acceptable later. The ER
failure was **open-ended extraction** ("find the entities in this prose"), which
can always emit something unresolvable. A **closed** question — *"does this
message refer to one of these 12 events from the last 3 days, or none?"* — has a
bounded candidate set and cannot produce a floating mention by design. That is a
real difference, but it is closely related to the approach that was deleted, so
the constraint has to be written into the design up front.

### Noting

`wiki_notes` is **the only place the system stores a claim that has not been
accepted yet** — the proposal type, the one tier where a claim can be
*pending*. Its rules, all enforced in the schema and code rather than by prompt:

- **A machine note cites its evidence and makes no decision itself.** It must cite:
  `wiki_notes_machine_must_cite` is a DB CHECK, not a prompt instruction. A
  cited note is useful *even when wrong*, because it can be checked in seconds;
  an uncited claim with a confidence score is useless when wrong.
- **The writer never touches the graph.** Notes are the machine's only output
  channel.
- **Notes never expire.** They leave the pending state in exactly three ways —
  `accepted`, `dismissed`, `absorbed`.

`write_machine_notes` has **zero production callers**.

**What may NOT be a note.** [`wiki-editor.md`](../build/wiki-editor.md) already
specifies this: a machine note comes only from a pass that had a complete
session in context, never from a sweep over isolated rows, and by default no
note is written. So:

- ✅ the detective or the narrator noticing something it cannot fit in prose
- ❌ **attention telemetry** — *"later data referred back to 18:45–20:30 six
  times."* That is a sweep over isolated rows, it has no reachable exit (no human
  will ever "accept" a count), and `wiki_notes` has **no unique constraint**, so
  an automated writer would re-insert it on every run.

If anything automated ever writes notes, a uniqueness constraint has to come
first. `wiki_refs` already has one — `(entity_id, source_table, source_id, role)
NULLS NOT DISTINCT` — which is exactly the idempotency `wiki_notes` lacks.

### Asserting

`SEGMENT_PROMPT` carries `MESSAGES ARE PLANS TOO` (a message arranging something
is a plan exactly as a calendar entry is) and `DO NOT QUOTE PEOPLE` (both
directions may be *read*; another person's words may not be *reproduced*).

**A hard limit on ever allowing "name the occasion" from quotes.** If anything is
ever allowed to name an event by quoting its evidence, the quotable text must be
**owner-authored only** — messages the owner sent, titles on an owned (not
subscribed) calendar, user notes. Never `data_audio_session.content`: that field
is *already model prose* (the stitched summaries), and ambient audio mixes
podcasts, TV and strangers. Otherwise a podcast line could become the headline
occasion, with a citation that makes the fabrication look checked.

## The novelty vector uses the wrong input

Now that the day article reads the day directly, `event_summary`'s embedding only
matters for **novelty** — and that is where it does harm.

| path | input | serves |
|---|---|---|
| search index — `EmbeddingConfig.embed_text_sql` | `label` + `event_summary` + `user_notes`, NULL for unknown/hidden | retrieval |
| novelty — `embed_input_for_event` (`dayline/embedding_ops.rs`) | `summary.trim()` — the entire function body | the day line and its "Most unlike your usual" badge |

The *weaker* input feeds the *stronger* claim. There are two problems:

- **Input.** The vector describes prose *about* the event. The same morning
  written in 12 words and in 60 gets a different vector.
- **Geometry.** `score_global` is distance from a kernel-weighted **centroid**. A
  summary spanning many topics embeds *between* those regions and is therefore
  far from any single-topic mean. Multi-topic → high z.

**The geometry fix is already wired.** `local_novelty_z` / `lof_raw` is a Local
Outlier Factor — density-relative, no centroid, "unusual compared with events of
the same kind". The day page reads it first: `usualScore` (`lib/wiki/dayLine.ts`)
takes `local_novelty_z`, else `novelty_z`, for the day line's heights and for
the event timeline's "Most unlike your usual" badge (the highest at 1.0 or
above, in Data).

**The input fix is to compose the input from evidence.** The prose stays as it
is; the vector is built from the event's *evidence* — label, entities, source
ontologies, merchants, places, dossier slice.

The rule: **an embedding used for retrieval may be built from prose; an
embedding used to score or judge something must be built from evidence.** The
day article's vector feeds search and Similar days, where prose is a reasonable
proxy. The event vector feeds a score that is shown as a judgement about
someone's life, so it must not be built from prose.

## Revision: dirty vs clean

| mechanism | state |
|---|---|
| `wiki_days.sources_fingerprint` | written and read — gates re-segmentation. Works. |
| `wiki_days.segmented_at` | written, read by nobody |
| `wiki_days.narrated_at` | written, read by the catch-up queue |
| `wiki_events.dirty_at`, `wiki_days.dirty_at` | **0 writers, 0 readers** |
| `wiki_articles.dirty_at`, `refresh_after_new_refs` | superseded by article resolution's `maintenance` setting; not dropped yet |

The revision trigger should be **fingerprint drift, not a counter** — the
predicate already exists and works; today it is used only to *skip* work, never
to *trigger* it.

**But it needs two fingerprints, not one.** Re-segmentation is destructive — new
content-addressed ids, stranded search chunks, discarded scores — while
re-narration only regenerates prose and is safe to repeat.

| fingerprint | covers | gates | destructive |
|---|---|---|---|
| `sources_fingerprint` | the day's own rows | re-segmentation | yes — leave exactly as is |
| `refs_fingerprint` | + inbound refs | re-narration only | no |

A single fingerprint over both cannot work: a Saturday text about Thursday must
be able to trigger Thursday's *rewrite* without being able to trigger Thursday's
*re-cut*.

Preconditions before any revision queue is switched on:

1. **A narrate-only path the applet uses.** `--narrate-only` exists as a CLI flag
   (`cli/mod.rs`); the catch-up applet runs the whole chain.
2. **The loop must converge.** Catch-up runs hourly and takes one day per tick.
   With no note dedup and no worth gate, a nightly pass that re-dirties days
   produces a stable oscillation of up to 24 Chat-slot narrations per day,
   compared with 1 today.
3. **A worth gate with an actual writer.**

A human edit is the strongest attention signal in the system, and it is already
recorded in the article's history; `save_day_article`'s guards (maintenance off,
human edit, open in the editor) protect it.

## Rejected proposals — do not re-propose these

| proposal | why it fails |
|---|---|
| attention anchored on `entity_id` | entity co-occurrence ≠ event reference; monotone in contact frequency, so a family thread outranks a one-off occasion — and it scores a first meeting with a new correspondent at **zero** |
| attention telemetry as machine notes | breaks the note rules (sweep over isolated rows), no reachable exit, no uniqueness constraint |
| a model writing refs | this is semantic ER again, with known numbers: 0.14% yield, 11,113 floating mentions |
| hashing the dossier text as `sources_fingerprint` | moves the gate behind ~27 queries; with a cross-day window, a neighbouring day's data triggers a *destructive* re-cut |
| naming an occasion from any quotable span | the quotable text includes model prose and ambient TV; only owner-authored text is allowed |
| "the revision loop is free" | understates the cost by up to 24× |
| deleting `local_novelty_z` / `lof_raw` as unread columns | LOF is the *better* novelty statistic, unaffected by the centroid effect that produced the bad badge. Wire them up. |
| deleting the "Most Novel" badge outright | the problems are the word and the input. Surprise is a real, useful signal; rename the badge and point it at the right input. |
| renaming `entity_id`/`entity_type` → `subject_*` | the identifier is overloaded across three unrelated concepts (this table, the ref-*route* addressing scheme, dead `er_mentions`) — 166 occurrences, 27 files, and it breaks the ref picker |

## Next

In dependency order:

1. **Surface `wiki_events.confidence` as coverage** — add to the event SELECTs
   and render it in the Record view. Display only, zero risk.
2. **The badge is fixed, not deleted**: it reads "Most unlike your usual"
   and takes `local_novelty_z`, else `novelty_z`. Importance, when it
   exists, becomes a *second* badge.
3. **Compose the novelty embed input from evidence** (`embed_input_for_event`),
   prose unchanged. Re-embed the whole corpus afterwards.
4. **Correction surfacing.** `UpdateTemporalEventRequest` carries only
   `user_label`, `user_location`, `user_notes` — there is **no way to hide an
   event** from the API, and deleting is a hard `DELETE FROM wiki_events`. Add
   `user_hidden` to the request before wiring any "hide" in the UI.
5. **Unblock the catch-up queue** — its floor counts all events while narration's
   floor counts `NOT is_unknown AND NOT user_hidden`, so empty days can block
   real failures behind them. Export the predicate rather than duplicating it.
6. **Deterministic anticipation** — write `event`/`thread` refs where the link is
   literal (explicit date/time in owner-sent text, thread continuing across the
   event, calendar title match). Feed them to the dossier as **intention** lines,
   under the calendar rules.
7. **`refs_fingerprint` + narrate-only in the applet**, with a worth gate and a
   per-horizon call ceiling. Only after step 6 produces refs worth revising for.

Still undecided:

- Whether a bounded closed-question model resolver should ever be allowed for
  temporal reference.
- Whether the **activation gate** should change. It refuses to segment a day with
  no *span* source, and messages are discrete — so a message-heavy day still gets
  no events. Its reasoning still applies to bursts ("a thousand text messages
  never say when anything started"; a burst's span is derived from clustering,
  not measured), so changing it carries real fabrication risk.

## Related

- `day-article-plan.md` — the article written from the day itself
- [`the-day.md`](../build/the-day.md) — the day page and its data model
- [`event-timeline.md`](../build/event-timeline.md) — segmentation as evidence fusion
- [`wiki-editor.md`](../build/wiki-editor.md) — the note rules; [`wiki-notes-plan.md`](wiki-notes-plan.md) — the unbuilt note writer
- [`article-resolution.md`](../record/article-resolution.md) — article maintenance
- [`privacy-boundary.md`](../build/privacy-boundary.md) — egress; message bodies reach the Chat slot, which under BYO AI
  is whatever endpoint the user configured
