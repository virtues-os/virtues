# CodeMirror 6 Page Editor Contract

Single source of truth for the page editor's document model. The document IS markdown — plain text stored in a Yjs Y.Text CRDT. No intermediate AST, no XML, no conversion layer. CodeMirror decorations provide live preview rendering while keeping the underlying text editable.

## Architecture

```
Y.Text ("content")  ←→  WebSocket sync  ←→  Rust yrs (server)
       ↕
CodeMirror 6 EditorView
       ↕
Decoration extensions (live preview, widgets, etc.)
```

- **Source of truth**: Y.Text named `"content"` in the Yjs Doc
- **Storage**: `yjs_state` `bytea` in `app_pages` (Yjs v1 encoded), `content` TEXT column materialized on save for search/fallback
- **Sync**: y-websocket → `/ws/yjs/{pageId}` → Rust yrs server with 2s debounced save queue
- **Editor factory**: `codemirror/editor.ts` — `createCodeMirrorEditor()` (Yjs collab) + `createReadOnlyEditor()` (public pages)

## Block Elements

| Element | Markdown Syntax | Slash Command | Decoration | Extension File | Notes |
|---------|----------------|---------------|------------|----------------|-------|
| Paragraph | plain text | (default) | — | — | |
| Heading 1–6 | `# ` to `###### ` | `/heading1-3` | Serif font, hides `#` markers | `live-preview.ts` | Lezer `ATXHeading` nodes |
| Blockquote | `> ` | `/quote` | Left border, hides `>` marker | `live-preview.ts` | Line decoration on each line |
| Fenced Code | ` ``` ` | `/code` | Header widget (language + copy) | `code-blocks.ts` | StateField for block widget |
| Bullet List | `- ` | `/bullet` | List-line padding, marker styling | `live-preview.ts` | |
| Ordered List | `1. ` | `/numbered` | List-line padding, marker styling | `live-preview.ts` | |
| Task List | `- [ ] ` / `- [x] ` | `/task` | Interactive checkbox widget | `checkboxes.ts` | Click toggles `[ ]`↔`[x]` in Y.Text |
| Horizontal Rule | `---` | `/divider` | `<hr>` widget replaces text | `live-preview.ts` | Fallback regex for setext edge cases |
| Table | GFM pipe syntax | `/table` | Full HTML `<table>` widget | `tables.ts` | Tab/Enter nav, drag reorder, add/delete rows/cols |
| Image | `![alt](url)` or `![alt\|600](url)` | `/image` | `<img>` widget below line | `media-widgets.ts` | `\|width` in alt sets pixel width (Obsidian convention) |
| Audio | `![name](url.mp3)` | — (paste/drop) | `<audio>` player widget | `media-widgets.ts` | Header with icon + native controls |
| Video | `![name](url.mp4)` | — (paste/drop) | `<video>` player widget | `media-widgets.ts` | Native controls, preload=metadata |
| File | `![name](url.pdf)` | `/file` | File card (icon + name + ext) | `media-widgets.ts` | Non-media extensions, click to download |

## Inline Elements

| Element | Markdown Syntax | Decoration | Extension File | Notes |
|---------|----------------|------------|----------------|-------|
| Bold | `**text**` | `.cm-strong` mark, hides `**` | `live-preview.ts` | Lezer `StrongEmphasis` |
| Italic | `*text*` | `.cm-emphasis` mark, hides `*` | `live-preview.ts` | Lezer `Emphasis` |
| Code | `` `text` `` | `.cm-inline-code` mark, hides `` ` `` | `live-preview.ts` | Lezer `InlineCode` |
| Strikethrough | `~~text~~` | `.cm-strikethrough` mark, hides `~~` | `live-preview.ts` | Lezer `Strikethrough` (GFM) |
| Underline | `<u>text</u>` | `.cm-underline` mark, hides tags | `live-preview.ts` | Regex-based (not in Lezer) |
| Highlight | `==text==` | highlight mark, hides `==` | `live-preview.ts` | Parsed by `highlight-parser.ts` |
| Reference link | `[@Label](/type/id)`, `[text](https://...)`, `[text](/path)` | Plain underlined link (`.cm-ref-link`), `@` stripped | `ref-links.ts` | One inline density for every link; target shown on hover |

## Interaction Triggers

| Trigger | Scope | What Opens | Result |
|---------|-------|-----------|--------|
| `/` | Block (line start / after whitespace) | Slash menu | Insert markdown syntax |
| `@` | Inline (after whitespace / line start) | Ref picker | Insert `[@Label](/type/id)` |
| Text selection | Inline | Selection toolbar | Toggle marks (bold, italic, code, strikethrough, underline, link) |
| Cursor in table | Block | Table widget UI | In-cell editing, hover strips for add row/col, drag to reorder |
| Paste image/media | Block | — (auto) | Upload → `![name](url)` |
| Drag file | Block | — (auto) | Upload → `![name](url)` |
| `- [ ] ` | Block (line start) | — (auto) | Checkbox decoration rendered |
| Right-click link/media | Inline/Block | Context menu | Go to, Copy, Turn into embed/reference, Edit, Remove |

## Mental Model

Two triggers, one swap gesture:

- **`@`** = inline reference — always produces `[label](url)`, rendered as a plain link
- **`/`** = block command — inserts block structures (headings, lists, code) or embeds (`![alt](url)`)
- **Right-click** = swap between reference and embed (plus Go to, Copy, Edit, Remove)

The `@` trigger searches entities (people, pages, places, orgs) and drive files via `/api/pages/search/entities`. Pasting a URL in the `@` search creates a synthetic Link result. The `/image` and `/file` commands upload from disk and insert `![name](url)` blocks.

## Link Rendering

`ref-links.ts` (exported as `entityLinks`) renders every `[label](url)` — entity,
file, internal path, or external URL — with one widget, `RefLinkWidget`: a
plain underlined link that belongs to the prose (`.cm-ref-link`, dotted
hairline, styled in `ref-links.css`). No pill, no chip, no favicon; the `@`
marker is stripped for display. The target and its type surface in a hover
preview (`refHoverPlugin`, `RefPreview.svelte` over `RefCard.svelte`), never in
inline chrome. Entities have no block card: a legacy `![label](/person/…)`
renders as the same inline link, `!` swallowed.

| Gesture | Effect |
|---------|--------|
| Plain click | Places the caret in the line; the text does not change |
| ⌘/Ctrl-click, entity URL (`/person/`, `/page/`, `/org/`, `/place/`, `/day/`, `/year/`, `/source/`, `/chat/`, `/drive/`, `/space/`) | Opens the route beside the page |
| ⌘/Ctrl-click, `http(s)://` | Opens in a new tab |
| ⌘/Ctrl-click, other `/` path | Dispatches a `page-navigate` event |
| Right-click or long-press | Link context menu |

Touching a link with the editor focused reveals that one link's raw
`[label](url)` in place; its neighbours stay rendered. The context menu's Edit
opens a panel for fixing the label or URL without entering the text.

## Context Menu

Right-click (or long-press) on any link or media embed opens a context menu with actions that operate on the raw markdown text.

**Link context menu** (`ref-links.ts`):

| Action | Effect |
|--------|--------|
| Go to | Navigate (internal) or open in new tab (external) |
| Open in New Tab | Always opens in new tab |
| Copy link | Copy full URL to clipboard |
| Turn into embed | Insert `!` before `[` — converts `[label](url)` → `![label](url)` |
| Edit | Open the link editor panel for the label and URL |
| Remove | Delete entire `[label](url)` from document |

**Media context menu** (`media-widgets.ts`):

| Action | Effect |
|--------|--------|
| Go to | Open media URL in new tab |
| Copy link | Copy full URL to clipboard |
| Turn into reference | Remove `!` — converts `![alt](url)` → `[alt](url)` |
| Edit | Move cursor to embed position, revealing raw markdown |
| Remove | Delete entire `![alt](url)` from document |

## Keyboard Shortcuts

| Shortcut | Action | Extension |
|----------|--------|-----------|
| `Mod-b` | Toggle bold (`**`) | `keybindings.ts` |
| `Mod-i` | Toggle italic (`*`) | `keybindings.ts` |
| `Mod-e` / `Mod-`` ` | Toggle inline code (`` ` ``) | `keybindings.ts` |
| `Mod-u` | Toggle underline (`<u>`) | `keybindings.ts` |
| `Mod-Shift-s` / `Mod-Shift-x` | Toggle strikethrough (`~~`) | `keybindings.ts` |
| `Mod-z` / `Mod-Shift-z` | Undo/Redo (Y.UndoManager) | y-codemirror.next |
| Tab / Shift-Tab (in table) | Navigate cells | `tables.ts` |
| Enter (in table) | Move to cell below | `tables.ts` |
| Escape (in table) | Exit table editing | `tables.ts` |

## Media Upload Flow

```
User Action (paste, drop, or /image command)
    ↓
uploadMedia(file) → POST /api/media/upload (FormData)
    ↓
Backend: SHA-256 hash → .media/{prefix}/{hash}.{ext} (content-addressed, deduped)
    ↓
Response: { url: "/api/drive/files/{id}/download" }
    ↓
Editor: insert ![filename](url)
    ↓
media-widgets.ts detects pattern → renders inline widget
```

- **Placeholder**: `![Uploading filename...]()` shown during upload
- **Size limit**: 100 MB per file
- **Accepted types**: All file types. Media (image/video/audio) get rich previews, others get file cards.
- **Storage**: Content-addressed (SHA-256) in `.media/` system folder, filesystem or S3

## AI Edit Flow

AI edits are applied directly to Y.Text via Yjs — the document IS markdown, no conversion needed.

1. AI calls `edit_page` tool → backend auto-snapshots for undo, then calls `apply_text_edit()` (simple string find/replace on Y.Text)
2. Yjs sync pushes the change to the frontend editor in real-time
3. User can revert via version history (snapshot saved before each AI edit)

## Reveal-on-Touch

Syntax is hidden until the selection touches the construct, then it appears in
place, dimmed, with the styling still applied — per construct, not per line.
Inline marks (bold, italic, strikethrough, code, highlight, underline) reveal
only the construct the selection is inside (`inline-marks.ts` answers where
the marks are and whether the selection touches one; `live-preview.ts`
renders from that). Block markers (heading `#`, quote `>`, list markers)
reveal on the caret's line. Every reveal also requires the editor to **have
focus**: the selection survives blur, so without that gate a blurred editor,
an inactive split pane, or the read-only view would hold raw syntax on the
caret line. Links reveal per link, like inline marks (see above).

## Extension Architecture

| Extension | Type | Facet | Role |
|-----------|------|-------|------|
| `live-preview.ts` | ViewPlugin | decorations | Heading, bold, italic, code, strikethrough, underline, blockquote, HR, lists |
| `ref-links.ts` | ViewPlugin | decorations | `[label](url)` → one plain-link widget + hover preview + context menu |
| `checkboxes.ts` | ViewPlugin | decorations | `- [ ]` / `- [x]` → interactive checkbox widgets |
| `media-widgets.ts` | StateField | EditorView.decorations | `![alt](url)` → image/audio/video/file widgets + context menu |
| `code-blocks.ts` | StateField | EditorView.decorations | Fenced code → header widget (language + copy) |
| `tables.ts` | StateField | EditorView.decorations | GFM tables → interactive HTML `<table>` widgets |
| `shiki-highlight.ts` | ViewPlugin | decorations | Shiki token-level syntax highlighting in code blocks |
| `slash-commands.ts` | ViewPlugin | — | `/` detection → opens slash menu (callback-based) |
| `ref-picker.ts` | ViewPlugin | — | `@` detection → opens the ref picker (callback-based) |
| `selection-toolbar.ts` | ViewPlugin | — | Text selection → floating toolbar (callback-based) |
| `media-paste.ts` | domEventHandlers | — | Paste/drop → upload + insert markdown |
| `keybindings.ts` | keymap | — | Mod-b, Mod-i, Mod-e, Mod-u, Mod-Shift-s |

## File Extension Detection

Used by `media-widgets.ts` and `media-paste.ts` to determine media type:

```
Image: png, jpg, jpeg, gif, webp, svg, bmp, ico, avif, heic, heif, tiff
Audio: mp3, wav, ogg, m4a, aac, flac, opus, wma
Video: mp4, webm, mov, avi, mkv, m4v, ogv
Other: renders as file card (icon + name + extension badge + download)
```

## Entity Link URL Prefixes

Entity links use the `[@Label](/type/id)` convention. Recognized prefixes
(`ENTITY_PREFIXES` in `ref-links.ts`):

```
/person/, /page/, /org/, /place/, /day/, /year/, /source/, /chat/, /drive/, /space/
```

## Implementation Files

| File | Role |
|------|------|
| `apps/web/src/lib/codemirror/editor.ts` | Editor factory (Yjs collab + read-only) |
| `apps/web/src/lib/codemirror/theme.ts` | CodeMirror base theme |
| `apps/web/src/lib/codemirror/theme.css` | Widget CSS (images, audio, video, code, tables) |
| `apps/web/src/lib/codemirror/extensions/live-preview.ts` | Heading, bold, italic, code, strikethrough, underline, blockquote, HR, lists |
| `apps/web/src/lib/codemirror/extensions/ref-links.ts` | Link → plain-link widget, hover preview, context menu |
| `apps/web/src/lib/codemirror/extensions/checkboxes.ts` | Interactive checkbox decorations |
| `apps/web/src/lib/codemirror/extensions/media-widgets.ts` | Image/audio/video inline widgets |
| `apps/web/src/lib/codemirror/extensions/code-blocks.ts` | Code block header widget |
| `apps/web/src/lib/codemirror/extensions/tables.ts` | Interactive GFM table widget |
| `apps/web/src/lib/codemirror/extensions/shiki-highlight.ts` | Shiki syntax highlighting in code blocks |
| `apps/web/src/lib/codemirror/extensions/slash-commands.ts` | Slash command detection + default commands |
| `apps/web/src/lib/codemirror/extensions/ref-picker.ts` | Ref picker trigger (`@`) + `insertRef` |
| `apps/web/src/lib/codemirror/extensions/selection-toolbar.ts` | Floating formatting toolbar on selection |
| `apps/web/src/lib/codemirror/extensions/media-paste.ts` | Paste/drop media upload handler |
| `apps/web/src/lib/codemirror/extensions/keybindings.ts` | Markdown formatting shortcuts |
| `apps/web/src/lib/components/pages/CodeMirrorEditor.svelte` | Svelte editor component (wires extensions + UI overlays) |
| `apps/web/src/lib/components/pages/PublicPageViewer.svelte` | Read-only public page renderer |
| `apps/web/src/lib/yjs/document.ts` | Yjs document creation (WebSocket + IndexedDB) |
| `virtues-core/src/server/yjs.rs` | Rust Yjs server (sync, save queue, text extraction) |
| `virtues-core/src/tools/page_editor.rs` | AI page editing tool (find/replace on Y.Text) |
| `virtues-core/src/api/media.rs` | Media upload API (content-addressed storage) |

## Planned Features

| Feature | Markdown Syntax | Priority | Notes |
|---------|----------------|----------|-------|
| Drag-to-resize images | drag handle on `<img>` | medium | Updates `\|width` in alt text |
| Embed | `[embed](url)` or custom | medium | YouTube, Twitter, Spotify. Provider detection + iframe. |
| Callout | `> [!type]` (GFM alerts) | medium | Info/warning/tip/danger variants. |
| Toggle | `<details><summary>` | low | Collapsible content block. |
| Math block | `$$...$$` | low | KaTeX rendering. |
| Math inline | `$...$` | low | Inline KaTeX. |
