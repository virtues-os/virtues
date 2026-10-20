# Narrative Resolution

**Status:** Open. Build step 1 is done: active `wiki_rules` are read into the
chat system prompt (`build_rules` in `api/chat.rs`, 8a54e7a6). No queue, no
generators, no page yet. Entity resolution already computes and discards
exactly the questions this queue wants (see **Inventory**), which makes step 2
cheap.

Goal: the box keeps a list of what it does not know about the owner's life,
ordered by how useful each answer would be, and asks one question at a time in
the place where the answer visibly improves something.

## Problems this fixes

The two problems below look unrelated, but they have the same cause seen from
two sides.

### 1. Onboarding asks everything up front

The interview asks fourteen questions — roughly an hour of writing about grief,
vice, faith and ambition — **before the box has shown the person anything at
all.** The census, the first point where the box shows something useful, comes
only after that hour.

The questions are also asked when the box knows least. "In each of those
chapters, who were the people?" is an essay question for someone who has owned
the appliance for four minutes. Six weeks later the same question can be
grounded in data — *"you have exchanged 4,000 messages with this person since
2019; who are they to you?"* — and then it is a much easier question: the box
shows the person something true about their own life and asks one simple thing
about it.

People find it easier to recognise than to recall. An ungrounded question asks
the person to do work; a grounded one shows them something about their life
first, which they are more likely to want to answer.

### 2. Nobody is asked to resolve ambiguous graph links

The note rules in [`wiki-editor.md`](../build/wiki-editor.md) say that
**the writer may never write `wiki_refs`**, at any confidence and with or
without a flag. Only a human click or an editor pass promotes a link. The
graph stays deterministic and user-authored.

That rule is right, and it came out of two failed attempts at semantic ER (see
`entity_resolution/mod.rs`). But it leaves a gap: **if only a human may resolve
an ambiguity, something has to ask a human to do it.** Today the only way is a
click on a page the person has to remember to visit. So the ambiguous cases are
never resolved. The design is fine; it was just only built for the case where
the person goes looking.

**The queue that fixes problem 1 is also the push mechanism problem 2 is
missing.** This document treats them as one system.

## What a question is

An **open question** is something the box does not know, that a person could
answer, and that is worth answering. There are two kinds, and the design depends
on keeping them separate:

| | **identity** | **meaning** |
|---|---|---|
| asks | who is this handle? are these the same person? what is this address? | who matters? what are you up against? what should I never raise? |
| answer shape | a link, a merge, a label | prose, a choice, a rule |
| lands in | the graph — `wiki_refs`, `wiki_people`, `wiki_places` | `wiki_narrative_identity.document`, or `wiki_rules` |
| has a right answer | yes; verifiable, and can be wrong | no; authored, and cannot be wrong |
| supply | infinite, generated continuously from the owner's data | finite, ~15 of them, fixed |

**The two kinds share the queue, the scoring and the asking surface. They do not
share answer shapes.** Each question declares what resolves it, and the surface
renders the control that shape needs. The two kinds share a lifecycle and
nothing else; that keeps the combined queue from blurring them together.

## Where questions come from

### Deterministic generators

These are cheap, exact, unlimited, and use no model. They produce all the
identity questions and some of the meaning questions. Each is a query over the
owner's data:

- a correspondent above N messages with no `wiki_people` row
- two entities sharing an identifier — a merge candidate
- a place visited above N times with no label
- a proper noun appearing above N times that resolves to nothing
- a stretch of days with a trace volume far below the trailing median — a quiet
  period worth a sentence
- an entity with many refs and no article
- a person in the graph with no stated relation to the owner

### Model generators

Use these sparingly, and never on a schedule that costs money by the hour:

- reading `wiki_narrative_identity.document` for what is thin or stale
- noticing that the owner's data and the stated identity disagree
- following up on a previous answer that raised something new

### Models may ask but never answer

> **A model may propose a question. A model may never supply the answer.**

A model may say *"this person should probably be asked about."* It may never
write the edge, pick the merge, or decide the label. Every model-generated
question goes into the same queue as a deterministic one and is answered the
same way — by a person.

If this rule is ever relaxed for convenience, this design becomes the third
attempt at semantic ER and will fail the same way the first two did.

## Priority

Value of an answer ≈ **how much of the owner's data the unknown touches × how
much it blocks.**

The first term is measured, not guessed: it is a `COUNT` over `wiki_refs`. This
is what widening `wiki_refs` to `event`/`day`/`thread` subjects was for — the
[attention plan](attention-plan.md)'s stated purpose is to make "how often later
data refers back to a thing" countable rather than inferred, *including for
subjects not yet in the graph*, which is the same set of subjects this queue
asks about. An unresolved correspondent you write to daily outranks a single
message from 2014 by orders of magnitude, and the system can compute that
without a model.

The second term is a small fixed weight per generator: a missing rule blocks
more than a missing label.

**Do not order by score alone.** Deterministic identity questions are easy to
generate and will flood the queue, pushing out the meaning questions, which are
the main reason for building this. The surface must enforce a mix — no more than
N identity questions between meaning questions — rather than draining the queue
in score order.

## Lifecycle

    generated → open → asked → answered
                            ↘ dismissed (this time)
                            ↘ never (permanent, and itself a fact worth keeping)
                      ↘ retired (the owner's data answered it; the person never had to)

Two states matter most:

**`never` is user-authored data**, not a UI preference. "Do not ask me about my
father" is the same kind of fact as a rule, and it must survive regeneration,
re-sync, and a generator re-deriving the same question later.

**`retired`** keeps the queue from asking what it already knows. If someone
labels a place in the wiki, the question about that place must disappear
without being asked. A queue that asks what it already knows is worse than no
queue.

## Where questions get asked

**On the daily page, one at a time, in the margin of the write-up of
yesterday.**

There is no "Questions" section. The placement matters for three reasons:

- it has context — you have just read the paragraph the question is about
- it can be answered in one line, because it is about one thing
- **the answer visibly improves tomorrow's page**, so the feedback loop is
  short

> Yesterday you spent four hours at Fell Street. What is that place?

A dedicated inbox would make this feel like a chore list. On the daily page it
feels like a conversation with something that was paying attention. If it ever
gets a badge with a count, people will stop using it.

## What onboarding still asks

Two questions, plus an optional third that is offered and never required.

| | why it cannot wait |
|---|---|
| **What are you working on right now? Five or ten things.** | Highest immediate utility, no emotional cost. It decides whether tomorrow's page is merely accurate or actually useful. |
| **Anything I should never bring up?** | One line. It cannot be answered well yet, but getting it wrong is the one failure that cannot be recovered from. |
| *(optional)* **Where have you lived, and what were the chapters?** | The structure everything else hangs on, and it sets the tone — it tells the person straight away that this is not a CRM. It takes time to answer, so it is offered, not required. |

Everything else is seeded into the queue as `open` and waits.

## Where the fourteen questions go

The existing questions are well written and none of them are deleted. The `why`
text on each is some of the most persuasive writing in the product, and it is
kept verbatim as the question's rationale.

| question | disposition |
|---|---|
| `now` | **onboarding** |
| `rules` | **onboarding**, short form; also ongoing — new rules keep arriving |
| `chapters` | onboarding, optional; else early queue |
| `people` | **replaced by generated identity questions.** One question per real person, grounded, once sources have synced. This is the biggest single improvement in the design. |
| `novelty` | early queue — it gives the most information per word, because it explicitly corrects the model's population prior |
| `admire`, `pride`, `vices`, `belief` | queue, spaced |
| `high_point`, `ambitions` | queue, mid |
| `loss`, `low_point`, `shadow_future` | queue, **late and gated on trust.** The box should ask these only after it has been useful for a while. Asking about grief in week one is the wrong question at the wrong time. |

## Schema sketch

Not final; included so the shape can be discussed.

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
which is the one failure most likely to make people turn this off.

## Inventory — what is already here

Taken from the schema and the code, not from memory.

| | where | state |
|---|---|---|
| interview answers | `wiki_narrative_interview` | built |
| the long document | `wiki_narrative_identity.document` | built |
| the injected core | `wiki_narrative_identity.content` | built, and actually injected — `{narrative_identity}` in `agent/prompt.rs` |
| rules | `wiki_rules` | capture UI built; active rules injected into chat by `build_rules` |
| attention substrate | `wiki_refs` | built; `event`/`day`/`thread` subjects allowed, none written |
| entity resolution | `entity_resolution/{people,places}` | built, deterministic, discards ambiguity |

### There is no "portrait"

What exists is two artifacts: `content` is the distilled core, 60–110 words,
injected into every chat prompt — which is why it is short, and why the
document a person actually reads cannot live there.

**The human reads `document`. The model reads `content`.** One set of answers
produces two artifacts with different lifetimes.

So the regeneration question splits into two narrower ones: when is `document`
redrafted from new answers, and when is `content` redistilled from `document`?
`drafted_at` exists for the first — regeneration is offered when there are new
answers since the last draft, not whenever the row was touched.

### Rules are enforced in chat only

The interview tells people *"What you write here stops being context and
becomes a rule."* `build_rules` makes that true for chat. Rules must be enforced
before the box ever asks about grief, addiction, or a marriage that ended, so
any new surface that asks or writes (the daily-page slot, model generators)
must read the same rules before it ships. A failed read degrades to "no rules
this turn" and is logged.

### Entity resolution already finds these questions and throws them away

From `entity_resolution/people.rs`:

> an ambiguous handle owns nobody and its messages simply stay unresolved

Under the ER rules that is correct — guessing is worse. But those discarded
cases **are the raw material for this queue**, recomputed every pass and then
dropped. `wiki_refs.resolved_by` currently contains only `system` and `alias`,
because there has never been a third path meaning *a human was asked*.

This is the cheapest part of the plan. The detection exists; it just has
nowhere to put the cases it could not resolve.

## The relationship to entity resolution

These are not two parallel systems. **Entity resolution becomes one of the
producers for the queue.**

    entity resolution (exists)  ──emits──┐
    deterministic gap queries   ──emits──┼──▶ queue ──▶ /wiki/resolution
    model generators            ──emits──┘              + the daily page
                                                              │
                          identity answers ──▶ the graph ─────┤
                          meaning answers  ──▶ document / rules ┘

"Resolution" is the umbrella name and the page. "Entity resolution" keeps its
name and its code, and changes in exactly one way: it emits the cases it cannot
resolve instead of discarding them.

The identity/meaning distinction still shows up in two places — as `kind` on
each question, which decides the answer control, and as two sections on the
page. But there is **one queue, one lifecycle, one scoring function.** If they
were split into two systems, the mix rule could not be enforced, and that is
the failure that matters most.

## The page: `/wiki/resolution`

1. **Open questions**, identity and meaning mixed — the current list of what
   the box does not know.
2. **What you have answered**, editable. This is also where the nine held-back
   interview questions live for someone who *wants* to sit and answer twenty in
   a row; one-at-a-time is the right default but wrong for that person.
3. **Every rule, listed completely.** If rules are scattered across four
   hundred wiki entities, nobody can audit them; you have to be able to read
   every rule your box follows, and today there is no page for that.
4. **The `never` list** — what the box has stopped asking, and why.

## Build order

1. **ER emits instead of discarding** → `wiki_open_question` rows. The queries
   already exist; this is plumbing.
2. **`/wiki/resolution`** — the page, answering, the rules audit.
3. **Scoring from `wiki_refs`**, then the mix rule.
4. **The daily-page slot** — one question at a time.
5. **Redraft cadence** on `drafted_at`.

Step 1 is small and stops the existing detection from throwing away its
results. It is worth doing whatever happens to the rest.

## Risks

- **It becomes an inbox.** One at a time, always dismissible, no badge, no count.
- **The queue drifts to identity questions** because they are easy to generate.
  Enforce the mix.
- **Cold start.** In week one nothing has synced yet, so there is nothing
  grounded to ask. The two onboarding questions and the inward seeds cover that
  period.
- **Fewer total answers from people who would have finished.** Anyone who
  finishes fourteen questions today gives more than one-at-a-time asking will
  get in a month. The assumption is that far more people will answer something
  than currently answer everything — and that a grounded question six weeks in
  is worth more than an ungrounded one on day zero.
- **Asking about grief badly.** The late-and-gated rule is a requirement. A box
  that asks about a dead parent in week one loses the person's trust.

## Open questions

- Does `chapters` belong in onboarding at all, or is the tone it sets worth the
  time it takes?
- What retires a meaning question — can one ever be answered "enough"?
- Does `document` redraft on every answer, or on a threshold? The first is
  expensive; with the second, answers appear not to matter. `drafted_at` gives
  the signal either way.
- When is `content` redistilled from `document`? It goes into every prompt, so
  a stale core is worse than a stale document.
- Is there a second surface for someone who *wants* to sit and answer twenty in
  a row? One-at-a-time is the right default but wrong for that person.
