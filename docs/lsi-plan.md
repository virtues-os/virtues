# The Sitting — a Life Story Interview for onboarding

*Plan, 2026-08-26. Replaces the textarea interview (and the one-evening
chapter-cards experiment) with a conducted interview. Status: DIRECTION
CONFIRMED by Adam 2026-08-26 ("the most intellectually felt and authentically
received we can do besides human interviews"); design below locked pending the
specimen review. The five questions and DraftReview survive; Interview.svelte's
form UI and ChapterCards.svelte are replaced by what's described here.*

## The stated goal (said to the user, in these terms)

The user must know exactly what they are doing and why, in one breath:

> **We are doing this to fill in your past, understand your present, and
> explore your future** — through the chapters of your life, the people who
> mattered, and the stories that made you.

And the honesty that makes it approachable:

> **This will never be finished, and it isn't supposed to be.** A record of a
> life can't be complete — there will always be another story worth telling.
> What matters is an honest start; the rest arrives over years, a question at
> a time.

That second paragraph is load-bearing: it converts "I must now summarize my
entire life" (paralyzing) into "I must start honestly" (doable), and it
pre-announces the resolution queue — the box will keep asking, gently, for
years. Perfectionism is the silent killer of this step; we disarm it at the
door.

## Thesis

Onboarding's "In your own words" step asks for the most expensive thing in the
product — testimony about a life — with the cheapest instrument software has: a
blank textarea. McAdams' Life Story Interview (the research instrument the term
"narrative identity" comes from) shows why that fails: the power of the method
is not the questions, it is the **interview** — a listener who applies a fixed
probe to whatever the person offers ("you called those the Wisconsin years —
what ended them?"). A form cannot do that. A chat product can, but chat's
costume (bubbles, avatars, streaming shimmer) is the aesthetic of every cheap
AI product and would negate the gravitas this step needs.

So: take chat's mechanism, refuse its costume. **The person gives the
interview of their life and watches it become a manuscript as they speak.**
The form to steal from is the Paris Review interview — a typographic dialogue
on paper. The interviewer is the assistant they named two screens earlier.
That is onboarding's missing arc: meet her → she interviews you → she hands
back the draft ("In your own words", already built as DraftReview).

Say the frame to the user outright, in the door copy:
**"Most onboarding asks you to fill out a profile. This is closer to sitting
for a portrait."**

## What the person is feeling, and what we must produce

Arriving state: ten minutes in, transactions behind them (read a letter, gave
names, clicked Connect), now asked to *give*. Ambient feelings: wariness ("why
does software want my grief?"), blank-page anxiety, time worry. The feeling to
produce: **being taken seriously**. Safety is not a reassurance problem — it is
architecture (stays on the box, no other reader exists) + conduct (the
interviewer demonstrably follows rules) + reversibility (nothing binds until
confirmed, everything editable, skip honored without comment). All three are
*showable*; none requires a soothing tone.

## The door (one screen before the first question)

Trent Horn's move: answer the objection before it is raised, plainly. The door
buys the gravitas; a person who walks through stated terms writes differently
than one dropped into a form. Draft copy (registers: letter's serif, no
corporate we):

> **Sitting for a portrait**
>
> Most onboarding asks you to fill out a profile. This is closer to sitting
> for a portrait: {assistant} will ask about the life your record can't reach —
> the years before any device, and the things no device can see.
>
> The terms: this never leaves your box, and there is no one else who can
> read it. She will never interpret you or press on anything you didn't
> offer. Anything can be skipped without a mark. And none of her words enter
> the record — only yours.
>
> About twenty minutes. It saves as you go; you can stop anywhere. And it
> will never be finished — a record of a life can't be. What matters is an
> honest start.
>
> [Begin the sitting]

## The lifeline (the door's visual argument)

Words claim; a picture proves. The door shows a **lifeline** — one horizontal
axis from birth to today (and a breath beyond) — that makes "filling in your
past" visible as a matter of **resolution**:

- **The device era** (from the census's real oldest date — e.g. December
  2017): a dense band of fine ticks. Deterministic record: days, hours,
  minutes. This is what the box already holds, and the density is drawn from
  the person's real counts, not a stock illustration.
- **Before the record**: near-void. This is the argument for the sitting made
  visually — most of the axis is dark.
- **What writing does**: chapter bands light the void at *thematic*
  resolution — translucent arcs with serif titles ("The Wisconsin years") —
  and salient stories land as point-stars inside them at *episodic*
  resolution. The gradient the user should perceive: **thematic → episodic →
  deterministic** as time approaches the present.
- **The future edge**: past "today," a faint dotted continuation — the
  ambitions/shadow-future questions (queued) will draw there. "Explore your
  future" gets its place on the same axis rather than a separate metaphor.

Two rows, same axis, is the strongest form (Tufte small-multiple contrast):
**"your record now"** (dense right edge, void left) above **"after the
sitting"** (chapters lit, stars placed). The delta between the rows IS the
pitch for the next twenty minutes.

The lifeline is born here but does not die here: it is the seed of a
permanent Lifeline surface (the wiki timeline of eras/chapters), where the
resolution keeps rising for years as the queue asks its questions. The door
introduces the object; the product later inhabits it.

## Choreography

**Five movements**, McAdams' spine with the Virtues flavor kept (the novelty
question stays — it is the highest-information thing a person can write for an
AI, and no LSI section covers it):

| # | movement | question (verbatim from questions.ts) | LSI ancestry |
|---|---|---|---|
| I | chapters | What were the chapters of your life? | §1 Life chapters |
| II | novelty | What makes you different from most people you have met? | ours (calibration) |
| III | admire | Which well-known figures do you admire, and what specifically about them? | values-as-people |
| IV | vices | Which pull is strongest for you — money, power, pleasure, or fame? | Brooks' four idols = Aquinas' four substitutes |
| V | belief | What is your religion, or your worldview? | §5 Personal ideology |

Rising exposure preserved. Each movement:

1. She asks the **authored question, verbatim** — questions are never
   generated — plus one authored line of why (compressed from the `why` texts).
2. They answer, at whatever length. She never interrupts.
3. She offers **at most one probe per answer, at most two per movement**,
   drawn only from the McAdams four — *what happened · when, and who was
   there · what were you thinking and feeling · what does that say about who
   you are* — or the neutral "say more about —", always quoting their own
   words back.
4. She asks, authored: **"Shall we go on?"** Advancement is consented to,
   never sprung. "Skip" moves on instantly and is never remarked on.
5. On close, the movement **files the person's turns verbatim** (joined,
   lightly whitespace-stitched) into the existing `wiki_narrative_interview`
   answer for that `question_id`. See "the artifact stays the document".

Stalling gets the authored hint ("rough years are fine"), never pressure.
"Why do you want to know?" — a quiet always-visible affordance per movement —
inserts the full authored disclosure as her turn. The guide explains herself;
no model involved.

**The return**: after V, the existing DraftReview runs unchanged — their words
arranged into the document + core, rules proposed and confirmed. The census
and reveal then pay everything off ("the oldest thing it found is from
December 2017" beside a document titled In your own words).

## The interviewer's vows (goes into the system prompt, near-verbatim)

- **Method (Lonergan):** be attentive, be intelligent, be reasonable, be
  responsible. Attend to what was said; understand before probing; never go
  beyond the evidence of their words; you are helping make a record someone
  must live with.
- **Register (Lewis):** dignity without flattery. No praise, no "that's so
  insightful." You have never met a mere mortal; do not condescend to one.
- **Discipline (McAdams):** you are a witness, not a judge. The four probes
  and "say more" are your entire vocabulary. Never name a feeling they did not
  name. Never open a door they did not open — grief gets a follow-up only if
  grief was offered. A decline is honored instantly and never acknowledged.
- **Demand (Peterson):** specificity. "The hard year" earns "which year?"
  Vague is comfortable and useless; the particular is what sorts a past.
- **Aim (Aristotle):** bend toward what the person is *for*, not what they
  have consumed. Telos over inventory.
- **Order (Aquinas):** one thing at a time, properly named. Precision is a
  form of reverence.
- **Close (Tolkien):** ordinary chapters are a story. Dignify the homely
  years; never dramatize them.
- **Distress:** if acute distress appears, do not probe it, do not interpret
  it, do not perform concern. One authored line — "We can leave this here;
  everything you've written is saved" — and follow their lead. You are not a
  therapist and must never simulate one.

## Architecture — deliberately almost no model

The safety and the cost model come from the same decision: **the interviewer
is authored theater with the model at exactly one joint.**

- **Questions, whys, hints, transitions, "shall we go on?", the door, the
  distress line: authored strings.** Zero latency, zero drift, work with no
  account balance.
- **The one model joint: probe selection/generation.** Input: the movement,
  its vows, the person's turns so far. Output contract (JSON): either
  `{"advance": true}` or `{"probe": "<one sentence, quoting their words>"}`.
  Chat slot, capped, one call per person-turn at most. Malformed output or any
  error → advance. This is the entire generative surface.
- **Filing is verbatim, not distillation.** The person's turns join into the
  answer field as-is; *arranging* is the drafter's job and already exists.
  This removes a whole class of their-words-got-rewritten failures and keeps
  the guarantee literal: only their words enter the record.
- **No streaming, anywhere.** Her lines are one sentence and mostly authored;
  text appears set, like type, with a quiet fade. Streaming shimmer is the
  single strongest "cheap AI product" signal and we simply don't emit it.
- **Degradation is graceful by construction:** model unreachable or wallet
  empty → probes silently stop, the interview continues authored-only —
  questions, hints, consented advancement. The sitting still works; it is
  merely a quieter interviewer. (The account gate already fronts this view,
  so the normal path has AI by construction.)

## Data model

- **Turns**: new table (claim the migration number when building), e.g.
  `wiki_interview_turns(id, movement, role interviewer|subject, content,
  created_at)`. Written as they happen (autosave doctrine: an hour of writing
  is at stake and the save is visible).
- **Turns are scaffolding, not record**: never read by chat, the drafter, or
  search. On DraftReview acceptance the transcript is **deleted** — "she
  keeps none of her own words; only yours enter the record" is enforced by a
  DELETE, not a promise. Until acceptance it persists so a refresh resumes
  mid-movement, exactly where they stopped, with her last line re-shown.
- **Answers**: unchanged — `wiki_narrative_interview` rows per `question_id`,
  filed at movement close, `completed_at` stamped. The drafter contract does
  not move.
- **Resume**: movements with filed answers show as settled (their prose
  visible, reopenable); an unfiled movement resumes its transcript. Backward
  navigation reopens a movement conversationally; corrections after the draft
  exist happen in the document (EDIT, NOT REDRAFT doctrine).

## Specimen v2 rulings (Adam, 2026-08-26, after seeing v1)

- **One continuous stream.** All questions flow in a single conversation on a
  single page — no movement sub-screens, no per-question navigation. The
  movements survive only as internal sequencing (prompt structure, the small
  I–V numeral). The next question arrives IN the flow ("Then — what makes
  you…"), never on a new screen.
- **Nameless interviewer, or their assistant's name — never an invented one.**
  v1's placeholder "Mira" was wrong. Current treatment: the interviewer's
  turns carry the ∴ mark as their speaker mark (nameless, branded, and the
  mark finally speaks); swap to the person's own `assistant_name` (already
  collected in Introductions, default exists) if named feels better. Their
  own turns are unmarked body prose — the page is mostly them.
- **Chrome dies.** v1 showed four affordances at once (Go on / skip this /
  why do you ask? / stop here) — inundating. v2: the writing surface, one set
  mark (—), one tiny "skip". Consent-to-advance is conversational (her
  question IS the invitation), "why?" is a superscript on the question line
  itself, and "saves as you go" is said once at the door, not carried as a
  footer.
- The door is its own screen; Begin replaces it with the stream. (v1 showed
  both stacked, which read as two competing CTAs.)

## The founder video (open, leaning yes)

The sitting asks for maximum trust, and a human face asking for it is the one
trust instrument copy can't match — prosody and sincerity don't survive
transcription. But two videos in one onboarding dilute both, and a mediocre
video negates gravitas exactly the way cheap UI does. Options:

1. **One-video strategy (lean):** the founder's-letter film carries a final
   beat that hands off to the sitting — "later it will ask about your life;
   answer it honestly, I did" — and the door stays text.
2. **A dedicated door video:** ≤60 seconds, personal testimony not
   instructions — why these questions exist, that he sat for it himself, the
   never-finished line spoken aloud. Rules if so: never autoplay; a quiet
   poster-frame affordance beside the lede; the door must work without it
   (offline boxes, skippers); bundled small or fetched with graceful absence.

Either way the register is testimony ("I did this") not tutorial ("you
should"). Decide when the letter film gets made — they are one shoot.

## UI register (the sitting, typographically)

- **Paris Review page**: a single column in the onboarding document register.
  Her lines: small, quiet, apparatus-voiced (mono or small-caps label with her
  name, like a printed interview's "INTERVIEWER:"). Their prose: full serif
  body — visually, the page is mostly *them*, which is the truth of it.
- **No bubbles, avatars, timestamps, or read receipts.** The transcript grows
  downward like a manuscript; gentle scroll; reduced-motion collapses even
  that.
- **Input**: a generous writing surface pinned below (grows with content;
  Enter is a newline, a set button — "—" or "Continue" — sends; never a paper
  plane icon). It must feel like writing, not messaging.
- **Progress**: Roman numerals, I–V, small, top right. The word-count meter
  dies with the form; the manuscript's own length is the meter.
- **Affordances always visible, always quiet**: "skip" · "why do you ask?" ·
  "stop here" (with "saved as you go" beside it).
- **One motion budget rule holds**: the reveal keeps onboarding's expensive
  motion; the sitting's only motion is type appearing and the page breathing.

## Phases

1. **Phase 1 — the sitting, authored-only.** Door, five movements, turns
   table, verbatim filing, resume, DraftReview unchanged. Fully shippable; no
   model calls at all. (This is also the permanent degradation mode.)
2. **Phase 2 — the probe joint.** The one model call, vows in the system
   prompt, JSON contract, caps, error→advance. Ship 1+2 together if possible;
   1 alone is already better than the form.
3. **Phase 3 — voice.** `mode: 'speak'` finally honored: the LSI is an oral
   instrument, people narrate scenes aloud with more specificity than they
   type, and the box transcribes locally — telling your life to a machine in
   your house that sends it nowhere *is* the product thesis demonstrated.
   Requires the mic path on desktop; design later.
4. **Later — the resolution queue moves in.** The nine held-back questions
   (high point, low point, people, loss, pride, now, ambitions, shadow
   future, rules) are asked in this same room, weeks later, grounded in
   evidence ("you've had dinner with Sam eleven times — who is he to you?").
   The sitting is the front door of that entire future surface; build it as
   the room, not a wizard.

## Open questions (for Adam, before build)

1. **Her warmth dial**: the vows forbid flattery and therapy-voice; how warm
   is she allowed to be at the seams ("thank you — shall we go on?"), if at
   all? Recommend: warmth lives in patience and precision, not in adjectives.
2. **Transcript deletion timing**: at draft acceptance (recommended, above) or
   kept until the person deletes it? The guarantee reads strongest as an
   automatic DELETE.
3. **Chapters inside the sitting**: movement I could still end with her
   playing the answer back as a titled list for confirmation ("So: the
   Wisconsin years, till 2015…") — the cards' one good idea, returned as
   conversation instead of form. Include?
4. **Door skip**: is the sitting skippable wholesale from the door ("not
   now — the questions will wait for you"), matching the words doorway's
   current softness? Recommend yes, same copy register.

## What dies, what survives

- Dies: the per-question form UI in Interview.svelte; ChapterCards.svelte
  (its learnings — single-barreled prompt, experiential purposes, the
  four-to-eight norm — survive in questions.ts and in movement I's hint).
  ChapterCards stays in the tree only until Phase 1 replaces the view.
- Survives untouched: questions.ts (the corpus, whys, staging), DraftReview,
  the drafter and its ONE-WRITER-ONCE article contract, the rules
  confirmation flow, the reveal.
