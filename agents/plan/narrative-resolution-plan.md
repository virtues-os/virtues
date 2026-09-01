# Narrative Resolution

**Status:** Not built — 2026-08-17. No queue, no generators, no page. But more of
the substrate exists than this plan first assumed: see **Inventory**, which was
taken from the schema and is the part to trust. Two findings there — rules are
never read, and entity resolution already computes and discards exactly the
questions this queue wants — change the build order and are cheap to fix.
Everything named here lives on `wave` and has never run on a box.

How the box comes to know a life: by keeping a standing list of what it does not
know, ordered by what knowing would be worth, and asking one question at a time
in the place where the answer will visibly pay off.

## The two failures this exists to fix

They look unrelated. They are the same failure seen from two ends.

### 1. The intake form

The interview asks fourteen questions — roughly an hour of writing about grief,
vice, faith and ambition — **before the box has shown the person anything at
all.** The census, the first moment of demonstrated value, sits on the far side
of that hour.

Worse, the questions are asked at the moment the box knows least. "In each of
those chapters, who were the people?" is an essay assignment handed to someone
who has owned the appliance for four minutes. The same question asked six weeks
later, grounded — *"you have exchanged 4,000 messages with this person since
2019; who are they to you?"* — is a different act entirely: it is the box
showing someone something true about their own life and asking one easy thing
about it.

**Recognition beats recall.** An ungrounded question is homework. A grounded one
is a gift.

### 2. The graph that cannot resolve itself

From `wiki-plan.md`, and it is correct:

> **The writer may never write `wiki_entity_refs`.** Not at confidence 0.5, not
> flagged, not ever. Promotion is a human click or an editor pass gated on
> `auto_update`. The graph stays deterministic and user-authored.

That doctrine is right and was bought with two failed attempts at semantic ER
(migrations 0061, 0062). But it has a hole nobody has filled: **if only a human
may resolve an ambiguity, there must be a mechanism by which humans are asked
to.** Today that mechanism is a click on a page you have to think to visit. So
the ambiguous cases are never resolved — not because the design is wrong, but
because it was only ever built pull-side.

**The queue in failure 1 is the push mechanism missing from failure 2.** They
are one system. This document treats them as one.

## What a question is

An **open question** is a thing the box does not know, which a person could
settle, which is worth settling. Two kinds, and keeping them apart is the whole
discipline of this design:

| | **identity** | **meaning** |
|---|---|---|
| asks | who is this handle? are these the same person? what is this address? | who matters? what are you up against? what should I never raise? |
| answer shape | a link, a merge, a label | prose, a choice, a rule |
| lands in | the graph — `wiki_refs`, `wiki_people`, `wiki_places` | `wiki_narrative_identity.document`, or `wiki_rules` |
| has a right answer | yes; verifiable, and can be wrong | no; authored, and cannot be wrong |
| supply | infinite, self-generating from the record | finite, ~15 of them, fixed |

**Merge the queue, the scoring and the asking surface. Never merge the answer
shapes.** A question declares what resolves it, and the surface renders the
control that shape needs. This is the line that keeps the union from becoming
mush: the two kinds share a lifecycle and nothing else.

## Where questions come from

### Deterministic generators

Cheap, exact, infinite, no model. These produce the identity half and some of
the meaning half. Each is a query over the record:

- a correspondent above N messages with no `wiki_people` row
- two entities sharing an identifier — a merge candidate
- a place visited above N times with no label
- a proper noun appearing above N times that resolves to nothing
- a stretch of days with a trace volume far below the trailing median — a dark
  period worth a sentence
- an entity with many refs and no article
- a person in the graph with no stated relation to the owner

### Model generators

Used sparingly, and never on a schedule that costs money by the hour:

- reading `wiki_narrative_identity.document` for what is thin or stale
- noticing the record and the stated identity disagree
- following up on a previous answer that opened something

### The line that keeps the ER doctrine intact

> **A model may propose a question. A model may never supply the answer.**

The model is allowed to say *"this person should probably be asked about."* It
is never allowed to write the edge, pick the merge, or decide the label. That is
`auto_update` gating restated: propose, never dispose. Every model-generated
question lands in the same queue as a deterministic one and is settled the same
way — by a person.

If this line is ever crossed for convenience, this design becomes the third
attempt at semantic ER and will fail the same way the first two did.

## Priority

Value of an answer ≈ **how much of the record the unknown touches × how much it
blocks.**

The first term is measured, not guessed: it is a `COUNT` over `wiki_refs`. This
is exactly what migration 0099 was written for — the attention plan's stated
purpose is to make "what the record returned to" countable rather than inferred,
*including for subjects not yet in the graph*, which is precisely the population
this queue asks about. A correspondent you write to daily who is unresolved
outranks a single message from 2014 by orders of magnitude, and the system can
know that without a model.

The second term is a small fixed weight per generator: a missing rule blocks
more than a missing label.

**Ordering by score alone is a trap.** Deterministic identity questions are easy
to generate and will flood the queue, pushing out the meaning questions that are
the reason any of this matters. The surface must enforce a mix — no more than
N identity questions between meaning questions — rather than draining the queue
in score order.

## Lifecycle

    generated → open → asked → answered
                            ↘ dismissed (this time)
                            ↘ never (permanent, and itself a fact worth keeping)
                      ↘ retired (the record answered it; the person never had to)

Two states carry weight:

**`never` is user-authored data**, not a UI preference. "Do not ask me about my
father" is the same kind of fact as a rule, and it must survive regeneration,
re-sync, and the question being re-derived by a generator that has forgotten.

**`retired`** is what keeps the queue honest. If someone labels a place in the
wiki, the question about that place must disappear without being asked. A queue
that asks what it already knows is worse than no queue.

## Where questions get asked

**The daily page, one at a time, in the margin of the write-up of yesterday.**

Not a "Questions" section. The placement is the design:

- it is contextual — you have just read the paragraph the question is about
- it is answerable in one line, because it is about one thing
- **the answer visibly improves tomorrow's page**, so the loop is short and
  closed

> Yesterday you spent four hours at Fell Street. What is that place?

A dedicated inbox turns this into a chore list. The daily page turns it into a
conversation with something that was paying attention. The moment it grows a
badge with a number on it, it is dead.

## Onboarding's remaining share

Two questions. Possibly a third, offered and never required.

| | why it cannot wait |
|---|---|
| **What are you working on right now? Five or ten things.** | Highest immediate utility, no emotional cost. It is the difference between tomorrow's page being accurate and being useful. |
| **Anything I should never bring up?** | One line. Not because it can be answered well yet, but because getting it wrong is the single unrecoverable failure. |
| *(optional)* **Where have you lived, and what were the chapters?** | The scaffold everything else hangs on, and it sets the register — it says immediately that this is not a CRM. Expensive, so it is offered, not demanded. |

Everything else is seeded into the queue as `open` and waits.

## How the fourteen redistribute

The existing questions are good writing and none of them are deleted. The `why`
text on each is the most persuasive material in the product and survives
verbatim as the question's rationale.

| question | disposition |
|---|---|
| `now` | **onboarding** |
| `rules` | **onboarding**, short form; also ongoing — new rules arrive forever |
| `chapters` | onboarding, optional; else early queue |
| `people` | **dissolves into generated identity questions.** One question per real person, grounded, once sources have synced. This is the single biggest improvement in the whole design. |
| `novelty` | early queue — highest information per word, being an explicit correction to the model's population prior |
| `admire`, `pride`, `vices`, `belief` | queue, spaced |
| `high_point`, `ambitions` | queue, mid |
| `loss`, `low_point`, `shadow_future` | queue, **late and gated on trust.** These are asked by a box that has earned the right to ask. Asking about grief in week one is a category error. |

## Schema sketch

Not final; here to make the shape arguable.

```sql
wiki_open_question (
  id            text primary key,   -- deterministic for generated ones, so a
                                    -- regenerated question is the SAME question
  kind          text not null,      -- 'identity' | 'meaning'
  answer_shape  text not null,      -- 'prose' | 'link' | 'merge' | 'choice' | 'rule'
  prompt        text not null,
  why           text,               -- shown behind a disclosure, as today
  subject_type  text,               -- what it is about, when grounded
  subject_id    text,
  source        text not null,      -- 'seed' | generator name
  score         double precision,   -- recomputed; never trusted as fresh
  state         text not null default 'open',
  asked_at      timestamptz,
  settled_at    timestamptz,
  answer        text                -- prose/rule answers; links land in the graph
)
```

**`id` must be deterministic for generated questions.** A generator that
produces a new id each run will re-ask something the person said `never` to,
which is the one failure that would make people turn this off.

## Inventory — what is already here

Taken from the schema and the code on 2026-08-17, not from memory. Several
pieces of this design turned out to be half-built, and two findings below change
the build order.

| | where | state |
|---|---|---|
| interview answers | `wiki_narrative_interview` | built |
| the long document | `wiki_narrative_identity.document` | built (0102) |
| the injected core | `wiki_narrative_identity.content` | built, and genuinely injected — `{narrative_identity}` in `agent/prompt.rs` |
| rules | `wiki_rules` (0101, renamed 0103) | table and capture UI built; **never read by anything** |
| attention substrate | `wiki_refs` (0099) | built, subject types widened |
| entity resolution | `entity_resolution/{people,places}` | built, deterministic, discards ambiguity |

All of it is on `wave`. None of it has run on a box.

### There is no "portrait"

An earlier draft of this plan used that word for a thing that does not exist.
What exists is a two-artifact split, already made and better reasoned, in 0102:

> `content` is the DISTILLED core: 60-110 words, and it is injected into every
> chat prompt. That is why it is short, and why the document a person actually
> reads cannot live there — a few thousand words in that column would ride along
> on every message they ever send.

**`document` is read by the human. `content` is read by the model.** One set of
answers, two artifacts, different lifetimes.

So the regeneration question is not "when do we redraw the portrait" but two
narrower ones: when is `document` redrafted from new answers, and when is
`content` redistilled from `document`? `drafted_at` exists precisely for the
first — 0102 added it so that regeneration is offered "when there are NEW
answers since the last draft, not whenever the row was touched."

### Finding 1: rules do not do anything

`wiki_rules` is touched only by `narrative_draft.rs` — read to list them, delete
and re-insert to save them. **No prompt reads them.** The box does not obey a
single rule anyone has written.

The interview tells people, in as many words, *"What you write here stops being
context and becomes a rule."* That is currently false. It is not a missing
feature; it is a broken promise on the most sensitive input in the product, and
it is the precondition for ever asking about grief, addiction, or a marriage
that ended.

**Nothing in this plan should be built before this is fixed.**

### Finding 2: the generators already run, and discard their findings

From `entity_resolution/people.rs`:

> an ambiguous handle owns nobody and its messages simply stay unresolved

That is the correct call under the ER doctrine — guessing is worse. But those
discarded cases **are this queue's raw material**, recomputed every pass and
dropped on the floor. `wiki_refs.resolved_by` currently holds only `system` and
`alias`, because there has never been a third path meaning *a human was asked*.

This is the cheapest thing in the entire plan. The detection exists. What is
missing is somewhere for it to put what it could not settle.

## The relationship to entity resolution

Not two parallel systems. **Entity resolution becomes a producer.**

    entity resolution (exists)  ──emits──┐
    deterministic gap queries   ──emits──┼──▶ queue ──▶ /wiki/resolution
    model generators            ──emits──┘              + the daily page
                                                              │
                          identity answers ──▶ the graph ─────┤
                          meaning answers  ──▶ document / rules ┘

"Resolution" is the umbrella and the page. "Entity resolution" keeps its name
and its code, and changes in exactly one way: it emits what it cannot settle
instead of discarding it.

The identity/meaning distinction stays real and surfaces twice — as `kind` on
each question, which decides the answer control, and as two sections on the
page. But **one queue, one lifecycle, one scoring function.** Split them into
two systems and the mix rule becomes unenforceable, which is the failure mode
that matters most.

## The page: `/wiki/resolution`

1. **Open questions**, identity and meaning mixed — the standing list of what
   the box does not know.
2. **What you have answered**, revisable. This is also where the nine held-back
   interview questions live for someone who *wants* to sit and answer twenty in
   a row; the drip is right by default and wrong for that person.
3. **Every rule, in full.** 0103 insists on this and it is currently homeless:
   *"A rule scattered across four hundred wiki entities is a rule nobody can
   audit... you have to be able to read every rule your box obeys."*
4. **The `never` list** — what it has stopped asking, and why.

## Build order

1. **Make rules work.** Read `wiki_rules` into the system prompt. Small, and it
   closes a live false promise.
2. **ER emits instead of discarding** → `wiki_open_question` rows. The queries
   already exist; this is plumbing.
3. **`/wiki/resolution`** — the page, answering, the rules audit.
4. **Scoring from `wiki_refs`**, then the mix rule.
5. **The daily-page slot** — the drip.
6. **Redraft cadence** on `drafted_at`.

Steps 1 and 2 are small and both make existing things honest. They are worth
doing whatever happens to the rest.

## Risks

- **It becomes an inbox.** One at a time, always dismissible, no badge, no count.
- **The queue drifts to identity questions** because they are easy to generate.
  Enforce the mix.
- **Cold start.** Week one has no synced record, so nothing grounded to ask. The
  two onboarding questions and the inward seeds carry it.
- **Fewer total answers from completers.** Anyone who finishes fourteen questions
  today gives more than the drip will get in a month. The bet is that far more
  people give something than currently give everything — and that a grounded
  question six weeks in is worth more than an ungrounded one on day zero.
- **Asking about grief badly.** The late-and-gated rule is not a nicety. A box
  that asks about a dead parent in week one has destroyed the relationship.

## Open questions

- Does `chapters` belong in onboarding at all, or is the register it sets worth
  the expense?
- What retires a meaning question — can they ever be answered "enough"?
- Does `document` redraft on every answer, or on a threshold? The first is
  expensive; the second means answers appear not to matter. `drafted_at` gives
  the signal either way.
- When is `content` redistilled from `document`? It rides on every prompt, so a
  stale core is worse than a stale document.
- Is there a second surface for someone who *wants* to sit and answer twenty in a
  row? The drip is right by default and wrong for that person.
