# Editor kernel: the chat composer moves to the block editor

On 2026-10-20 the chat composer moved from CodeMirror onto the block
editor's kernel (Tiptap on ProseMirror, the page contract's schema), and
CodeMirror left the app. It was the last CodeMirror surface; block pages
had moved on 2026-10-16 ([document-contract.md](document-contract.md)).
How the composer works now is in
[build/document.md](../build/document.md#the-chat-composer).

## Why the 2026-09-14 reason no longer held

[editor-kernel-2026-09-14.md](editor-kernel-2026-09-14.md) kept the
composer on CodeMirror because reading markdown into ProseMirror lost text
without saying so, and the composer's document had to be the exact string
the model receives. Two things the document contract built removed that
problem:

- **Markdown in** goes through the crate's own converter, compiled to
  WebAssembly and run in the page (`converter.ts`). It is the same reader
  the server uses, and it reads a paste as text where HTML would lose words.
  There is no second parser.
- **Markdown out** is the page's markdown export (`markdown.ts`), held
  byte for byte to the crate's `render::markdown`. The composer adds a mode
  that writes words as typed (`messageMarkdown`). There is no second
  serializer.

What closes the gap the 2026-09-14 record measured: a draft or a prefill is
opened as the converter reads it only when that reading writes back as the
same text, and as its text otherwise (`readMessage`). Opening a message can
never change it.

## What a message holds, and what is sent

A message holds part of the contract: paragraphs, line breaks, lists, a
quote, a code block, mentions, and inline marks and links. Headings,
tables, media, applets and to-dos stay text, because the thread shows a
user's message as typed (`UserMessage.svelte`), and `# Title` or
`- [ ] milk` reads there as it was written.

The wire format did not change. The CodeMirror composer was driven through
26 typed messages before it was deleted and its output recorded
(`apps/web/src/lib/document/composer.corpus.ts`). The new composer sends
the same bytes for 24 of them. The two that differ were changed on purpose:

| Typed | CodeMirror sent | Now sent | Why |
|---|---|---|---|
| a list, Shift+Enter twice, `done` | `- pears\ndone` | `- pears\n\ndone` | markdown read the old line as part of the last item |
| `* a`, Shift+Enter, `b` | `* a\n* b` | `- a\n- b` | a list is written with `-`, whatever sign started it |

Mark rules read delimiters as CommonMark does. Tiptap's own rule read
`2 * 3 *` as emphasis and sent `2  *3*  4`; the composer's (`flanking` in
`composer.ts`) leaves it as typed. Block pages still use Tiptap's rules.

## What was deleted

`apps/web/src/lib/codemirror/` (the composer factory, its language, theme,
caret, keybindings, inline-mark and code-context helpers, list renumbering,
mouse freeze, ref links and the ref picker, about 3,200 lines with tests),
the `@codemirror/*` and `@lezer/markdown` dependencies, the
`--selection-blend` theme token only the composer read, and
`agents/build/codemirror.md`. `onContextGesture`, which the block editor's
node views use, moved to `components/contextMenu/gesture.ts`.

## Checked

`composer.test.ts` (the corpus, drafts read back over 23 cases, keys, refs,
fences and pastes) and the whole web suite pass, and `pnpm check` reports no
errors. The real chat was driven in a browser on a scratch database with
invented text: typed marks, a list started under a line, a ref picked with
`@`, a code block opened with Enter and sent with Cmd+Enter, a draft kept
across leaving the chat and a reload, markdown pasted, and Return on a
phone layout. The stored message matched what the corpus predicts. The
Tauri apps were not run. On iOS, ProseMirror lets the keyboard's Return
through and replays it as Enter, the same path block pages use, so a phone's
Return reaches the composer's own Enter.
