# Day article: the page written from the day itself

Status: **Partly built**, unreleased. Slices 1, 2 and 5 are on `wave`
(2549d156, 1c7535e3, 3757013e, f2e2fe26); 3 (backfill) and 4 (photo figure)
are open. Spiked end to end 2026-09-24 → 09-28. Supersedes the narration half
of `api/day_summary.rs`; segmentation (the timeline) stays.

**Redesigned 2026-10-06 and again 2026-10-07**, each time from a specimen
the owner settled in review. What is built is below under "The page",
"Figures" and "Frontend". Gone:
the fact strip, the Notes dropdown, the dateline's "heard N of 24 hours" bar,
the Veil button, the Dayline's Sleep and Autonomic modes, Data's AI Chats
and Metadata sections, and the Article/Data transition.

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
not tabs; switching is instant). Toolbar:
`Article | Data · Write a note · Edit · ⋯`. Write a note shows on the
Article of a day that has begun; Edit shows when the day has a page and
opens it in the Pages editor. The ⋯ menu holds the rest, in this order:

- **Rewrite this page…**, on a past day that has a page (see "Rewrite this
  page").
- **Paint this day**, on the same days while pictures are on (see
  "Figures").
- **History**; **Open in Timeline** (the one Timeline tab, moved to this
  day); **Copy link** (the day's URL; the row says "Link copied", or
  "Couldn't copy the link" where there is no clipboard).
- **Hide names / Show names**: the veil, a per-device setting
  (`stores/veil.svelte.ts`). It hides names, the writer's ⟦marked⟧ phrases,
  your notes, and the names and words in a card; while it is on, holding `V`
  lifts it.
- **Pictures**: Oil · Watercolor · Pencil · Gouache · Off, saved as
  `ui_preferences.day_pictures` (absent reads as oil).

Focus starts on the menu's first row, ↑ and ↓ move, and Esc hands focus back
to ⋯. The rewrite's and the painting's status lines sit above the Abstract.

Article, top to bottom:

1. **Title**: the weekday and date, "Saturday, March 14", which opens the
   date picker. Under it a dateline: the year · the weather
   ("2026 · 73° / 51°", the day's high and low in °F from the weather
   records, left out when the day has none). No coverage line: what was
   recorded is the margin's to say, and Data's.
2. **Abstract**: one or two plain sentences: the day's one event and what
   made it that day. Body size. Its first letter is a two-line drop cap
   (`initial-letter: 2`, a float where unsupported) and the rest of its first
   word is EB Garamond SemiBold (`--font-serif-lead`), the one semibold on
   the page. An Abstract that opens on a link, a veiled phrase or emphasis
   keeps the cap and has no bold word.
3. **Your numbers** (DayNumbers): up to five measures you pin, the same on
   every day. Each cell has its label, the day's value, 31 bars (the 30 days
   before and this one: a light band for the middle half of the days before,
   this day dark, a day with no value a tick on the baseline) and "Usual N"
   in words, with the middle-half sentence as its tooltip. A day the measure
   cannot judge reads "Not recorded", never zero: rows outside a measure's
   `coverage` are not measured, and a day with no rows at all is a zero only
   where the registry sets `empty_is_zero` (messages, visits, purchases,
   workouts), never for sleep, steps, speech heard or a rate.
   - **Most unusual today** (the pin `unusual`, chosen on the server in
     `lifeline::day_measures`): among the measures whose lane no other pin
     shows, those with a value today and at least 7 days before holding 4 or
     more distinct values, the one furthest from its usual by
     |today − median| / IQR, when that is at least 1. Its cell puts a small
     claret "Most unusual today" over the winner's label and draws today's
     bar in claret; with no winner it reads "Nothing stood out".
   - Before you pin any: steps, people who messaged you, messages sent,
     screen time and the unusual pin, where your record holds them.
   - **Edit numbers** hangs in the margin beside the strip (under it with no
     margin) and opens the picker: "Your numbers · n of 5", "The same five
     on every day.", the unusual pin first, then "In your record" (each
     measure, "From <source>", its 31 days, the most-held first), then "Not
     in your record yet" (what would give it, such as "Needs Apple Health on
     your phone", and Connect it, which opens Sources). At five, the rest are
     disabled under "Uncheck one to choose another." One request
     (`/api/wiki/day/:date/measures`) brings every measure's 31 days, so the
     picker asks nothing more. Pins live in `ui_preferences.day_measures`.
4. Body: up to three sections, headings of 3–6 words naming a thing or a
   moment (never a bare proper noun). Tables are allowed where the day holds
   a list. Figures sit at the end of their section (see Figures).
5. Previous / next day as plain text: "← Saturday, October 3" and its
   Abstract, clamped to three lines; a neighbor with no page reads "No page
   yet".
6. **Similar days.**

A day with no page still shows your numbers and your notes, then says why it
has no page.

**The margin** sits beside the text column on a wide screen and drops under
its block on a narrow one. The writer writes none of it:

| | What it says | From |
|---|---|---|
| **Section times** | the section's span, quiet, beside its heading | the `[^cx-N]` footnotes code writes after the writer (`day_article::render`) |
| **Gaps** | "Nothing recorded" and a 12-hour span after a short dashed rule, for every stretch of 30 minutes or more inside the day the microphone did not record; beside the first paragraph whose evidence starts at or after the gap ends, gaps that land on one paragraph sharing one mark | `coverage` from `/api/wiki/day/:date/facts` (recording spans merged within 10 minutes), placed by `recordingGaps` and `placeMarks` (`lib/wiki/dayArticle.ts`). An older page's paragraph that only says "Nothing was recorded between…" is dropped from the body. |
| **Firsts** | a claret "new", then "St. Paul's Cathedral / First visit in your record", "First visit in your record" alone for a place with no real name, or "First message from Nick"; beside the paragraph whose evidence spans the moment, one to a paragraph, three to a page, people and named places first | `/api/wiki/day/:date/firsts` (`wiki::day_firsts`): a place whose earliest visit, or a person whose earliest message, falls on the day, once that kind's record has run 14 days before it |
| **Your hand** | your notes | below |

**Your hand.** Handwriting on the page is only ever yours (the
`--font-hand` face): a note written from a card sits beside its sentence
with a pen bracket over that sentence; Write a note in the toolbar writes
about the whole day beside the Abstract. Notes are `wiki_notes` with an
`anchor` (`{quote, sentence}`, 0046) and keep the sentence's words, so they
find their place again after a rewrite, or sit beside the Abstract saying
what they were about. Nothing reads a day's notes yet; feeding them to the
writer is open.

**Evidence on click.** Click a sentence (or a table, a photo, a figure with
a source) and a card shows the record's own words: a message as sent, or the
recording's turns nearest the sentence, with the words the sentence shares
with them marked (`sharedRanges`, `lib/wiki/recordWords.ts`: content words,
stopwords dropped, a light stem). A sentence that shares no content word
with its sources says "This sentence shares no words with its source."
On a wide screen the card opens in the margin beside its sentence; with no
margin it is a popover, and on a phone a sheet. Its anatomy
is the person card's: a kicker (kind · time · who; "Voices not identified"
on a recording), the body, one row of actions (Open in Data · n of N ·
Write a note). ← and → step to the previous and next sentence or figure with
a source; Esc closes. It reads the writer's `[^ev-N]` tags; nothing shows
until asked.

**The person card** (DayGloss): a person's name opens it on hover, click or
Enter. "Person · as of Mar 14", the name, how you know them (the bond in your
own words, else the relationship on their record; left out when neither),
"Messages on N of the 31 days before" with a dot a day, then
Messages that month, First on record, Last before this day (which opens that
day), and Open <first name>. Computed as of that day by `wiki::person_gloss`
(`/api/wiki/person/:id/gloss`), for resolved people only.

Data: see Frontend. Arriving from a citation shows the cited record's words
at the top and puts "← Back to the sentence" in the toolbar, which returns to
the sentence that was clicked.

## Markdown

The article stays one ordinary markdown page, editable in the Pages editor.

- Marginalia are GFM footnotes rendered as sidenotes (the Tufte/Gwern
  convention); inline under the paragraph on a phone. Labels carry the kind:
  `[^ev-3]` evidence, `[^cx-1]` context.
- Evidence footnotes hold a ref to the record item (message id, transcription
  id), so the page can open it in Data.
- A figure is a fenced block of `key: value` lines that the page renders as
  DayFigure, at the end of the section that holds its time:

  ````md
  ```figure
  kind: quote
  text: Did you get home safe?
  who: Nick
  ref: data_communication_message:<id>
  time: 9:44 PM
  ```
  ````

  Kinds and their fields: `quote` (`text`, `who`, `ref`, `time`), `thread`
  (`refs`, comma-separated `table:id`), `route` (none), `picture` (`src`,
  `caption`, `style`, `time`).

## Pipeline (`narrate_day`, v2)

1. **Assemble** (code): cleaned transcripts grouped into conversations;
   messages by thread with reactions folded and lurked-in group threads
   collapsed to a count; the owner's own words; the day's AI chats and page
   edits; the day's people with name, aliases and their wiki page excerpt;
   narrative identity; the previous three day pages; gaps (coverage, missing
   streams).
2. **Scout** (Lite slot): reads all of it and chooses the day's shape, its
   lede and the moments worth keeping; code attaches those passages word for
   word. **Write** (Chat slot, reasoning low): tagged sentences, `Abstract:`
   first, and at most one figure line at the end.
3. **Check** (Lite slot): each sentence against the evidence it cites plus the
   rest of that conversation; unsupported sentences are deleted. No repair
   pass. The Abstract is checked loosely against the body and never rewritten.
4. **Names** (code): every person's name resolves to a person record or
   appears in the day's record, else its sentence goes.
5. **Figures** (code): the writer's quote or exchange is kept only when the
   record bears it out, and the day's route is added when the day travelled
   (see Figures).
6. **Render** (code): tags → evidence footnotes; section time spans →
   context footnotes. The margin's gaps and firsts are not in the markdown:
   the page computes them when it draws (see "The page").
7. **Save**: `save_day_article` writes through the pool, so it writes only a
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

Built on `wave` 2026-10-07: the server door and the day page's action, the
first row of the ⋯ menu on a past day that has a page (`DayPage.svelte`,
`lib/wiki/dayRewrite.ts`). The row reads "Writing this page…" and Edit is
disabled while the rewrite runs; how it went is said in a line above the
Abstract. The owner's way to write a past day's page again, from the same
record and the same writer (`day_summary::rewrite_day_page`), when the page
is wrong or the writer has improved.

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

Purpose: show what prose cannot. None is the default. A page carries at most
the one figure its writer chose, the route when the day travelled, and now
and then a picture.

### Built

- **A quote or an exchange**, the writer's one choice
  (`day_article::take_figure`, `verify_figure`). The writer may end its
  output with one figure line, taken from a light moment (something funny,
  warm or plainly the owner's), the owner's own words first, never from a
  hard conversation: `Quote: "<the words, exactly>" [17:59]`, under 25
  words, or `Exchange: [msg 21:44] [msg 21:45]`, two to four messages in a
  row from one thread. The line never stays in the prose. Code keeps a quote
  only when its words stand verbatim in a passage it cites (veil marks
  dropped, curly quotes straightened, spaces collapsed; 3 to 35 words), and
  an exchange only when its messages are consecutive in one thread;
  otherwise there is no figure. A quote's `who` is "You" for the owner's
  message, the sender's name when the wiki knows them, "In a message" when
  it doesn't, and "In a recording" for audio, because the recordings never
  say which voice is the owner's. The page sets a `quote` large with who and
  the 12-hour time under it, and a `thread` as dialogue; either opens its
  records in the evidence card. The prompt's rule for hard conversations
  still says to paraphrase other people there and never to take the figure
  from one.
- **The route**, placed by code (`day_article::route_moment`): when the
  day's GPS fixes (accuracy 100 m or better, spikes dropped) moved 20 km in
  all or lie 200 km apart, a `route` figure closes the section that holds
  the middle of the day's longest movement. The page draws it from the day's
  timeline (`lib/wiki/dayRoute.ts`): a map of the day's city with its stops
  and, when the fixes lie 200 km apart, a second map of the long move from
  the phone's own fixes, dashed into any fix that came more than ten minutes
  after the one before. Caption: "Where your phone was. Dashed where it went
  quiet." Nothing draws without location data.
- **The weekly picture** (`api/day_picture.rs`): about once a week, a
  painting of one place a page names, set into its section.
  - Only what an image model can draw truthfully: a well-known public place
    the page names (a church, an airport, a stadium, a museum, a skyline),
    or one of a short list of generic settings (an airport gate, the view
    from a plane window, a train platform, a beach, a park, a rainy street,
    a lakeshore, a hiking trail) when the page's own words put the day
    there. Never a particular person: people only as a few small figures
    seen from behind or far away, with no faces. Never a home, a medical
    place, a workplace, or anything from a hard or private conversation.
  - A director on the Lite slot reads the page, footnotes stripped, and picks
    a section, a subject, a scene for the painter (the place, the light, the
    weather, the season, the time of day) and a caption, or nothing. Code
    checks the pick: the section is on the page and holds no picture; the
    subject is on the page, or is a generic setting the page's words support,
    and is no home or medical place; neither scene nor caption names a person
    from the page or a veiled phrase other than the subject; the caption is
    25 words at most. A failed check is nothing to paint, never a retry.
  - The Image slot paints it (about $0.067) in the owner's style: oil by
    default, watercolor, pencil or gouache, or no pictures at all (the
    ⋯ menu's Pictures choice). Every prompt carries the guardrails: no text,
    no logos, no signature, no frame, no faces. Stored as a JPEG, longest side
    1400 px, in the media store, and set in as a `picture` figure at the end
    of its section under "Painted from the record" ("Drawn from the record"
    for pencil), its caption veiling the place as the page does.
  - It is written the way a rewrite writes: under the day's lock, through
    the editor's document, after a restore point, as a version credited to
    the server ("Added a picture" in History). The edition (`machine_text`)
    takes the picture only when the page equals it, so a picture never makes
    a page look edited by its owner.
  - **Paint this day** in the ⋯ menu asks for one now
    (`POST /api/wiki/day/:date/picture`, optional `{style}`, about twenty
    seconds). It answers `{painted: true}`, and the page reloads its text,
    or a reason (`nothing_to_paint`, `busy`, `billing`, `failed`) the page
    says in a line above the Abstract. Otherwise a daily tick in the core
    (`day_picture::spawn`, off in a dev checkout unless
    `VIRTUES_DAY_PICTURES=1`) paints when pictures are on and no page dated
    in the last seven days holds a picture and none was painted in that
    time: it walks that week's past days with pages, most unlike the usual
    first, skipping pages whose upkeep is off, and asks the director about
    three at most.

### Open

- **Photo**: only with words attached (sent with a caption, replied to,
  talked about). Needs the Mac collector to upload message image attachments
  (images only, ≤10 MB, resized on the box; not-yet-downloaded files
  recorded and retried; deleting the message removes the image). iOS cannot
  read iMessage.
- Later, a cheap scout with `sql_query` (≤5 queries, query saved with the
  figure) may propose a comparison against the owner's own baseline. Never a
  causal claim.
- A replay of the day's moment, as a still or a clip (tabled, below).

Collections (questions asked, songs, purchases) are **tables in the prose**,
written by the writer, not figures.

Tried and rejected on the page: a coverage chart as the figure, a sky
ribbon, a month of dots (presence was every day; nothing to see). Rejected
2026-10-07 in the redesign's specimen: a painting as the page's header, and
a symbolic ink tailpiece (an object from the day, drawn at the page's end).

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
  photo of recurring people and pets is the next step there. The weekly
  picture uses painted stills for places only (above); a moment with its
  people or pets stays here.
- **Video: only Veo passes zero data retention.** Seedance and Kling have no ZDR
  provider on the gateway and were refused. Veo 3.1 Lite: 4 s at 720p, $0.12,
  ~40 s, medium kept intact; it declined one scene on moderation with no reason.

Open before resuming: a look that is unmistakably Virtues (none of the above
was), a cast that stays recognizable across days, and whether a replay is daily
or only when the day has a picturable moment.

## Frontend

Built (see "The page"): DayNumbers; DayArticleBody, drawing paragraphs
sentence by sentence (DayInline) beside its margin; DayEvidence; DayGloss;
DayHand; DayFigure. Data, top to bottom:

1. **Cited in the article**, when a citation opened Data: the cited
   record's time, kind, label and words.
2. **The day line** (`DaylineChart`, `lib/wiki/dayLine.ts`): the day's
   events on its own clock, local midnight to local midnight, so a
   daylight-saving day is 23 or 25 hours wide and an event that ends at
   midnight ends at the right edge. Each known event is a block from its
   start to its end, as tall as it was unlike the owner's usual: above the
   line when unlike, below when more usual than usual, clamped at ±3. The
   score is `local_novelty_z`, else `novelty_z` (`usualScore`), the same one
   the event timeline's badge reads. Unknown stretches stay empty; an event
   nobody scored is an outline on the line. Hover gives its label, its times,
   and "Unlike your usual", "Like your usual", "Your server hasn't scored
   this" or "Most unlike your usual". Under it, a thin lane per stream
   (Audio, Location, Messages, Health, Screen), filled where that stream
   recorded something, from the day's lifeline lanes. What was heard is read
   here now.
3. **Places** (`DayPlaces`), when the day has location: the stops on a strip
   across the day's clock and the track and stops on a map; hovering the
   strip moves a dot along the track. A stop nobody named reads "Unnamed
   place".
4. **Event timeline** (`EventTimeline`): the segmenter's events, collapsed,
   with the one most unlike your usual open and badged "Most unlike your
   usual". Each lists its people only, never the owner, and its place only
   when the place has a real name; an unknown stretch reads "Not recorded".
   The event labels show only here; they are the weakest data on the page.
5. **Data ontologies**: every record of the day in one chronological grid,
   with a chip per ontology to filter it.

The timeline's words come from the segmenter (`SEGMENT_PROMPT`,
`day_summary.rs`). Its dossier opens with a `<you>` block naming the owner,
so the owner is "you" and never a companion. A label is 2–6 specific words
that use the day's people and what came before, naming a person only where
the record puts them (on the thread, on the call, in a corroborated calendar
entry). A summary is one or two plain sentences on what mattered: who, what
was said or done, and where only when the place has a real name; never
distances, speeds, coordinates, message counts, silence or ambient noise.
Days cut before that prompt keep their old words until they are cut again:
`virtues day-summary --segment-only --from YYYY-MM-DD --to YYYY-MM-DD`
re-cuts a range oldest first, one Chat-slot call a day, then rescores what it
cut, since each day's novelty is measured against the days before it.

Open on the page:

- Re-segmenting the box's backlog with that command.
- Naming places. A stop nobody named reads "Unnamed place", and nothing
  names a city yet, so the dateline carries no place. A place-naming agent
  is open.
- Custom measures described in a sentence, and practices with a check-in
  (the specimen showed both).
- A clip or video of the day's moment (see Figures).

## Slices

1. Backend: assembly, prompt, check, names, marginalia, oldest-first queue.
   Behind the existing narration path; re-run on dev against a box copy.
2. Frontend: the page above: Article | Data, the margin, the cards, your
   numbers, figures.
3. Backfill three weeks on the box, re-embed.
4. Mac attachments upload; photo figure.
5. Veil, Similar days; the chaos/order mark is not built.

Delete this plan when slice 5 ships; what survives is a record and a manual page.
