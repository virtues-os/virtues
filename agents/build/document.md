# Block pages: the document contract and its editor

A block page's text is a Yjs XML tree under one contract,
[`crates/virtues-document/contract.json`](../../crates/virtues-document/contract.json).
The Rust crate reads it to check every write and render HTML and markdown;
the browser builds its Tiptap schema from the same file
([`apps/web/src/lib/document/schema.ts`](../../apps/web/src/lib/document/schema.ts)).
The decision is [record/editor-kernel-2026-10-07.md](../record/editor-kernel-2026-10-07.md);
the work still to do is [plan/document-contract-plan.md](../plan/document-contract-plan.md).

## Two editors, for now

`app_pages.format` says which editor a page gets.

| Format | Document | Editor | Binding |
|---|---|---|---|
| `markdown` | markdown in a Y.Text (`content`) | CodeMirror (`components/pages/CodeMirrorEditor.svelte`, [codemirror.md](codemirror.md)) | `createYjsDocument` (`lib/yjs/document.ts`), contract 0, IndexedDB `v2-<id>` |
| `tree` | XML tree in the `doc` fragment, stamped `meta.contract` | Tiptap (`components/pages/DocumentEditor.svelte`) | `createTreeDocument`, contract 1, IndexedDB `tree-<id>` |

Both bindings sit on `lib/yjs/socket.ts`: the contract on the socket, the
server's 4426 and 4422 close codes, and the rule that the server's sync is
authoritative while this device's copy stands in only when the binding may
read it. Until every page is a tree, a change to one editor's behaviour is
checked against the other's ([Parity with the CodeMirror editor](#parity-with-the-codemirror-editor)).

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
- **Decorations, not nodes, for anything transient**: find matches,
  focus dimming, Shiki colours (`code.ts`), an upload or a paste waiting on
  the server, the assistant's caret, trail and flash (`presence.ts`). None
  reaches the shared document or another device.
- **A place held across a wait is an anchor** (`anchor.ts`), never a
  number: the link panel and menu, the applet list, the mention picker, the
  file dialog, an upload, a paste waiting on the server, the inline writer.
  Another device's edit reaches the editor as one step over the whole
  document, which collapses a mapped position, so a decoration placed by
  position is placed again from its anchor on such a step (`media.ts`,
  `paste.ts`), and an open `/` or `@` is placed again from the caret, which
  the binding keeps as one (`triggers.ts`). What acts on a link or a
  selection checks it still reads as it did, and leaves it alone if not.
- **What the binding drops, the editor keeps** (`KeptThroughOthersEdits` in
  `anchor.ts`). The binding places the selection again as text or a node
  only, and its one step clears the marks waiting to be typed: selected
  table cells and a gap cursor are placed again from anchors taken before
  the edit, and a mark turned on at the caret stays on.
- **Undo calls off what waits first** (`callOffNewestWaiting` in
  `editor.ts`): a paste or an upload waiting on the server is no edit, so
  y-undo would pass over it to the person's writing, and its landing would
  clear the redo that could bring that writing back.
- **What a link to a file draws** (an image, audio, video or a file card) is
  `kindOfLink` (`media-kind.ts`) in both editors, the ref card, the file
  view and a file ref's icon, and the server's converter reads the same
  cases (`media-kinds.json`).
- **Markdown goes through the server's converter** (`POST
  /api/pages/convert`, `convertMarkdown` in `api/client.ts`): a paste
  (`paste.ts`) and the inline writer. There is no second markdown parser in
  TypeScript. It takes suggestions in as accepted (`parse_written_markdown`):
  a suggestion is made in the page, never brought into it. A paste is read
  as one (`convertPastedMarkdown`, `parse_pasted_markdown`): a raw tag that
  is no HTML (`<Login/>`, `Vec<String>`) and CriticMarkup stay text. Only a
  block's sign at the start of a line (a heading, a list, a quote, a fence,
  a table row) makes pasted plain text markdown; the words of a line are
  formatted by the editor's own paste rules, which read emphasis as
  CommonMark does (`a == b` and `2 * 3 * 4` stay text) and run on plain
  text only, never in inline code. A code editor's HTML (VS
  Code's syntax colours) is pasted as its text. Word's lists, paragraphs
  marked `mso-list` with their bullet as text, are made lists
  (`wordLists`). A drop is read as the same data pasted: markdown text
  dropped is converted, and the plain-text rules never run on dropped
  HTML, nor any rule on a block dragged from another page.
- **Markdown is written in TypeScript** (`markdown.ts`), only for text
  copied out of a page (`clipboardTextSerializer`): the crate's
  `render::markdown` rule for rule, held to the same bytes by the
  conformance corpus.
- **A paste lands as whole blocks** (`placeConverted` in `paste.ts`): one
  paragraph joins the text at the caret; anything else splits the
  paragraph there and steps out of the lists around it, so a pasted heading
  never merges into the line it was pasted on.

## The model's edits

- **A table's rows carry ids** (`tableRow` in the contract): a row is added,
  replaced or deleted by its id, and a table too long for one read opens by
  its rows as a list opens by its items. HTML for a row (`<tr>`) is read in
  table context (`ingest.rs`), and a table part written where a block goes
  is refused, as HTML's parser would drop its tags. A row edit that leaves
  the table no grid is refused.
- **A read's base holds what the read showed.** A block a read opened (a
  list or a table too long to show whole) is in the base with the blocks
  inside it that the read listed; replacing or deleting it, or a block
  around it, waits until a read has shown all of them (`partly_read` in
  `page_editor.rs`); so does deleting it beside a write in the same batch,
  a rewrite of it (`rewritten_in_part`). Opened again, it keeps the blocks inside it an
  earlier read showed as that read showed them, never as the page has
  them now, so a refusal over one that changed since says where reading on
  inside it starts (`changed_in_part`), not to read it by id. A block a
  read could show but a window had no room for (the chat's view of an open
  page) is said to be read, and is not in the base unread; only a block too
  long for any read is, to delete or replace whole.
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
  `server/yjs.rs`): a client holding edits that follow ones nobody has
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

## Parity with the CodeMirror editor

What a markdown page's editor does, and what a block page does instead.

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
| Ask AI, a code block included | Kept: `treeAiSession.ts`; the words a rewrite will replace are washed while the model thinks (`setAiTelegraph`); in a code block it replaces the selection, across a code block's edge it is not offered |
| Copy: a selection is its markdown; a range of table cells is tab-separated text | Kept: `clipboardText` in `markdown.ts`; a copy inside one list item is its blocks |
| The platform's undo (shake, three fingers, Edit > Undo) | Kept: `editor.ts` routes `historyUndo` to y-undo, calling off a paste or an upload that waits first, as ⌘Z does |
| Focus and typewriter | Kept: `focus.ts` |
| Long-press menus on iOS | Kept: `gesture.ts`, `onContextGesture`; Shift-F10 opens the same menus from the keyboard, on their first row, on a page that is read only the focused link's or widget's; the lift after a hold never closes the menu (`swallowLift`); while one is open every key that would edit the page is the menu's (`contextMenu/keys.ts`) |
| Media widgets, image width, media paste | Kept: `MediaNode`, `media.ts`; the width and an applet's height are sliders the arrows move; files pasted over a selection replace it as they land, over selected cells every cell empties and they land in the first |
| Refs and their hover preview | Kept: `MentionNode` |
| Raw markdown editing (`pageDisplay.rawMode`, `render-mode.ts`) | **Replaced** by the read-only View as markdown (`MarkdownView.svelte`); markdown is not edited as text on a block page |
| CriticMarkup comments (`{>>note<<}`) and Resolve comment (`review-marks.ts`) | **Cut**: the contract has no comment mark |
| The drawn caret (`caret.ts`): sized to the font, gliding on keyboard moves, a soft blink once still | **Cut**: the browser's own caret, in the page's ink (`caret-color`) |
| Live preview, render mode, widget height, mouse freeze, list renumber, empty-mark tidy | **Cut**: no meaning on a tree; ProseMirror renders the tree and numbers ordered lists itself |

## Page checkboxes

Task lists are page content: `taskList` and `taskItem` in the contract,
Tiptap's `TaskItem` view, and the To-do command in the insert menu, which
`/task` (and `todo`, `checkbox`, `check`) finds as it does in the CodeMirror
editor; typed `- [ ] ` makes one too. Checking a box writes the item's
`checked`, and leaves the focus where it was (`QuietTaskItem` in
`schema.ts`): the stock box focused the editor, which raises a phone's
keyboard. Keys pressed on a focused box are the box's, never edits where
the caret is, and ⌘Enter ticks the to-do the caret is in (`TodoKeys` in
`commands.ts`).

## Where things are

| File | What |
|---|---|
| `lib/document/editor.ts` | `createPageEditor`, `TreeAiDriver` |
| `lib/document/views.ts`, `nodeviews.svelte.ts` | the node views |
| `lib/document/commands.ts`, `triggers.ts` | the insert menu, `/` and `@` |
| `lib/document/find.ts`, `focus.ts`, `code.ts` | find, focus and typewriter, highlighting |
| `lib/document/paste.ts`, `media.ts`, `media-kind.ts`, `place.ts`, `links.ts`, `tables.ts` | paste, uploads, what a link draws, a block put into text, links, tables |
| `lib/document/markdown.ts` | the markdown export, for text copied out |
| `lib/document/toolbar.ts`, `gesture.ts`, `resize.ts`, `print.ts` | the selection toolbar's manners, menus from the keyboard, size handles, printing |
| `lib/document/anchor.ts` | places that hold through every device's edits |
| `lib/document/suggestions.ts`, `presence.ts` | proposals, carets and the assistant |
| `lib/document/outline.ts`, `stats.ts`, `registry.ts` | outline, counts, open editors by page |
| `lib/document/document.css` | typography from the `--md-*` tokens, the A4 page view, print |
| `components/pages/DocumentEditor.svelte` | the surface and its menus |
| `components/pages/DocumentPreview.svelte` | a version's HTML, read only |
| `components/pages/MarkdownView.svelte` | a block page's markdown, read only (View as markdown) |
