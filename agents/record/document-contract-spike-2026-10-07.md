# Document contract spike: a Yjs tree, ProseMirror, and HTML at the model's edge

**Measured 2026-10-07.** Can a page be a Yjs tree edited with ProseMirror,
with HTML as the language the model reads and writes, without the silent
losses that ruled ProseMirror out on 2026-09-14
([editor-kernel-2026-09-14.md](editor-kernel-2026-09-14.md))? For the model's
path, measured here: yes. This record is evidence, not a decision. The
2026-09-14 decision stands until a decision record replaces it.

## Why it was reopened

The 2026-09-14 spike measured ProseMirror with **markdown as the stored
document**: parse on open, serialize on save. Reformat-on-open was the
disqualifier, and the record asked for new evidence before reopening.

Two things are new. The requirements grew: pages with many widgets (applets),
documents in the Notion or Google Docs sense, A4 output, multiplayer. And the
pain is now on CodeMirror's side. Every block widget there is a foreign
object the editor measures and routes the caret around: margin on a widget
root teleported the caret to position 0, late-loading images broke arrow
keys, entity cards were removed for cursor fragility, width-breaking was
shelved. None of those is a bug list; it is the shape of a text editor
asked to hold components.

The design tested here was not tested on 2026-09-14: **the tree is the stored
document**. Nothing converts on open. HTML is produced from the tree for the
model to read, and parsed into the tree only when the model writes, through a
parser that refuses what it cannot represent instead of dropping it.

## What was built

A scratch build, not in this tree, sized to answer the open questions:

- **One contract file** (`contract/document.json`): 21 node types and 7
  marks, each with its content expression, attributes and their types, and
  the HTML it reads from and renders to. Widgets are nodes like any other:
  `<virtues-applet ref="…">` (block), `<virtues-mention to="…" label="…">`
  (inline), `<aside data-tone="…">` (a callout that holds real blocks).
- **Rust server**, reading that file at build time: content expressions
  compiled to regexes; a strict HTML ingest; a writer and reader for the Yjs
  XML tree in the exact shape Tiptap's binding uses; block-id edit ops;
  canonical HTML and markdown export; a migration converter for our markdown
  dialect; a y-websocket server with the same framing as
  `virtues-core/src/server/yjs.rs`. yrs 0.28.
- **Browser editor**: Tiptap 3.31 core (MIT) with its schema built from the
  same file; Svelte 5 components as node views for the two widgets;
  collaboration, remote carets and block ids (`@tiptap/extension-unique-id`,
  MIT since v3); an A4 page view and print stylesheet.

## Results

### Two implementations of one contract agree

70 hand-written HTML cases: everyday model output, whitespace and mark
nesting, lists, tables, code, widgets, and 19 inputs outside the contract.

| Check | Result |
|---|---|
| Canonical HTML is a fixed point (Rust renders it; ProseMirror parses and re-renders it byte-identically) | 51/51 |
| Tree Rust writes to Yjs reads back identically in Tiptap's binding | 51/51 |
| Tree Tiptap writes to Yjs reads back identically in Rust | 51/51 |
| Raw (non-canonical) input parses to the same tree on both sides | 47/51 |

The canonical fixed point is the guarantee that matters: the model reads a
block, writes it back unchanged, and nothing moves. The four raw-input
differences are ProseMirror's parser restructuring input it should not: a
list item that opens with a sublist loses its nesting, an image lifted out of
a paragraph leaves an empty paragraph, a space survives a lift. The fourth
was the test DOM (happy-dom) keeping the newline after `<pre>`, which a real
browser drops. These only matter for HTML pasted in the browser; the model's
writes are parsed in Rust alone.

### What a refusal replaces

Rust refused all 19 out-of-contract inputs with a message naming the tag or
attribute. ProseMirror's parser accepted all 19 and changed each silently:

| Input | ProseMirror, silently | Rust |
|---|---|---|
| `<virtues-mention …/> about the trip.` | the rest of the sentence vanishes into the widget | refused: a custom element cannot self-close in HTML |
| `<virtues-applet …/><p>Next paragraph.</p>` | the next paragraph vanishes | same |
| `<h4>` | a paragraph | refused: not in the contract |
| `<time datetime="…">Tuesday</time>` | the text, attribute gone | refused |
| `<img alt="…">` with no `src` | an image node with no source | refused: needs `src` |
| `<li>` without `data-type` in a task list | an empty task plus a separate bullet list | refused: `taskList` holds `taskItem+` |
| `<a href="javascript:…">` | the link dropped | refused: script URL |

The self-closing case is worth knowing on its own: HTML has no self-closing
custom elements, so `<x/>` opens an element that swallows what follows. A
model will write it.

### Migrating existing markdown

Our dialect (`==highlight==`, `<u>`, `[@Label](/person/id)`, `![alt|600]`,
task lists) maps onto the contract, renders with pulldown-cmark, and ingests
in migration mode, which unwraps what it cannot represent and reports each
step. Text preservation is checked on every non-whitespace character.

| Corpus | Converted | Unchanged | With reported notes | Characters lost |
|---|---|---|---|---|
| 179 pages, a local copy of a real box | 179 | 176 | 3 | 0 |
| 132 repo docs (`docs/`, `agents/`) | 132 | 115 | 17 | 0 |

The notes are the contract's gaps: table column alignment (one page, 16
cells; two docs), `####` and deeper headings, front matter (docs only), and
angle-bracket placeholders like `<boxhash>` that markdown already renders as
tags. Audio, video and file embeds have no widget in this contract; none
occurred in the page corpus, but the CodeMirror editor supports them today.

Bold inline code (`` **`x`** ``) occurs 333 times in 76 docs. Tiptap's code
mark excludes every other mark by default, so stock Tiptap would strip one of
the two; the contract overrides it (`"excludes": "code"`).

The markdown export, read back through the same converter, gives the
identical tree for 176/179 pages and 125/132 docs. The misses: whitespace
that moves across a mark edge (6), and four list-shape edge cases. The
export is a one-way projection for search, embeddings and the model's
reading, so this is a quality bar, not a fidelity requirement.

### Writing while someone else is typing

- The model edits by block id. A replace of a text block with one of the
  same type is applied as a text diff inside it, so a person's caret
  elsewhere in that block holds. A block that holds an inline widget is
  swapped whole instead; patching around the widget is not built.
- A read returns a version. A write that names it is merged per block
  against what has changed since: edits to different words both land; edits
  to the same word are refused with the block's current text. Verified with
  two browser tabs: the model changed a word based on an older read while the
  other tab appended a sentence to the same paragraph; both survived in both
  tabs.
- Without a version, a block replace overwrites the block. Today's
  find/replace gets optimistic concurrency for free, because the text it
  looks for must still be there. A tool built on block ids must require the
  version to keep that property.

### A new hazard: an old client deletes what it cannot read

Measured (`skew.test.ts`): an editor whose schema lacks a node type, opening
a document that holds one, **deletes that element from the shared document**,
for every client. Tiptap's binding cannot build the node and its error path
removes the Yjs element. Phones carry a bundled web app that updates on its
own schedule, so a widget added on the server would be destroyed by the
first stale phone that opens the page. The spike stamps the contract version
into the document and the editor refuses to bind a newer one. The other
direction, a newer client writing a node type an older server does not know,
is not handled: the server's markdown export would skip it.

### Devices and print

- Desktop Chromium: typing, formatting, tables, task lists, a callout holding
  a list, widget controls writing their attributes through Yjs, two users
  with carets.
- iOS 26.5 simulator, Safari: typing; Return inside a task item makes a new
  one with fresh ids; a widget's button by touch, synced. Not tested: a
  physical phone, dictation, selection handles, the Tauri app's web view.
- A4: the page view is a 210 mm sheet; printing uses `@page { size: A4 }`.
  The demo printed to two pages without splitting a table, image or callout.
  A widget needs a print rendering: a bar chart built from `div`s printed
  blank in Chromium; the same chart as SVG printed.

### Size

Server, excluding tests: contract interpreter 360 lines, ingest 617, Yjs tree
236, edit ops and merge 512, HTML and markdown export 391, tree model 74;
plus the websocket server 367 and migration 389. Browser: schema builder 276
(generic over the contract), editor setup 45, node-view adapter 56, two
widgets 121. Adding a widget is one contract entry, one Svelte component and
one markdown export arm. For comparison, the CodeMirror extensions under
`apps/web/src/lib/codemirror/extensions/` are 7,091 lines including tests.

## What adopting it would cost (not measured)

- **yrs 0.18 → 0.28 in the core.** 0.18 stores XML attributes as strings
  only. Measured: a heading the server writes with level `"2"` matches no
  render rule and the editor throws drawing it.
- **About 11 server files write page text** through the Yjs text paths
  (`tools/page_editor.rs`, applets, chat, console, scheduler, webhook, wiki,
  the tool executor). They move to block ops, or keep producing markdown
  through the migration converter, which reports what it changed.
- **About 18 server files read `app_pages.content`** as markdown (search,
  indexer, wiki, day article, publishing, `sql_query`). They need nothing if
  `content` is the markdown export, materialized on save as today.
- **The model's page tool changes shape:** read returns HTML with block ids
  and a version; write takes ops. How often the production models trip the
  contract is unmeasured.
- **Paste.** The parse rules come from the contract alone, so formatting that
  Google Docs or Word express as inline styles is not recognized. That needs
  browser-only paste rules.
- **Unknown nodes on the server**, text diffs around inline widgets, and a
  one-time reported migration of every page.
- **The chat composer stays** on CodeMirror and markdown. Chat is model
  output, and the composer works.

## Reproducing

The spike lived in a session scratch directory and is not kept here. Its
shape, so it can be rebuilt:

```
contract/document.json        the contract
server/   cargo test           20 unit tests
          doc-spike batch       cases → tree, canonical HTML, markdown, Yjs update
          doc-spike migrate json pages.json | migrate files docs agents
          doc-spike serve 7341  y-websocket + /api/docs/:id, /ops, /seed
web/      vitest run            conformance (70 cases), version skew, string attrs
          vite                  the editor, proxied to the server
```

## Corrections (2026-10-07, same day)

A read-only audit of this record against the code found these statements
wrong as written. The measurements of the spike itself are unaffected.

- **"About 11 server files write page text."** Most of those only pass a
  `YjsState` handle along. The writers are `tools/page_editor.rs`
  (`apply_text_edit`), `api/wiki_editor.rs` (`replace_text`,
  `apply_text_diff`), `api/day_summary.rs` (`replace_text`),
  `YjsState::append_markdown` behind `POST /api/pages/:id/append` (the PDF
  pane, writing user pages), and `api/pages.rs`'s version snapshots. Others
  write `content` without Yjs: `api::pages::create_page` (the model's
  `create_page`, the CLI, an applet), `PUT /api/pages/:id`, and `edit_page`'s
  fallback.
- **"About 18 server files read `app_pages.content`; they need nothing."**
  Publishing does not read `content`: `api/publish_page.rs` decodes
  `yjs_state` as Y.Text and would publish a tree page blank. `pins` reads
  titles, not `content`.
- **"Today's find/replace gets optimistic concurrency for free."** Except
  an empty `find`, which replaces the whole page with no check.
- **"Bold inline code occurs 333 times in 76 docs."** That is the
  converter's count of bold-and-code clash notes across the working tree's
  `docs/` and `agents/` at the time, not a count of the `` **`x`** `` form,
  which appears 71 times in 28 tracked docs.
