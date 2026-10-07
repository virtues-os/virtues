# Document contract: pages on a Yjs tree

Pages move from a markdown string in CodeMirror to a Yjs XML tree edited with
Tiptap's MIT core, under one contract file that the browser and the Rust
server both read. Decided in
[record/editor-kernel-2026-10-07.md](../record/editor-kernel-2026-10-07.md);
measured in [record/document-contract-spike-2026-10-07.md](../record/document-contract-spike-2026-10-07.md).
The chat composer is not part of this and stays on CodeMirror.

**Death condition:** every page is a tree page, the CodeMirror page editor and
the Y.Text page paths are deleted, and a record describes what shipped.

## What done looks like

- **`crates/virtues-document`** owns the contract: `contract.json` plus the
  Rust that interprets it. Content expressions compile to regexes; a strict
  HTML ingest refuses what the contract cannot hold; the Yjs writer and
  reader use the shape Tiptap's binding (`@tiptap/y-tiptap`) expects; block-id
  edit ops merge against the version the writer read; canonical HTML and
  markdown export; the markdown converter for migration and for machine
  writers that keep producing markdown. The spike's code is the starting
  point.
- **`apps/web/src/lib/document/`** builds the Tiptap schema from the same
  `contract.json`, with node views as Svelte components. One conformance test
  feeds the same corpus to both sides and fails on any disagreement:
  canonical HTML a fixed point on both, the Yjs tree identical in both
  directions.
- **`app_pages.format`** is `markdown` or `tree`. A tree page's Yjs state
  holds the fragment `doc`; `content` is its markdown export, materialized on
  save as today, so every reader of `content` is untouched (search, indexer,
  `wiki_first`, `day_memory`, `projects`, `pins`, `sql_query`, and the rest).
- **The model's page tools** read HTML with block ids, a version, and the
  markdown export for whole-page context, and write ops (`replace`,
  `insert_after`, `insert_before`, `append`, `delete`) that carry the version.
- **The contract version is in every document** (`meta.contract`). A client
  never binds a document newer than its contract; the server never accepts a
  client whose contract is newer than its own.

## Traps the spike found

Each became a slice requirement below. None is in the libraries' docs.

1. **A stale client deletes what it cannot read.** Tiptap's binding catches
   the error building an unknown node and deletes the Yjs element, from the
   shared document, for everyone. Phones run a bundled web app on their own
   update schedule. The version guard is not optional.
2. **yrs 0.18 stores XML attributes as strings.** A heading at level `"2"`
   matches no render rule and the editor throws drawing it. The core runs
   0.18; the spike ran 0.28.
3. **Block-id writes lose concurrent typing** unless they carry the version
   read. `edit_page`'s find/replace has that property for free today, because
   the text it looks for must still be there.
4. **Tiptap's attribute parsers override parse rules.** Generated attributes
   must return null when the HTML lacks them, or `h2` parses as level 1.
5. **Tiptap's code mark excludes every other mark.** Bold inline code occurs
   333 times in the repo docs; the contract sets `excludes: "code"`.
6. **HTML has no self-closing custom elements.** `<virtues-applet .../>`
   swallows what follows. Ingest refuses it by name.

## Slices

### 0. Foundations (no visible change)

- Upgrade `yrs` 0.18 → 0.28 in `virtues-core` and drop the unused `y-sync`
  dependency. Existing Y.Text pages keep loading: test that `yjs_state` bytes
  from a real box decode and materialize the same `content` before and after.
  `server/yjs.rs` is the main caller; `api/pages.rs`'s `state_from_text` and
  `extract_text_content` and their callers in `wiki_editor.rs`,
  `wiki_articles.rs`, `day_summary.rs` and `tools/page_editor.rs` follow.
- Create `crates/virtues-document` from the spike, with its unit tests and the
  70-case conformance corpus. The conformance test needs the Rust binary and
  the web schema. Add it to the full gate in `ci.yml`, which today runs
  `pnpm check` for the frontend and no web tests.
- Version guard. The editor reads `meta.contract` after sync and refuses to
  bind a newer document, with a reload prompt. The websocket URL carries the
  client's contract version; the server answers a newer one read-only. This
  is the document's case of [version-compat-plan.md](version-compat-plan.md)'s
  declared minimum and should share its comparison.

### 1. New pages are tree pages

- Claim a migration for `app_pages.format` (`make migration NAME=page_format`,
  then the rules in `.claude/rules/migrations.md`). Default `markdown`; pages
  created with `kind = 'page'` after this ships are `tree`.
- `server/yjs.rs` loads, saves and materializes by format. Server-side writes
  to a tree page go through `virtues-document` ops, never through a text
  path. A tree page with an element the server's contract does not know is
  refused at save, not silently skipped by the export.
- `DocumentEditor.svelte` takes over from `CodeMirrorEditor.svelte` inside
  `views/PageContent.svelte` for tree pages: collaboration and carets over
  `lib/yjs/document.ts`'s provider, block ids, and the existing components
  driven by the new editor instead of CodeMirror: `RefPicker.svelte` for `@`,
  `SlashMenu.svelte` for `/`, `SelectionToolbar.svelte`, `PageOutline.svelte`
  from headings, `VersionHistoryPanel.svelte`, `AiPromptPopover.svelte`.
  Uploads keep `/api/media/upload`.
- Paste rules, browser only: map the inline styles Google Docs and Word use
  for bold, italic and strikethrough onto contract marks. Anything else
  pasted falls to text.
- Publishing: `api/publish_page.rs` renders a tree page's canonical HTML
  with ids off, instead of rendering its markdown.
- A page view at 210 mm and a print stylesheet (`@page { size: A4 }`). Every
  widget declares how it prints; the spike's `div` bar chart printed blank.

### 2. The model's tools for tree pages

- `tools/page_editor.rs`: `get_page_content` returns, for a tree page, the
  canonical HTML with block ids, the version, and the markdown export;
  `edit_page` takes ops plus the version. Refusals name the block, the tag or
  attribute, and what the contract allows. A refused batch writes nothing.
- Keep the tool definitions inside the size budget the PR checks hold sudo
  mode to (19,600 characters when 035d44b4 trimmed a description to fit). The
  definitions live in `crates/virtues-registry/src/tools.rs`.
- **Measure before defaulting:** a fixed set of real edit tasks on real
  pages, run against the production models, counting refusals, recoveries on
  retry, and wrong edits. The spike never ran a production model.
- Replacing a block that holds an inline widget is a whole-block swap in the
  spike; patch text around the widget so a person's caret holds.

### 3. Widgets

One contract entry, one node view, one markdown export rule and conformance
cases each.

- **Applet**: `<virtues-applet ref height>`, the node view mounting
  `components/applets/FaceFrame.svelte`. The face keeps its sandbox; the page
  stores the reference and the height, never the applet's data.
- **Mention**: entity references as an inline atom, with the existing
  hover preview. Today's `[@Label](/person/id)` links migrate to it.
- **Media parity with today's editor**: audio, video and file cards, image
  width. The spike's contract has no audio, video or file node.
- **Gaps the migration measured**: table column alignment, headings below
  `h3` (decide: add them, or migrate them to `h3` with a note).
- **Callout** is in the spike already. Review marks (CriticMarkup) are not
  carried over in v1; nothing produces them yet.

### 4. Move user pages

- One batch over `kind = 'page'`: cut a restore point (`cut_restore_point`),
  convert through the migration converter, write the tree, flip `format`. A
  report lists every page with a note and is kept.
- Dry run on a copy of a real box first. The bar is the spike's: no
  character lost, every change reported. The spike measured 179/179
  converted, 176 unchanged.
- History: a markdown-era snapshot restores into a tree page by conversion,
  with its notes shown.

### 5. Move articles

Articles (`kind = 'article'`, 126 of 183 pages on the measured box) are
written by machines: `api/wiki_editor.rs` (text diffs, `write_turn`,
provenance as the difference from what the editor last wrote),
`api/day_summary.rs` (narration and "Rewrite this page", 113b800c),
`api/wiki_articles.rs`, and `server/api/pages.rs`'s `append_markdown`.

- Machine writers keep producing markdown; the server converts it through
  the migration converter and writes the difference as block ops, so an open
  editor sees the change live and block ids survive.
- Provenance runs on the markdown export to start, which is the text it
  reads today. Moving it to blocks is a later question.
- The ownership rule stands: the server refuses a machine edit that loses a
  sentence the person wrote
  ([record/article-resolution.md](../record/article-resolution.md)).

This is the largest slice. Design it when slices 0 to 4 have shipped.

### 6. Delete the old way

- The CodeMirror page editor: `codemirror/editor.ts`'s page factory and the
  extensions only it uses (live preview, media widgets, tables, checkboxes,
  code blocks, slash commands, selection toolbar, media paste, review marks,
  render mode, outline). What `codemirror/composer.ts` imports stays: caret,
  keybindings, list renumber, mouse freeze, ref links, language, theme.
- The Y.Text page paths in `server/yjs.rs`, the find/replace branch of
  `edit_page`, `format = 'markdown'`, and [build/codemirror.md](../build/codemirror.md)
  reduced to the composer.
- Then delete this plan and write the record.

## Before any slice ships

- A physical iPhone, in the Tauri app's web view: dictation, selection
  handles, IME composition (Japanese, Chinese), the keyboard accessory, a
  widget's controls by touch. The spike tested only the simulator's Safari.
- Two devices on one page with the model writing.

## Not in this plan

- Live A4 pagination. The page view and print cover it.
- Tiptap's paid extensions and hosted services.
- Comments and suggestions.
- The chat composer and chat messages.
