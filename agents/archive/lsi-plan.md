# Life Story Interview for onboarding ("the sitting")

> **FINAL FORM (2026-08-27, built):** the interview left onboarding entirely.
> Onboarding is letter → introductions → sources → reveal, and the reveal's
> door says what happens next. The interview is the product's FIRST
> CONVERSATION: one seeded chat ("In your own words",
> `chat_narrative_interview`), opened in the real ChatView, where the server
> forces interview mode by chat id (standalone prompt, zero tools) and the
> authored opening greets them. "Write it up" in that chat runs the drafter
> over the whole transcript and opens the document beside it. All the bespoke
> UI was removed: the words step, both doors, InterviewChat, DraftReview (its
> rules-consent flow is an OPEN item to restore on the NI page or in the
> draft flow). The sections below are design history. The territories, the
> interviewer rules, the door copy, and the reciprocity assets (founder video +
> his own NI excerpt) all survive in the prompt and as planned additions to
> the chat's empty state.

*Plan, 2026-08-26. Replaces the textarea interview (and the one-evening
chapter-cards experiment) with a conducted interview. Status: DIRECTION
CONFIRMED by Adam 2026-08-26 ("the most intellectually felt and authentically
received we can do besides human interviews"); design below locked pending the
specimen review. The five questions and DraftReview survive; Interview.svelte's
form UI and ChapterCards.svelte are replaced by what's described here.*

## The goal as stated to the user

The user should understand what they are doing and why in one sentence:

> **We are doing this to fill in your past, understand your present, and
> explore your future** — through the chapters of your life, the people who
> mattered, and the stories that made you.

A second statement makes the task approachable:

> **This will never be finished, and it isn't supposed to be.** A record of a
> life can't be complete — there will always be another story worth telling.
> What matters is an honest start; the rest arrives over years, a question at
> a time.

That second paragraph matters. It changes the task from "I must now summarize
my entire life" (paralyzing) to "I only need to start" (doable), and it
announces the resolution queue in advance: the box will keep asking, gently,
for years. Perfectionism is the most likely reason people abandon this step,
and this paragraph addresses it up front.

## Problem and approach

Onboarding's "In your own words" step asks for the most expensive thing in the
product, a person's account of their life, using the cheapest input software
has: a blank textarea. McAdams' Life Story Interview (the research instrument
the term "narrative identity" comes from) shows why that fails. The method's
power comes from the **interview** more than the questions: a listener applies
a fixed probe to whatever the person offers ("you called those the Dunmore
years — what ended them?"). A form cannot do that. A chat product can, but the
standard chat UI (bubbles, avatars, streaming shimmer) looks like every cheap
AI product and would undercut the seriousness this step needs.

So we use the chat mechanism without the chat UI. **The person is interviewed
about their life and sees their answers laid out as a manuscript as they
write.** The visual model is the Paris Review interview, a typographic dialogue
on paper. The interviewer is the assistant they named two screens earlier.
That gives onboarding the sequence it was missing: meet the assistant → the
assistant interviews you → the assistant hands back the draft ("In your own
words", already built as DraftReview).

The door copy states the framing to the user directly:
**"Most onboarding asks you to fill out a profile. This is closer to sitting
for a portrait."**

## What the person is feeling, and what we need them to feel

Arriving state: ten minutes in, transactions behind them (read a letter, gave
names, clicked Connect), now asked to *give*. Likely feelings: wariness ("why
does software want my grief?"), blank-page anxiety, worry about time. The
feeling we want: **being taken seriously**. Reassuring copy does not solve
safety. Three concrete properties do: architecture (it stays on the box, and
no other reader exists), conduct (the interviewer visibly follows rules), and
reversibility (nothing is final until confirmed, everything is editable, and
skipping gets no comment). All three can be shown to the user, so the copy
does not need a soothing tone.

## The intro screen ("the door"), shown before the first question

Following Trent Horn: answer the objection plainly before the person raises
it. The intro screen sets the tone; a person who starts after reading stated
terms writes differently from one dropped into a form. Draft copy (style: the
letter's serif, no corporate "we"):

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

## The lifeline graphic on the intro screen

A picture makes the case better than copy can. The intro screen shows a
**lifeline**: one horizontal axis from birth to today (and a little beyond)
that shows "filling in your past" as a matter of **resolution**:

- **The device era** (from the census's real oldest date — e.g. March
  2019): a dense band of fine ticks. Deterministic data: days, hours,
  minutes. This is what the box already stores, and the density is drawn from
  the person's real counts instead of a stock illustration.
- **Before the device era**: nearly empty. This shows visually why the
  interview is needed: most of the axis is dark.
- **What writing adds**: chapter bands fill the empty part at *thematic*
  resolution (translucent arcs with serif titles such as "The Dunmore years"),
  and important stories appear as point-stars inside them at *episodic*
  resolution. The gradient the user should see is **thematic → episodic →
  deterministic** as time approaches the present.
- **The future edge**: past "today," a faint dotted continuation where the
  ambitions/shadow-future questions (queued) will draw. "Explore your future"
  gets a place on the same axis instead of a separate metaphor.

The strongest layout is two rows on the same axis (Tufte small-multiple
contrast): **"your record now"** (dense right edge, empty left) above
**"after the sitting"** (chapters filled in, stars placed). The difference
between the rows is the reason to spend the next twenty minutes.

The lifeline is not only for this screen. It is the starting point for a
permanent Lifeline surface (the wiki timeline of eras/chapters), where the
resolution keeps rising for years as the queue asks its questions. The intro
screen introduces the object, and the product later builds on it.

## Interview structure

**Five movements**, following McAdams' structure with the Virtues-specific
question kept (the novelty question stays: it is the highest-information
thing a person can write for an AI, and no LSI section covers it):

| # | movement | question (verbatim from questions.ts) | LSI ancestry |
|---|---|---|---|
| I | chapters | What were the chapters of your life? | §1 Life chapters |
| II | novelty | What makes you different from most people you have met? | ours (calibration) |
| III | admire | Which well-known figures do you admire, and what specifically about them? | values-as-people |
| IV | vices | Which pull is strongest for you — money, power, pleasure, or fame? | Brooks' four idols = Aquinas' four substitutes |
| V | belief | What is your religion, or your worldview? | §5 Personal ideology |

The questions keep their order of increasing exposure. Each movement:

1. She asks the **authored question, verbatim** (questions are never
   generated), plus one authored line explaining why (compressed from the
   `why` texts).
2. They answer, at whatever length. She never interrupts.
3. She offers **at most one probe per answer, at most two per movement**,
   drawn only from the McAdams four — *what happened · when, and who was
   there · what were you thinking and feeling · what does that say about who
   you are* — or the neutral "say more about —", always quoting their own
   words back.
4. She asks, authored: **"Shall we go on?"** The person agrees before the
   interview advances; it never advances on its own. "Skip" moves on
   instantly and is never remarked on.
5. On close, the movement **files the person's turns verbatim** (joined,
   lightly whitespace-stitched) into the existing `wiki_narrative_interview`
   answer for that `question_id`. See "the artifact stays the document".

If the person stalls, they get the authored hint ("rough years are fine"),
never pressure. "Why do you want to know?" is a quiet, always-visible link per
movement that inserts the full authored explanation as her turn. The
explanation is authored text, so no model is involved.

**After the interview**: after V, the existing DraftReview runs unchanged:
their words are arranged into the document + core, and rules are proposed and
confirmed. The census and reveal then follow up on everything ("the oldest
thing it found is from March 2019" beside a document titled In your own
words).

## Interviewer rules (go into the system prompt, near-verbatim)

- **Method (Lonergan):** be attentive, be intelligent, be reasonable, be
  responsible. Pay attention to what was said; understand it before probing;
  never go beyond what their words support; remember the person has to live
  with what gets written down.
- **Tone (Lewis):** respectful, without flattery. No praise, no "that's so
  insightful." Do not condescend.
- **Discipline (McAdams):** listen and record; do not judge. The four probes
  and "say more" are the only questions you may ask. Never name a feeling they
  did not name. Never raise a topic they did not raise: grief gets a
  follow-up only if they mentioned grief. If they decline to answer, move on
  immediately and do not comment on it.
- **Specificity (Peterson):** ask for specifics. "The hard year" gets "which
  year?" Vague answers are comfortable but not useful; specific details are
  what let us place events in the past.
- **Aim (Aristotle):** steer toward what the person cares about and is working
  toward, more than what they have consumed.
- **Order (Aquinas):** one thing at a time, named precisely.
- **Ordinary years (Tolkien):** ordinary chapters count as a story. Treat the
  quiet years with respect; never dramatize them.
- **Distress:** if acute distress appears, do not probe it, do not interpret
  it, do not perform concern. Say one authored line — "We can leave this here;
  everything you've written is saved" — and follow their lead. You are not a
  therapist and must never act like one.

## Architecture: one model call, everything else authored

The safety model and the cost model come from the same decision: **the
interviewer is authored text, with the model used at exactly one point.**

- **Questions, whys, hints, transitions, "shall we go on?", the door, the
  distress line: authored strings.** Zero latency, zero drift, and they work
  with no account balance.
- **The one model call: probe selection/generation.** Input: the movement,
  its rules, the person's turns so far. Output contract (JSON): either
  `{"advance": true}` or `{"probe": "<one sentence, quoting their words>"}`.
  Chat slot, capped, at most one call per person-turn. Malformed output or any
  error → advance. This is the only generated text.
- **Filing is verbatim; there is no distillation step.** The person's turns
  join into the answer field as-is; *arranging* them is the drafter's job, and
  the drafter already exists. This removes the whole class of failures where
  their words get rewritten, and it keeps the guarantee literal: only their
  words are saved.
- **No streaming, anywhere.** Her lines are one sentence and mostly authored;
  text appears all at once, like set type, with a quiet fade. Streaming
  shimmer is the strongest "cheap AI product" signal, and we don't emit it.
- **Degradation is graceful by design:** if the model is unreachable or the
  wallet is empty, probes silently stop and the interview continues with
  authored text only (questions, hints, agreed advancement). The interview
  still works with a quieter interviewer. (The account gate already sits in
  front of this view, so on the normal path AI is available.)

## Data model

- **Turns**: new table (claim the migration number when building), e.g.
  `wiki_interview_turns(id, movement, role interviewer|subject, content,
  created_at)`. Written as they happen (autosave rule: an hour of writing is
  at stake, and the save is visible).
- **Turns are temporary**: never read by chat, the drafter, or search. On
  DraftReview acceptance the transcript is **deleted**. The promise "she keeps
  none of her own words; only yours are saved" is enforced by a DELETE. Until
  acceptance the transcript persists, so a refresh resumes mid-movement
  exactly where they stopped, with her last line shown again.
- **Answers**: unchanged — `wiki_narrative_interview` rows per `question_id`,
  filed at movement close, `completed_at` stamped. The drafter contract does
  not change.
- **Resume**: movements with filed answers show as finished (their prose
  visible, reopenable); an unfiled movement resumes its transcript. Going back
  reopens a movement as conversation; corrections after the draft exists
  happen in the document (EDIT, NOT REDRAFT rule).

## Decisions after the v1 specimen (Adam, 2026-08-26)

- **One continuous stream.** All questions flow in a single conversation on a
  single page, with no movement sub-screens and no per-question navigation.
  The movements remain only as internal sequencing (prompt structure, the
  small I–V numeral). The next question arrives in the flow ("Then — what
  makes you…"), never on a new screen.
- **The interviewer is nameless or uses the user's assistant name; never an
  invented name.** v1's placeholder "Mira" was wrong. Current treatment: the
  interviewer's turns carry the ∴ mark as their speaker mark (nameless and
  branded, and it gives the mark a use); switch to the person's own
  `assistant_name` (already collected in Introductions, default exists) if a
  name feels better. The person's own turns are unmarked body prose, so most
  of the page is their writing.
- **Remove the extra controls.** v1 showed four controls at once (Go on /
  skip this / why do you ask? / stop here), which was overwhelming. v2: the
  writing surface, one send mark (—), one small "skip". Agreeing to advance
  happens in the conversation (her question is the invitation), "why?" is a
  superscript on the question line itself, and "saves as you go" is said once
  at the door instead of as a persistent footer.
- The door is its own screen; Begin replaces it with the stream. (v1 showed
  both stacked, which looked like two competing CTAs.)

## Founder video (open question, leaning yes)

The interview asks for a lot of trust, and a human face asking for it does
something copy can't: tone of voice and sincerity don't survive transcription.
But two videos in one onboarding weaken both, and a mediocre video undercuts
the seriousness the same way cheap UI does. Options:

1. **One-video strategy (preferred):** the founder's-letter film ends with a
   segment that leads into the interview ("later it will ask about your life;
   answer it honestly, I did"), and the door stays text.
2. **A dedicated door video:** ≤60 seconds, a personal account rather than
   instructions: why these questions exist, that he did the interview
   himself, the never-finished line spoken aloud. Rules if so: never autoplay;
   a quiet poster-frame control beside the lede; the door must work without it
   (offline boxes, people who skip); bundled small or fetched, and absent
   without error if missing.

Either way the video should be a first-person account ("I did this") and not
a tutorial ("you should"). Decide when the letter film gets made, since both
are one shoot.

## UI and typography

- **Paris Review page**: a single column in the onboarding document style.
  Her lines: small, quiet, set as apparatus (mono or small-caps label with her
  name, like a printed interview's "INTERVIEWER:"). Their prose: full serif
  body, so visually most of the page is their writing, which matches what the
  page is for.
- **No bubbles, avatars, timestamps, or read receipts.** The transcript grows
  downward like a manuscript, with gentle scroll; reduced-motion removes even
  that.
- **Input**: a large writing surface pinned below (it grows with content;
  Enter is a newline, and a send button — "—" or "Continue" — sends; never a
  paper plane icon). It should feel like writing a document and not like
  sending messages.
- **Progress**: Roman numerals, I–V, small, top right. The word-count meter is
  removed with the form; the manuscript's own length shows progress.
- **Controls always visible and always quiet**: "skip" · "why do you ask?" ·
  "stop here" (with "saved as you go" beside it).
- **The motion budget rule still applies**: the reveal keeps onboarding's
  expensive motion; the interview's only motion is text appearing and the page
  scrolling.

## Phases

1. **Phase 1 — the interview, authored text only.** Door, five movements,
   turns table, verbatim filing, resume, DraftReview unchanged. Fully
   shippable; no model calls at all. (This is also the permanent degradation
   mode.)
2. **Phase 2 — the probe call.** The one model call, rules in the system
   prompt, JSON contract, caps, error→advance. Ship 1+2 together if possible;
   1 alone is already better than the form.
3. **Phase 3 — voice.** `mode: 'speak'` finally honored. The LSI is an oral
   instrument, people describe scenes aloud with more specificity than they
   type, and the box transcribes locally. Telling your life to a machine in
   your house that sends it nowhere demonstrates the product's core idea.
   Requires the mic path on desktop; design later.
4. **Later — the resolution queue moves in.** The nine held-back questions
   (high point, low point, people, loss, pride, now, ambitions, shadow
   future, rules) are asked in this same view, weeks later, grounded in
   evidence ("you've had dinner with Teodor eleven times — who is he to you?").
   The interview is the entry point to that whole future surface, so build it
   as a permanent view and not as a one-time wizard.

## Open questions (for Adam, before build)

1. **How warm she should be**: the rules forbid flattery and therapy-voice;
   how warm is she allowed to be at transitions ("thank you — shall we go
   on?"), if at all? Recommend: show warmth through patience and precision
   instead of adjectives.
2. **Transcript deletion timing**: at draft acceptance (recommended, above) or
   kept until the person deletes it? An automatic DELETE makes the strongest
   guarantee.
3. **Chapters inside the interview**: movement I could still end with her
   playing the answer back as a titled list for confirmation ("So: the
   Dunmore years, till 2011…"). That is the one good idea from the cards,
   done as conversation instead of a form. Include?
4. **Door skip**: can the whole interview be skipped from the door ("not
   now — the questions will wait for you"), matching how soft the words step
   currently is? Recommend yes, with copy in the same style.

## What is removed and what is kept

- Removed: the per-question form UI in Interview.svelte; ChapterCards.svelte
  (its learnings — single-barreled prompt, experiential purposes, the
  four-to-eight norm — survive in questions.ts and in movement I's hint).
  ChapterCards stays in the tree only until Phase 1 replaces the view.
- Kept unchanged: questions.ts (the corpus, whys, staging), DraftReview,
  the drafter and its ONE-WRITER-ONCE article contract, the rules
  confirmation flow, the reveal.
