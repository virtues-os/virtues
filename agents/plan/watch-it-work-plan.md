# Watch it work: how a chat answer was found

**Status: slices 1 and 1b are on `wave`, unreleased.** Nothing else here
is built.

Four features for one assistant turn in chat:

1. **The citation popover.** Clicking a citation in an answer opens a card
   under it. The card's body depends on the kind of record, and the arrow
   keys step through the turn's sources. Slice 1b made the same card,
   `RecordCard`, open for data everywhere in the app: in chat answers, in
   pages, and on the day page.
2. **The search, live in the status label.** While a turn searches, the
   status label shows the two searches filling, merging, and the results
   that go to the model. It then settles into the label.
3. **The search inspector.** The opened status block shows how the search
   ranked its results, with a control for how much words count, so you
   can see what would have changed.
4. **Watch it work.** The turn's status label opens a short replay of how
   the answer was found: what your server searched, what came back, what
   was sent to the model, and which records the answer cites.

The designs are on the "Sources paradigms" design canvas: "1, round 4. The
popover, polished"; "6e" and "6f" in the round 6 chat row for the live
search; "6a, in the app. The ranking you can move" for the inspector; and
"Tech prosumer, round 4. Watch it work" for the replay. This plan is written
against the code as of 2026-10-22. Where the canvas and the code disagree,
the code wins.

## The rule for the replay

**Every beat in the replay shows something this turn recorded.** If the turn
did not record a fact, the beat that would show it is skipped. The replay
never fills a gap with an animation that only looks like evidence.

The canvas breaks this rule in one place. It shows a reranker reordering the
top two results, and the engine has no reranker: `search/query.rs:14-18` says
none was kept, and why. The replay has no rerank beat.

## What the code gives today

### The status label

- `ThinkingBlock.svelte` takes `toolCalls`, `reasoningContent`, `narration`
  and `seconds` from `ChatView.svelte:2018-2034`. Those come from
  `splitTurn` in `chat/state/transcript.ts:111-138`.
- When the turn is done, the header reads "Worked for 14s", followed by up
  to three tool nouns from `uniqueToolNames` (`ThinkingBlock.svelte:250-256`).
  Opening the header shows the reasoning, the narration and one line per
  tool call, from `describeTool` (`chat/state/toolPresentation.ts:309-318`).
- A tool call on the client is a `ToolPart` (`toolPresentation.ts:17-25`):
  `type: "tool-<name>"`, `toolCallId`, `input`, `state`, `output` and
  `errorText`. The whole output is stored with the message
  (`api/turn_recorder.rs:487-553`), so a turn opened next week still has it.

### What each search tool returns

- **`semantic_search`** (`tools/semantic_search.rs:196-229`) returns
  `{results: [{ontology, record_id, rank, title, preview, author, timestamp, ref}], count, tip, from_your_wiki?}`.
  `ref` is `/record/<ontology>/<id>`, or a `/drive/...` route for document
  chunks. `preview` is the passage that matched. There is no score, by
  design (the comment at `semantic_search.rs:206-212` explains why).
- **`sql_query`** (`tools/sql_query.rs:612-628`) returns
  `{operation, row_count, columns, rows, truncated?}`. The executor adds a
  top-level `ref: /chat/<chat>/tool/<call>` to it (`agent/executor.rs:388-402`).
- **`web_search`** returns external URLs with no `ref`, so it can't be
  cited. Its query goes out through the Lite model (`api/web_search.rs:166-180`).
- **The wiki** has no tool of its own. `wiki_first` runs inside
  `semantic_search` and comes back as `from_your_wiki`
  (`semantic_search.rs:114-137`).

### What the search engine computes and throws away

`search_multi` (`search/query.rs:305`) does the following for each phrasing
of the question:

1. A meaning search and a word (BM25) search, 200 candidates each
   (`CANDIDATE_POOL`, `query.rs:50`).
2. A blend of the two, with a weight α that leans toward words when the
   query's words are rare (`fusion_alpha`, `query.rs:220-263`).
3. One passage per record, cut to `recall_limit` (`query.rs:325`).

Several phrasings are merged by rank (RRF, `query.rs:355`). The caller gets
`SearchResult {ontology, record_id, score, title, preview, author, timestamp}`
(`query.rs:33`).

The scoring SQL (`query.rs:469-503`) works like this:

- Every passage in the union of the two arms gets a meaning score, computed
  from its vector, including passages that only the word search found.
- Its word score is its BM25 score if the word search found it, and 0
  otherwise.
- Both scores are standardized across the whole union (`dz`, `bz`), then
  blended as `(1 − α)·dz + α·bz`. A project chat adds 1.0 for project
  members.
- Only after that does each record keep its best passage (`DISTINCT ON
  record_id`).

These are computed and then dropped: α, the size of each arm and of their
union, which arm found each result, and each passage's `dz` and `bz`. The
live animation and the replay need the first three. The inspector needs
the scores as well.

### What reaches the model

- The client and the stored row get the tool output with real routes
  (`to_event`, `executor.rs:45`).
- The model gets the same data with each `ref` shortened to a handle `r1..rN`,
  clipped to 96 KiB (`to_llm_content_cited`, `executor.rs:102`, clip at
  `:140`). `LinkRewriter` turns handles back into routes before any text
  streams, so stored answers contain routes (`agent/citations.rs`).
- Nothing records which handles the answer cited. The cited records exist
  only as links in the answer's text. The client can read them from there.
- The server sends `model` with each message (`api/chats.rs:182`), and the
  client drops it: `MessageMeta` (`transcript.ts:29-39`) has no field for it.
- `app_ai_calls` gets one row per turn, with `model`, tokens, cost and
  `route` (`wallet` or `byo`) (`api/chat.rs:2585`, `:2648-2672`). The message
  row does not record the route.
- Zero data retention is enforced in virtues-api per catalog model
  (`services/virtues-api/src/catalog.rs:288`). A BYO route skips virtues-api
  entirely (`virtues_api/client.rs:51`, `:230`), so nothing is enforced for
  it. The box keeps no per-call record of whether retention applied.
- The chat title is a separate call on the Lite slot, sent the first 200
  characters of each of the first six messages (`api/chats.rs:956-1058`).

### Citations on the client

- Answers render through `Markdown.svelte`. Its link snippet
  (`Markdown.svelte:199-235`) asks `linkTarget` (`utils/refRoutes.ts:126`).
  A query ref, and a link to a page the turn's web search returned, become
  citations that open RecordCard. An entity or record route becomes
  `<Ref>`. A query's rows open in a tab of their own at its route
  (`QueryView.svelte`), which reads the call from the stored chat.
- A plain click on a `Ref` opens the route in the current tab
  (`Ref.svelte:40-45`). Cmd-click opens a new tab, and "Open beside" is only
  in the right-click menu.
- Hovering shows `RefPreview` after 350 ms (`utils/refHover.svelte.ts`).
  `RefCard` fetches summaries only for people, places and organizations
  (`RefCard.svelte:32-46`). A record shows its label and the word "Record".
- `GET /api/records/:ontology/:id` (`api/records.rs:52`) returns the whole
  row as JSON. `DataView.svelte` renders any record as a list of keys, with
  no per-kind layout.
- The comment at `stores/window-shell.svelte.ts:1380` says Cmd-click on a Ref
  opens beside, and the code does not do that. Fix the comment in slice 1.

### Timing

Each tool call stores `timestamp`, which is when the model started naming
the call, not when it ran (`turn_recorder.rs:195`). A message stores
`started_at` and `ended_at`. Tool finish times are not recorded, and the
calls in one step run in parallel (`executor.rs:228`). The replay is paced
for reading, so it does not need step times.

## Slice 1: the citation popover

Client only. **Built on `wave`**, with these differences from the text
below, which is kept as the spec it was built to:

- The purchase amount uses tabular figures in the sans. The design lint
  allows mono only for code.
- The voice body has no play button or waveform yet. It shows the
  transcript with the cited words marked.
- The card stays inside the window, 8px from its edges. It is not held to
  the answer column.
- The footer has no found-by line, since that needs slice 3.
- The stale comment at `stores/window-shell.svelte.ts:1378` was not fixed.
- It was checked in the running app on invented data. It has not been seen
  on a real chat turn.

- **Look.** Citations in answers are navy with a dotted underline, and
  hovering tints them. Add this as a `ref-link--cite` variant in
  `styles/ref-badge.css`, and list it in that file's header next to the two
  existing variants.
- **Behavior.** Hovering only tints. A click, Enter or Space opens the card.
  Cmd-click still opens a new tab. In an open card, Escape closes it and ←
  and → move to the previous or next source. Focus moves into the card when
  it opens and back to the citation when it closes. Clicking another
  citation slides the card there over `--motion-layout`, and the body
  cross-fades.
- **Scope.** This applies only to citations in chat answers. Refs in pages,
  the wiki and the day biography keep the hover preview they have now.
- **Placement.** The card is anchored under the citation, with a small
  pointer, and stays inside the answer column. It opens above the citation
  when there is no room below. Position it by measuring the citation; a
  fixed offset breaks on wrapped lines.
- **Order.** The sources are numbered by their first appearance in the
  answer ("3 of 6"). The client parses the routes from the answer text, and
  no server change is needed.
- **The body for each kind.** It comes from the record (`GET /api/records`)
  plus the passage the search matched (the `preview` field of that result
  in the turn's own tool output). The passage is highlighted.

| Ontology | Body |
|---|---|
| `communication_transcription` | Play and pause, a waveform with the cited span lit, and the transcript line |
| `financial_transaction` | The amount in mono with the cents in a lighter gray, then payee and category |
| `calendar_event` | A date tile, the time, the place |
| `app_page`, `wiki_article` | The serif title, the matched passage, word count |
| `communication_message`, `communication_email` | Sender, time, the matched passage |
| `content_bookmark` | Title, site, the matched passage |
| A query ref (`/chat/<c>/tool/<call>`) | A count, plus a small chart or a grid of days when the rows have dates, then "Your server ran this query. The model saw the rows, not the rest of the table." |
| Anything else | Title, time, and the first three fields, the way `DataView` names them |

The footer holds the found-by line ("Found by meaning" needs slice 3; until
then it says nothing), the "3 of 6" stepper, and "Open beside".

**Done when:** each body above renders from a real record, the keyboard path
works with no mouse, and the card never leaves the answer column at any
window width from phone to wide.

## Slice 1b: one RecordCard wherever data is cited

Client only. **Built on `wave`.** A link to a record opens the same card,
in the same way, in a chat answer, in a page, and on a day page. It
replaced two cards: the day page's evidence card and slice 1's chat card.

### What it is

- **`components/RecordCard.svelte`** is the shell. It has three modes: in
  the margin beside a day page's sentence, a popover under the link, or a
  sheet from the bottom on a phone (`cardMode`). The popover is placed by
  `$lib/floating`, with its arrow, and slides to the next stop when the
  anchor changes. The card closes on Escape or a click outside, and ← and →
  call the caller's `onstep`. With the veil on, it shows no words or names.
- **`components/RecordCardBody.svelte`** is one source's body, by kind:
  - A message as it was sent, and a recording as the turns nearest the
    citing sentence (`citedTurns`). A search's passage stands in until the
    record loads, and it places a recording's window.
  - A purchase's amount, an event's date tile, and a title with its words
    for mail, bookmarks and pages.
  - For anything else, the title and the first three fields.
  - On every kind, the words the citing sentence shares with the record
    are marked, using `wiki/recordWords.ts`.
- **A query citation** is drawn by RecordCard itself, as a row count and
  the first rows. Show all rows opens the query's tab (`QueryView`).
- **A web citation** is drawn by RecordCard as the page's title, its site
  and date, and the text the search returned, with the shared words marked.
- **`parseRecordRef`** (`utils/refRoutes.ts`) reads both
  `/record/<ontology>/<id>` and `table:id`. `isRecordRoute` says whether a
  link goes to a record. The citation builder uses the same parser.
- **`utils/recordCards.svelte.ts`** holds the open card for a chat answer
  or a page. Its stops are the `[data-cite-route]` links in DOM order, one
  per record. It finds the sentence a link sits in (`sentenceAround`) and
  keeps the card on the same record when the text re-renders.
- **`Ref`** draws any link to a record as a citation, whatever variant it
  was given. A click sends a `recordcite` event up to the text it is in.
  Text that draws no card (a list, the chat composer) leaves the event
  alone, and the record opens in a tab as before. Records have no hover
  preview anywhere now.

### On each surface

| Surface | What opens the card | Stops for ← → | The open action |
|---|---|---|---|
| Chat answer (`Markdown.svelte`) | A link to a record, or a query citation | Links to records and cited queries, in order | "Open beside", or "Show all rows" for a query |
| Day page (`DayArticleBody.svelte`) | A sentence's sources, or a table, photo or figure | The page's sentences with sources, as before | "Open in Data", which opens the day's Data view |
| Page (`DocumentEditor.svelte`, `document/links.ts`) | A mention of a record, or a link to one (`doc-cite-link`) | Links to records, in order | "Open beside" |

Every surface marks the words shared with the sentence around the link,
including a mention in a page.

### Where it differs from what was planned

- **The open action is the surface's.** It does not read "Open beside"
  everywhere. On the day page it opens the record in the day's Data view,
  as it did before.
- **Inside a page's editor, the open link is not tinted.** A class set on
  the editor's own elements is a change to its DOM, which it would read
  back and redraw, so the page's host passes `mark: false`.
- **In an editable page,** a click on a link to a record opens the card
  and also places the caret, so the link's words can still be edited.
  This was decision 6.
- **Stepping stops at the first and last stop** on every surface. Slice
  1's wrap-around is gone.

### What is not done

- In the page editor, a keyboard user cannot reach a mention's card. The
  editor has no key that opens it.
- Voice playback in a recording's body. Slice 1 lacked it too.
- It was checked by `RecordCard.svelte.test.ts`, which mounts a chat answer
  and a day page card under happy-dom with invented records. It has not
  been seen in the running app.

## Slice 2: the status label says what left

- Keep `model` on `MessageMeta` in `convertMessageToParts`
  (`transcript.ts:156`).
- Record the route on the assistant message. Add a `route` column to
  `app_chat_messages`, written in `persist_turn` from the same value the
  `app_ai_calls` row gets. Claim the migration number first with
  `make migration NAME=chat_message_route`.
- When a turn used record tools, the done header reads "Worked for 14s ·
  Searched your records 2 times, sent 9 records to Claude Sonnet 5.5". The
  count of records sent is the number of distinct `ref`s across the turn's
  successful tool outputs. Turns with no record tools keep today's tool
  nouns.
- The open block keeps its tool list and adds one line about what left:
  - **Wallet route, catalog `zdr` is `all`:** "Sent under zero data
    retention."
  - **BYO route:** "Sent to your own endpoint at <host>." No retention
    claim.
  - **Anything else:** no retention claim.
  - **Clipped:** when a tool output was over the 96 KiB clip, say how many
    rows the model saw.
- The title call gets one line: "The Lite model wrote this chat's title from
  its first lines."

**Done when:** the label for a BYO turn makes no retention claim, and a test
covers each route case.

## Slice 3: a search trace

- `recall_and_fuse` returns, for each phrasing: the size of the meaning arm
  and the word arm, the size of their union, α, and for each result which
  arm found it (`meaning`, `words` or `both`).
- **The inspector also needs each passage's scores.** For each phrasing,
  `recall_and_fuse` returns every passage in the union with its record,
  `dz`, `bz`, and whether the project boost applied. Store these as compact
  arrays, not objects:
  - `trace.records: [[ontology, record_id], ...]`, listing each record once
    per search call.
  - `trace.phrasings[i].candidates: [[record_index, dz, bz, boost], ...]`,
    with scores rounded to 3 decimals and `boost` as 0 or 1.
  - At most 400 passages per phrasing, this is about 8 KiB per phrasing, or
    32 KiB for a four-phrasing search.
- **With these, the client can rerun the ranking exactly for any α:**
  1. Blend each passage's scores, adding 1.0 where the boost applied.
  2. Keep each record's best passage.
  3. Cut to `recall_limit`.
  4. Merge by rank when there is more than one phrasing (k = 60, a pool
     of 30).
  5. Cut to the limit.

  The trace stores `recall_limit` and the limit so the client uses the same
  numbers the server did. Keep the client's version next to a Rust test
  that checks both give the same order for one fixture.
- `search_multi` returns all of this as a `SearchTrace` alongside the
  results, plus `n_docs` from `search_index_meta`, which it already reads in
  `fusion_alpha`.
- `semantic_search` writes the trace into its output under `trace`.
- `to_llm_content_cited` removes `trace` before the model sees the output,
  so the prompt and the token budget don't change. Add a test that the
  model's view never contains it. Compaction already clips replayed results
  to 2 KiB (`api/compaction.rs:827`). Check that the trace can't take up that
  budget ahead of the results.
- `n_docs` counts passages, not records (`search/indexer.rs:795`). Slices 4
  and 6 say "passages" until the indexer keeps a record count next to
  `n_docs`. The full count that `/api/search/status` runs is too slow to run
  per turn.

**Done when:** a stored `semantic_search` output carries a trace, the
client's rerun at the turn's own α returns the turn's own results in order,
and the model's input is byte-identical to what it was before the change.

## Slice 4: the search, live in the status label

This is the first thing people see, and it comes before the inspector and
the replay. There are two designs on the canvas, and Adam picks one
(decision 5):

- **6e, a strip:** a strip about 48px tall under the label, with a meaning
  row and a words row. The rows fill with one tick per passage, slide
  together into one combined row, and the top results rise as navy dots.
  The strip folds away when the answer starts.
- **6f, a mark:** a 96px plot beside the label, with meaning across and
  words up. The dots come in from the two edges and the cutoff line draws
  in at the turn's α. The results turn navy, and when the answer starts the
  plot shrinks to a 16px mark at the front of the label. The mark stays
  there and opens the inspector (slice 5).

Both play in the same order, and every step reads a value the turn already
has:

1. **`tool-input-available` for `semantic_search`:** the label reads
   "Searching your records for <query>", as `describeTool` does today
   (`toolPresentation.ts:98-117`). The strip or plot appears empty.
2. **`tool-output-available`, with a trace:** the arms fill from
   `trace.phrasings`, the two merge, and the results are picked out. The
   counts come from the trace ("200 by meaning · 200 by words", "341
   combined", "words count 20%", "10 kept").
3. **Sending:** the label reads "Found 10. Sending them to <model>", with
   the claret dot. The model name comes from slice 2.
4. **The first text delta:** the strip folds, or the plot shrinks into the
   mark, and the label becomes the done label from slice 2.

Rules:

- **It never holds back the answer.** The animation runs on the client
  after the output arrives, while the model reads the results. If text
  starts before the animation finishes, it jumps to its last frame within
  `--motion-state`.
- **Several phrasings:** the arms show every phrasing's candidates
  together, one tick or dot per record, and the result is the merged list.
  The plot can't show several phrasings' scores on one pair of axes, so
  with more than one phrasing the mark shows the merged results without
  the cutoff line.
- **Several search calls in one turn:** each call plays in turn. The mark
  shows the last one, and the inspector can switch between them.
- **No trace** (older turns, `sql_query`, `web_search`): today's label,
  unchanged.
- **Reduced motion:** the strip or plot appears in its final state with its
  counts, and only the label's text changes.
- **Size:** at most about 120 ticks or dots, so plain DOM is fine. The
  travel times (600 to 700 ms) are content timings, declared in the
  component on `--ease-premium`, like the replay's.
- **Screen readers:** the strip or plot is `aria-hidden`. The label already
  says the words, and its announcement (`ThinkingBlock.svelte:292-297`) is
  not changed, so this adds nothing for a reader to hear.
- **Where it lives:** a new component in the header of `ThinkingBlock`
  (`ThinkingBlock.svelte:300-363`), fed the running or finished
  `semantic_search` part from `toolCalls`.

**Done when:** a real turn plays it between the search finishing and the
first word of the answer. A turn whose answer starts at once skips straight
to the end, and an older turn shows today's label.

## Slice 5: the search inspector

In the opened status block, under the `semantic_search` line:

- **A plot** of every record in the union, at its best passage for the
  current α: meaning across (`dz`), words up (`bz`). The records sent to the
  model are navy, and a dashed line marks the cutoff for the top results.
- **Beside it, the list** of what was sent, with "meaning", "words" or
  "both" on each row.
- **A control, "Words count for":** 0 to 100%, with marks at the turn's own
  α and at the 40% cap. Moving it reruns the ranking from the trace (slice
  3), reorders the list, and turns the cutoff line. A line under it names
  what would come in or drop out, and says the answer used the turn's own
  list. "Back to this search" resets it.
- **Several phrasings:** one plot per phrasing, with a switcher. The list
  is the merged list.
- **Pointing at a dot or a row** highlights both and shows the record's
  title and its two scores.

The canvas draft gets one thing wrong. It puts the word-only matches in a
column at the far left, as if they had no meaning score. The engine scores
meaning for every candidate (see above), so they spread across the plot.
It is the passages the word search did not find that share one word score
and sit on one line along the bottom.

The control only shows what would have changed. Nothing it does changes the
answer or the next search.

**Done when:** at the turn's own α the plot and list match what was sent.
Moving the control gives the same order as the Rust ranking given the same
scores. Keyboard users can reach the control and every row.

## Slice 6: Watch it work

**Where it opens:** "Watch it work" in the open status block opens a tab
beside the chat, at `/chat/<id>/watch/<message>`. It plays once on open and
has Replay.

| Beat | Shows | Comes from |
|---|---|---|
| 1. What could be searched | The count, and a field of dots spread by time and kind | `trace.n_docs`, plus a new cached per-kind monthly histogram (`GET /api/search/shape`). The field shows how the index is spread out. It is not one dot per record. |
| 2. The question | Each phrasing, the two arms, how much words counted ("Words 40%") | Tool `input`; `trace` |
| 3. What came back | The results, placed by date and ordered by rank, each marked meaning, words or both | `results[].timestamp`, `.rank`, `.ontology`; `trace` |
| 4. What left | A claret "Leaves your server" line, the count sent, the model, and the retention line from slice 2 | Tool outputs; `model` and `route` |
| 5. What the answer used | Small record cards with a thread to each sentence that cites them, and a short note under each ("Found by meaning", "Counted on your server") | Links in the answer; `trace` |
| 6. Back to the answer | The answer as it reads in chat | Message text |

- **`sql_query`:** shows at beat 3 as "Counted on your server: 61 rows", and
  at beat 4 says whether the model got the rows or a clipped part.
- **`from_your_wiki`:** shows as "Read your wiki" with the subject names.
- **`web_search`:** shows at beat 4 as "Your search words went to the web
  search provider."
- **Older turns:** these have no trace, so beat 2 shows only the phrasing
  and beat 3 has no meaning or words marks.
- **Local mode:** a local chat turn skips beat 4 and says "Nothing left your
  server."
- **Reduced motion:** under `prefers-reduced-motion`, each beat is a still
  frame with Next and Back. The information stays and the travel goes
  (`agents/build/design.md:371`).
- **Timing:** the replay's travel times (600 to 900 ms) are longer than the
  shell scale allows. The replay is content, not chrome, so it declares its
  own timings in its component, on `--ease-premium`.
- **Drawing:** draw the dot field on a canvas. A field of several hundred
  sampled dots is too many DOM nodes to animate together, and a canvas is
  also what recording a clip (below) needs.

**Done when:** a real turn replays with each beat drawn from its stored
data. A turn from before slice 3 plays its shorter version. Reduced motion
shows the same facts as still frames.

## Later, not in this plan

- **Record a clip.** `MediaRecorder` on the replay's canvas would make a
  video in the browser, so nothing leaves to render it. Remotion or
  HyperFrames could do it on a server instead, but that costs Chromium and
  ffmpeg on the box.
- **The owner number on the clip** ("Server No. 0412" on the canvas) is a
  product decision that hasn't been made.
- **Matched words for each result** ("Matched 'cello rental'"). BM25 would
  have to return which query terms hit each document.
- **A second source agreeing in a card** ("Your location history has you
  there from 6:12 to 8:40 PM"). This needs a `location_visit` lookup by time
  window, and must show only when the owner has location data.
- **Per-tool start and finish times,** for a timeline of the turn.
- **Watch it work elsewhere:** the day page, the wiki editor, transcription
  and search. The canvas has round 5 drafts of each, and each needs its own
  trace.

## Decisions for Adam

1. Links to data open RecordCard everywhere (slice 1b). Should people,
   places, organizations and days also open a card on a click, in place of
   the hover preview? The recommendation is not yet: they have pages of
   their own, and RecordCard can gain bodies for them later.
2. Links to data take the navy dotted look everywhere (slice 1b). Should
   people, places, organizations and days take it too, or keep
   `.ref-link`?
3. Should the replay open in a tab beside the chat, as planned here, or in
   place over the turn?
4. Should the index size say "passages" now, or wait for a record count?
5. For the live search in slice 4, which design: 6e, the strip that folds
   away, or 6f, the plot that shrinks into a mark? The mark stays in the
   label as the way into the inspector and takes no room afterwards. The
   strip is easier to read while it plays.
6. A click on a mention of a record in a page now opens its card, where a
   click on a mention used to open its tab. Should Enter open the card for
   a selected mention, so a keyboard user has a way in?
