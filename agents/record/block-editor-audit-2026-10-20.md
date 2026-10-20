# Block editor audit against Linear docs, 2026-10-20

The block-page editor (Tiptap 3 on ProseMirror and Yjs, `apps/web/src/lib/document/`)
was driven by hand in the running web app and checked against what Linear docs
does with the same keys. The bar was keys, caret, paste and undo, not typography.
Fixes landed in `a16474706`, each with a test in `editor.test.ts` or
`LinkEditorPopover.svelte.test.ts`.

## How it was tested

- A core on a scratch database and a vite server pointed at it, so no test
  page touched a real record. The page held invented text: headings, nested
  bullets, a numbered list, to-dos, a quote, a Python block, a table, a rule
  and a closing paragraph with bold, italic, code and a link.
- Keys went through the browser pane as real key events. The selection was
  placed and the result read through the editor instance in the page.
- The pane's `type` action sends a whole string as one input event, so a
  shortcut prefix was typed apart from the text after it. ProseMirror reads
  the DOM selection a moment after an arrow key. Neither is an editor bug.
- Not tested: long-press menus (the pane's touches arrive as mouse events);
  pastes from the real Google Docs, Notion and Slack apps (stand-in HTML of
  their shapes was pasted); images and applets in the page (none in the
  scratch box's Drive).

## Results

Steps start from the test page; "at X" means the caret is placed there.

| Behavior | Steps | Result |
|---|---|---|
| Enter at the end of a nested item | at end of a level-2 item, Enter | pass: a new level-2 item |
| Enter on an empty item | then Enter, Enter | pass: lifts a level, then leaves the list |
| Backspace at an item's start | at start of a level-2 item, or the last top item | pass: lifts it a level, or to a paragraph; no join |
| Backspace at the first item that has children | at start of the first top item, which holds a nested list | **fixed** in 1861cda2a: it became a paragraph and left its children and the rest of the items as two bullet lists side by side. Now one list |
| Enter and Backspace on to-dos | Enter at end of a ticked item, Enter on an empty one, Backspace at start | pass: new unticked item; leaves the list; lifts to a paragraph |
| Enter and Backspace in a quote | Enter twice at the end; Backspace at start | pass: leaves the quote; lifts the line out |
| Enter in a heading | Enter at its start, middle and end | pass: paragraph above; split heading; paragraph below |
| Enter in a code block | Enter after an indented line; three Enters at the end | pass: keeps the indent; leaves the block |
| Backspace and Delete next to a rule, table or code | Backspace after a rule; Delete at a code block's end | pass: selects the rule, then deletes it; caret moves into the table, nothing joins |
| Tab in a code block | at a code line, Tab | **fixed**: focus left the page for the next checkbox. Now two spaces; Shift-Tab outdents |
| Tab on an item that cannot nest | at the first item of a list, Tab | **fixed**: focus left the page. Now taken, nothing moves |
| Tab and Shift-Tab in lists | at a second item, Tab, then Shift-Tab | pass |
| Cmd-A in a code block | at a code line, Cmd-A | **fixed**: selected the whole page. Now the code first, the page on a second press |
| Markdown shortcuts | type `# `, `1. `, `[] `, `- [ ] `, `> `, three backticks then a space, `---` | pass: each makes its block |
| Backspace right after a shortcut | type `# ` or `- `, Backspace | **fixed**: emptied the line. Now gives back `# `, the block keeping its id |
| Typing after a link | at the end of a link's text, type | **fixed**: the letter joined the link. Now plain text |
| Typing after bold and code | at the end of bold or inline code, type | pass: continues the mark, as Linear does |
| Cmd-B, I, U, E | select a word, press each | pass |
| Cmd-K on words | select a word, Cmd-K, paste an address | **fixed**: focus was in the Text field, so the address replaced the words. Focus now goes to Link for a new link |
| URL pasted over words | select a word, paste an address | pass: makes a link |
| Arrows across a table and a rule | Down from the last row, Down again, Up | pass: gap cursor after the table, the rule selected, back up |
| Click below the last block | click under the last line, inside the editor's box | pass |
| Click on the blank page below the editor | click in the scroller's padding under the editor | **fixed**: blurred the editor. Now caret at the end |
| Triple-click | triple-click a quote's line | pass: that paragraph only |
| Drag-select across blocks | drag from a numbered item into the quote | pass |
| Undo of typing and of a paste | type a phrase, wait, paste, Cmd-Z, Cmd-Z, Cmd-Shift-Z | pass: paste one step, the phrase one step, redo works |
| Undo of quick formats | Cmd-B, I, U, E within half a second, Cmd-Z | note: one step undoes all four. y-undo groups changes within 500 ms, as ProseMirror's history does |
| Undo leaves another device's edit | type in a second tab, type here, Cmd-Z twice | pass: the other tab's words stay |
| Paste: markdown, Google Docs and web HTML | paste each into an empty line | pass: headings, lists, marks, table and image |
| Paste: chat HTML with a `<br>` | paste a line, a `<br>`, a line | pass: a hard break |
| Paste: code as plain text | paste a Python function | pass: lines as written, per `document.md` |
| Copy out and paste back | select from the list to the quote, copy, paste at the end | pass: same structure |
| Code colours after a drag | drag a selection and drop it where it started | **fixed**: the code block lost its colours until it was edited |
| Slash menu by keyboard | `/li`, Down, Enter; `/` then Escape | pass |
| Slash menu matching | `/nmbr` | **fixed**: matched nothing. Letters in order now match after exact ones |
| Slash menu near the bottom, click away | `/` on the last line of the screen; click elsewhere | pass: opens above; closes |
| `@` menu | `@` and a query, Escape | pass: closes, keeps the text and focus |
| Placeholder | empty line with and without the caret; empty list item | pass: only on the caret's empty paragraph |
| Layout on focus and blur | blur and focus the editor | pass: no change in height |
| Caret in view while typing | Enter 25 times at the end, type | **fixed**: the caret rode 5px from the bottom edge. Now 80px |
| Mobile width: selection toolbar | 375px wide, select a word | pass: toolbar under the selection |
| Mobile width: page toolbar | 375px wide, a page with headings | open: the centered outline title is drawn over the toolbar's icons |

## Still open

- At phone width, `PageOutline`'s centered title overlaps the toolbar icons.
  It is the page toolbar's layout, not the editor's: something has to give
  way below a width, and which one is a design decision.
- Long-press menus and real cross-app pastes need a device and the apps
  themselves.

Fixed after the audit, in 1861cda2a: Backspace at the start of a first item
that has children, and `[] ` typed right under a to-do list, each left two
lists of one kind side by side, which looked like one and exported as two
(`continuesList`). Each now leaves one list.
