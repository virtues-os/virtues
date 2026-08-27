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

## Its place in the system prompt — the three speeds

The whole personal half of the system prompt is three blocks moving at three
speeds, and NI is the slowest:

| block | answers | changes | source |
|---|---|---|---|
| **narrative identity** | who am I talking to — the arc, past AND future (telos, feared self) | months–years | authored by the person |
| **rules** | what must I never/always do | rarely | confirmed by the person |
| **prudential context** | what is the situation right now | hourly–daily | computed from the record, no LLM |

So the formula is **NI + rules + prudential context** — and no separate
"future" block: the *aspirational* future (who they mean to become) is
inside NI, while the *operational* future (tomorrow's flight) is prudential.
Prudence is the right name: the virtue of right action in the particular
situation, which is exactly what this block supplies.

**Prudential context** (today a thin `build_user_context`: occupation/home,
last three day-proses, connected sources, timezone) should grow toward a
budgeted (~600–800 token), deterministic, SQL-only block computed fresh at
conversation start:

- the clock: local date, time of day, day of week
- the place: current or last visit; home
- the day so far: today's deterministic spine (visits + calendar + sleep —
  the zero-LLM today view that already exists)
- the calendar: today's and tomorrow's events
- recent people: the last ~2 weeks' correspondents WITH entity ids (so tool
  calls can join) — labeled as *recency, not significance*: this is "who is
  around lately," never a ranking of who matters
- live threads: recently edited pages, active notebooks, last night's sleep

Same discipline as the core: hard budget, deterministic queries, and
absence stays silent — a prudential block that lists what it doesn't know
is noise.

## What it is NOT

- Not a psychological evaluation, and never a place the machine records its
  own opinions of the person.
- Not a memory store or fact database (that's the record and the graph).
- Not a summary of the record — it is exactly the part the record can't say.
- Not private notes *about* the user by the system: the person can read
  every word, because every word is theirs.
