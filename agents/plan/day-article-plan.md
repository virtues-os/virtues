# Day article: the page written from the day itself

Status: **Partly built**, unreleased. Slices 1, 2 and 5 are on `wave`
(2549d156, 1c7535e3, 3757013e, f2e2fe26); 3 (backfill) and 4 (photo figure)
are open. Spiked end to end 2026-09-24 → 09-28. Supersedes the narration half
of `api/day_summary.rs`; segmentation (the timeline) stays.

**2026-10-06: the page was redesigned** from a specimen of the Sep 23 page the
owner chose (evidence on click, glosses, your hand in the margin, your
numbers). What is built is below under "The page"; the fact strip, the
evidence margin cards and the Notes dropdown it replaced are gone.

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

One page, two views of the same day: **Article | Data** (a segmented switch,
not tabs). Toolbar: `Article | Data · Veil · Note · Edit`.

Article, top to bottom:

1. **Dateline**: weekday · weather · "heard 8 of 24 hours" with the day's
   clock as a bar (DayDateline). The one fixed statement of what the page can
   know. Title: the full date.
2. **Abstract**: one or two plain sentences: the day's one event and what made
   it that day.
3. **Your numbers** (DayNumbers): up to five lane measures you pin, the same
   on every day, each with a tick against your own 30 days before and the
   usual in words. A day with nothing the measure can judge reads "Not
   recorded", never zero (`LaneMeasure.coverage`). Before you pin any, common
   ones show if your record holds them. Pins live in the assistant profile's
   `ui_preferences.day_measures`.
4. Body: up to three sections, headings of 3–6 words naming a thing or a moment
   (never a bare proper noun). Tables are allowed where the day holds a list.
5. At most one figure, only when earned (see Figures).
6. **Rewrite this page**, a quiet action at the foot of a past day that has
   a page (see "Rewrite this page" below). Not in the toolbar, which a phone
   cannot widen.
7. Previous / next day as two cards, each with its Abstract.
8. **Similar days.**

Three ways the page answers "how do you know?", each with one job:

| | Form | From |
|---|---|---|
| **Evidence on click** | Click a sentence (or a table, a photo): a card with the record's own words, a message as sent or the recording's turns nearest the sentence, and "Open in Data ↗" | the writer's `[^ev-N]` tags; no apparatus shows until asked |
| **Glosses** | A person's name has a help cursor and a dotted line; hover or click for facts as of that day (earliest message on record, days with messages in the 31 before, last day before) | `/api/wiki/person/:id/gloss`, computed; only for resolved subjects |
| **Section times** | Quiet text in the margin beside each heading | code, after the writer |

**The margin is the owner's.** Handwriting on the page is only ever yours:
a note written from a sentence's card sits beside it with a pen bracket over
that sentence; "Note" in the toolbar writes about the whole day beside the
Abstract. Notes are `wiki_notes` with an `anchor` (`{quote, sentence}`, 0046)
and keep the sentence's words, so they find their place again after a
rewrite, or sit beside the Abstract saying what they were about. Nothing
reads a day's notes yet; feeding them to the writer is open.

Data: dayline, timeline, and the full data grid; arriving from a citation
shows the cited record's words and offers "← Back to the sentence", which
returns to the sentence that was clicked.

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
6. **Save**: `save_day_article` writes through the pool, so it writes only a
   page nobody has written in and nobody has opened: it keeps a page whose
   upkeep is off, one with a human edit, and one with a CRDT (opened in the
   editor, however long ago), and says which (`SaveOutcome`). Narration
   checks the same guards before the writer runs, so a refusal costs no model
   call, and writes a day once (`NarrateOutcome`). A page it writes lands in
   one transaction with its edition (`machine_text`) and the day's
   `narrated_at`, so a failure leaves no written page for the queue to pay
   for again.

Order: the catch-up queue narrates **oldest first**, so continuity flows
forward on the first pass. The intended follow-on, a day re-narrating when the
day before it is rewritten, is **not built**: a rewritten day's next three
pages keep what they read of the old one. A backfill of the last three weeks
runs once, oldest first, then the novelty index re-embeds all of it together
(the embedding detects the writer's voice; a mixed corpus would light up every
new day as novel).

## Rewrite this page

Built on `wave` 2026-10-07: the server door and the day page's action
(`DayPage.svelte`, `lib/wiki/dayRewrite.ts`). The owner's way to write a past day's page again, from the same record and the
same writer (`day_summary::rewrite_day_page`), when the page is wrong or the
writer has improved.

- **One writer per day.** Narration and the rewrite both take a session
  advisory lock on the date, on a connection detached from the pool
  (`try_lock_day`). It saves money, not pages; the guards above and the
  CRDT's staleness check still do that.
- **The request answers at once.** `POST /api/wiki/day/:date/rewrite` refuses
  what needs no model call (`not_over` 422, `no_page` 404, `needs_consent`
  409, `rewrite_in_progress` 409 while a rewrite of the day runs, `busy` 409
  while another writer such as narration holds it), takes the lock, and hands
  it to a task; `GET` on the same path reads how it went from an in-memory
  board (`DayRewrites`). A restart forgets the board, and an interrupted
  rewrite is not resumed, because resuming would pay again.
- **Time limit on the writer only.** Its model calls get 20 minutes
  (`REWRITE_TIME_LIMIT`). Once a draft exists the write runs to the end,
  because one stopped partway could leave the page changed in open editors
  but not saved, versioned or recorded.
- **Consent.** A page that may hold the owner's words (a human edit or a
  version put back, upkeep off, text that is no longer what the server last
  wrote, or, on a page from before narration recorded its edition, any
  version at all) is rewritten only when they agree to replace them.
- **Through the CRDT, undoable.** The draft must have an Abstract and a
  section. The page is read again, kept as a restore point (fail closed), and
  replaced by line through the server's own `YjsState`, so open editors
  receive it; any change since the rewrite began refuses it. The rewrite's
  version is credited to the record, and History shows one entry whose undo
  puts the old page back.
- **Bookkeeping**, in one transaction: the edition (`machine_text`), the
  owner's edit stamp cleared unless an edit landed after the write, and
  `narrated_at` restamped, which, with the wiki editor on, earns the year
  and chapter one revision each at their next interval. Upkeep is not
  changed. A draft that is on the page but not saved yet is a rewrite made:
  it is versioned and recorded, and the save queue lands it.
- Notes are untouched; a note whose sentence went sits beside the Abstract.

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

Built (see "The page"): DayDateline, DayNumbers, DayArticleBody drawing
paragraphs sentence by sentence (DayInline), DayEvidence, DayGloss, DayHand.
Data holds `DaylineChart`, `EventTimeline` and the data grid; the event labels
show only there (they are the weakest data on the page). Article and Data
switch along the day's clock (View Transitions: the dateline's bar becomes the
dayline), crossfading where unsupported and under reduced motion. The veil
(hold `V`) hides names, the writer's marked spans, your notes, and the names
and words in an evidence card.

Open on the page: the handwriting face itself (`--font-hand` falls back to
Bradley Hand, which only Apple systems have), custom measures described in a
sentence and practices with a check-in (the specimen showed both), and the
writer marking the exact words that support each sentence so the evidence
card can highlight them.

## Slices

1. Backend: assembly, prompt, check, names, marginalia, oldest-first queue.
   Behind the existing narration path; re-run on dev against a box copy.
2. Frontend: header, Abstract, fact strip, sidenotes, Article | Record, Notes
   dropdown.
3. Backfill three weeks on the box, re-embed.
4. Mac attachments upload; photo figure.
5. Transition, Veil, Similar days, the chaos/order mark.

Delete this plan when slice 5 ships; what survives is a record and a manual page.
