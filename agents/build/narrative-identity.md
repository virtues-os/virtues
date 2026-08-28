# Narrative Identity

*The canonical definition, 2026-08-27. This concatenates and supersedes the
scattered definitions in `lsi-plan.md`, `narrative-resolution-plan.md`, the
drafter's prompt, and three weeks of session decisions. Those documents keep
their build detail; this one says what the thing IS. Vocabulary note: "NI",
"the document", "In your own words", and Adam's "values doc / life checkpoint
manifesto portrait" all name the artifact defined here.*

## What it is

**Narrative identity is the part of a person's context that no record can
derive: who they are, why they are that way, and what they are for — authored
in their own words, held on their own box, and read by their AI before every
conversation so that it stops assuming they are the average person.**

The record answers *what happened* — where you went, who you messaged, what
you opened. Narrative identity answers everything the record cannot: the
**who/what/when/where/why of a life across its whole arc** — history (the
chapters, the formative events, the losses), present (worldview, cares,
temperament, what they're up against), and future (goals lined up, the person
they mean to become, the person they fear becoming). It is a manifesto as much
as a portrait: not only *what is true of me* but *what I hold*.

## Why it exists (the one-sentence justification)

Everything an AI has not been told, it fills in from the population mean —
agreeable, agnostic, faintly therapeutic, average. The record fixes what the
AI knows *happened*; narrative identity fixes **who it is talking to**. It is
the single highest-leverage document in the product: every conversation,
every day page, every suggestion is downstream of it.

## The two artifacts (and the neighbor)

One idea, two carriers:

| artifact | what | reader | size |
|---|---|---|---|
| **the document** | the full prose — chapters, worldview, the arc | the person (and the drafter) | pages; a real wiki article page: editor, history, marginalia |
| **the core** | the distillation carried into every chat (`wiki_narrative_identity.content`) | the AI, every message | 80–120 words (hard ceiling ~2k tokens) |

**Rules are NOT part of narrative identity** (settled 2026-08-27). They are a
neighboring channel: NI is *who the person is* — theirs, prose, weighed by
the model; rules (`wiki_rules`, avoid/defend) are *standing orders to the
machine* — governance, enforced, absolute. "My father died last year"
belongs in the document (context, weighed); "never raise my father unless I
do" is a rule (obeyed). They ride adjacent in the prompt, but a rule is an
instruction and the NI is a portrait, and conflating them makes the
portrait read as a policy file.

The core stays deliberately short: a longer core does not make the AI
understand better, it makes it *perform* understanding more often (chat.rs's
own doctrine).

## What belongs in it

Everything below — **when the person offered it**:

- **History**: the chapters and their changepoints; formative events; losses
  and grief; what shaped them. Trauma belongs here when they put it here —
  named plainly, in their words, never excavated.
- **Present**: worldview and religion; what they care about; the manifesto
  lines they actually hold; temperament, traits, virtues and vices; the
  strongest pull (money/power/pleasure/fame); addictions and what they are
  up against — both what they have overcome and what they haven't yet.
- **Future**: goals lined up; the three-year and ten-year wants; the feared
  future — the version of themselves the AI should help them notice they are
  drifting toward.
- **Bonds**: the constitutive relationships — who they are bound to and how.
  The mother, the business partner, the friend of twenty years are not
  circumstances of a life; they are constituents of the identity (a self is
  only a self among other selves). One authored line per person, linked to
  the entity id. This is where the resolution queue's best question lands
  ("4,000 messages with this person since 2019 — who are they to you?").
  Distinct from the recent-people list in circumstances: recency says who is
  *around*; bonds say who *matters* and *how*.
- **Self-frameworks**: MBTI, Big Five, Enneagram, attachment styles — any
  vocabulary the person uses about themselves. These are welcome and
  valuable precisely because models natively understand them: "INFJ, high
  openness, low conscientiousness about admin" is enormous bandwidth in six
  words. **Self-reported only.** The machine never administers, infers, or
  assigns a type — a framework in the NI is a quote, not a diagnosis.
- **Voice**: how they speak. This is captured structurally rather than
  described — the drafter keeps their words, so their diction, cadence, and
  the names they call things survive into the document, and the AI absorbs
  the register from the sample.

## Provenance — the two iron rules

1. **User-authored, never inferred.** Values, wounds, telos, and type cannot
   be derived from behavior; a machine guessing them from message volume
   would be both wrong and insulting. The observed-data portrait generator
   was deleted for violating this (2026-08-26). Significance is
   user-sourced; the graph stays deterministic; the NI stays authored.
2. **The machine writes it only while it is empty.** The drafter arranges
   the person's own interview words into the first document; from then on
   the person edits and the machine never overwrites. Growth after that
   happens by *asking* (the resolution queue), never by writing.

Stated more precisely (2026-08-28): **ratification, not composition, is the
locus of authorship.** The machine may propose anything — a chapter draft, a
candidate articulation, a Socratic "is it fair to say…?" — so long as
nothing enters an identity surface without the person's explicit act. The
line runs through Lonergan's levels: the machine may operate at experience
and understanding (data, proposed insights), must stop at judgment
(offering, never affirming — the person's *edit* is the judgment), and
cannot touch decision (telos, values) — not as a safety policy but
constitutively: a self authored by another is not a self, it is a
description. Two invariants fall out and are load-bearing:

- **A chapter never sediments into the life story unedited.** Sedimentation
  is ratification; skipping the person's pass skips the act that makes the
  words theirs.
- **Machine drafts use chronicle-language.** A draft titled "The Decline"
  has already smuggled a verdict into a provisional block. Drafts narrate
  what happened, in the person's own prior vocabulary; evaluative namings
  are offered only as questions.

## How the AI uses it — the subconscious contract

The NI is **personal knowledge, not conversation material**. The AI reads it
before every exchange and lets it shape everything — tone, register, what to
suggest, what never to suggest, which future to gently weigh against — while
almost never surfacing it:

- Never recite it back ("as an INFJ, you…", "given your father…"). A person
  should *feel* understood, not be shown the file.
- Never use it to explain them to themselves ("you do this because…") — the
  never-psychologize rule survives from authoring into use.
- It calibrates defaults silently: what "a good suggestion" means for THIS
  person, which vices not to feed, which register lands.
- The rules are the exception: they are enforced, not weighed, and absolute.
- **Never side with the document by default.** An every-conversation AI
  holding a person's self-authored identity is a maximal self-verification
  engine, and sycophantic agreement measurably reduces people's willingness
  to repair conflicts (Cheng et al., *Science* 2025). Knowing someone's
  story is not a license to take their side. Attribute, don't assert
  ("you've written that…"), and hold the account as *theirs, of a date* —
  not as fact about them.

## How it comes to exist, and how it grows

- **Born in the first conversation**: the interview chat ("In your own
  words", the product's first conversation — see lsi-plan.md final form)
  covers five territories: the chapters, what makes them unlike others, who
  they admire, the strongest pull, what they believe. "Write it up" arranges
  their words into the document + core.
- **Never finished, on purpose**: a record of a life can't be complete, and
  saying so is what disarms the perfectionism that kills the first draft.
- **Grows by grounded questions**: the resolution queue asks one thing at a
  time, later, with evidence attached ("4,000 messages with this person
  since 2019 — who are they to you?"). Recognition beats recall; an
  ungrounded question is homework, a grounded one is a gift.
- **Corrected by editing**: the document is a page; the person rewrites it
  whenever they like, and the core follows.

## Its place in the system prompt — the formula

*Settled 2026-08-28 after a seven-perspective review (Thomist, Ricoeur/
McAdams, personality-psychology literature, 2026 context-engineering
evidence, product comps, backend design, naming). The design survived; what
follows is the ratified shape.*

The system prompt is an ordered set of named blocks — each a different map
from the same life into text, ordered stable-first for prompt caching, with
the one imperative block last for constraint recency:

| # | tag | UI name | carries | author | changes |
|---|---|---|---|---|---|
| 1 | `<character>` | Character | the machine's persona and house style | us | never |
| 2 | `<tool_usage>` / `<mode>` | — | competence | us | never |
| 3 | `<narrative_identity>` | In your own words | the life story: arc, worldview, telos, bonds | the person | years |
| 4 | `<current_chapter>` | The current chapter | the open period, drafted from the record | machine drafts, person edits | weeks |
| 5 | `<memory>` | What I've learned | facts / manner / practices lanes | machine, person can edit | continuously |
| 6 | `<circumstances>` | Right now | the computed present | SQL only, no LLM | hourly |
| 7 | `<active_notebook>` / `<active_context>` | — | the room and the open page | the UI | per turn |
| 8 | `<rules>` | Precepts | the person's absolute imperatives | the person | rarely |

Prose vocabulary: "your life story" (never "portrait" — a portrait is
painted by another's hand of a sitting subject, which is the exact
connotation the doctrine forbids; the word also names a deleted feature).
The two machine-facing surfaces get first-person UI names on purpose —
theirs is "In your own words," the machine's is "What I've learned" —
**voice marks ownership**.

**Precedence**, stated once in the prompt, rendered from block metadata:
rules > narrative identity > current chapter > memory > character >
circumstances. Declarative blocks describe; the one imperative block binds.
Six voices describe, one commands, and the command goes last.

### The current chapter (block 4)

The middle duration the three-speed model was missing: too fast for the
life story, too slow for today. A rolling narration of the open period,
machine-drafted from the record in chronicle-language, edited by the person
while they live it, and — when they declare it closed — **sedimented into
the life story as a written chapter**. Sedimentation is how the NI grows
without another interview, and closing is a first-class interview moment
(exploration first: "what was that period really about?" — then
resolution: "where did it leave you?"). Closed chapters stay versioned and
reopenable: a *written* identity uniquely enables narrative foreclosure,
and reinterpretation of the past is half of growth. Exactly one chapter is
open at a time. No shipping product has this construct.

### Memory (block 5)

The reform of the invisible `<memory>` blob. Three lanes — **facts** (their
world: the dog's name), **manner** (concise, numbered lists), **practices**
(what they're holding to) — per-note rows with add/revise/retire, per-lane
caps, dates on every note, and a page the person can read and edit. A
machine channel about the person that the person cannot see is the thing
this product deleted once already. The test for what goes here vs the NI:
could a stranger who spent 200 hours beside you learn it? Then memory.
Does being wrong about it insult rather than inconvenience? Then it may
only ever arrive user-authored.

### Circumstances (block 6)

Formerly "prudential context" — retired, because prudence is a virtue *of
the agent* and this block is the supply prudence consumes. The right term
of art already existed: *circumstantiae* (ST I-II q.7), what "stands
around" the act, and Cicero's canonical list — who, what, where, when,
how, why — is nearly a spec for the fields:

- the clock (quantized to 15 minutes — honest, and cache-friendly)
- the place: current or last visit; home
- the day so far: today's deterministic spine
- the calendar: today's and tomorrow's events
- recent people: last ~2 weeks' correspondents WITH entity ids — labeled
  *recency, not significance*: who is around, never who matters (bonds, in
  the NI, carry who matters)
- live threads: recently edited pages, active notebooks, last night's sleep
- observances: recurring time the person keeps — fasts, sabbaths,
  anniversaries. Lived time is cyclical as well as linear, and a machine
  that knows the clock but not that it is Lent or a death-anniversary is
  missing a dimension of time, not a feature. User-authored sources only.

Hard budget (~600–800 tokens via fixed line caps), deterministic queries,
absence stays silent. Prudence keeps one sentence, in the docs rather than
the prompt: *the system is memoria made durable and circumstantiae made
legible, offered into the person's counsel.*

### Rules (block 8)

Kept few — 3 to 10, capped — because of what bindingness *is*: a rule is
an exclusionary reason; it doesn't outweigh considerations, it excludes
them from deliberation. Many rules collide, colliding rules get weighed,
and a weighed rule has been demoted back to a preference. Discipline:

- **Only imperatives.** One aspiration ("I'm trying to read more") in the
  block decays every rule toward advice. Aspirations live in the NI or
  memory.
- **Revision is ceremonious** — restatement and an explicit act, never a
  settings toggle. A rule you can casually flip mid-conversation was never
  a rule; it was a preference wearing a uniform. (A rule is a promise
  deposited with a witness — self-constancy against your own future moods.)
- **Affirmative rules cost more than prohibitions.** "Never raise X" binds
  always; "help me hold Y" binds always *but not at every moment* — each
  commission spends the machine's judgment about occasions. Cap
  commissions harder.
- **Last in the prompt** for recency, plus a compressed digest re-injected
  near the live turn in long conversations — at 60k tokens of transcript
  the end of the prompt is the dead middle of the context.

### Assembly notes

Ordered registry, one error policy (log and omit, never fabricate — the
house rule made structural), per-block budgets with sentence-boundary
truncation, empty blocks render nothing (an empty `<rules>` teaches the
model the section is noise), cache breakpoint after memory, every query
deterministic (total ORDER BY; the clock computed once, quantized).
Machine-written blocks carry provenance — memory poisoning via ingested
content is a documented attack class, and *who wrote this, from what
evidence, when* is the practiced defense.

## What it is NOT

- Not a psychological evaluation, and never a place the machine records its
  own opinions of the person.
- Not a memory store or fact database (that's the record and the graph).
- Not a summary of the record — it is exactly the part the record can't say.
- Not private notes *about* the user by the system: the person can read
  every word, because every word is theirs.
