# Editor kernel: pages move to a Yjs tree; the composer stays on CodeMirror

**Decided 2026-10-07.** Supersedes
[editor-kernel-2026-09-14.md](editor-kernel-2026-09-14.md) for pages. The
evidence is [document-contract-spike-2026-10-07.md](document-contract-spike-2026-10-07.md);
the work is [plan/document-contract-plan.md](../plan/document-contract-plan.md).

## What is decided

- **A page is a Yjs XML tree**, edited with Tiptap's MIT core, which is
  ProseMirror underneath. The tree is the stored document. Nothing converts
  on open.
- **One contract file** names every node, mark and attribute a page may hold
  and the HTML each reads from and writes to. The browser builds its schema
  from it and the Rust server checks writes against it. Tiptap supplies
  behaviour (keymaps, commands, collaboration); it does not supply the schema.
- **The model writes HTML**, one block at a time, addressed by block id and
  carrying the version it read. The server refuses anything outside the
  contract with a message the model can act on, and merges or refuses a write
  whose block changed since the read.
- **The model reads** HTML with block ids for the blocks it edits, and the
  markdown export for whole-page context.
- **Markdown is a projection.** The `content` column holds the markdown
  export, materialized on save, so search, embeddings, publishing and the
  wiki's readers keep reading text.
- **The chat composer and chat messages stay** on CodeMirror and markdown.
  Chat is model output, and the composer works.

## Why the 2026-09-14 decision no longer holds for pages

That decision measured ProseMirror with markdown as the stored document and
was right about it: reformat-on-open is a write nobody made. The design
decided here never parses markdown on open, so that failure has no place to
happen. What remains of the old concern is at the model's write, and there
it is refused out loud instead of normalized silently: of 19
out-of-contract inputs, Rust refused all 19 and ProseMirror's parser changed
all 19, losing text in two.

Meanwhile the cost moved to CodeMirror's side. Pages now need many widgets,
documents in the Notion and Google Docs sense, and print. A text editor holds
a widget as a foreign object it measures and steers around, and every cursor
bug Pages has had came from that.

## Not decided here

- Live A4 pagination: not built. A page view and print cover A4.
- Tiptap's paid extensions and hosted services: not used. Our server is the
  sync server.
- When articles (`kind = 'article'`) move: last, after their writers are
  ported. See the plan.

## Correction (2026-10-07, same day)

"Search, embeddings, publishing and the wiki's readers keep reading text"
is wrong about publishing: `api/publish_page.rs` decodes `yjs_state` as
Y.Text rather than reading `content`, so it must be ported for tree pages
(plan, slice 1). The decision is unchanged.
