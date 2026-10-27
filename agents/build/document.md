# Block pages: the document contract and its editor

A block page's text is a Yjs XML tree under one contract,
[`crates/virtues-document/contract.json`](../../crates/virtues-document/contract.json).
The Rust crate reads it to check every write and render HTML and markdown;
the browser builds its Tiptap schema from the same file
([`apps/web/src/lib/document/schema.ts`](../../apps/web/src/lib/document/schema.ts)).
The decision is [record/editor-kernel-2026-10-07.md](../record/editor-kernel-2026-10-07.md);
what was built, and what is left, is [record/document-contract.md](../record/document-contract.md).

## One editor, two formats on the server

`app_pages.format` says how the server stores a page.

| Format | Document | In the app |
|---|---|---|
| `tree` | XML tree in the `doc` fragment, stamped `meta.contract` | Tiptap (`components/pages/DocumentEditor.svelte`), bound by `createTreeDocument` (`lib/yjs/document.ts`), contract 2, IndexedDB `tree-<id>.<lineage>` |
| `markdown` | markdown in a Y.Text (`content`), until the server moves the page | read only: the server's markdown of it in `MarkdownView`, with "Your server couldn't move this page from markdown to blocks" (`MARKDOWN_READ_ONLY` in `components/views/pageDocument.ts`); no socket, no copy on the device, no Restore in History |

**Block pages are the default.** Two switches, each on unless set to `0`
(`pages::on_unless_zero`), are kill switches: `VIRTUES_TREE_PAGES` (a new
user page is created as blocks; anything moves to blocks at all) and
`VIRTUES_CONVERT_PAGES` (user pages move at startup and as they are
opened). A user page in markdown moves as the app opens it: `GET
/api/pages/:id` runs the run's own move on it first
(`page_convert::convert_on_open`, `apply_page`), so the answer says
`format: "tree"`. A page that cannot move (it would lose text, or holds a
suggestion its blocks cannot) stays markdown, logged with why, is tried
again at its next open, and opens read only.

The app has no markdown editor. The server still stores markdown pages, edits
them for the model, the CLI and the writers, and binds the Y.Text of one for
an older app that sends `contract=0`, which is how such an app edits it.
The app's floor (`MIN_BOX_API` in `lib/boxApi.ts`, 4) is the box that stores
pages in blocks under contract 2; below it the app shows "Your server needs
an update".

The binding sits on `lib/yjs/socket.ts`: the contract on the socket, the
server's 4426 and 4422 close codes, and the rule that the server's sync is
authoritative while this device's copy stands in only when the binding may
read it. A page's stamp only rises: a server's write raises it to the
server's contract, and a client's change to a block page stamped below the
client's contract raises it to the client's before the change goes out
(`PageDoc::raise_for_writer`), since what that client wrote may hold what an
older client's schema cannot read and its binding would delete. Every
socket bound below the new stamp is closed with 4426 before anything is
relayed to it. The other way round, a client above its server's contract
opens every block page read only, whatever the page holds, and says the
server needs an update (`TREE_CONTRACT > box_contract` in
`lib/yjs/document.ts`; the server binds its socket read only too). A
phone or a Mac updates itself ahead of its box, so a release that raises
the contract reaches boxes before the apps that carry it.

## Rules the editor keeps

- **Opening a page writes nothing.** No extension rewrites what it is given:
  no `TrailingNode` (it appends a paragraph after a final table, image or
  applet, once per client), no `appendTransaction` that normalizes. A click
  below a final table, image or applet adds a paragraph as the person's own
  edit.
  `editor.test.ts` counts updates on mount, resize, blur and reconnect.
- **Never bind an empty fragment.** Tiptap writes its empty paragraph into
  an empty shared document. `createTreeDocument` never reports synced on
  an empty tree; the server always writes at least one block.
- **Ids arrive with remote changes.** UniqueID skips transactions from the
  shared document (`isChangeOrigin`), or every client rewrites ids.
- **Anything transient is a decoration, never a node**: find matches,
  focus dimming, Shiki colours (`code.ts`), the assistant's caret, trail
  and flash (`presence.ts`). None reaches the shared document or another
  device.
- **Nothing waits between the person's action and the page.** A paste, a
  drop and files given (pasted, dropped or picked) change the page once, in
  the edit the person makes, an undo step of its own (`ownUndoStep` in
  `history.ts`); no result of a wait is ever written at a place reserved for
  it. Markdown converts in the page (`converter.ts`), and a file's block
  goes in at its final address with the bytes following outside the page
  (`media.ts`, `uploads.ts`). Undo is plain y-undo, and other devices'
  edits are the CRDT's to merge.
- **A place reserved while a person chooses is an anchor** (`anchor.ts`),
  never a number: the link panel and menu, the applet list, the mention
  picker, the file dialog, the inline writer. Another device's edit reaches
  the editor as one step over the whole document, which collapses a mapped
  position, and an open `/` or `@` is placed again from the caret, which
  the binding keeps as one (`triggers.ts`). What acts on a link or a
  selection checks it still reads as it did, and leaves it alone if not.
- **The editor keeps what the binding drops** (`KeptThroughOthersEdits` in
  `anchor.ts`). The binding places the selection again as text or a node
  only, and its one step clears the marks waiting to be typed: selected
  table cells and a gap cursor are placed again from anchors taken before
  the edit, and a mark turned on at the caret stays on.
- **A moved block is the same block.** ⌥↑ and ⌥↓ (`moveBlock`) and a drop
  from the grip beside a block (`drag-handle.ts`, `moveUnitTo`) take it out
  and put it back in, two edits and one undo step: it keeps its id, and an
  edit another device makes meanwhile is lost with the move rather than
  landing in a neighbour. Lists a move leaves or makes side by side are
  joined. The grip shows only with a pointer that hovers; there is none on
  a phone.
- **A numbered item keeps its number.** An item whose number is not one
  past the item before it (the list's `start`, for the first) carries its
  own: `value` on `listItem`, HTML's `<li value>`, and the items after it
  count on from it. Read from HTML, a list starts at its first item's
  number and an item keeps its own only where it departs from the count, a
  bullet list's none (`count_items` in `ingest.rs`, `itemNumber` and
  `listStart` in `schema.ts`, which agree on every corpus case); a page's
  own tree keeps what it holds, an item the editor left holding the
  number the count gives it included. Enter starts an item that counts on
  from the one before (the number is not kept on a split); a paste that
  splits a list starts the second at its first item's number
  (`placeConverted`); a typed `N. ` under a list joins it only when `N` is
  its next number (`CountingOrderedList`). The markdown export writes every
  item's number, and the converter reads each back as written
  (`item_number` in `migrate.rs`), but for a list numbered lazily: every
  item written with the first one's number (`1.` `1.` `1.`) counts on from
  it, as CommonMark and the chat read it, in a writer's markdown and a paste
  (`lazily_numbered`). A page moved from the markdown editor keeps such a
  list as typed, and its export reads back counted. Two lists of one kind
  side by side are one list to markdown, so the export writes the second
  with its kind's other marker (`*` for `-`, `)` for `.`:
  `continues_list` in `render.rs`, `continuesList` in `markdown.ts`), and
  each reads back as its own.
- **What a link to a file draws** (an image, audio, video or a file card) is
  `kindOfLink` (`media-kind.ts`) in the editor, the ref card, the file
  view and a file ref's icon, and the server's converter reads the same
  cases (`media-kinds.json`).
- **Markdown goes through the crate's converter, in the page**
  (`converter.ts`): `convert_for_editor`, compiled to WebAssembly from the
  workspace's own source and `Cargo.lock` (`crates/virtues-document-wasm`,
  built by `scripts/build-converter.mjs`, never committed), and called
  synchronously inside a paste, a drop and the inline writer's end. There
  is no second markdown parser in TypeScript, and `converter.test.ts` checks
  the module against the native build byte for byte over the corpus's markdown
  and every case's export. The editor mounts once the module has loaded,
  failed or taken three seconds (`converterSettled`); without one,
  markdown pastes as its text and the person is told once. A writer's
  markdown takes suggestions in as accepted (`parse_written_markdown`): a
  suggestion is made in the page, never brought into it. A paste is read
  as one (`parse_pasted_markdown`): a raw tag that is no HTML (`<Login/>`,
  `Vec<String>`), HTML whose words the page would lose (a comment,
  `<script>`, `<details>`: `ingest::keeps_its_words`) and CriticMarkup stay
  text, each line stays a line, and a line under a list item or a quote
  that is not indented into it is a line of its own; a link reference
  definition is the line it was, a link naming its label still going where
  it says, a table is as wide as its widest row (`tables_widened`), as
  GFM drops a row's cells past its header's, and a paste opening with
  `---`, a line and `---` is a rule and the text under it, not front
  matter left out. More than 256 KiB
  (`EDITOR_MAX_BYTES`), or more nodes than a page holds, pastes as text; a
  paste or a drop that would take the page past `MAX_NODES`, or nest it
  past `MAX_DEPTH` levels, however it would land, files given included
  (`addFiles`), is refused and said (`pastPageLimit` in `paste.ts`). A
  paste with Shift held (Paste and Match Style) is the text as written, a
  paragraph a line, never converted nor formatted. Only a
  block's sign at the start of a line (a heading, a list, a quote, a fence,
  a table row) makes pasted plain text markdown, and only when no line
  reads as code (`readsAsCode` in `code-text.ts`): a Python, shell, YAML or
  Dockerfile comment's `# ` is a heading's sign, a diff's `-` and `+` lines
  are bullets of one kind, a terminal's `> npm start` a quote, and code's
  `__init__` is no bold to the converter or the paste rules (a name in two
  underscores after a `.`, called, compared or quoted, a Python block's
  line, a YAML key opening a block with a key under it, a command, a
  Dockerfile's instruction, SQL's statements and C's directives, read as
  code). Code in a fence, at any indent (one
  nested in a list item is indented four spaces or more), or indented
  inside a list item is the markdown's own, and a line opening with a
  command's word or an assignment that ends as a sentence does is prose
  (`git keeps every version.`). The words of a line are
  formatted by the editor's own paste rules, which read emphasis as
  CommonMark does (`a == b` and `2 * 3 * 4` stay text) and run on plain
  text only, never in inline code. A code editor's HTML (VS
  Code's syntax colours) is pasted as its text; with VS Code's own data a
  markdown file's text is converted, a line of prose included, and another
  language's is a code block, as is a markdown file's text that reads as
  code (VS Code names the file's language, not the fence's). Word's lists, paragraphs
  marked `mso-list` with their bullet as text, are made lists
  (`wordLists`). A drop is read as the same data pasted: markdown text
  dropped is converted, and the plain-text rules never run on dropped
  HTML, nor any rule on a block dragged from another page.
- **Markdown is written in TypeScript** (`markdown.ts`), for text copied
  out of a page (`clipboardTextSerializer`) and for a chat message
  (`messageMarkdown`, below): the crate's `render::markdown` rule for rule,
  checked for the same bytes by the conformance corpus. A message differs
  only in its text, which is written as typed.
- **A paste lands as whole blocks** (`placeConverted` in `paste.ts`): one
  paragraph joins the text at the caret; anything else splits the
  paragraph there and steps out of the lists around it, so a pasted heading
  never merges into the line it was pasted on.
- **A file's block has its final address from the start**
  (`addFiles` in `media.ts`): `/drive/<id>` with a Drive id made in the
  browser (`mintFileId`: `file_` and 16 hex digits, the shape every Drive id
  takes), so the page's markdown, copies, versions and publishing carry
  the address the file will have. The upload (`uploads.ts`) belongs to the
  app, not the editor: it outlives the page, retries a failure it can
  recover from, and writes nothing to any page. The server stores the file
  under that id (`POST /api/media/upload` with `id` first,
  `api::media::store_page_file`), refuses an id another file has, and says
  how each file stands (`GET /api/media/uploads?ids=`): stored, arriving,
  stopped with why, whether it refused the file and how long ago, or
  missing. An upload nothing has reached for two minutes (`STALLED_AFTER`:
  a laptop asleep, a phone out of range, with the connection never closed)
  is stopped, not waited on, and one whose request is dropped (a reset
  connection) is stopped at once. What a request says of the file counts
  only while it is the latest to begin for its id (`PageFileUpload`): a
  file sent again while the server is still handling the request that carried it
  before (a phone moved from Wi-Fi to cellular) reads as arriving whatever
  the older request says after. A stop is missing an hour after it
  (`FAILED_KEPT`), on the next look: no device that has the file is still
  trying, and a publish no longer waits on it. A box out of space stops an upload short of a refusal (507):
  emptying the Drive trash makes room, and the same file sent again is
  stored. Room is checked once, after the file already at its address is
  looked for (`drive::store_system_file`): a retry of a file stored before
  its answer was lost is answered stored however full the box is, and an
  upload that loses the last room to another is a 507, never a refusal.
  `MediaNode` draws it: the picture itself while it uploads here,
  "uploading on another device" elsewhere, a failure here with Try again
  short of a refusal (a refused file is never sent again: every other
  device has stopped asking about it). Elsewhere a stop short of a refusal
  says the sending device may send it again, with no Remove, for as long as
  that device goes on trying (`SENDER_TRIES_FOR_MS`, from when the upload
  stopped, as the server says), then "didn't finish
  uploading"; only a stored file or a refused one ends the asking, so a
  file sent again later still arrives. A block new to a page asks at once,
  whatever pace an older block's asking has slowed to. A file's card loads
  nothing and so asks from the start; a file that does not load asks once
  it fails, and once stored is drawn again from the server, once: it may
  have arrived between the load and the asking. One that still does not
  load is one the browser cannot draw (a TIFF in Chrome) and stays a card.
  A file whose bytes have not reached the server is at stake until stored
  or refused (`uploadsAtStake`): sending, waiting on a retry, or out of
  retries and waiting on Try again. The app asks before it closes while
  one is; every reload goes through `lib/reload.ts`, since the Mac and iOS
  apps' web views ask nothing before one: a reload the person asks for
  ("Reload for the latest", Relaunch) waits and says why, and the app
  reloads itself (a new UI, a new build, a stale chunk) only while none is
  (`tauri/quietReload.ts`). Quitting the desktop app is not delayed, by
  decision: delaying it took a runtime patch to the windowing library's app
  delegate, installed for every Mac user, for an edge a person can recover
  from. A file stopped by a quit shows Try again on its block, here or on
  another device. A reload the person asks for offers to go anyway. A
  failure short of a refusal is tried again after 2, 8, 30 and 60 seconds,
  then every few minutes for as long as the app is open, and at once when
  the device is back online or a page's socket reaches the server again
  (`serverBack`), since the file is only in the page's memory and its
  block is already on every device. At most three files are sent at once,
  fewer than the connections a browser opens to a server, and each is said
  to be coming before its bytes can be (`POST /api/media/uploads`,
  `media::page_file_announced`): the browser queues requests beyond its
  connections where the server cannot see them, and every other device,
  and a share of the page, would take such a file for one never sent. The
  Share sheet on the device sending an image does not share its page until
  it arrives (`imagesOnTheirWay`). A block that leaves every page open
  here (undo, Remove on its card or from its menu, a cut, another device's
  delete, its page closed) may be in a page this device does not show, so
  its file goes on, at stake, until the server says no page names it
  (`GET /api/media/uploads/named`, `media::page_files_named`, which reads
  the docs the server has loaded before the rows); then it is let go, not at
  stake and not sent, until the block is back in a page here (redo, a
  paste), which sends it again, with Try again on its card if that fails
  (`followFileBlocks` in `media.ts`, `uploadBlocksLeft` and
  `uploadBlocksBack` in `uploads.ts`). A block still in another open page
  is not gone, and nor is one turned into a link to its address
  (`filesNamedIn`): the page still points where the file goes.
  `sendPageFile` takes a 2xx only for the file stored at the id it sent: a
  box below API version 4 stores the bytes under an id of its own. A file
  with no extension is named one from its type, since the markdown reads a
  Drive file's kind from its name.

## The model's edits

- **A table's rows carry ids** (`tableRow` in the contract): a row is added,
  replaced or deleted by its id, and a table too long for one read opens by
  its rows as a list opens by its items. HTML for a row (`<tr>`) is read in
  table context (`ingest.rs`), and a table part written where a block goes
  is refused, as HTML's parser would drop its tags. A row edit that leaves
  the table no grid is refused.
- **A batch takes only what reads showed.** A read's text and what its
  base learns are read off one value (`virtues_document::Shown`, built by
  `read_by_id`, `children_window` and `page_reads::markdown_window`), and
  the base stores each block on its own as last shown (`Seen`): its own
  attributes and text, a stub where each block inside it goes, which of
  those blocks no read showed inside it (an opened read counts the blocks
  past its window without showing them, and one shown elsewhere is no block
  shown there, nor one a later read shows standing anywhere else, inside
  another block or at the page's top level), and
  whether it was shown as HTML or only as markdown. A
  read on among the blocks inside one shows them there (`Shown::inside`). A batch with a base is checked by
  what it does to the page, the page before it against the page it would
  leave (`check_ops`, and again inside `apply_ops`): a block it takes off was
  shown as it stands, and, taken with a block around it, shown inside that
  block or deleted by name (one moved there since, an item indented, is not
  taken on a read that showed it elsewhere, nor on one that counted it
  there without showing it, nor by a replace naming it, which means it to
  stay, nor lends its id to a block the replace writes without one); one it
  writes over was shown as it stands as HTML, or is a
  text block merged with a change made since and holding the merge at the
  batch's end (an attribute both changed, a heading's level or a code
  block's language, is refused; a later op writing over the merge is
  checked as any write over); a block listed
  as too long for any read goes only by an op deleting it, while it hashes as
  listed, never with a block around it, even when a markdown read showed
  it; a block shown only as markdown is not taken off the page while the
  batch writes back what its markdown would read as, as a block of
  another kind (a mention as a link, audio, a video or a file as an image
  at its address, a callout as a quote opening `[!TONE]`:
  `FromMarkdown` in `ops.rs`); a block written back by the id of one
  deleted since the read is refused. What a block taken out leaves its parent unable to hold is checked
  at the batch's end, so its only paragraph deleted before the item around
  it lands. So a list read in part is replaced
  or deleted once reads have shown all of it, whatever else the batch
  writes, in that edit or the next. A refusal shows the blocks it is about
  when they fit and says where to read on when they do not (`refusal_of`):
  a block added to one the model read is shown inside that block, cut down
  to the way to it, or read on inside it, or named by an op when it was
  shown elsewhere; a block listed as too long for any read that changed
  since is read again when a read now shows it; blocks no read has shown
  in the cells of a row too long for any read are named in one `ids` read.
  A block whose op was refused for anything but the block itself (its
  HTML, wherever the problem is charged, or the batch's shape) is neither
  shown nor learned (`to_rewrite`): the retry fixed from the model's copy
  merges with a person's change, as the first try would have. An op that
  was not refused, on a block changed since that a refused op's problem is
  about, is refused with it: the reply shows that block and its base
  learns it, so the op is written again from what is shown, never sent as
  written over the change its first try merged with. A refusal
  that waits only on reads of blocks no read has shown says so
  (`read_first`), and the turn's guard does not count it toward closing
  `edit_page` (`agent/guard.rs`); sent again unchanged it is refused as
  ever. An edit's base stores what
  it wrote as written, in a block someone else changed since too
  (`Seen::advance`). `page_editor_unseen_tests.rs` tests all of this against
  random pages, reads, a person's edits and moves and the model's, with a
  refusal showing more than a read holds and less, as a box's does, and
  fails on any refusal a model doing what it says cannot get past. An
  `ids` read shows a block with the way to it, several inside one block on
  the way to as many as fit, the rest named for the next read, or alone
  when the way to even one does not fit, never leaving a block that fits a
  read unread; one a read showed elsewhere comes with
  the block it is in now, so the read shows it there.
- **A base serves the turn that made it.** A chat turn's tool results come
  back to later turns cut to 32 KiB, then 2 KiB (`api/compaction.rs`), while
  a base would still let an edit write over all a read showed. So a base
  made in a turn carries the turn's tag (`<hash>.<tag>`,
  `page_reads::serves_turn`), and a later turn's replace or delete with it
  is refused until it reads again or takes the open page's base, which each
  turn makes afresh. The CLI and other callers outside a turn take any base.
- **A page holds at most `MAX_NODES` nodes**, every text run counted: every
  check of a keystroke reads the whole page. Ingest, an edit, an append and
  an editor's update are each refused past it, and the socket's check runs
  off the async workers.
- **The server's own tree keeps its text as typed.** A version put back,
  or a page put back inside the contract, goes through HTML read as the
  page's own tree (`parse_own_blocks`, `parse_own_slice` in `ingest.rs`),
  which keeps spaces, tabs and line ends that HTML's whitespace rules
  would collapse. So does a model's replace written from a read: wherever
  its text is the read's as HTML reads it back, the spaces typed stay
  (`keep_typed_space` in `ops.rs`), and only what it changed is its own.
- **A refusal of what a block cannot hold names what does not fit**
  (`misfit` in `ops.rs`), never every block beside it: a page of thousands
  of blocks would carry the refusal past what a tool's result holds.
- **An update that waits on items the page lacks is refused**
  (`check_update`): what waits is outside the check's sight. The page's
  socket asks its client to sync once per state of the page (`Asking` in
  `server/yjs.rs`): a client with edits that follow ones nobody has
  answers every request with them, and is asked again once the page
  changes. A saved tree
  holding what the contract refuses is put back inside it when the page is
  opened (`put_back_in_contract`).

## Adding a widget

A widget is a node in the contract that the editor draws itself. Each needs
all five, in one change:

1. **A contract entry** in `contract.json`: group, `atom`, `id`, attributes
   with their types and HTML names, the tag. Custom elements are never
   self-closed (`<virtues-x></virtues-x>`); ingest refuses `<virtues-x/>`.
   Raise the contract version if documents written under the current one
   have shipped.
2. **A node view**: a Svelte component in `components/pages/nodes/`, wired
   in `lib/document/views.ts` through `svelteNodeView` (an atom) or
   `svelteContentNodeView` (a block with editable content and chrome). The
   component writes the document only through `view.setAttrs`, in answer
   to the person, never on mount. A widget that costs a frame or a request
   mounts lazily (`{ lazy: true }`, as the applet does). Its menu opens on
   right-click and on a long press (`onContextGesture`), since iOS fires no
   `contextmenu`.
3. **A markdown export arm** in the crate's `render.rs`: what the `content`
   column, search and embeddings read for it.
4. **Conformance cases** in `crates/virtues-document/tests/corpus/cases.json`,
   so `conformance.test.ts` proves Rust and Tiptap read and write it alike.
5. **A print rule** in the component's `@media print`: what it prints as
   when a frame or a player cannot (an applet prints as a box naming it;
   audio, video and a file print as their card).

## Page checkboxes

Task lists are page content: `taskList` and `taskItem` in the contract,
Tiptap's `TaskItem` view, and the To-do command in the insert menu, which
`/task` (and `todo`, `checkbox`, `check`) finds; typed `- [ ] ` makes one
too. Checking a box writes the item's
`checked`, and leaves the focus where it was (`QuietTaskItem` in
`schema.ts`): the stock box focused the editor, which raises a phone's
keyboard. Keys pressed on a focused box are the box's, never edits where
the caret is, and ⌘Enter ticks the to-do the caret is in (`TodoKeys` in
`commands.ts`).

## Moving pages to blocks

`virtues pages convert --dry-run | --apply` (run as the `virtues` service
user on a box, as every command that reads the database is:
`cli::types::RUNS_AS_SERVICE_USER`), the server's startup unless
`VIRTUES_CONVERT_PAGES=0`, and opening a page, move every markdown user
page to blocks (`api/page_convert.rs`); with `--articles`, and at startup
with `VIRTUES_CONVERT_ARTICLES=1` (off unless set to `1`), they move
articles instead ([Articles](#articles)). All move pages only while block
pages are on (unless `VIRTUES_TREE_PAGES=0`). `--dry-run` runs in the CLI's own process against
`DATABASE_URL` and writes no page, so it runs on a copy of a box;
`--apply` runs in the server through the console door
(`POST /api/console/pages/convert`), since only the server has the pages'
live docs and their queued saves. The CLI finds that server by its port
alone, so `--apply` opens the database `DATABASE_URL` points at and names
it by its name and when its Postgres server started
(`page_convert::DatabaseIdentity`): a server serving another, a copy
restored under the same name into another Postgres included, refuses and
moves nothing, as it does a request naming none (`page_convert::apply_on`).

- **What moves:** `kind = 'page'` in markdown, by default. An article
  (`kind = 'article'`, or any page a `wiki_articles` row owns) moves only in
  a run over articles. A page in the trash is reported and left, since
  History takes no version of it; it moves on the run after it is put back.
- **Each page, in its write turn:** its queued save is written
  (`YjsState::save_page_now`); the row is read, the Y.Text of its state or
  `content` for a page nobody opened; it is converted and checked
  (`block_move::convert`, the one conversion every move makes) as its
  markdown editor showed it (`migrate::measure_as` with `Dialect::Page`:
  raw HTML and comments are the text they read as, `<u>…</u>` on a line is
  underline, each line of a paragraph stays a line, a link reference
  definition is the line it was and a link naming its label still goes
  there, a character reference or an escape is what was typed (`&amp;`,
  `\*`, a table's `\|` its pipe), a single `~` strikes nothing, and a page
  opening with `---`, a line and `---` opens with a rule and a heading, as
  the editor read no front matter; an ordered list keeps the number each
  item was written with (`2019.`, `2021.`); a run of spaces, a tab or a
  line's indent, a table row's cells past its header's, a code block's info
  past its language and a NUL (written as U+FFFD) are noted, and a backslash
  typed at a line's end stays, before the line break it ends; checked
  against the source too, every raw run of HTML, every backslash at a
  line's end and every address kept or noted, an email autolink's as
  `mailto:`);
  the
  page as it was is kept as "Before blocks" (`pages::keep_before_blocks`),
  a version never pruned and always listed; then the flip
  (`YjsState::move_to_tree`) and a version of the blocks, "Moved to
  blocks".
- **The flip** holds the write lock of the page's doc (loaded for it when
  nothing holds it), refuses a doc with typing the database lacks and a row
  written since the read (`updated_at`), and writes state, export, `format`
  and a fresh `tree_lineage` in one transaction (`yjs::move_row`). The
  table's `set_updated_at` trigger keeps `updated_at` on a change of format
  (migration 0051), so the page keeps its place in every list; the statement
  locks only its row, and waits at most 5 s (`lock_timeout`) for a write to
  it already in flight. The markdown doc is retired, evicted and
  its queued save dropped: its sockets close with 4409, a keystroke
  waiting on the lock is refused, and an append that had the doc in hand
  (`append_markdown`, which takes no write turn) is written to the page's
  blocks instead.
- **A page that fails stays markdown**, reported with the reason: markdown
  the converter refuses, a saved state that does not read, blocks that
  would lose a character of the text, a pending suggestion the blocks
  cannot hold, a page that changed between read and flip three times
  running, with what converting it last found. Nothing is written for it
  except the restore point of a flip that then failed, which the report names;
  the restore point cut for a flip that found the page changed is dropped
  (`pages::drop_before_blocks`), as the next attempt keeps the page as it
  then is. A run is idempotent and resumable: only markdown pages are read,
  and a restore point the latest version already holds is not cut again.
- **Suggestions:** pending CriticMarkup proposals become suggestion marks
  (`parse_markdown`, the owner's reading); comments and substitutions stay
  text, noted. A proposal holding a mention, an image or a line break is
  one no suggestion's marks can cover, which accept or reject would leave
  half done: it is kept as its text, noted (`critic_markup` in
  `migrate.rs`), and the page stays markdown, said, until its owner accepts
  or rejects it.
- **The report** is a JSON file per run and kind of page
  (`<started>-<run>-<pages|articles>.json`), every page the run read with
  its notes, text lost, export differences, suggestions carried, restore
  point and version, under `page-conversion/` in the state root
  (`data_disk::data_dir`, `/var/lib/virtues` on a box;
  `VIRTUES_PAGE_CONVERSION_DIR` overrides it, and a dev checkout uses
  `.page-conversion-state/`). The log has a line for each page with notes or
  a failure, and the run's counts.
- **After the move:** a markdown binding from an older app that knows block
  pages (it sends `contract=0`) meeting a tree is closed with 4409,
  "converted, open it again"; an app that sends no contract still gets
  4426. This app binds no markdown page, so it meets neither. The tree
  binding keeps this device's copy under `tree-<id>`, so a converted page
  never binds its `v2-<id>` copy (`lib/yjs/document.ts`). That copy can
  hold typing that never reached the server (the box out of reach, the app
  closed before it reconnected): the first time a device opens the page as
  blocks, it sends the copy (`lib/yjs/keptCopy.ts`, `POST
  /api/pages/:id/device-copy`), the server merges it over "Before blocks"
  and keeps what it adds as the owner's version, "Typed on a device before
  the move to blocks" (`pages::keep_device_copy`), the person is told, and
  the copy is deleted from the device. A version from
  before the move, put back, is converted and checked on the server as the
  move checks a page, its pending suggestions still suggestions, and
  History shows its notes, text or an address its blocks lost among them
  (`pages::restore_version`, `VersionHistoryPanel.svelte`). A version of an
  article put back is its owner's edit, as typing is
  (`wiki_articles.last_human_edit_at`), so its writers wait before
  revising it.

## Articles

Articles are the pages machines write: day pages (`api/day_article.rs`),
wiki articles, years, stories, chapters and the life page, each owned by a
`wiki_articles` row (`kind = 'article'`). They become block pages behind
`VIRTUES_TREE_ARTICLES=1`, with block pages on
(`block_move::tree_articles_enabled`). The switch gates the move only: an
article that holds blocks opens, edits and saves with it off. Migration 0051
dropped the constraint that kept articles markdown
(`app_pages_tree_is_a_user_page`).

**Their syntax is the contract's (contract 2).** Read only for an article
(`Dialect::Article`, `virtues_document::parse_article_markdown`): on a user
page, a paste or a page moved in slice 5, `[^1]` and `⟦` stay what was typed.

| Markdown | Node | Export |
|---|---|---|
| `[^ev-1]` after a sentence, `[^ev-1]: Message · 7:40 PM · table:id` at the end | `footnote`, an inline atom carrying its `label` and its definition's first paragraph as `note` (`<virtues-footnote label note>`) | `[^label]` where it stands, never escaped; after the last block one line per label in the order first cited, `[^label]: note`, so definitions always come last. A label carrying a second note (a sentence pasted from another day) writes it under the next label of its kind, `ev-2` beside `ev-6` as `ev-7` (`footnotes_as_exported`, `footnotesApart` in `markdown.ts`), so every sentence's source is written |
| a ```` ```figure ```` fence of `key: value` lines | `figure`, a block atom: `kind` (quote, thread, route, picture) and `text`, `who`, `ref`, `refs`, `src`, `caption`, `style`, `time` | the fence, one `key: value` a set attribute in contract order |
| `⟦…⟧` in one run of text | the `veil` mark, innermost | `⟦…⟧` inside every other delimiter, so `[⟦Nick⟧](/person/p)` and `**⟦the clinic⟧**` read back as they were |

A definition no sentence cites is left out, noted and counted, when its
label is one the day page lifts as a note (`ev-` or `cx-` and digits); any
other (`[^note]: …`), which the day page draws as a paragraph, stays that
paragraph's text where it stood. A reference with no definition is a
footnote with no note; a fence that is not a figure (an unknown key, a kind
out of the enum, four backticks) stays a code block and exports the same
fence; an unpaired bracket stays text. What the readers drew as text stays
text: a raw tag that is no HTML element and no tag of the contract
(`<unk>`, `Vec<String>`) and HTML whose words the page would lose, as a
paste reads them, while HTML the export writes (`<u>`, `<br>`, an applet)
is markup; and an opening `---`, a line and `---` is a rule and a heading,
no front matter left out.

**Writers keep writing markdown.** `YjsState::draft` converts a writer's
markdown for the page's document (as an article on an article), and
`write_draft` writes it: a Y.Text diff on a markdown page, block edits on a
block page (`virtues_document::revise_ops`: a block the markdown shows as
it stands is left alone, whatever it holds that markdown cannot show; one a
writer changed a little is written in place and keeps its id). Inside a
list, table or quote the writer changed, the same holds child by child
(`diff::revised`): a row or item it did not touch stays as it stands, a
cell's span and alignment included, one it reworded keeps its id and its
live text, and one it added is new, taking no id of the page's. Where
the page's rows and its markdown do not line up (a cell two rows tall
over the row the writer changed) the table is written as the markdown
reads, and stays a grid. A block is
compared as the page's export shows it, its footnotes' labels and notes as
the export writes them, and, where that differs from every block the
writer wrote, as that markdown reads back, so a run of spaces or a tab the
export cannot carry is no change; a block the export shows as nothing (an
empty paragraph) is left where it is, unless the write keeps no block of
the page. A writer reads the export, so it
carries the owner's pending suggestions back as CriticMarkup: a block that
shows one as the page has it keeps it, and any other suggestion in the
markdown is the writer's and comes in accepted
(`virtues_document::parse_draft`). A write that would replace or delete a
block holding the owner's pending suggestion is refused with nothing
written (`suggestions_taken`): the suggestion is theirs to accept or
reject, as the chat's page editor refuses (`proposal_problem`). What a
writer records as its edition (`wiki_articles.machine_text`) is the page's
export, and every "has the owner changed it?" compares both sides as this
server writes them (`block_move::as_compared`, a pending suggestion read
as one, never as decided), so a release that escapes differently does not
make every article its owner's. The wiki editor's side is in
[wiki-editor.md](wiki-editor.md).

**An article moves when it is first opened.** Having a document still means
someone opened it, so narration can write a day nobody has read. When the
server first builds an unopened article's document (`DocCache::get_or_create`,
for a socket, a `GET`, a chat or CLI read, or a machine write), it moves it
(`block_move::first_open`): converts and checks it, keeps it as it was
("Before blocks"), flips the row against the read with a fresh lineage and
the article's rebased edition in one transaction, and versions the blocks
("Moved to blocks"). Neither version is an edit in History's article feed
(`pages::is_move_version`). An article held back opens as markdown, and is
not converted again until it changes; one held back for the database or
the converter failing that once (`block_move::Unrebased::ForNow`) is tried
again at its next open.

**What holds an article back,** at first open and in a run:

- what holds a user page back: text or an address lost, a pending
  suggestion its blocks cannot hold. Its pending suggestions are its
  owner's and move as suggestion marks, counted in the report; one around
  a source, a mention, a line break or an image, which a suggestion's
  marks would cover only in part, holds it back
  (`ArticleCounts::suggestions_over_nodes`);
- its syntax read otherwise (`ArticleCounts::holds`): a reference that is
  no footnote, a definition that is neither a note nor an orphan, a figure
  that is neither a figure nor its fence, a definition running onto a
  second line (the day page reads its first line only) or a label defined
  twice;
- its edition: the writers' last edition does not convert, or a sentence of
  the owner's (`theirs`) has no place in the export (`block_move::rebase`,
  which otherwise moves the edition and the owner's sentences to the
  export's spelling in the flip's own transaction);
- a note beside one of its sentences (0046's anchor) that would lose its
  place (`block_move::anchors_lost`): its quote a whole sentence of a
  paragraph before and none after, split as the day page splits them
  (`block_move::day_sentences`) and read as it reads them
  (`block_move::plain_sentence`, checked against `dayNotes.plainSentence` by
  `lib/wiki/plain-sentences.json`): no case folded, a single `~` and `__`
  as the page draws them, so a move that redraws the sentence holds the
  article back. A ref the editor wrote either way (`[@Nick](/person/id)`,
  `![@Nick](/person/id)`) is one mention, which the export writes as the
  link: every reader draws both as the name, without the `@`.

**The run over articles** is `virtues pages convert --articles`
(`page_convert::Which::Articles`). It reads every markdown article, oldest
first, and moves the opened ones; an unopened one is measured and reported
"converts when opened". A dry run measures the trash too. Each article's
report carries `article`: where its edition went (`kept`, `rebased`,
`none`), whether it read as its writers left it before the move and after
(they must agree), the owner's sentences kept, the notes landing before and
after, its syntax counted, the first line the export changed, and what
changed it by kind (escapes, list markers, table rules, orphan notes, veil
placement, spaces, note order, line breaks, blank lines, the final newline,
other).

**The dry run on a copy of a box** (the only step that touches one): copy
it from its template, run `virtues pages convert --dry-run --articles
--json --texts <dir>` against the copy, which also writes each day page's
markdown, export and notes' anchors into `<dir>`, then run
`DAY_ARTICLE_TEXTS=<dir> pnpm vitest run src/lib/wiki/dryrun.readers.test.ts`,
which fails on any day page its readers read differently as its export,
naming files and kinds of reading, never words. Drop the copy and the
folder after; only counts leave them.

**The readers read the export.** `content` is the export once an article
moves, with definitions last, markers bare, fences exact and veils bare, as
the writers wrote them, and with what would read as markup escaped
(`R\&B`, `user\_name`, `\~20`) and a few characters as references
(`&#32;`, `&#124;`, `&#10214;`).

| Reader | Reads |
|---|---|
| the day page (`lib/wiki/dayArticle.ts`, DayInline, DayFigure, RecordCard) | sentences and their sources; `inlineMarkdown.tokenize` reads an escape as its character, a reference as the character it names and a backslash ending a line as the hard line break the export writes it for, and a sentence never ends on, or opens a link or a veil with, an escaped character; a ref's label without its `@`, in either form; a veiled phrase is looked for as the page draws it (`veilMarks`), its escapes read |
| notes beside a sentence (`dayNotes.placeNotes`) | the sentence's plain words, by `tokenize`'s rules |
| ledes (`lede.ts`'s `ledeSentence`), `recordWords` | escapes read |
| prompts (`day_memory::plain`, `day_article::earlier_page`, `day_picture`'s sections, candidates and veiled places) | escapes and hard line breaks read (`wiki_editor::unescape`); definitions last and fences exact, as the writers wrote them |
| wiki article views, tables, headings | Streamdown, which reads escapes; `Markdown.svelte` draws a ref's label without its `@`, in either form |
| search, embeddings, backlinks | `content`: a moved article re-indexes once |

`apps/web/src/lib/wiki/articleExport.test.ts` tests the day page's readers
against the crate's article corpus (`crates/virtues-document/tests/corpus/articles.json`):
every page reads the same as its export.

**Sharing an article** (`api/publish_page.rs`) leaves its sources on the
box, since their notes name records; a veiled phrase leaves as its words,
and an article's stray `⟦ ⟧` go too. A quote leaves as a quotation with
who said it and when, the speaker among the names the Share sheet lists
unless it is you or unnamed; a picture as its image and caption; a thread
or a route, which the box draws from its records, is left out and counted,
as is a figure fence that never converted. A markdown article is converted
in memory and shared by the same rules; one that will not convert is
shared as its prose, sources and figures stripped.

**A device's copy** of a block page is kept under its lineage
(`app_pages.tree_lineage`, written at every tree's creation; IndexedDB
`tree-<id>.<lineage>`, `lib/yjs/document.ts`), so a copy of an earlier
document of the page (put back from a backup and moved again) is never
merged in as a second copy: it is sent once (`keptCopy.ts`'s
`keepEarlierTreeCopies`) and kept as a version only when it holds typing
the page never had ("Typed on a device in an earlier copy of this page").

## The chat composer

The chat composer runs on the same kernel as a block page: Tiptap on
ProseMirror, with the contract's own node and mark specs
(`contractExtensions(extras, MESSAGE)`), but no Yjs and no ids. Its undo is
plain prosemirror-history. It holds part of the contract (`MESSAGE` in
`message.ts`): paragraphs, line breaks, bullet and numbered lists, a quote,
a code block, mentions, and bold, italic, strike, highlight, inline code and
links. Headings, tables, media, applets and to-dos are left out, so `# `
and `- [ ] ` stay the text they are.

**What is sent is markdown with the words as typed.** The thread shows a
user's message as plain text (`UserMessage.svelte`), and the model reads
what the person wrote, so `messageMarkdown` writes the page export's
structure (list markers, fences, `> `, mark delimiters, `[label](url)`, a
mention as `[@label](route)`) and never escapes text: `snake_case` and
`2 * 3 * 4` go as typed. A line break is a new line and paragraphs are a
blank line apart. A list, a quote or a fence directly under a paragraph,
and anything under a fence, is written on the next line, since markdown
reads them without a blank line. On send (`outgoingMessage`) the `@` comes
off a mention (`[Label](url)`, what the thread and the model have always
read), a trailing empty list item goes, and a fence typed as text and left
open is closed.

**A draft is that markdown** (`virtues.draft.<id>` in `localStorage`,
`new` for an unsent chat, none for a ghost chat; `chat/state/drafts.ts`). It
is read back by the page's converter as a paste (`readMessage`), and the
reading is used only when it writes back as the same text; otherwise the
draft opens as its text, line for line (`textBlocks`). So nothing a draft or
a prefill holds is changed by opening it. A composer that opens before the
converter has loaded shows the text and reads it again once the converter
is there, unless the person has typed by then.

**Keys.** Enter sends on a desktop and breaks the line on a phone, whose
send button sends. Shift+Enter breaks the line everywhere: in a list it
starts the next item (an item opening with `[ ] ` starts the next one with
it), on an empty item it ends the list, and on an empty last line of a
quote it ends the quote. Cmd+Enter (Ctrl+Enter) sends from anywhere,
including a code block. In a code block Enter and Shift+Enter are new lines
that keep the line's indent, Tab indents two spaces and Shift+Tab takes them
back; Enter on a line of three backticks (and a language) opens one. Tab
nests a list item and otherwise moves focus; Shift+Tab outdents a nested
item and otherwise cycles the chat's mode. Escape closes the ref picker,
then leaves the field. There are no slash commands.

**Typing.** A list's or a quote's sign typed at the start of a line, after
a line break, starts the list or quote there (`LineSigns`), as it does at
the start of a paragraph. Marks typed with their delimiters read them as
CommonMark reads emphasis (`flanking`): the opening one is followed by no
space and the closing one follows none, so `2 * 3 * 4` stays text. An
address typed is not made a link (`autolink` off); `[label](url)` typed is
one.

**`@`** opens the ref picker (`triggers.ts`, as in a page), and a pick
replaces the `@` and what was typed after it with a mention (a route) or a
link (an address elsewhere), drawn by the page's `MentionNode`. A space
follows it unless it is alone on its line.

**Paste.** Files are attachments, and so is text longer than 1,500
characters (`Pasted Text.txt`). In a code block the text goes in as it is.
Text that reads as markdown (a block's sign, or bold, a link or a highlight
in a line, and no line of code) is converted and placed as a page's paste is
(`placeConverted`), when the result holds only what a message does;
anything else, and a paste with Shift, goes in as its text, line for line.
Pasted HTML is read as its text. A copy out of the composer is what it would
send.

**The field.** The editor's height is reported to `ChatInput.svelte`,
which animates the box to it up to 200 px and scrolls past that. On a phone
the editable carries `enterkeyhint=enter`, autocorrect and sentence
capitalization on, and spellcheck off; dictation types into it like any
text field.

`composer.corpus.ts` holds typed messages and what is sent for each,
measured against the CodeMirror composer this one replaced on 2026-10-20
([record/editor-kernel-2026-10-20.md](../record/editor-kernel-2026-10-20.md));
`composer.test.ts` holds the composer to it, with its drafts, keys, refs and
pastes.

## Where things are

| File | What |
|---|---|
| `lib/document/editor.ts` | `createPageEditor`, `TreeAiDriver` |
| `lib/document/views.ts`, `nodeviews.svelte.ts` | the node views |
| `lib/document/commands.ts`, `triggers.ts` | the insert menu, `/` and `@` |
| `lib/document/find.ts`, `focus.ts`, `code.ts` | find, focus and typewriter, highlighting |
| `lib/document/paste.ts`, `media.ts`, `media-kind.ts`, `place.ts`, `links.ts`, `tables.ts` | paste, files given, what a link draws, a block put into text, links, tables |
| `lib/document/converter.ts`, `crates/virtues-document-wasm` | the markdown converter, in the page |
| `lib/document/uploads.ts`, `history.ts` | files on their way to the server; a paste or files as an undo step of their own |
| `lib/document/markdown.ts` | the markdown export, for text copied out and a chat message |
| `lib/document/composer.ts`, `message.ts` | the chat composer; a message's part of the contract, its markdown in and out |
| `lib/document/toolbar.ts`, `gesture.ts`, `resize.ts`, `print.ts` | the selection toolbar's manners, menus from the keyboard, size handles, printing |
| `lib/document/anchor.ts` | places that hold through every device's edits |
| `lib/document/suggestions.ts`, `presence.ts` | proposals, carets and the assistant |
| `lib/document/outline.ts`, `stats.ts`, `registry.ts` | outline, counts, open editors by page |
| `lib/document/document.css` | typography from the `--md-*` tokens, the A4 page view, print |
| `components/pages/DocumentEditor.svelte` | the surface and its menus |
| `components/pages/DocumentPreview.svelte` | a version's HTML, read only |
| `components/pages/MarkdownView.svelte` | a page's markdown, read only: a block page's View as markdown, and the whole of a page still in markdown |
| `components/views/pageDocument.ts` | how a page opens: a block page bound, a markdown page read only |
