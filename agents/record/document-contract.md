# The document contract, as built

Pages moved from a markdown string in CodeMirror to a Yjs XML tree edited
with Tiptap's MIT core, under one contract file the browser and the Rust
server both read. Decided in
[editor-kernel-2026-10-07.md](editor-kernel-2026-10-07.md), measured in
[document-contract-spike-2026-10-07.md](document-contract-spike-2026-10-07.md),
built in eight slices from 2026-10-07 to 2026-10-13. How it works now is
[build/document.md](../build/document.md); this is what was built, in what
order, and what was left.

## What shipped, by slice

| Slice | What |
|---|---|
| 0, foundations | `yrs` 0.18 to 0.28 and `y-sync` dropped; `crates/virtues-document` (contract, ingest, tree, ops, render, converter) and the conformance corpus both sides read; the contract on the page socket (`?contract=N`, 4426 below a page's contract, 4422 for an update outside it, checked before it is applied or relayed); `API_VERSION` 2 |
| 1, the server handles both formats | `app_pages.format` (migration 0048); every writer of markdown converts it for a block page; History cut and restored on the server; publishing a block page; `API_VERSION` 3 |
| 2, the editor | `DocumentEditor.svelte` on `lib/document/`, with every CodeMirror feature kept, replaced or cut by name (the table below); markdown converts in the page (the crate compiled to WebAssembly); a file's block lands at its final address and its bytes follow (`API_VERSION` 4) |
| 3, the model's tools | `get_page_content` as canonical HTML with block ids and a read base; `edit_page` as block ops judged against what reads showed (`check_ops`); durable read bases (`app_page_read_bases`) |
| 4, widgets | mentions, media, headings 1 to 6, table alignment, callouts, applets |
| 5, user pages | `virtues pages convert`, the startup switch, "Before blocks" and "Moved to blocks", a device's `v2-<id>` copy sent once |
| 6, articles | contract 2 (`footnote`, `figure`, `veil`); the writers' door (`YjsState::draft`, `revise_ops`); an article moves at first open; migration 0051 |
| 7, the old editor deleted | below |

## Slice 7, on the integration branch (2026-10-13)

Deleted from the app: the CodeMirror page editor (`CodeMirrorEditor.svelte`,
`codemirror/editor.ts`, `outline.ts`, `widget-height.ts`, `theme.css`, and
the extensions only it reached: live preview, media widgets and paste,
tables, checkboxes, code blocks, Shiki, slash commands, the selection
toolbar, render mode, focus mode, review marks, the AI cursor and parse
progress), the inline writer that streamed into a Y.Text
(`ai/aiCursorSession.ts`), the markdown binding (`createYjsDocument`), the
browser's own markdown versions (`saveVersion`, `restoreVersion`), the chat's
copy of a bound page's text, the raw-markdown toggle, and the
`y-codemirror.next` and `@codemirror/search` dependencies. About 7,000
lines. What the chat composer reaches stayed: caret, keybindings, list
renumber, mouse freeze, ref links, the ref picker, inline marks, empty-mark
tidy, code context, the highlight parser, long press, the language and the
theme ([build/codemirror.md](../build/codemirror.md)).

What took their places:

- A page still in markdown opens read only, as its server's markdown, with
  "This page is still in markdown, so it opens read only. You can edit it
  here after your server moves it to blocks." It binds no socket and keeps
  no copy on the device. History cuts and previews its versions on the
  server and offers no Restore, which the server refuses on a markdown page.
- Copy markdown reads the server's copy for either format.
- The chat binds a page by its id; the server reads it for the model.
- A chat edit's animation flashes the blocks it wrote, and only on a block
  page.

`MIN_BOX_API` rose from 0 to 4: an app refuses a box below API version 4
and shows "Your server needs an update". 4 is the first `API_VERSION` any
box reports with contract 2: on the shared `wave` the media-upload change
(4) and contract 2 landed in one commit (`f6167306`), and no release
carried 4 before it, so the row for 4 in `api_version.rs` now names
contract 2 too.

The release note it needs, for the stable release that carries it: "This
version of the app edits pages as blocks. Update your server first: on a
server older than this release, the app asks you to update before it
opens. A page your server hasn't moved to blocks yet opens read only."

### What slice 7 did not do

The plan's slice 7 also deleted the server's Y.Text page paths, the
find/replace form of `edit_page` and of `virtues page edit`, and
`format = 'markdown'`. None of that was done, because markdown pages still
exist on every box:

- New pages are created as markdown unless `VIRTUES_TREE_PAGES=1`, and user
  pages and articles move only with `VIRTUES_CONVERT_PAGES=1`,
  `VIRTUES_CONVERT_ARTICLES=1` and `VIRTUES_TREE_ARTICLES=1`. The plan turns
  these on at its gate, after the device checks below and a measurement of
  the model's edits, neither of which has run. The defaults were left off.
- A page whose move would lose text, or that holds a suggestion its blocks
  cannot, stays markdown by design, as does an article nobody has opened.
- Older apps edit markdown pages through the Y.Text binding, and the model,
  the CLI and the writers edit them through find/replace.

So the app as built here cannot edit a page on a box with the defaults: the
branch is not shippable until the flags default on, or markdown pages move
when opened.

## Left open

- **The gate.** A physical iPhone in the Tauri web view (dictation,
  selection handles, IME composition while the model edits the same block,
  the keyboard accessory, a widget's controls by touch); Android if its app
  edits pages; two devices on one page with the model writing; a large page
  (thousands of blocks, a dozen applets) on an iPhone; the converter on the
  reference iPhone (a 256 KiB paste under 300 ms, loading under every
  content security policy, a first open over the relay with nothing
  cached); a file uploaded from the phone while the laptop watches, and the
  phone's app closed mid-upload. Then the model measurement: fixed edit
  tasks on real pages against the production chat models and the applet
  runner's, counting refusals, recoveries and wrong edits.
- **The server's markdown paths,** once every page is a tree: the Y.Text
  paths in `server/yjs.rs`, find/replace in `edit_page` and `virtues page
  edit`, `format = 'markdown'`, the `snapshot` a version request may carry,
  and the 4409 close an older app's markdown binding gets.
- **Pages left in markdown:** whether a user page moves when opened, as an
  article does, and what happens to one that cannot move.
- The converter's `regex` is a quarter of its WebAssembly module; a small
  content-match automaton would save about 85 KB gzip.
- An upload's state is held in memory and lost on a restart; a table
  (`app_media_uploads`) would let any device finish one.
- A table row too long for any read is listed whole; an edit that takes
  the blocks in its cells is refused until they are read by id.
- `revise_ops`'s similarity threshold (0.5) is unmeasured.
- `escape_md` escapes more than it must (14 of 126 articles on the measured
  box).
- Ten day pages hold rows of pipes with no rule row; whatever wrote them
  should write the rule row.
- A page that moves when read moves on a read; serving a converted copy
  without moving would take a second path.

## Parity with the CodeMirror editor

What a markdown page's editor did, and what a block page does instead.

| CodeMirror page | Block page |
|---|---|
| Task lists, `/task`, typed `- [ ]` | Kept: `TaskItem`, the To-do command; typed `- [ ] ` or `- [x] ` makes a to-do (`commands.ts`); Enter after a checked item starts an unchecked one; ticking a box writes the tick without taking the keyboard; ⌘Enter ticks the to-do at the caret (`TodoKeys`), and keys on a focused box are the box's |
| Slash commands | Kept: `commands.ts`, Heading 1 first, so `/` and Enter make one as there; with Text after the headings, Indent and Outdent; with no command matching, Enter, Tab and the arrows are the editor's (`menuKey`) |
| Nesting a list item with spaces before its `-`, ⌘] and ⌘[ | Kept: Tab, ⌘] and ⌘[ (`BlockMoves` in `commands.ts`, taken even where there is no item, as ⌘[ is the browser's Back), and on a phone two spaces at an item's start or `/indent`; in a code block ⌘] and ⌘[ indent and outdent the lines the selection touches (`indentCode`) |
| Find with replace, replace all, match case, regexp, whole word | Kept: `find.ts`, `FindBar` |
| The selected words echoed where else they occur (`highlightSelectionMatches`) | Kept: `selectionEchoes` in `find.ts` |
| Word and link counts | Kept: `treeStats` |
| Code blocks: language, Copy, Shiki; Enter keeps the line's indent | Kept: `CodeBlockNode`, `code.ts`, coloured in the page's theme; any language typed after the picker's Other…; Enter in `schema.ts` |
| Table menus: add, delete, align, drag rows and columns | Kept: `tables.ts`, `TableMenu`; rows and columns move by Move up, down, left and right, behind More |
| A row of cells typed, then its delimiter row (`\| a \| b \|`, `\| --- \| ---: \|`), is a table | Kept: the input rule in `commands.ts`, once the delimiter row's cells match the row above; its colons align the columns, and the caret goes into a new row |
| Enter in a table moves to the cell below; a wide table scrolls inside itself | Kept: `TableKeys` in `tables.ts` (Shift-Enter breaks the line in a cell); `.tableWrapper` scrolls in `document.css` |
| Backspace at a heading's start makes it text; ⌘-backtick for code, ⌘⇧X for strikethrough | Kept: `HeadingBackspace` in `commands.ts`; the marks' own keys in `schema.ts` |
| Backspace or Delete beside a widget or a code block never takes it in one key | Kept: `BlockEdges` in `commands.ts` selects a widget first and moves the caret into a code block, never joining it to text, on every key Tiptap joins with (⌥⌫, ⌘⌫, Ctrl-Backspace, ⌃H, ⌥⌦, ⌃D…); an empty line beside either goes, the widget then selected |
| ⌥↑ ⌥↓ move a line, ⇧⌥↑ ⇧⌥↓ copy it, ⌘⇧K deletes it | Kept: `BlockMoves` in `commands.ts`, on the caret's list item or block or every one a selection touches, a table's row in a table; ⌘Enter is Tiptap's, leaving a code block |
| Typed `[label](url)` | Kept: the link's markdown input and paste rules |
| Typed `![name](url)` draws what its address is: an image, audio, a video, a file | Kept: the input rule in `schema.ts`, by `kindOfLink`; `![name\|600](url)` is an image's width (`altWidth`) |
| Link menu, hover preview of a ref and of a link elsewhere | Kept: `links.ts`; on a read-only page a tap opens a link; ⌘ or Ctrl-click off a link is a plain click, never a selection of the whole block |
| Selection toolbar: never during a drag, a keyboard selection waits | Kept: `toolbar.ts` |
| Ask AI, a code block included | Kept: `treeAiSession.ts`; the words a rewrite will replace are washed while the model thinks (`setAiTelegraph`); in a code block it replaces the selection, across a code block's edge it is not offered; on an article the model reads each source as its marker, its note after the context, and a veiled phrase inside `⟦ ⟧`, a rewrite writes the words replacing a sourced sentence before that sentence's source (which no suggestion can hold, so it stays), leaves out a marker naming any other, and writes `⟦…⟧` veiled |
| Copy: a selection is its markdown; a range of table cells is tab-separated text | Kept: `clipboardText` in `markdown.ts`; a copy inside one list item is its blocks |
| The platform's undo (shake, three fingers, Edit > Undo) | Kept: `editor.ts` routes `historyUndo` to y-undo, as ⌘Z does |
| Focus and typewriter | Kept: `focus.ts` |
| Long-press menus on iOS | Kept: `gesture.ts`, `onContextGesture`; Shift-F10 opens the same menus from the keyboard, on their first row, on a page that is read only the focused link's or widget's; the lift after a hold never closes the menu (`swallowLift`); while one is open every key that would edit the page is the menu's (`contextMenu/keys.ts`) |
| Media widgets, image width, media paste | Kept: `MediaNode`, `media.ts`, `uploads.ts`; the width and an applet's height are sliders the arrows move; files pasted over a selection replace it at once, over selected cells every cell empties and they land in the first |
| Refs and their hover preview | Kept: `MentionNode` |
| Raw markdown editing (`pageDisplay.rawMode`, `render-mode.ts`) | **Replaced** by the read-only View as markdown (`MarkdownView.svelte`); markdown is not edited as text on a block page |
| CriticMarkup comments (`{>>note<<}`) and Resolve comment (`review-marks.ts`) | **Cut**: the contract has no comment mark |
| The drawn caret (`caret.ts`): sized to the font, gliding on keyboard moves, a soft blink once still | **Cut**: the browser's own caret, in the page's ink (`caret-color`) |
| Live preview, render mode, widget height, mouse freeze, list renumber, empty-mark tidy | **Cut**: no meaning on a tree; ProseMirror renders the tree, and the browser numbers a list itself, an item carrying its own number by that ([A numbered item keeps its number](../build/document.md#rules-the-editor-keeps)) |
