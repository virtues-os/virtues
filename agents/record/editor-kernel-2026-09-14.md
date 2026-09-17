# Editor kernel: CodeMirror, not ProseMirror

**Decided 2026-09-14.** Every text surface that hands markdown to the model
or takes markdown from it runs on CodeMirror. The document is a markdown
string on every surface. No surface holds a tree that the markdown is a
serialization of. This record is the evidence, so the question does not get
reopened on the strength of "everyone uses ProseMirror."

## Why it came up

A polish ticket asked for list continuation in the chat composer (type
`- foo`, press Shift+Enter, get `- `). The composer is a hand-rolled
`contenteditable` with a tree walker that flattens pills into markdown on
send. Pages already run CodeMirror 6 with decorations over a markdown text
buffer (`agents/build/codemirror.md`). The chat composer was the one surface
not on that kernel.

The open-source chat products that wanted rich composers (Open WebUI,
Hugging Face chat-ui, Continue, LobeChat) all reached for a tree editor
(TipTap on ProseMirror, or Lexical) and then had to own an HTML-to-markdown
serializer. ChatGPT and Claude also run ProseMirror under the composer, but
as a well-behaved contenteditable: nothing structural reaches the user. So
the question was whether a tree editor with a markdown converter could be the
single kernel for Pages and chat, and whether the converter is lossless
enough for a product whose stored document is the markdown.

## Method

Parse markdown into the editor's document, serialize it back, and compare.
Two corpora:

- **89 repo docs** (`docs/`, `agents/`): hard-wrapped prose with tables,
  code fences, task lists, strikethrough, front matter, raw HTML.
- **74 real pages** from a development database: soft-wrapped prose written
  in the CodeMirror page editor, mostly paragraphs and links.

Two converters:

```
prosemirror-markdown 1.13.7   default schema (CommonMark)
@tiptap/markdown 3.31.3       StarterKit + Table + TaskList + TaskItem
```

Comparison is on the rendered HTML of input and output with whitespace
normalized, so unwrapping a hard-wrapped paragraph does not count as a loss.
Byte-identical is reported separately, because a converter that reformats a
document it did not edit is its own problem here.

## Results

| Round trip | Repo docs (89) | Real pages (74) |
|---|---|---|
| prosemirror-markdown, default: render-identical | 17 | 72 |
| TipTap with GFM extensions: render-identical | 51 | 73 |
| TipTap: byte-identical (no reformatting) | 0 | 50 |

Default ProseMirror has no table, task-list, or strikethrough nodes: tables
flatten to a paragraph, task boxes and `~~` become escaped literal text. The
TipTap stack carries those and recovers most of the docs corpus.

The remaining failures are the finding. Each is silent: the output is a
plausible document and nothing errors.

- **Silent deletion.** A literal `<time>` in prose is gone after the TipTap
  round trip. Anything that looks like a tag and is not in the schema is
  dropped.
- **Silent injection.** Every `&` in a heading comes back as `&amp;`. It
  renders the same, and the stored text now contains an HTML entity the
  model reads and the owner did not write.
- **Silent restructuring.** Indented continuation lines inside list items
  were rewritten; in one doc an indented paragraph became a fenced code
  block. List nesting changed in seven docs. Front matter lost its second
  line.
- **Churn without edits.** TipTap reformatted 24 of 74 pages on open with
  no keystroke: table padding, bullet markers, bare URLs autolinked.

Raw HTML blocks survived both converters. Ref links are ordinary markdown
links and survived both.

## Why churn is the disqualifier here

The percentages say the converter works for pages as people write them today
and fails at the edges the model and power users produce. That would be
acceptable for a message box. It is not acceptable for this product, for two
reasons that have nothing to do with the percentages.

The model's edit tools anchor on exact text spans. An editor that reformats
a page on open means the model's next edit misses its anchor. Pages
collaborate over a Yjs text buffer, so a reformat is a write the other side
did not make. The string stops being the shared thing, and the string being
the shared thing is the whole premise of a person and a model working the
same document.

The failure class is the one this repo already refuses in queries: a lossy
step produces a plausible result and nothing surfaces. With a tree editor
that is the steady state, not a bug to fix.

## What a tree editor would have won

Two things, honestly: dragging blocks, and tabbing through table cells the
way Notion does. Both are rendering conveniences. The WYSIWYG preference is
real, and the CodeMirror answer to it is the reveal-on-touch live preview
Pages already have, pushed further where it is worth it.

## TipTap versus ProseMirror

ProseMirror is a kernel (model, state, view, transforms, schema) by the same
author as CodeMirror, MIT, sponsorship-funded. TipTap is the batteries on top:
each node or mark as an extension, framework bindings, a Yjs extension, and
since v3 an official markdown extension. The venture business is the cloud
services and a shifting set of paid extensions. For this decision the
difference was only whose converter we would maintain. TipTap's handled GFM
better and carried the three silent failures above. Either way the
serializer becomes a file we own forever.

## Consequences

- The chat composer moves onto CodeMirror as a second, smaller configuration
  of the page editor's kernel: Enter sends, Shift+Enter runs the existing
  continue-markup command, the existing `@` picker and ref-pill extensions,
  no live preview of headings or emphasis, no Yjs. The tree walker, the
  saved-text-node mention hack, and the DOM serializer are deleted.
- The list-continuation ticket closes inside that swap rather than as a
  keydown patch on the contenteditable.
- Short fields stay plain textareas.

## Reproducing

The spike is thirty lines and worth rerunning if a schema changes:

```js
import { defaultMarkdownParser as p, defaultMarkdownSerializer as s } from 'prosemirror-markdown';
import MarkdownIt from 'markdown-it';
const md = new MarkdownIt();
const ws = (x) => x.replace(/\s+/g, ' ');
for (const src of corpus) {
  const out = s.serialize(p.parse(src));
  const sameBytes = out === src;
  const sameRender = ws(md.render(src)) === ws(md.render(out));
}
```

For TipTap, stand up an `Editor` under jsdom with `StarterKit`, `Markdown`,
`TableKit`, `TaskList`, `TaskItem`; load with
`setContent(src, { contentType: 'markdown' })` and read `getMarkdown()`.
