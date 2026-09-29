# Narrative Identity

*The canonical definition: what the thing IS, and how the chat prompt carries
it. Vocabulary note: "NI", "the document", "In your own words", and Adam's
"values doc / life checkpoint manifesto portrait" all name the artifact
defined here.*

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

## One artifact (and the neighbor)

One idea, ONE carrier — no abridged version at all:

| artifact | what | reader | size |
|---|---|---|---|
| **the document** | the full prose — who they are, the arc | the person AND the AI, every message | a real wiki article page: editor, history, marginalia; injected whole (paragraph-boundary ceiling ~2k tokens) |

There is deliberately no distilled "core" or "capsule" beside it. Two versions
of one identity drift, and a machine abridger invents standing directives ("be
direct, don't go easy") that silently steer every chat. What the person edits
is byte-for-byte what the assistant carries: `chat.rs::build_narrative_identity`
reads the article prose (`wiki_articles`, subject `narrative_identity`)
directly, cut at a paragraph boundary past ~2k tokens. The
interview's structured sibling is `wiki_chapters` — the authored partition of
the life, with a seeded article page per chapter.

**Rules are NOT part of narrative identity.** They are a
neighboring channel: NI is *who the person is* — theirs, prose, weighed by
the model; rules (`wiki_rules`, avoid/defend) are *standing orders to the
machine* — governance, enforced, absolute. "My father died last year"
belongs in the document (context, weighed); "never raise my father unless I
do" is a rule (obeyed). They ride adjacent in the prompt, but a rule is an
instruction and the NI is a portrait, and conflating them makes the
portrait read as a policy file.


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
   would be both wrong and insulting — which is why there is no generator
   that drafts a portrait from observed data. Significance is
   user-sourced; the graph stays deterministic; the NI stays authored.
2. **The machine writes it only while it is empty.** The drafter arranges
   the person's own interview words into the first document; from then on
   the person edits and the machine never overwrites. Growth after that
   happens by *asking* (the resolution queue), never by writing.

Stated more precisely: **ratification, not composition, is the
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

- **Born in the interview**: Setup's Interview step asks one question per
  screen, over the same interviewer chat (`chat_getting_started`) and prompt
  (`agent/prompt.rs`, "The territory"). Six territories, in order: the
  chapters, what makes them unlike others, who they admire, the strongest
  pull, what they believe, the shape of a day; chapters drawn in Setup's
  Timeline step are sent as the first answer. "Write it up" — the
  interview's one TOOL, the agent's to call — CLOSES the interview: it
  arranges their words into the document, the chapters, and a page per
  chapter, and the interview is over. The interviewer offers the close once
  the sixth territory is answered, because a conversation that just runs on
  leaves people unsure whether they are done.
- **In the first person.** The document reads as theirs because it is
  written as they would write it — "I", not "you". A second-person draft
  reads as the machine describing them back, which is the exact posture the
  artifact exists to refuse. The assistant's prompt says so: "I" in the
  document is the person, never the model.
- **Never finished, on purpose**: a record of a life can't be complete, and
  saying so is what disarms the perfectionism that kills the first draft.
- **Grows by grounded questions**: the resolution queue asks one thing at a
  time, later, with evidence attached ("4,000 messages with this person
  since 2019 — who are they to you?"). Recognition beats recall; an
  ungrounded question is homework, a grounded one is a gift.
- **Corrected by editing**: the document is a page; the person rewrites it
  whenever they like, and the next message carries the edit — there is no
  derived copy to lag behind it.

## Its place in the system prompt

The chat prompt is an ordered registry of named blocks
(`api/chat.rs::build_system_prompt_blocks`, rendered by
`agent/prompt_blocks.rs::assemble`) — each a different map from the same life
into text, stable blocks first for prompt caching, the one binding block last
for constraint recency. What runs, in order:

| # | tag | carries | author | cadence |
|---|---|---|---|---|
| 1 | `base` | one fused string (`agent/prompt.rs::build_personalized_prompt`): the character and house style, the owner's `<style_notes>`, `<narrative_identity>` (the document, whole), `<tool_usage>` and the `<mode>` guidance | us + the person | slow |
| 2 | `precedence` | the precedence ladder, as text | us | static |
| 3 | `memory` | what the machine has learned, in three lanes | machine; the person can edit | session |
| 4 | `circumstances` | the computed present | SQL only, no LLM | quarter-hour |
| 5 | `coverage` | what the record holds, per table, as a date range — so "the record is silent" differs from "nothing happened" | SQL only | daily |
| 6 | `active_project` | the Project (room) the chat lives in | the UI | session |
| 7 | `skill` | the running skill's file body, if any | us | per turn |
| 8 | `active_context` | the open page's live content | the UI | per turn |
| 9 | `rules` | the person's absolute imperatives | the person | rarely |

`<current_chapter>` — a machine-drafted, person-edited narration of the open
period between the life story and today — is **not built**; the last narrated
days in `<circumstances>` stand in for it.

Prose vocabulary: "your life story" (never "portrait" — a portrait is
painted by another's hand of a sitting subject, which is the exact
connotation the doctrine forbids; the word also names a deleted feature).
The two machine-facing surfaces get first-person UI names on purpose —
theirs is "In your own words," the machine's is "What I've learned" —
**voice marks ownership**.

**Precedence**, stated once in the prompt (`prompt_blocks.rs::precedence_line`):
rules outrank everything and are absolute; narrative identity outranks the
machine's own memory; both outrank house guidance; circumstances are
situational fact, not instruction. Declarative blocks describe; only rules
command, and the command goes last. The registry's `rung` values carry the
same ranking and must stay in sync with the line.

### Memory

Three lanes — **facts** (their world: the dog's name), **manner** (concise,
numbered lists), **practices** (what they're holding to) — as per-note rows
(`api/assistant_memories.rs`) with add/revise/retire, per-lane caps (12
facts, 8 each of the others; a full lane refuses new notes until one is
retired), and a page in Settings the person can read and edit. A note the
person wrote is marked `[theirs]` and the machine never revises it. A
machine channel about the person that the person cannot see is the thing
this product deleted once already. The test for what goes here vs the NI:
could a stranger who spent 200 hours beside you learn it? Then memory.
Does being wrong about it insult rather than inconvenience? Then it may
only ever arrive user-authored.

### Circumstances

Not "prudential context": prudence is a virtue *of the agent*, and this block
is the supply prudence consumes. The right term
of art already existed: *circumstantiae* (ST I-II q.7), what "stands
around" the act, and Cicero's canonical list — who, what, where, when,
how, why — is nearly a spec for the fields:

Its sections are one registry (`api/circumstances.rs::SECTIONS`), in order:
the clock (floored to the quarter-hour, computed once so every line derives
from the same instant), identity, the chapters as they named them (names and
years only), place, today's spine, the calendar, recent people, live threads,
last night's sleep, recently narrated days, and connected sources.

- **Recent people** carry entity ids and are labelled *recency, not
  significance*: who is around, never who matters (bonds, in the NI, carry
  who matters).
- **Observances** — recurring time the person keeps: fasts, sabbaths,
  anniversaries — are not built. A machine that knows the clock but not that
  it is Lent or a death-anniversary is missing a dimension of time; when
  built, from user-authored sources only.

Fixed line caps, deterministic queries, and absence stays silent: a section
with no data renders nothing, and a section whose query fails is logged and
omitted, never defaulted. Prudence keeps one sentence, in the docs rather than
the prompt: *the system is memoria made durable and circumstantiae made
legible, offered into the person's counsel.*

### Rules

Kept few — aim for 3 to 10; nothing in code caps them yet — because of what
bindingness *is*: a rule is
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
- **Last in the prompt** for recency, and outside the cached prefix, so they
  are re-sent every turn. Not built: a compressed digest re-injected near the
  live turn in long conversations — at 60k tokens of transcript the end of
  the prompt is the dead middle of the context.

### Assembly notes

One ordered registry and one error policy: each block logs and omits on
failure, never fabricates. An empty block renders nothing (an empty `<rules>`
would teach the model the section is noise). Bodies run concurrently and are
concatenated strictly in list order. The cache split is fixed by cadence: the
first per-turn block (`skill`) starts the uncached tail, so `skill`,
`active_context` and `rules` are re-sent each turn whether or not a block
rendered — a page opening must not move the boundary. Every query is
deterministic (total `ORDER BY`; the clock computed once). Machine-written
blocks carry provenance — memory poisoning via ingested content is a
documented attack class, and *who wrote this, from what evidence, when* is
the practiced defense.

## What it is NOT

- Not a psychological evaluation, and never a place the machine records its
  own opinions of the person.
- Not a memory store or fact database (that's the record and the graph).
- Not a summary of the record — it is exactly the part the record can't say.
- Not private notes *about* the user by the system: the person can read
  every word, because every word is theirs.
