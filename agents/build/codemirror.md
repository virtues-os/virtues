# CodeMirror: the chat composer

CodeMirror 6 is the editor kernel for the chat composer and is used nowhere
else. Pages are
edited in the block editor ([document.md](document.md)); the decision for
pages is [record/editor-kernel-2026-10-07.md](../record/editor-kernel-2026-10-07.md),
and [record/editor-kernel-2026-09-14.md](../record/editor-kernel-2026-09-14.md)
still governs the composer.

**The editor's document is the message itself:** a markdown string, exactly
what the model receives. There is no Yjs, no live preview of headings or
emphasis, and no slash commands, because a message box that renders formatting
while you type behaves like a word processor.

## Files

Everything is under `apps/web/src/lib/codemirror/`.

| File | Role |
|------|------|
| `composer.ts` | `createComposerEditor`: the composer's configuration, its Enter (send, except inside a code fence: `inFencedCode`), paste (long text becomes an attachment), and `closeOpenFence` on send |
| `language.ts` | `virtuesMarkdown`: the one markdown dialect (CommonMark, GFM, `==highlight==` from `highlight-parser.ts`) |
| `theme.ts` | the base theme |
| `extensions/caret.ts`, `caret.css` | `smoothCaret`: the drawn caret, sized to the type, gliding on keyboard moves, a soft blink once still; its CSS travels with it |
| `extensions/keybindings.ts` | `markdownKeybindings`: Mod-b, Mod-i, Mod-e or Mod-`, Mod-u, Mod-Shift-s or x, Mod-Shift-h toggle marks, Tab and Shift-Tab indent list items, Backspace at a heading's start takes its marker; `empty-mark-tidy.ts` clears a pair of delimiters left with nothing between |
| `extensions/inline-marks.ts` | where each inline construct's delimiters are, and whether the selection touches it; `code-context.ts` says where markdown syntax is code |
| `extensions/list-renumber.ts` | keeps an ordered list's numbers sequential |
| `extensions/mouse-freeze.ts` | holds selection-driven rebuilds while a pointer is down |
| `extensions/ref-links.ts`, `ref-links.css` | `entityLinks`: `[label](url)` drawn as a plain link, `@` stripped, with a hover preview and a context menu |
| `extensions/ref-picker.ts` | `@` opens the ref picker (`ChatInput.svelte`); `insertRef` writes `[@Label](/type/id)` |
| `extensions/long-press.ts` | `onContextGesture`: right-click or a 450 ms hold, since iOS fires no `contextmenu`. The block editor's node views use it too |
| `test-utils.ts` | mounts a view under happy-dom with the composer's language |

## Links

`ref-links.ts` renders every `[label](url)` (entity, file, internal path or
external URL) with one widget: a plain link styled as part of the prose
(`.cm-ref-link`, dotted hairline). There is no pill, chip or favicon; the `@` is
stripped for display. The target appears in a hover preview
(`RefPreview.svelte` over `RefCard.svelte`), never in inline chrome.
`getEntityTypeFromRoute` (`lib/utils/refRoutes.ts`) determines what kind of
record a route names.
