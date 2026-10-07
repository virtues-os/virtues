# Document contract: pages on a Yjs tree

Pages move from a markdown string in CodeMirror to a Yjs XML tree edited with
Tiptap's MIT core, under one contract file that the browser and the Rust
server both read. Decided in
[record/editor-kernel-2026-10-07.md](../record/editor-kernel-2026-10-07.md);
measured in [record/document-contract-spike-2026-10-07.md](../record/document-contract-spike-2026-10-07.md).
The chat composer is not part of this and stays on CodeMirror.

**Death condition:** every page is a tree page, the CodeMirror page editor and
the Y.Text page paths are deleted, and a record describes what shipped.

Revised 2026-10-07 after three read-only audits of the first draft (claims
against the code, conflicts with other plans, coverage of what pages do
today). The first draft had five false claims and missed most of the server
and browser code that touches page text; this version is written against
what those audits found.

## What done looks like

- **`crates/virtues-document`** owns the contract: `contract.json` plus the
  Rust that interprets it (content expressions, strict HTML ingest, the Yjs
  tree in the shape `@tiptap/y-tiptap` reads, block-id ops with merge,
  canonical HTML, markdown export, the markdown converter).
- **`apps/web/src/lib/document/`** builds the Tiptap schema from the same
  `contract.json`. A conformance test feeds one corpus to both sides and
  fails on any disagreement.
- **`app_pages.format`** is `markdown` or `tree`. Every server and browser
  path that reads or writes page text checks it. `content` stays the
  materialized markdown, so readers of `content` keep working.
- **The model reads and writes HTML.** Reads return canonical HTML with block
  ids and a read base; writes are block-id ops carrying it. ProseMirror's
  JSON was considered as the write format and rejected: models write HTML
  fluently and JSON trees poorly, the JSON is several times noisier for the
  same paragraph, and a bracket out of place fails the whole write where a
  bad tag fails one block. Do not reopen without a measurement on the
  production models.
- **The contract version is enforced on the wire,** not only in the editor.

## Traps

From the spike and the audits. Each is owned by a slice below.

1. **A client that cannot read a node deletes it from the shared document**
   (Tiptap's binding, on a schema error). Today's shipped clients cannot read
   any tree node: they bind `ydoc.getText('content')`
   (`lib/yjs/document.ts:42`), see an empty page, and type into a stray Y.Text
   that never reaches `content`.
2. **The server applies and rebroadcasts every client update before checking
   anything** (`server/yjs.rs`, the sync handler). A check at save time is
   too late; stale peers have already acted on it.
3. **yrs 0.18 stores XML attributes as strings;** a heading at level `"2"`
   matches no render rule and the editor throws. The core runs 0.18.8.
4. **A NULL `yjs_state` is reseeded from `content`** (`get_or_create`). For a
   tree page that is the lossy markdown export: widgets would be lost.
5. **Browser IndexedDB copies** (`v2-${pageId}`, `document.ts:61`) sync back
   on reconnect, and the fast path binds them before the server has synced.
   An old copy of a converted page pushes a Y.Text into it.
6. **A state vector does not move on deletion,** so it cannot key a read
   base; and in-memory read bases die with the doc cache (30 minutes idle, 100
   docs) and the nightly restart.
7. **Tiptap details:** generated attribute parsers must return null when the
   HTML lacks the attribute, or `h2` parses as level 1; Tiptap's code mark
   excludes every other mark (the contract sets `excludes: "code"`); UniqueID
   must skip remote transactions or every client rewrites ids.
8. **HTML has no self-closing custom elements;** `<virtues-applet .../>`
   swallows what follows. Ingest refuses it by name.

## Slices

New pages do not default to the tree until slices 0 to 4 have shipped and
the device checks pass. Until then tree pages exist only behind a flag.

### 0. Foundations

- `yrs` 0.18 → 0.28 in `virtues-core`; drop `y-sync` (unused, and it pulls
  in yrs 0.17). The text API lives in `server/yjs.rs`
  (`state_from_text`, `extract_text_content`, the `YjsState` methods);
  `api/pages.rs` has its own Y.Text reader (`yjs_state_to_markdown`) and the
  other direct `yrs` import. Test that `yjs_state` bytes from a real box decode
  and materialize the same `content` before and after the upgrade.
- `crates/virtues-document` from the spike, with its tests. Add the
  conformance test to the full gate in `ci.yml` (it runs `pnpm check` and the
  airlock's `node --test`, no vitest today).
- **Version guard, built on what local-ui shipped** (`api_version.rs`
  `API_VERSION`, `boxApi.ts` `MIN_BOX_API`; see
  [local-ui-plan.md](local-ui-plan.md)), not on the unbuilt
  [version-compat-plan.md](version-compat-plan.md):
  - the websocket URL carries the client's contract version; a client that
    sends none is contract 0;
  - the server refuses to bind a client below the document's contract (that
    is every shipped client, for every tree page) and answers one above its
    own read-only;
  - the server validates an update against the contract **before** applying
    and broadcasting it, and drops the connection that sent an invalid one;
  - raising a document's contract disconnects sockets below it;
  - the IndexedDB fast path does not bind a tree page until the server has
    confirmed the contract.
  This raises `API_VERSION`, and ships in a release before any tree page can
  exist.

### 1. The server handles both formats

Everything that reads or writes page text on the server checks `format`.
Tree pages are created only by tests and the flag in this slice.

- **Writers that send markdown** convert it through the contract's converter
  and write block ops, reporting notes: `api::pages::create_page` (the
  model's `create_page`, `POST /api/pages`, the CLI's `page new`, the morning
  examen applet, deep research), `YjsState::append_markdown` behind
  `POST /api/pages/:id/append` (the PDF pane's "send highlight to page",
  `PdfPane.svelte`, which writes user pages), and `edit_page`'s fallback when
  Yjs is unavailable. `PUT /api/pages/:id` refuses `content` for a tree page.
- **Load and save:** a tree page with NULL `yjs_state` is an error, never a
  reseed. A Y.Text arriving for a tree page is refused. `content` is the
  export; a tree page with an element the server's contract does not know is
  refused before it is applied (trap 2), not skipped by the export.
- **History:** `cut_restore_point` and `keep_first_draft` compare text
  through `extract_text_content`, which reads `""` from a tree page, so no
  restore point would ever be cut; make them format-aware.
  `wiki_articles.rs`'s snapshot readers too. Restore runs in the browser
  today (`lib/yjs/versions.ts`); a tree restore runs on the server.
- **Publishing:** `api/publish_page.rs` decodes `yjs_state` as Y.Text and
  would publish a tree page blank. Port its rules, not just the HTML: refs go
  out as names, the names are listed for the Share sheet, Drive images are
  inlined, outside images counted. A mention leaves as its label; an applet
  leaves as a static snapshot or not at all. Day article veil spans
  (`⟦…⟧`, `publish_page.rs:47`) keep their handling.
- **The chat's page context:** `api/chat.rs` tells the model to use
  find/replace with an empty `find` for a full rewrite. Tree pages get the new
  tool's wording (slice 3).

### 2. The editor

`DocumentEditor.svelte` inside `views/PageContent.svelte` for tree pages,
reusing `RefPicker`, `SlashMenu`, `SelectionToolbar`, `PageOutline`,
`VersionHistoryPanel`, `AiPromptPopover`. Uploads keep `/api/media/upload`.
Typography uses the `--md-*` tokens ([build/design-grammar.md](../build/design-grammar.md)
makes them the one definition the read path and the editor share).

Parity with today's editor, each kept or cut by name:

- task lists and the `/task` keyword (`tasks-plan.md` keeps
  page checkboxes as page content); slash commands; find-in-page; word and
  link counts (`PageStatusBar`); code blocks with language picker, copy and
  Shiki highlighting; table menus (add, delete, align); the link menu (open
  beside, turn into embed, `LinkEditorPopover`); focus and typewriter mode;
  long-press for menus on iOS, which fires no `contextmenu`; the cover,
  projects and references panels.
- **The raw-markdown toggle** (`pageDisplay.rawMode`) has no meaning on a
  tree. Replace with a read-only "view as markdown", or cut.
- **The inline AI writer** (Cmd-J, `/ai`, "Ask AI"; `ai/aiCursorSession.ts`)
  streams into the Y.Text and proposes rewrites as CriticMarkup
  (`{--old--}{++new++}`). Port it to tree transactions, with suggestion
  marks in the contract if proposals stay, or cut it. Note that `<del>` is a
  parse alias for strike today.
- **Browser code that reads `ytext` outside the editor:** the chat's page
  binding (`components/chat/state/pageBinding.ts`,
  `stores/editAllowList.svelte.ts`), which would hand the model an empty
  page; Copy markdown (`PageContent.svelte`); the version preview and restore
  (`yjs/versions.ts`); the chat edit animation (`ai/aiPresence.ts`). Chat
  cards that render old `edit_page` results must still render.
- **Paste:** markdown pasted from chat's Copy button lands as literal `**`
  today. Convert pasted markdown through the server's converter (one
  converter, not a second one in TypeScript). Map the inline styles Google
  Docs and Word use for bold, italic and strike onto contract marks.
- **Collaboration:** carets via the existing awareness relay, which keeps no
  state (`server/yjs.rs`): late joiners see no carets and closed tabs'
  carets linger. Keep last awareness per client on the server, or have
  clients re-announce on join.
- A page view at 210 mm and a print stylesheet, shared with the frozen
  bundle's paged print in [publishing-plan.md](publishing-plan.md). Every
  widget declares how it prints.
- Measure a large page (thousands of blocks, a dozen applets) on an iPhone
  before default: ProseMirror renders the whole document where CodeMirror
  renders what is visible, and every applet is an iframe.

### 3. The model's tools

- `tools/page_editor.rs`: `get_page_content` returns canonical HTML with
  block ids, a read base, and the markdown export for whole-page context;
  `edit_page` takes ops plus the read base. A refused batch writes nothing.
- **Read bases are durable** (a table, not the doc cache) and keyed by a hash
  of the tree, which changes on deletion (trap 6). A write whose base is gone
  is refused if its block changed, not guessed at. Per-block revisions are
  the alternative to weigh: survive anything, but can refuse only, never
  merge.
- **An empty `find` replaces the whole page with no check today**
  (`server/yjs.rs`, `apply_text_edit`). The new tool has no unchecked
  whole-page write.
- The four page tools share the chat mode's "pages" group budget, 2,400
  characters (`api/chat_mode.rs`, `tool_definitions_stay_inside_their_budgets`),
  which binds before the sudo total.
- The CLI: `virtues page edit --find --replace` and `page get` (`cli/data.rs`)
  get the block form; find/replace stays for markdown pages until slice 7.
  Applets get `edit_page` too ([applets/AGENTS.md](../../applets/AGENTS.md)).
- **Measure before default:** fixed real edit tasks on real pages, run
  against the production chat models and the applet runner's, counting
  refusals, recoveries on retry, and wrong edits.
- Replacing a block that holds an inline widget is a whole-block swap in the
  spike; patch the text around the widget so a person's caret holds.

### 4. Widgets and contract coverage

Each: a contract entry, a node view, a markdown export rule, conformance
cases.

- **Mention** as an inline atom. Its export must match today's syntax byte
  for byte, `[@Label](/type/id)`, because backlinks search `content` for
  `/page/{id})` (`api/pages.rs`) and the project graph scans for refs
  (`api/projects.rs`); conformance cases against both parsers.
  [record/names.md](../record/names.md) says a ref shows the text written in
  the page, because that text is what is edited: decide stored or live
  label, and keep `to` a route so redirects work.
- **Media:** image width, audio, video, file. The converter must classify
  Drive embeds by the alt text's extension, as `media-widgets.ts` does, not
  treat an extensionless URL as an image (the spike did).
- **Headings `h1`–`h6`** ([build/codemirror.md](../build/codemirror.md)
  supports six today; dropping to three is a regression) and **table column
  alignment**.
- **Callout**, exported as `> [!type]`.
- **Applet**, mounting `components/applets/FaceFrame.svelte`. After
  `applets-refactor-plan.md` slices 0 and 1 (faces
  that load, a token that outlives an hour, theme).

### Gate: new pages default to the tree

Slices 0 to 4 shipped, the device checks below passed, the model measurement
acceptable. Then `kind = 'page'` pages are created as `tree`.

### 5. Move user pages

- Dry run on a copy of a real box. The bar: no character lost, every change
  reported. (The spike's 179-page corpus was mostly articles; about 57 were
  user pages.)
- Per page: evict the doc from the cache and drain its queued save
  (`DocCache.held`, the save queue) so no Y.Text save lands after the flip;
  cut a restore point; convert; write the tree; flip `format`; keep the
  migration from bumping `updated_at` (the `set_updated_at` trigger).
- IndexedDB copies are keyed by format so a converted page ignores its old
  copy.
- Pending CriticMarkup proposals are resolved or converted, not migrated as
  text.
- History: a markdown-era snapshot restores into a tree page by conversion,
  with its notes shown.

### 6. Move articles

Articles (`kind = 'article'`, 126 of 183 pages on the measured box) are
written by machines.

- **Their syntax enters the contract:** footnotes (`[^ev-N]`, `[^cx-N]`), the
  figure fence, veil spans (`⟦…⟧`), all of
  [day-article-plan.md](day-article-plan.md)'s format. Hand-note anchors
  (migration 0046) parse the exported markdown; the dry run checks they still
  land.
- **Provenance:** "the owner edited this" is `content <> machine_text`
  (`api/day_summary.rs`, [record/article-resolution.md](../record/article-resolution.md)).
  If the export differs from what the writer produced by one space, every
  article reads as owner-edited and the wiki editor freezes it. Reset
  `machine_text` to the export at conversion, and writers store the export as
  their edition from then on.
- **Writers:** `api/wiki_editor.rs` (`replace_text`, `apply_text_diff`,
  `write_turn`), `api/day_summary.rs` (narration, "Rewrite this page"),
  `api/wiki_articles.rs`. They keep producing markdown; the server converts
  and writes the difference as block ops. Reuse `revise_article`'s
  whole-article diff path, not a new one.
- **Lazy CRDT:** a first draft writes `content` only while `yjs_state IS
  NULL`, and having state means "opened" (`api/day_summary.rs`). Keep that:
  convert at first open, and keep each document's lineage, because a
  reseeded document merges with its IndexedDB copy as a second copy.
- The ownership rule stands: the server refuses a machine edit that loses a
  sentence the person wrote.

Design this slice when slices 0 to 5 have shipped.

### 7. Delete the old way

- Raise `MIN_BOX_API` to the release that has `format`, with a release note:
  an app without the CodeMirror editor cannot edit a markdown page on an
  older box.
- Delete the CodeMirror page editor: `codemirror/editor.ts`'s page factory;
  `outline.ts`; `widget-height.ts`; the extensions only it reaches
  (live-preview, media-widgets, tables, checkboxes, code-blocks,
  shiki-highlight, slash-commands, selection-toolbar, media-paste,
  render-mode, focus-mode); review-marks and ai-cursor once
  `aiCursorSession.ts` and `aiPresence.ts` stop importing them. **Keep**
  what the composer reaches, directly or not: caret, keybindings,
  list-renumber, mouse-freeze, ref-links, ref-picker (`ChatInput.svelte`),
  inline-marks, empty-mark-tidy, code-context, highlight-parser, long-press,
  `language.ts`, `theme.ts`. Grep importers before each deletion.
- The Y.Text page paths in `server/yjs.rs`, the find/replace form of
  `edit_page` and of `virtues page edit`, `format = 'markdown'`.
- Docs: [build/codemirror.md](../build/codemirror.md) cut to the composer and
  the calendar quick-add (`calendar-plan.md` calls
  CodeMirror the house kernel), its stale entries (`createReadOnlyEditor`,
  `PublicPageViewer`) gone with it; `build/README.md`'s row;
  [build/wiki-editor.md](../build/wiki-editor.md)'s find/replace path;
  [build/the-day.md](../build/the-day.md)'s "CodeMirror later". A new build
  doc for the document contract from slice 2 on, while both models coexist.
- Then delete this plan and write the record.

## Before default (gate) and before each release

- A physical iPhone in the Tauri app's web view: dictation, selection
  handles, IME composition (Japanese, Chinese) while the model edits the
  same block, the keyboard accessory, a widget's controls by touch. The spike
  tested only the simulator's Safari.
- Android, if [cross-platform-apps-plan.md](cross-platform-apps-plan.md)'s app
  can edit pages by then: keyboard input is ProseMirror's weakest platform.
- Two devices on one page with the model writing.
- [mobile-ux-plan.md](mobile-ux-plan.md) defers the phone editor to a "pages
  editor roadmap" that does not exist; this plan is it.

## Other plans this touches

- [publishing-plan.md](publishing-plan.md): its guest checklist ticks (wave
  2) and co-editing on the page CRDT (wave 4) should target block ops.
- `tasks-plan.md`: its note about page checkboxes moves from
  `build/codemirror.md` to the document build doc.
- [day-article-plan.md](day-article-plan.md): "the article stays one
  ordinary markdown page" holds until slice 6.

## Not in this plan

- Live A4 pagination. The page view and print cover it.
- Tiptap's paid extensions and hosted services.
- Comments.
- The chat composer and chat messages.

## Before slice 0

- The spike's code lives only in a session scratch directory, and it is this
  plan's starting point. Keep it somewhere durable first.
