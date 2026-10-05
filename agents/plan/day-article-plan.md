# Day article: the page written from the day itself

Status: **Partly built**, unreleased. Slices 1, 2 and 5 are on `wave`
(2549d156, 1c7535e3, 3757013e, f2e2fe26); 3 (backfill) and 4 (photo figure)
are open. Spiked end to end 2026-09-24 → 09-28. Supersedes the narration half
of `api/day_summary.rs`; segmentation (the timeline) stays.

## Why

The day article was written from the segmenter's event summaries: transcript →
5-minute summary → session content capped at 400 chars → a 1–3 sentence event
summary → the narrator. Four lossy hops, each told to be "factual", so each
removed what made the day matter. A day spent with someone in a long, hard,
good conversation came out as "desk, apps, messages, dog".

Significance is the model's judgment, not a rubric we compute. The work is the
shape of what it reads and the prompt it reads it under.

## What the spike established

- **Read the day, not summaries of it.** Full transcripts (silence dropped),
  messages grouped by thread, the owner's own words, the day's people as the
  wiki knows them, the owner's narrative identity as a lens, the pages for the
  days before, and an explicit list of hours with no recording.
- **Continuity finds the thread.** Narrating oldest first, with the previous
  days' *new* pages as context, is what surfaced "the day of her procedure" on
  a day whose audio never mentioned it. It is the single biggest quality lever.
- **Every sentence cites its evidence.** The writer tags each sentence
  (`[17:59]`, `[msg 19:40]`, `[gap]`); a light checker deletes sentences their
  evidence does not support. Tagging plus deletion made weak writers produce
  *short* pages rather than *wrong* ones.
- **Do not over-verify.** A strict checker plus repair plus "every conversation
  gets a clause" produced an accurate inventory, not a page. The light check
  stays; strictness does not.
- **Model.** Bake-off of 14 models, ZDR enforced: the Chat slot's
  `grok-4.7` writes the best pages for the price (~$0.06–0.15/day). Sonnet 5
  invented and ignored the discretion rule; Opus is best and ~3× the cost.
  Chat already defaults to grok-4.7, so the narrator needs no new slot.
- **Facts are computed, never written.** "First time at X" was false in the
  spike (X had come up 7 times before); the weather station recorded no rain on
  a day the conversation talked about a storm. Anything stated as a fact about
  the day comes from code.

## The page

One page, two views of the same day: **Article | Record** (a segmented switch,
not tabs). Toolbar: `Article | Record · Notes · Veil · Edit · ⋯`. History lives
in `⋯`.

Article, top to bottom:

1. Eyebrow: weekday. Title: the full date.
2. **Abstract** — one to three plain sentences: who, what, the thread.
3. **Fact strip** — one thin filled row: With · Weather · Recorded (8 of 24
   hours, with a micro coverage bar) · Wrote (chats, pages). Deterministic only;
   an absent fact is omitted, never "No data".
4. Body: up to three sections, headings of 3–6 words naming a thing or a moment
   (never a bare proper noun). Tables are allowed where the day holds a list.
5. At most one figure, only when earned (see Figures).
6. Previous / next day as two cards, each with its Abstract.
7. **Similar days.**

Marginalia (right column, soft filled cards) come in exactly two kinds:

| Kind | Form | Written by |
|---|---|---|
| Evidence — opens Record with the item highlighted | `Kind · time ↗` | the writer's tags; shown on hover for sentences, always for blocks (a figure, a table) |
| Context — plain facts, dates as links | "last came up Sep 19", "97° at 5 PM", section time spans | code, after the writer |

Captions and tables carry no links. `↗` has one meaning. `←` exists only in
Record, as the way back.

Notes are not marginalia: a note is an instruction queued for the AI editor,
opened from the toolbar as a dropdown ("1 waiting"). Resolved notes move to
History. (`NotesRail` already stores them.)

Record: dayline, timeline, and the full data grid; arriving from a citation
highlights that row and offers "← Back to the sentence".

## Markdown

The article stays one ordinary markdown page, editable in the Pages editor.

- Marginalia are GFM footnotes rendered as sidenotes (the Tufte/Gwern
  convention); inline under the paragraph on a phone. Labels carry the kind:
  `[^ev-3]` evidence, `[^cx-1]` context.
- Evidence footnotes hold a ref to the record item (message id, transcription
  id), so the renderer can open Record on it.
- A figure is a fenced block the page renders as a component:

  ````md
  ```figure
  kind: photo
  ref: data_communication_message/<id>
  caption: Figure 1. …
  ```
  ````

## Pipeline (`narrate_day`, v2)

1. **Assemble** (code): cleaned transcripts grouped into conversations;
   messages by thread with reactions folded and lurked-in group threads
   collapsed to a count; the owner's own words; the day's AI chats and page
   edits; the day's people with name, aliases and their wiki page excerpt;
   narrative identity; the previous three day pages; gaps (coverage, missing
   streams).
2. **Write** (Chat slot, reasoning low): the v5 prompt, tagged sentences,
   `Abstract:` first.
3. **Check** (Lite slot): each sentence against the evidence it cites plus the
   rest of that conversation; unsupported sentences are deleted. No repair
   pass. The Abstract is checked loosely against the body and never rewritten.
4. **Names** (code): every person's name resolves to a person record or
   appears in the day's record, else its sentence goes.
5. **Marginalia** (code): tags → evidence footnotes; section time spans,
   last-came-up / first-came-up for places and people, weather at the hour →
   context footnotes.
6. **Save**: the existing `save_day_article` guards (maintenance off, human
   edit, open in the editor) stay exactly as they are.

Order: the catch-up queue narrates **oldest first**, and a day re-narrates when
the day before it is rewritten, so continuity flows forward. A backfill of the
last three weeks runs once, oldest first, then the novelty index re-embeds all
of it together (the embedding detects the writer's voice; a mixed corpus would
light up every new day as novel).

## Figures

Purpose: show what prose cannot. At most one per day; none is the default.

- **Photo** — only with words attached (sent with a caption, replied to, talked
  about). Needs the Mac collector to upload message image attachments (images
  only, ≤10 MB, resized on the box; not-yet-downloaded files recorded and
  retried; deleting the message removes the image). iOS cannot read iMessage.
- Collections (questions asked, songs, purchases) are **tables in the prose**,
  written by the writer — not figures.
- Later, a cheap scout with `sql_query` (≤5 queries, query saved with the
  figure) may propose a comparison against the owner's own baseline. Never a
  causal claim.
- Tried and rejected on the page: a coverage chart as the figure, a sky ribbon,
  a month of dots (presence was every day; nothing to see).

### Tabled: a replay of the day's moment

The idea is right and the execution isn't there yet, so it waits. What it is: a
small illustrated replay of one salient moment (a dog sitting on command, a
bouquet that "exploded", a breakfast order from bed), anchored to the recording
or message it came from, with the source quoted beside it. Data charts become
the exception, roughly weekly, when a real pattern turns up.

What four spikes (2026-09-29) established:

- **Data figures as HTML are cheap and reliable, but read as data science.**
  Lite (`gpt-6-luna`) wrote five working figures at ~$0.002 each against Chat's
  ~$0.058, when the host sets `window.DATA` from read-only queries and the model
  writes only the view. Both models turned "a route" into distance-over-time
  unless the brief named the form.
- **A director beats an artist.** Lite reads the source lines and writes a scene
  script (stage, light, cast, props, two to five beats) in a fixed vocabulary for
  ~$0.0003; an engine acts it out. Scripts were thin (no leash on a dog walk) and
  once stated a joke as fact in the caption ("It's your birthday"), so captions
  need the sentence check.
- **Pixel art and vector puppetry are off-brand.** Tried: 8-bit sprites in two
  palettes, then shadow theater, Greek black-figure frieze, manuscript
  marginalia, an annotated scientific plate with chronophotographic ghosts, and
  a single self-drawing line.
- **Painted stills are close.** The Image slot model made usable watercolor,
  oil, charcoal and notebook-pen pictures of each moment at $0.067 and ~12 s,
  first try; the page can bring them to life for free (bloom, push, draw-on).
  They invent the surroundings (city, dog breed, clothes), so a real reference
  photo of recurring people and pets is the next step there.
- **Video: only Veo passes zero data retention.** Seedance and Kling have no ZDR
  provider on the gateway and were refused. Veo 3.1 Lite: 4 s at 720p, $0.12,
  ~40 s, medium kept intact; it declined one scene on moderation with no reason.

Open before resuming: a look that is unmistakably Virtues (none of the above
was), a cast that stays recognizable across days, and whether a replay is daily
or only when the day has a picturable moment.

## Frontend

`DayPage.svelte` today stacks seven equal sections. Rebuild:

- Header, Abstract, fact strip, article body (sidenote renderer over the
  existing `Markdown` component), figure block, prev/next, Similar days.
- `Article | Record` segmented control; Record holds `DaylineChart`,
  `EventTimeline` and the data grid that are on the page now. The event labels
  show only in Record (they are the weakest data on the page).
- Notes dropdown over `NotesRail`'s store.
- Transition between views along the day's clock (View Transitions: section
  time spans become timeline rows; the fact strip's coverage bar becomes the
  dayline). Crossfade where unsupported and under reduced motion.
- Veil (hold `V`): names from entity resolution plus spans the writer marks;
  hidden text is not in the DOM while veiled; particles per paragraph canvas.
  Day content only.

New endpoints: the fact strip's facts and Similar days (nearest day embeddings)
per date.

**Later:** a small novelty-vs-order (chaos/order) mark for the day, likely in
the fact strip.

## Slices

1. Backend: assembly, prompt, check, names, marginalia, oldest-first queue.
   Behind the existing narration path; re-run on dev against a box copy.
2. Frontend: header, Abstract, fact strip, sidenotes, Article | Record, Notes
   dropdown.
3. Backfill three weeks on the box, re-embed.
4. Mac attachments upload; photo figure.
5. Transition, Veil, Similar days, the chaos/order mark.

Delete this plan when slice 5 ships; what survives is a record and a manual page.
