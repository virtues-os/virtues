# One prose stylesheet for chat, pages and the composer

**Status: planned (2026-10-21).** The same markdown is styled three times
today, and the three copies disagree. This plan puts every rule that styles
rendered text in one stylesheet, `src/lib/styles/prose.css`, and leaves each
surface only what is its own: the editor's caret, selection and block
controls, chat's streaming, the composer's compactness.

## What exists today

Three stylesheets style the same elements. All three read the `--md-*`
tokens in `themes.css` (196-237), and each then sets its own margins,
padding, colors and fallbacks on top.

| Surface | Class | Stylesheet | Renderer |
|---|---|---|---|
| Chat replies, the wiki, day and entity articles, asset text, the setup interview | `.markdown` (`.markdown--article`) | `app.css` 325-604, plus `MarkdownCodeBlock.svelte` and per-surface overrides | `Markdown.svelte` over svelte-streamdown 3.1.2 |
| Pages and the version-history preview | `.doc-prose` | `lib/document/document.css` | Tiptap / ProseMirror, with Svelte node views |
| The chat composer | `.composer-prose` | `ChatInput.svelte` 546-614 | the same editor kernel as pages |

Streamdown also applies its own default theme as Tailwind classes. `.markdown`
rules are unlayered and win over those classes, but wherever app.css sets
nothing for a property, Streamdown's value leaks through.

## Where they disagree

The same markdown renders differently in a chat reply and on a page:

| Element | Chat (`.markdown`) | Page (`.doc-prose`) | Composer |
|---|---|---|---|
| Block gap | `mb-4` on p and lists | `> * + * { margin-top: 1em }` | 0.375rem |
| Heading margins | top 1.5rem leaked from Streamdown (zeroed for h1-h4 in chat only), bottom 1.5 / 1.25 / 1 / 0.5 / 0.25 / 0.25rem | top 1.5 / 1.4 / 1.25 / 1.1 / 1 / 1em, then 0.5em before the next block | none |
| h4-h6 family | serif (the global `h1-h6` rule) | inherits the body, as the token comment says | n/a |
| h6 color | foreground | muted | n/a |
| List items | 0.35em plus a leaked 0.25rem `py-1` each side | 0.35em | rem indents |
| Task lists | native checkbox, no styling | custom checkbox | n/a |
| Blockquote | italic (Avenir has none, so it is synthesized), muted color that never reaches its `<p>` | upright, muted | 2px strong border |
| Inline code | padding 0.125rem 0.375rem | 0.1em 0.3em | 0.0625rem 0.25rem, no border |
| Links | primary, underline, hover color | primary, 1px underline at 0.15em | inherit, dotted |
| `mark` | no rule | highlight token | highlight token |
| Code block | 12px frame, header in mono (breaks the mono rule), `p-4` | 12px frame, header in sans, `0 16px 12px` | 0.5rem 0.625rem |
| Tables | border on the wrapper, cells 14px and 200-400px wide (leaked), serif `th` at weight 600 (cannot render) | border on the table, cells at body size, sans `th` | n/a |
| Images | 10px corners | 6px | n/a |
| Rule | `my-6` | 1.5em | n/a |
| Callouts, footnotes, del, `dl` | Streamdown's hard-coded grays and blues, unthemed in dark mode | themed in node views | n/a |

Dead or broken on the chat side, from the inventory:

- `[data-streamdown-code]` rules (app.css 428-440), `customTheme.code`, the
  `shikiTheme` prop and its listener: the `code` snippet replaces that
  element.
- `[data-streamdown-table]` (521-524) and the table download control: the
  `table` snippet replaces that wrapper.
- `parseIncompleteMarkdown`: Streamdown 3.1.2 never reads it.
- Font-size and color set on a wrapper around `.markdown` never reach the
  text, because `.markdown` sets both on itself (StepInterview `.question`,
  `.example`; PdfPane `.pdf-note-preview`).
- `DayArticleBody`'s `.markdown > :first-child` misses Streamdown's
  `.streamdown-content` wrapper.
- A citation renders as a button inside a button.
- Math is enabled but nothing renders TeX.

On the page side, `--md-h3-weight: 500` and chat's serif `th` at 600 ask the
serif for a weight it does not ship, so both draw at 400 without a warning.

## The design

### One class, three variants

Every renderer puts `prose` on its root. The variants are modifiers:

- `prose` — the body text: chat replies, pages, asset text.
- `prose prose--article` — the serif reading text of the wiki and day
  articles, and a page in Serif mode (see below).
- `prose prose--compact` — the composer: the same elements, tighter rhythm.

`prose.css` writes every selector inside `:where(.prose)`, so a surface
overrides it with a plain class and no specificity contest. It stays
unlayered, so it still wins over Streamdown's Tailwind classes.

### What prose.css owns

Every element both renderers emit, by semantic HTML: p, h1-h6, strong, em,
s, u, mark, inline code, links, ul, ol, li, task lists, blockquote, hr,
tables, images, callouts, code blocks, footnotes. The values come from the
`--md-*` tokens, extended with the ones each copy hard-codes today:

- `--prose-gap` (the block gap), `--prose-heading-space-1..6` (space above a
  heading), `--prose-after-heading`
- `--prose-indent`, `--prose-item-gap`
- `--prose-code-pad`, `--prose-link-offset`, `--prose-image-radius`

Each surface tunes rhythm by setting these, never by rewriting the rule.
The composer's compact variant is a block of token values and nothing else.

### Shared block markup

Three blocks have chrome around their content, so one stylesheet only works
if both renderers emit one structure. Each gets shared classes, which
`MarkdownCodeBlock` and the node views both use:

- **Code block:** `.prose-code`, `.prose-code-head` (language, copy),
  `.prose-code-body`. Chat's header loses its mono and takes the page's.
- **Callout:** `.prose-callout` with `data-tone`. Streamdown's `alert` gets
  these classes through its theme, so `> [!NOTE]` in a reply looks like a
  page's callout.
- **Task list:** the page's checkbox for both. In a reply it is disabled.

### Streamdown's theme, neutralized

Pass a `customTheme` that empties every element's `base` classes. prose.css
then owns every property, and the leaks go away: heading `mt-6`, li `py-1`,
the 200-400px cells, and the grays on del, footnotes, alerts, `dl` and
blocked links. Chat-only behavior stays in Markdown.svelte: the word fade,
its reduced-motion override, citations, and query refs.

### What each surface keeps

- `document.css`: only the editor — `min-height`, caret, selection,
  placeholder, find highlights, collaboration carets, the assistant's marks,
  focus mode, proposals, the grip, drag source, the A4 sheet, print.
- `app.css`: nothing for elements. The `.markdown` block goes.
- `ChatInput.svelte`: the composer's caret, placeholder and field padding.
- ChatView: the reply's own spacing, such as `padding-top` and the welcome
  standfirst.

### Display settings

`pageDisplay` sets the prose tokens instead of `--editor-*`. Two behaviors
change:

- **Serif mode becomes `prose--article`**: 17px on a 1.65 line. Today a
  Serif page keeps 16px, and EB Garamond's x-height (0.405 of the size,
  against Avenir's 0.468) makes it read about 13% smaller than chat and
  smaller than the wiki's own serif.
- **Headings scale with the text size.** Today they are rem-sized and stay
  put at S and L, while h4-h6 do follow the font mode.

## Order of work

Each slice ships on its own, and the surface it touches looks the same
before and after unless the slice says otherwise.

1. **Tokens.** Add the `--prose-*` tokens with today's page values. Set
   `--md-h3-weight` and `--md-th-weight` to 400, the weight the serif
   renders anyway. First confirm Tailwind keeps an `@theme inline` variable
   that only a plain stylesheet reads; if it prunes it, move the prose
   tokens to a plain `:root` block.
2. **Pages onto prose.css.** Create prose.css from document.css's content
   rules, add `prose` to the editor's attributes and DocumentPreview, and
   delete those rules from document.css. No visible change. Move
   `access.test.ts`'s `read(...)` targets for the task list, `.tableWrapper`,
   `mark` and `a` rules to prose.css in the same commit.
3. **Chat onto prose.css.** Neutralize Streamdown's theme, put `prose` on
   Markdown.svelte's root, delete `.markdown`'s element rules and the dead
   rules listed above, and fix the `.streamdown-content` nesting and the
   wrapper overrides that never reached the text. Chat's look changes to the
   page's in every row of the table above. That is the point of the plan,
   and the decisions below pick the values.
4. **Shared blocks.** Code block, callout and task-list markup on the shared
   classes in both renderers.
5. **Article variant.** `prose--article` for the wiki and day articles
   (including DayArticleBody's hand-built paragraphs), and for Serif mode on
   pages.
6. **Composer.** `prose prose--compact`, and delete ChatInput's copies.
7. **Hold it.** A test fails when a selector naming an element under
   `.markdown`, `.doc-prose` or `.composer-prose` appears outside prose.css.
   One rule, so the copies cannot grow back.

## Checking it

Slices 2-6 are visual, and two of them change what chat looks like on
purpose. Before slice 3, add a dev-only route that renders one invented
specimen, every element and block once, through `Markdown.svelte` and
through `DocumentPreview` side by side. Screenshot it before and after each
slice, in a light and a dark theme, at Sans and Serif. The specimen's text
is invented, per `.claude/rules/fixtures.md`.

Two other constraints:

- `tools/design-lint.sh` exempts only `themes.css` and `app.css` from its
  color and mono rules. prose.css's mono lines (inline code, code blocks)
  need `design-ok` comments, as document.css's have.
- The ratchet baselines are cut from a clean clone, never from this
  checkout.

## Decisions to make before slice 3

Each has a recommended answer, which is the page's value unless noted.

| Question | Recommended | Why |
|---|---|---|
| Block rhythm | margin above each block after the first (`> * + *`, 1em) | One rule covers every element and nesting; `mb-4` leaves trailing space inside quotes and list items |
| h4-h6 family | the body's | The token comment already says so; a serif h5 at 15px is smaller than the body it heads |
| h6 color | muted | It is the only level a size step alone does not separate from body text |
| Blockquote | upright | The serif is never italic (grammar §4), and Avenir ships no italic, so chat's is synthesized |
| Table cells | body size; header 13px, body family, weight 400 | A table in a page is content, not a caption; the serif header at 600 cannot render |
| Image corners | 6px (`--radius-md`) | The radius set is {0, 6, 12, 50%, pill}; chat's 10px is off it |
| Code block header | the page's sans label | No mono outside a chart's labels and a clock (grammar §4) |
| Links | primary, 1px underline at 0.15em, chat's hover color | Combines the two; refs stay `ref-badge.css` |
