# Page grammar

[design.md](design.md) covers the **shell**: the window chrome and the
sidebar. This file covers the **page**: everything rendered in the pane. Where the
two touch (the 8pt grid, the interaction ramp), design.md wins; on radius this
file wins, and says why below.

This file exists because Getting Started was redesigned eight times on
2026-09-04 and every review was taste against taste. The first grammar,
written mid-way, described a printed book page — hairlines, oldstyle numerals,
a shoulder column, 6px cards — and the user called every build of it "an old
Windows XP thing." What we learned: the user responds to atmosphere, depth and
real controls, and dislikes bare pages. "Academic" here means the serif, the
pictures and the lines. It does not mean small gray type and rules.

What replaced it was a **spread** — a painting in the margin beside the work.
That was removed too; [Removed designs](#removed-designs) explains why, and
that explanation is the more useful part. What the pages share now is a
convention for **authorship**: each block shows who is speaking. §2 describes
it.

Order of precedence: the user's instructions, then this file, then personal
taste. Check claims against the code, not against an earlier revision of this
file. Every claim below was checked against the tree on 2026-09-16; unchecked
claims are how this document went stale the first time.

---

## 1. One sentence per page

Every page has one sentence that states its purpose, written down before
anything is drawn. Every object on the page must either **develop** that
sentence or **support** it with evidence. Remove an object that does neither,
however good it looks.

- A day article: *yesterday, written down.* The autobiography develops it; the
  dayline and the sources support it.
- The Getting Started room: *your server is reading your life.* The four steps
  develop it; the introductions it writes back support it.

The sentence may appear once, under the title, in the sans. It is the only
prose the page writes for itself; a row or a step gets at most one line under
its title. **Do not write text just to fill a slot.** A section with nothing to
say does not render, and a measure that needs a week of history before it
means anything shows nothing for that week instead of a meaningless score.

## 2. Framed and unframed blocks

**An unframed block is the box speaking; a framed card is where you answer.**
This is the only decoration a page has, and it carries meaning, so decide which
kind a new block is before you choose its style. (The rule was first written
for the home page, which was laid out as turns in a conversation. That page is
gone, see [Removed designs](#removed-designs), and the rule still applies.)

In the code the distinction is **framed or unframed**, and nothing else. The
box's turns are unframed: a serif line at the page's measure, flush to the
paper, no border and no ground of its own. Your turn is framed: `--color-surface`
inside a 1px `--color-border` at 12px, because it is a surface you write on and
the border is the only affordance the question needs. (Older notes call the
unframed turn a "rule". No hairline is actually drawn, so do not add an
`<hr>`.)

**The measure.** One column: the page is `max-width: 920px`, `.work` is
`56 / 56 / 48 / 64`, and every turn is capped at `40em`. A chart alone takes the
full width. Nothing inside the work paints its own ground; the pane's surface
is the page's.

**A room has one measure, and its padding lives on the outer scroller** — the
measure is centered inside that padding, never padded from within. The wiki
hand-rolled the page shell and padded inside the centered block, which landed
its text about 48px further in than every neighboring room *even though both
claimed 72rem*, so the columns fail to line up when you move between tabs. Its
own sections had run 72 / 54 / 50 / 44rem before that, shifting the column on
every tab change inside one room. There are exactly two measures: one for
grids and one for prose. When a layout responds to width, **use a container
query, not a viewport media query.** The pane is what runs out of space, and in
split view a pane is much narrower than the window.

**Three page genres**, so a reader knows which grammar applies before styling
anything:

- **The article** — the wiki's day, person, place, year. A centered title page
  (h1, meta, a 3rem hairline), one 48rem measure, `.section-title` sections
  that hide when empty, and rails either side for contents and notes. It is for
  reading, with no conversation on the page.
- **The room** — chat, and Getting Started since 2026-09-13. The thread is the
  whole page. Nothing is placed beside it; a picture sits inside the column at
  the measure of the words (`getting-started/RoomCover.svelte`).
- **The tools** — Settings, the lists. `Card` for surfaces,
  `UniversalDataGrid` for rows, the type scale below, and no page furniture of
  their own.

## 3. Sequence and progress

A sequence with state is numbered **on the reading axis** — as headings in the
thread, the way a conversational form numbers each question — and not as a
decoration beside it. `getting-started/StepEyebrow.svelte` is the shape:
`1 of 4 · Connect AI`, set as an `<h2>` so the room has the hierarchy of an
authored page rather than a caption strip. The last heading is where you are; a
step settled before it was ever asked still gets its heading, carried by its
settling line.

A step's work happens in the step when it is small (three fields, three Connect
rows, a sign-in) and goes to the page where it permanently lives when it is
large (the interview, the manual). Controls sit on their own line under the
words they belong to, never butted against a sentence
(`getting-started/ui/Choices.svelte`) — one rhythm everywhere in a room.

**Show progress as text.** Per-step marks — one glyph per step, inked for done
and hollow for later — were tried and looked like stray punctuation: the inked
run rendered as `— ——`, which looked like a typo. A fraction (`1 of 4`,
`3 of 7`) gives the same information and cannot be misread.

These were built and then removed: chips, a progress rule, an accordion, and
the vertical claret-dot stepper that replaced all three. See
[Removed designs](#removed-designs).

## 4. Type

Four faces, the ones `themes.css` declares: `--font-serif` (EB Garamond),
`--font-serif-ui`, `--font-sans` (Avenir), `--font-mono` (IBM Plex Mono).

`--font-serif-ui` is the same file with its vertical metrics normalized, and
it exists for **serif set beside an icon or inside a fixed-height row**.
EB Garamond declares ascent/descent 1.007/0.298, so centering its line box in
a row leaves the letters off-center against the icon beside them; the UI
registration sets the metrics so the cap band lands on the center line. The override is expressed in em, so
it holds at 13.5px and at 46px alike. Prose keeps `--font-serif`: applying the
correction there would move first-baseline position across the wiki and pages.

One scale:

| Size | Face | Used for |
|---|---|---|
| 36 | serif | the page title |
| `--md-h2-size` (22) | serif | a step's heading; a section title |
| 18 | serif | the box's line — the novelty caption, the ask, a list row, a card's question |
| 17 · 16 | serif | what you are typing · what you kept |
| 15 | sans | the sentence under the title; a paragraph |
| 14 | sans | buttons, links |
| 13 | sans | kickers, labels, quiet verbs, notes, dates, a chart's readout |
| 12 | sans / mono | hints and captions; mono only for a clock time |
| 9 – 9.5 | mono | a chart's own hour ticks and lane names, inside the SVG only |

**Take the size from the scale; do not invent one.** A step heading was set at
`1.25rem` because it looked about right, and it sat a step *below* the scale
rather than on it; reading it off `--md-h2-size` put the room's two serif
headings on one scale for free. The same applies to the markdown tokens: they
are the single definition both the read path and the editor cite, and changing
one there moves page and editor together. That is what keeps the editor
WYSIWYG.

**Use at most two text styles on a surface.** The mobile drawer had five text
styles and a bordered card, and looked assembled from parts. What ships has a
16px row and a 13px muted caption, with the serif masthead as the single
exception. A surface that needs a third text style usually has too many things
on it.

**A section heading is larger than the rows under it.** Overview headings were
once 11px serif — *smaller than their own body text* — so nothing on that page
looked like structure.

**Figures are lining and tabular** (`font-variant-numeric: tabular-nums`)
wherever numbers sit in a column or update in place, so `0` and `10` occupy the
same width and a row of cards agrees on a baseline instead of twitching.

Prohibitions, each of which has shipped:

- **The serif is never bold and never italic.** We ship the serif with no bold and no italic.
  Hierarchy comes from size and ink weight.
- **No uppercase serif with tracking.** A settings heading was set 12px
  uppercase letterspaced in the lightest ink on the page, on the theory that it
  is a label you scan past. On an `<h2>`, which this app sets in the serif, that
  produced faux small caps in a face we ship without a small-cap cut — stretched capitals
  with tracking added to keep them apart. It looked like decoration, and **it
  was the most conspicuous thing on every settings page, although the intent
  was to make it quiet.** Set a heading as an ordinary heading.
- **No mono outside a chart's own labels and a clock time.** Mono kickers
  ("recents", "map") at 9.5–10.5px were the clearest sign of the old, dated
  style.
- **Nothing under 11px in HTML.** A chart's SVG labels are the exemption and
  the only one.
- **No half-pixel sizes** (`12.5`, `13.5`).

## 5. Color

Paper and ink from the theme tokens, never literals in the pane. A chart's own
paint is the exception, and even there the values are token-derived.

**One accent color, with two meanings.** In the pane `--color-primary` is
the only color the page carries, and it means either *now* — the deck's live
head and its open clip, the novelty marker — or *pressable* — a link, a save.
Nothing else is colored.

The one exception is the filled action: claret (`--color-secondary`), on the
single `Button variant="primary"` a page is allowed. Claret is never a rule, a
fill, a border for emphasis, or a second button.

**Do not use color to suggest a category that does not exist.** The deck draws
nine lanes and separates them by form and weight alone, because a hue on a lane
would suggest a relationship the data does not have.

**Semantic color appears only on a state a person can act on.** The deck's
`blocked` lane is drawn at full opacity in the warning hue while `never` and
`silent` both fade, because `blocked` is the one the reader can fix and the
other two are varieties of "nothing to report." Neutral facts keep
`color: inherit`: tinting an applet's ordinary lifecycle text in the warning
hue made every applet's normal state look like a problem and left nothing
distinct for the state that *is* one. A warning is also different from an
error: a run that stopped at its budget ceiling did what it was told, and
should look paused, not broken.

**State a condition once.** Show it in one place, in one style. The attention
list used to give every row a red border, a red fill and red text, stating the
severity three times. As a result a list of two broken connections was the
most prominent thing on a page otherwise made of hairlines and one accent.
The fix: color appears once per row as a small mark, the remedy is the page's
ordinary quiet button, and no warning glyph sits beside a sentence that already
says what is wrong.

**The accent marks interactive things only; do not use it as decoration.** An
accent tint plus an accent border is the generic onboarding-widget style; a
status surface gets a neutral foreground wash and no border. A standing offer
is drawn in the info tokens, not the accent. The accent tells the reader that
something can be pressed, so a chip that only gives information must not use
it.

**Never hardcode the ink on a filled button.** Its text is
`--color-background`, not `white`: on the dark themes the primary hue is light,
and white on light vanishes. There is no `--color-primary-foreground` token, so
a hardcoded value fails silently rather than failing to resolve.

**Which spelling to use.** Every theme defines raw values (`--surface`,
`--border`, `--secondary`); `themes.css` bridges them into Tailwind through
`@theme inline` as `--color-surface`, `--color-border`, `--color-secondary`.
Both resolve. **In a component, write the `--color-*` spelling** — the tree
uses `--color-foreground` 1321 times against 95 for `--border`, so the bridged
name is the one a reader will recognize and the one a Tailwind utility
(`bg-surface`, `text-foreground`) already maps to. Reach for a raw `--*` name
only inside `themes.css` itself, where the values are defined. Two spellings of
one property already collided once inside a single settings view, and which one
you met depended on which page you had opened.

## 6. Radius, spacing, surfaces

- **Radius is a `--radius-*` step, 0, 50% or a pill, and nothing else.** The
  steps live in `themes.css` and drive the `rounded-*` utilities too: xs 2px,
  sm 4px, md 8px, lg 10px, xl 12px, full. md is the shell's box (design.md);
  xl is a card in the pane (`Card.svelte`); 50% is a dot. Write the token, not
  the length: `tools/design-lint.sh` counts every radius length written
  outside `themes.css`.
- **No shadows in the pane.** A card is a hairline (`--color-border`) on
  `--color-surface`. A card marks something you can act on; do not use one
  only to group content.
- **Use elevation only for things that float over the page.** Do not use it for
  a plane that fills the page. The mobile drawer sat on `--surface-elevated` so
  the two planes would look like different materials — but it covers the
  screen, so on a warm theme "elevated" just looked like the page turning beige
  when the menu opened. A full-bleed plane takes the page's own paint and gets its depth from
  the fact that it slides. (Interaction states are a separate matter and come
  from the foreground-mix ramp — see design.md; a surface token is a 3–4% shift
  that is too small to notice.)
- **Do not hand-roll a bordered box.** A sweep on 2026-08-31 found 22
  hand-rolled `rounded-lg border border-border` blocks across nine files,
  disagreeing about padding and about whether they carry a ground at all.
  Nobody chose those differences. They came from copying the markup instead of
  using a shared component, and they made the settings surface look
  inconsistent. `<Card>` is one padded surface, `<Card list>`
  is rows divided by hairlines each padding itself, and `list` drops the
  container padding on purpose — a divided list whose container is also padded
  puts a gutter outside the first divider. Not every bordered box is a `Card`:
  the QR quiet zone is white whatever the theme, because it is part of the
  image rather than chrome.
- **8pt grid.** Gaps of 8 · 12 · 16 · 20 · 24 · 32 · 40 · 48 · 56 · 64. A turn
  opens 48px below the last; the head clears 40px before the first turn. (The
  10px under a title's sentence is the one remaining off-grid value. It is
  recorded in the lint baseline, which does not mean it is approved.)
- **Buttons are pills**, 40px tall at `0 22px` with the 8pt grid either side
  (32px and 48px). Use `Button`: it was written in Tailwind utilities while 192
  of the app's 227 components are scoped CSS against the theme tokens, which is
  why it had fourteen importers against 418 hand-rolled `<button>` elements — a
  developer in a scoped-CSS file could not reach it from the stylesheet they
  were already writing. `variant` is a claim about the button's job, not its
  color: `primary` is the one action the page is for, at most one per page;
  `danger` destroys something and is never also the primary. Everything else is
  a quiet link in `--color-primary` at 14px/500 sans. Verbs name the place —
  `Start the interview`, `Open sources` — never an arrow alone.
- **A button with no words is `IconButton`; a button with no box is
  `TextAction`.** Both added 2026-09-16, because `Button` has no form for
  either and 99 call sites had therefore invented their own. `IconButton` is
  square at 20 / 24 / 28 / 44px, one radius (6px) at every size, `ghost` ·
  `secondary` · `danger`, and its `label` is required — it feeds `aria-label`
  and `title`, so an icon-only button's accessible name cannot go missing.
  It replaced ten box sizes and six radii drawn across 59 sites, nine of those
  ten rows off this section's radius rule. `TextAction` is the quiet link
  above, made real: it replaced **18** independent definitions using seven
  color tokens and seven font sizes — one of which, `--color-accent`, has
  never resolved at all. Its `inline` flag is for an action sitting
  mid-sentence, which keeps `font: inherit` so it does not break the line it
  is in; that is also the touch exception below.
- **On touch screens, enlarge the control itself, not the space around it.** Under
  `(pointer: coarse)` a list row zeroes its own padding and the button inside it
  takes that padding instead, so the 44pt floor is cleared by the thing you
  press rather than by the space beside it. An inline prose link is the one
  exception — a 44pt box around it reaches into the lines above and
  below and swallows their taps. A list row is not an exception.

## 7. Motion

Three keyframe animations in the pane, all **from-only** so the resting state
is the stylesheet's:

| Name | Where | Shape |
|---|---|---|
| `arrive` | the work's blocks, staggered 60ms | opacity from 0, translateY from 6px, 500ms |
| `wipe` | the deck's tracks, first draw | `clip-path: inset(0 100% 0 0)` → 0, 660ms |
| `pulse` | the deck's now-marker | a slow breath at 3.4s, and nothing else pulses |

One page, one load sequence. Wrapping the page in a second fade (the old `.rv`)
doubled the ghosting and is gone. A picture fades in on arrival rather than on
load when it has no load event to wait for — a one-ink mask has no `img.onload`,
so it fades on the next animation frame instead.

**Look for layout jumps; they are the most common unintended animation.**
Three rules, each written after a visible jump shipped:

- **A placeholder takes the real layout's shape.** Skeleton rows in the table's
  own geometry mean nothing reflows when the data lands, so the page loads
  without jumping.
- **Pin the height of anything whose contents swap.** A toolbar that trades
  search controls for selection controls moves everything below it unless it
  carries a `min-height`.
- **Do not start in a state you are about to leave.** A blanket `isLoading =
  true` painted the composer docked at the bottom for one frame before it
  jumped to its centered empty-state position; that frame was the launch
  flicker.

Under `prefers-reduced-motion`, keep the information and drop the travel — and
remember that a CSS `@media (prefers-reduced-motion)` block cannot reach
Svelte's JS transitions. Those have to zero their own duration.

## 8. Mounting

A component that computes its own phase is **mounted once**. Getting Started
was what told Home whether Home existed; re-creating it on a phase change — by
switching a parent's props on that phase — made the two chase each other at
twelve instances a second. The fix was to mount it once outside the `Page`
shell in a `.host` that is `display: none` when settled, never unmounted.

Getting Started left the page on 2026-09-13 and is a chat room now, and the
home page is gone too, so neither half of that pair remains. The rule still
applies: a component reports its own phase, and a parent must not remount it to
change that phase.

## 9. The checklist

Before a page ships, count each row in the table below. Every row is meant to
be greppable, and `tools/design-lint.sh` already counts some of them. The lint
is a ratchet against a frozen baseline instead of a hard failure, because a
check that fails on day one gets switched off by the end of the week. Escape a line it is wrong about with
`design-ok: <why>` in a comment on or above it.

| | Limit |
|---|---|
| sentences of prose the page writes for itself | 1 |
| filled buttons (`variant="primary"`) | ≤ 1 |
| colored things that mean neither *now* nor *pressable* | 0 |
| `border` + `background` on a block holding no input, button or link | 0 |
| `box-shadow` in the pane | 0 |
| `border-radius` that is not a `--radius-*` step, 0, 50% or a pill | 0 |
| off-grid px in `padding` / `margin` / `gap` | 0 |
| `var(--font-mono)` outside a chart's SVG and a clock time | 0 |
| `font-size` under 11px outside a chart's SVG | 0 |
| `font-size` with a half-pixel, or off the §4 scale | 0 |
| color literals (hex, `rgb(`, `white`) outside a chart's own paint | 0 |
| raw `--secondary` / `--border` / `--surface` spellings in a component | 0 |
| `--surface-elevated` used as a hover, active or selected state | 0 |
| uppercase + letterspaced serif | 0 |
| hand-rolled `rounded-* border` boxes where `Card` would serve | 0 |
| `padding` inside a centered measure rather than on its scroller | 0 |
| distinct `font-size` values on one list or drawer, its title aside | ≤ 2 |
| keyframe animations in the pane (`arrive`, `wipe`, `pulse`), from-only | 3 |
| animating files with no `prefers-reduced-motion` answer | 0 |

## Removed designs

These are kept for the reasoning. Do not rebuild any of them without addressing
the reason it was removed.

**The home page** (`/home`, the Daily Office; removed 2026-10-06). A
dateline, a novelty line, the day's streams as a deck with a map, recents, an
ask and a keep card, built as turns of a meeting. Home and chat had become
one place: the Home room's panel already held chats, projects, pages and
applets, fresh sessions already landed on a new chat, and the page was a
second front door nobody needed. The Home tile opens a new chat now. Today's
streams live on the Timeline; a day's account is the wiki's day article.

**The spread, and the frontispiece** (2026-09-04 → 2026-09-13). Every chapter
page was to be a spread: the work on the left in one measure, and on the right
a painting set in the page's margin as a 12px card — sticky, pane-tall, with a
line on it, up to three figures, and two ways onward. It was the one framed
object Home and the setup spread shared, and that was the only reason for it.
Getting Started became a chat room on 2026-09-13; Home shed the painting on
2026-09-08 and kept only the two adjacent-page doors. With one page left there
was nothing left to share, and a framed painting on a single page is only
decoration. The component, its line bank and its plates directory are all gone
from the tree. Three arrangements had already been removed inside it and stay
removed: a painting as a ground behind the text
column, a painting as a band cut by the page, a painting faded into the paper.

**The full-width picture over a room.** The room's first cover was an oil
painting running the pane's full width, which made the page look like two
products stacked — a cinematic band over a column of chat. What ships sits
*inside* the column, at the measure of the words. It is also a mask rather than
a picture: one ink, so the PNG carries only its alpha and the page paints it
with the foreground token, which means no rectangle, no paper color of its own
to clash with the page, and no border or fade needed to stop it looking like a
component — the drawing simply trails off into the page at its edges.

**The vertical stepper.** A rail of 20px dots — filled ink with a check for
done, filled claret with the number for now, hollow for later — with the
current step opening in place. It had replaced three earlier alternatives
(chips, a progress rule, an accordion). It was removed with the Getting Started
page on 2026-09-13: progress shown on the reading axis (§3) needs no rail, and
a rail beside a chat thread distracts from the thread.

**The mono kicker.** Small-caps-ish mono labels at 9.5–10.5px over sections
("recents", "map"). The clearest sign of the old, dated style.

**The 12px uppercase letterspaced serif label.** See §4 — faux small caps in a
face with no small-cap cut. It was the most conspicuous thing on the page,
although it was meant to be quiet.

**The wrapper fade** (`.rv`). A second fade around a page that already staggers
its blocks in. Doubled the ghosting.

## Worked examples

**A day** (`wiki/DayPage.svelte`). Two views, Article and Data. The Article
is the date as its title with the year and weather under it, the Abstract,
your numbers, then the writer's sections with a margin beside them that holds
section times, computed values, and your notes. Data is the day line,
Places, the event timeline and the day's records, each a `.section` that does
not render when it is empty. There is no frontispiece and there is no
right-hand card.

**The Getting Started room** (`chat/getting-started/`). A seeded chat whose
steps are derived from your data instead of stored. The cover mask, then the
thread: each step an `<h2>` on the reading axis, its controls on their own line
beneath the words, and the introductions it writes back as evidence.
