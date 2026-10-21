# Narrative Identity

*The canonical definition: what the thing IS, and how the chat prompt carries
it. Vocabulary note: "NI", "the document", "In your own words", and the
owner's older names "values doc / life checkpoint manifesto portrait" all
refer to the document defined here.*

## What it is

**Narrative identity is the part of a person's context that cannot be derived
from their data: who they are, why they are that way, and what they want their
life to be for. The person writes it in their own words, it is stored on their
own box, and their AI reads it before every conversation so that it stops
assuming they are an average person.**

The person's data shows *what happened* — where you went, who you messaged,
what you opened. Narrative identity covers what the data cannot show: the
**who/what/when/where/why of a life across its whole arc** — history (the
chapters, the formative events, the losses), present (worldview, cares,
temperament, what they're up against), and future (goals lined up, the person
they mean to become, the person they fear becoming). It records both what is
true of the person and what they believe and value.

## Why it exists

When an AI has not been told something about a person, it fills the gap with
population averages: agreeable, agnostic, slightly therapeutic. The person's
data tells the AI what *happened*; narrative identity tells it **who it is
talking to**. It is the most important document in the product, because every
conversation, every day page and every suggestion depends on it.

## One document, and how rules relate to it

There is one document and no abridged version:

| artifact | what | reader | size |
|---|---|---|---|
| **the document** | the full prose — who they are, the arc | the person AND the AI, every message | a real wiki article page: editor, history, marginalia; injected whole (paragraph-boundary ceiling ~2k tokens) |

There is no distilled "core" or "capsule" beside it. Two versions of one
identity drift apart, and a machine abridger invents standing directives ("be
direct, don't go easy") that silently steer every chat. The assistant receives
exactly the bytes the person edits: `chat.rs::build_narrative_identity` reads
the article prose (`wiki_articles`, subject `narrative_identity`) directly,
cut at a paragraph boundary past ~2k tokens. The interview also writes
structured data to `wiki_chapters`: the person's own division of their life
into chapters, with a seeded article page per chapter.

**Rules are NOT part of narrative identity.** They are a separate channel. The
NI describes *who the person is*: it is their prose, and the model weighs it as
context. Rules (`wiki_rules`, avoid/defend) are *standing instructions to the
model*, and the model follows them without exception. "I left my old job last
year" belongs in the document (context the model weighs); "never raise my old
job unless I do" is a rule (an instruction the model obeys). The two sit next
to each other in the prompt, but a rule is an instruction and the NI is a
description, and mixing them makes the NI look like a policy file.


## What belongs in it

Everything below — **when the person offered it**:

- **History**: the chapters and their changepoints; formative events; losses
  and grief; what shaped them. Trauma belongs here when they put it here —
  named plainly, in their words, never excavated.
- **Present**: worldview and religion; what they care about; the principles
  they actually live by; temperament, traits, virtues and vices; the
  strongest pull (money/power/pleasure/fame); addictions and what they are
  up against — both what they have overcome and what they haven't yet.
- **Future**: goals lined up; the three-year and ten-year wants; the feared
  future — the version of themselves the AI should help them notice they are
  drifting toward.
- **Bonds**: the relationships that make up who they are, and how. A
  person's mother, business partner, or friend of twenty years is part of
  their identity, not just background to their life. One line per person,
  written by the person and linked to the entity id. The resolution queue's
  most useful question feeds this section ("4,000 messages with this person
  since 2019 — who are they to you?"). This differs from the recent-people
  list in circumstances: that list shows who has been *around* lately; bonds
  say who *matters* and *in what way*.
- **Self-frameworks**: MBTI, Big Five, Enneagram, attachment styles — any
  vocabulary the person uses about themselves. These are welcome and
  valuable because models already understand them: "ISTJ, low openness, high
  patience for paperwork" carries a lot of information in six words.
  **Self-reported only.** The machine never administers, infers, or assigns a
  type. A framework in the NI is the person's own statement about themselves.
- **Voice**: how they speak. This is captured by keeping their words instead of
  describing their style: the drafter keeps their wording, so their diction,
  cadence, and the names they call things carry into the document, and the AI
  picks up their way of speaking from that text.

## Provenance: two rules

1. **User-authored, never inferred.** Values, wounds, telos, and type cannot
   be derived from behavior. A machine guessing them from message volume
   would be wrong and would offend the person, so there is no generator that
   drafts the document from observed data. Significance comes from the user,
   the graph is computed deterministically, and the person writes the NI.
2. **The machine writes it only while it is empty.** The drafter arranges
   the person's own interview words into the first document. After that, the
   person edits it and the machine never overwrites it. The system adds to it
   only by *asking* questions (the resolution queue), never by writing to it.

In short: **the machine may propose; only the person approves.** The machine
may propose anything — a chapter draft, a candidate wording, a question like
"is it fair to say…?" — but nothing enters an identity surface until the person
explicitly accepts it. The machine may collect data and suggest
interpretations. It must not confirm them: the person's *edit* is the
confirmation. It must not set the person's goals or values at all. This is not
only a safety policy: if someone else writes a person's identity, it is no
longer the person's own account. Two rules follow, and both matter:

- **A chapter never enters the life story unedited.** Moving a chapter into
  the life story is the person's approval of it, so the person must edit it
  first. That edit is what makes the words theirs.
- **Machine drafts describe events neutrally.** A draft titled "The Decline"
  already passes judgment in a block that is supposed to be provisional.
  Drafts describe what happened, in words the person has already used;
  evaluative names are offered only as questions.

## How the AI uses it

The NI is **background knowledge for the AI, not a topic of conversation**. The
AI reads it before every exchange and lets it shape everything — tone, style,
what to suggest, what never to suggest, which feared future to steer away from
— while almost never mentioning it:

- Never recite it back ("as an ISTJ, you…", "given your old job…"). The person
  should *feel* understood without being shown the file.
- Never use it to explain them to themselves ("you do this because…"). The
  rule against psychologizing applies when the AI uses the document, as well
  as when the document is written.
- It sets defaults without comment: what "a good suggestion" means for THIS
  person, which vices not to feed, which tone works for them.
- The rules are the exception: the model follows them without exception
  instead of weighing them.
- **Do not side with the document by default.** An AI that reads a person's
  self-written identity before every conversation can end up constantly
  confirming that person's view of themselves, and sycophantic agreement
  measurably reduces people's willingness to repair conflicts (Cheng et al.,
  *Science* 2025). Knowing someone's story does not mean the AI should take
  their side. Attribute instead of asserting ("you've written that…"), and
  treat the document as *the person's own account as of a date*, not as
  established fact about them.

## How it comes to exist, and how it grows

- **Created in the interview**: Setup's Interview step asks one question per
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
  sounds like the machine describing them back to themselves, which is the
  opposite of what the document is for. The assistant's prompt says so: "I"
  in the document is the person, never the model.
- **Never finished**: an account of a life can't be complete, and telling the
  person so keeps perfectionism from stopping the first draft.
- **Grows through questions with evidence**: the resolution queue asks one
  thing at a time, later, with evidence attached ("4,000 messages with this
  person since 2019 — who are they to you?"). People recognize things more
  easily than they recall them. A question without evidence feels like
  homework; a question with evidence is easy to answer.
- **Corrected by editing**: the document is a page; the person rewrites it
  whenever they like, and the next message carries the edit — there is no
  derived copy to lag behind it.

## Its place in the system prompt

The chat prompt is an ordered registry of named blocks
(`api/chat.rs::build_system_prompt_blocks`, rendered by
`agent/prompt_blocks.rs::assemble`). Each block renders a different part of
the same person's data as text. Stable blocks come first for prompt caching,
and the one block of instructions the model must obey (`rules`) comes last so
it is the most recent text. What runs, in order:

| # | tag | carries | author | cadence |
|---|---|---|---|---|
| 1 | `base` | one fused string (`agent/prompt.rs::build_personalized_prompt`): the character and house style, the owner's `<style_notes>`, `<narrative_identity>` (the document, whole), `<tool_usage>` and the `<mode>` guidance | us + the person | slow |
| 2 | `precedence` | the precedence ladder, as text | us | static |
| 3 | `memory` | what the machine has learned, in three lanes | machine; the person can edit | session |
| 4 | `circumstances` | the computed present, as of when the chat first read it | SQL only, no LLM | fixed per chat; read again hourly or when the day turns |
| 5 | `coverage` | which data exists, per table, as a date range — so "there is no data for this" differs from "nothing happened" | SQL only | fixed per chat like `circumstances` |
| 6 | `active_project` | the Project (room) the chat lives in | the UI | session |
| 7 | `skill` | the running skill's file body, if any | us | per turn |
| 8 | `active_context` | the open page's live content | the UI | per turn |
| 9 | `rules` | the person's absolute imperatives | the person | rarely |

`<current_chapter>` — a machine-drafted, person-edited narration of the open
period between the life story and today — is **not built**; the last narrated
days in `<circumstances>` stand in for it.

Prose vocabulary: "your life story" (never "portrait": someone else paints a
portrait of a sitter, which contradicts the rule that the person writes this
document; the word also names a deleted feature). The person's surface is
named in their voice, "In your own words". The machine's is called "Memory", a
section of Settings > Assistant, because
settings headings are plain (`Section.svelte`); ownership is marked per note
instead, where a note the person has edited reads "· yours".

**Precedence**, stated once in the prompt (`prompt_blocks.rs::precedence_line`):
rules outrank everything and are absolute; narrative identity outranks the
machine's own memory; both outrank house guidance; circumstances are
situational fact, not instruction. The other blocks describe; only rules give
instructions, and the rules block goes last. The registry's `rung` values carry the
same ranking and must stay in sync with the line.

### Memory

Three lanes — **facts** (their world: the dog's name), **manner** (concise,
numbered lists), **practices** (what they're holding to) — as per-note rows
(`api/assistant_memories.rs`) with add/revise/retire, per-lane caps (12
facts, 8 each of the others; a full lane refuses new notes until one is
retired), and a section in Settings the person can read and edit. A note
the person has edited is marked `[theirs]` in the prompt and "· yours" on the
page, and the memory tool never revises or retires it. The product has already
deleted one machine-written channel about the person that the person could not
see; do not build another. To decide whether something goes here or in the NI:
if a stranger who spent 200 hours beside you could learn it, it goes in memory.
If getting it wrong would offend the person, not just inconvenience them, it
may only come from the person.

### Circumstances

The block is called "circumstances", not "prudential context": prudence is a
quality of the person acting, and this block is the information prudence
works from. The name comes from *circumstantiae* (ST I-II q.7), the facts
surrounding an act, and Cicero's list — who, what, where, when, how, why —
matches the block's fields closely:

Its sections are one registry (`api/circumstances.rs::SECTIONS`), in order:
the as-of time (floored to the quarter-hour, computed once so every line
derives from the same instant), identity, the chapters as they named them (names and
years only), place, today's spine, the calendar, recent people, live threads,
last night's sleep, recently narrated days, and connected sources.

The block is fixed per chat (`prompt_blocks::held_per_chat`) and the current
time is not in the prompt at all: each of their messages opens with when it
was sent (`compaction::build_context_for_llm`). Grok reads from cache only
where an earlier request ended, so one changed byte in the system message
costs the prompt and the whole history behind it; with the clock and the
ingest-driven lines inside it, 57% of turns on the main box started with no
cache hit (measured 2026-10-09).

- **Recent people** carry entity ids and are labelled *recency, not
  significance*: the list shows who has been around, not who matters (bonds,
  in the NI, record who matters).
- **Observances** — recurring dates the person keeps: fasts, sabbaths,
  anniversaries — are not built. Without them the model knows the date but not
  that it is Lent or a death anniversary. When they are built, they come from
  user-authored sources only.

Each section has a fixed line cap and a deterministic query. A section with no
data renders nothing, and a section whose query fails is logged and omitted,
never filled with a default. The connection to prudence is stated here and not
in the prompt: the system keeps the person's memory and circumstances
available for the person's own decisions.

### Rules

Keep rules few — aim for 3 to 10; nothing in code caps them yet. A rule works
by removing options from consideration, not by being weighed against them.
With many rules, some conflict, the model has to weigh the conflicting ones
against each other, and a rule that gets weighed behaves like a preference.
Guidelines:

- **Only imperatives.** One aspiration ("I'm trying to read more") in the
  block makes the model treat every rule more like advice. Aspirations go in
  the NI or memory.
- **Changing a rule takes a deliberate step**: the person restates it and
  takes an explicit action; there is no settings toggle. If a rule can be
  flipped casually mid-conversation, it behaves like a preference. A rule is a
  commitment the person makes in advance so that it still applies when their
  mood changes later.
- **Affirmative rules cost more than prohibitions.** "Never raise X" applies
  at all times; "help me hold Y" also always applies, *but not at every
  moment*, so each affirmative rule requires the model to judge when to act on
  it. Keep affirmative rules fewer than prohibitions.
- **Last in the prompt** for recency, and outside the cached prefix, so they
  are re-sent every turn. Not built: a compressed digest re-injected near the
  live turn in long conversations — at 60k tokens of transcript the end of
  the prompt sits in the middle of the context, where models attend least.

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
documented attack class, and recording *who wrote this, from what evidence,
when* is the standard defense.

## What it is not

- Not a psychological evaluation, and never a place the machine records its
  own opinions of the person.
- Not a memory store or fact database (those are the database and the graph).
- Not a summary of the person's data. It contains what the data cannot show.
- Not private notes *about* the user by the system: the person can read
  every word, because every word is theirs.
