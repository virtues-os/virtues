# ProseMirror Schema Contract

Single source of truth for the page editor's document model. The ProseMirror schema (`schema.ts`), Rust parser (`parser.rs`), and Rust serializer (`serializer.rs`) must all conform to this spec.

## Block Nodes

| Node | XML Tag | Markdown Syntax | Trigger | Attrs | Notes |
|------|---------|----------------|---------|-------|-------|
| paragraph | `<paragraph>` | plain text | default | — | |
| heading | `<heading>` | `# ` to `###### ` | `/heading1-3` | `level: 1-6` | |
| blockquote | `<blockquote>` | `> ` | `/quote` | — | |
| code_block | `<code_block>` | `` ``` `` | `/code` | `language: string` | No marks inside |
| bullet_list | `<bullet_list>` | `- ` | `/bullet` | — | Contains list_item |
| ordered_list | `<ordered_list>` | `1. ` | `/numbered`, `/roman` | `order: number`, `listStyleType` | |
| list_item | `<list_item>` | (child of list) | — | `checked: bool \| null` | `checked: null` = normal item, `true/false` = checkbox todo item. |
| horizontal_rule | `<horizontal_rule>` | `---` | `/divider` | — | |
| table | `<table>` | GFM table | `/table` | — | Uses prosemirror-tables |
| table_row | `<table_row>` | pipe row | — | — | |
| table_header | `<table_header>` | first row | — | `align` | |
| table_cell | `<table_cell>` | data row | — | `align` | |
| media | `<media>` | `![alt](src)` | `/image`, `/audio`, `/video`, paste, drag | `src, alt, title, type` | Unified node. `type: 'image' \| 'audio' \| 'video'` detected by file extension. |

## Inline Nodes

| Node | XML Tag | Markdown Syntax | Trigger | Attrs | Notes |
|------|---------|----------------|---------|-------|-------|
| text | (XmlText) | plain text | typing | — | Carries marks |
| entity_link | `<entity_link>` | `[label](/type/id)` | `@` picker | `href, label` | URL prefixes: `/person/`, `/page/`, `/place/`, `/org/`, `/day/`, `/year/`, `/source/`, `/chat/` |
| file_card | `<file_card>` | `[name](/drive/id)` | `/file` + picker | `href, name` | `/drive/` URL prefix, non-media extension |
| hard_break | `<hard_break>` | `  \n` or Shift+Enter | Shift+Enter | — | |

## Marks

| Mark | Yrs Attr | Markdown Syntax | Trigger | Notes |
|------|----------|----------------|---------|-------|
| strong | `{strong: true}` | `**text**` | Cmd+B, `**` input rule | |
| em | `{em: true}` | `*text*` | Cmd+I, `*` input rule | |
| code | `{code: true}` | `` `text` `` | Cmd+E, `` ` `` input rule | |
| strikethrough | `{strikethrough: true}` | `~~text~~` | Cmd+Shift+S, `~~` input rule | |
| underline | `{underline: true}` | `<u>text</u>` | Cmd+U, toolbar | Uses HTML `<u>` tag for lossless roundtrip. |
| link | `{link: true, href: "..."}` | `[text](url)` | Toolbar link button | External URLs only. Entity URLs → entity_link node. |

## Interaction Triggers

| Trigger | Scope | What Opens | Result |
|---------|-------|-----------|--------|
| `/` | Block (start of line) | Slash menu | Insert block node |
| `@` | Inline (anywhere) | Entity picker | Insert entity_link node |
| Text selection | Inline | Selection toolbar | Toggle marks (bold, italic, etc.) |
| Cursor in table | Block | Table toolbar | Add/remove rows/cols |
| Paste image | Block | — (auto) | Upload → media node (type: image) |
| Drag file | Block | — (auto) | Upload → media node (type detected by extension) |

## AI Edit Flow

AI edits use an apply-first paradigm with decorations for accept/reject UI:

1. AI calls `edit_page` tool → backend strips markdown from find/replace to plain text, applies edit immediately via Yjs `surgical_text_edit`, returns `{ edit, applied: true }`
2. Yjs sync pushes the change to the frontend editor automatically
3. ChatView dispatches `ai-edit-applied` event with `{ pageId, editId, find, replace }`
4. PageEditor finds the replaced text in the PM doc, adds an inline decoration via `ai-edit-highlight` plugin
5. User clicks Accept → decoration removed, text stays as-is
6. User clicks Reject → `POST /api/pages/:id/revert-edit` applies the reverse edit (find=replace, replace=find), decoration removed, Yjs sync reverts the doc

Pending edits are tracked in `pendingEdits.svelte.ts` with the original find/replace for revert capability.

## Conversion Contract

Rules that both the Rust parser and Rust serializer must follow. After W1, the TS markdown layer is removed — only Rust handles markdown conversion.

### Markdown → XmlFragment

1. **Entity links:** Links where `href` starts with `/person/`, `/page/`, `/place/`, `/org/`, `/day/`, `/year/`, `/source/`, `/chat/` → `entity_link` element with `href` and `label` attrs (NOT a link mark)
2. **File cards:** Links where `href` starts with `/drive/` and the filename has a non-media extension → `file_card` element with `href` and `name` attrs
3. **Media:** Image syntax `![alt](src)` → `media` element with `type` detected by file extension:
   - Audio (.mp3, .wav, .m4a, .ogg, .flac, .aac, .wma) → `type: 'audio'`
   - Video (.mp4, .mov, .webm, .avi, .mkv, .m4v, .wmv) → `type: 'video'`
   - Image or unknown → `type: 'image'`
4. **Checkboxes:** `- [ ]` / `- [x]` in list items → `list_item` element with `checked` attr (`"false"` / `"true"`)
5. **Underline:** `<u>text</u>` HTML → `underline` mark
6. **All other links** → `link` mark

### XmlFragment → Markdown

1. `entity_link` → `[label](href)`
2. `media` → `![alt](src)` (type reconstructed from extension on re-parse)
3. `file_card` → `[name](href)`
4. `underline` mark → `<u>text</u>`
5. `list_item` with `checked` attr → `- [x] ` or `- [ ] ` prefix

### Known Lossy Roundtrips

- Entity links/file cards with URLs that don't match known prefix patterns
- Media files with unrecognized file extensions (default to `type: image`)

## Planned Nodes

| Node | Type | Markdown Syntax | Trigger | Priority | Notes |
|------|------|----------------|---------|----------|-------|
| embed | block | `[embed](url)` or custom | `/embed` + URL input | medium | YouTube, Twitter, Spotify. Provider detection + iframe. |
| callout | block | `> [!type]` (GFM alerts) | `/callout` | medium | Info/warning/tip/danger variants. |
| toggle | block | `<details><summary>` | `/toggle` | low | Collapsible content block. |
| math_block | block | `$$...$$` | `/math` | low | KaTeX rendering. |
| math_inline | inline | `$...$` | `$` trigger | low | Inline KaTeX. |
| column_layout | block | none (rich-only) | `/columns` | low | Not representable in markdown. |

## File Extension Detection

Used by both parser and media node view to determine `media.type`:

```
Image: jpg, jpeg, png, gif, webp, svg, bmp, ico
Audio: mp3, wav, m4a, ogg, flac, aac, wma
Video: mp4, mov, webm, avi, mkv, m4v, wmv
```

## Implementation Files

| File | Role |
|------|------|
| `apps/web/src/lib/prosemirror/schema.ts` | ProseMirror node/mark definitions |
| `apps/web/src/lib/prosemirror/plugins/ai-edit-highlight.ts` | AI edit decoration plugin |
| `apps/web/src/lib/events/aiEdit.ts` | AI edit event types and dispatchers |
| `apps/web/src/lib/stores/pendingEdits.svelte.ts` | Pending edit tracking for revert |
| `core/src/markdown/parser.rs` | Rust: markdown → Yjs XmlFragment |
| `core/src/markdown/serializer.rs` | Rust: Yjs XmlFragment → markdown + plain text |
| `core/src/server/yjs.rs` | Yjs doc management, AI edit application |
| `core/src/tools/page_editor.rs` | AI edit tool (strips markdown, applies via Yjs) |
