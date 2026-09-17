# Page grammar

[design.md](design.md) governs the **shell** — the desk, the chrome, the
sidebar. This governs the **page**: everything rendered in the pane. Where the
two touch (the 8pt grid, the interaction ramp), design.md wins; on radius this
file wins, and says why below.

This file exists because Getting Started was redesigned eight times on
2026-09-04 and every review was taste against taste. The first grammar,
written mid-way, described a printed book page — hairlines, oldstyle numerals,
a shoulder column, 6px cards — and the user called every build of it "an old
Windows XP thing." The lesson, so it is not re-learned: the reader responds to
atmosphere, depth and real controls, not to bare pages; "academic" means the
serif, the pictures and the lines — never small gray type and rules.

What replaced it was a **spread** — a painting in the margin beside the work.
That is gone too; see [Struck](#struck) for why, which is the more useful half.
What the pages actually share now is a grammar of **authorship**: who is
speaking in each block. That is §2.

Precedence, always: the user's words, then this file, then taste. Write
against the code, never against this file's last revision — every claim below
was checked against the tree on 2026-09-16, and the ones that were not are how
this document went stale the first time.

---

## 1. One sentence per page

Every page has a sentence, written down before anything is drawn. Every object
on the page either **advances** it or **proves** it. An object that does
neither is cut, however handsome.

- Home: *the record and the person in the room at the same time, with the day
  still open.* The deck advances it; the novelty line proves it.
- A day article: *yesterday, written down.* The autobiography advances it; the
  dayline and the sources prove it.
- The Getting Started room: *your server is reading your life.* The four steps
  advance it; the introductions it writes back prove it.

The sentence may appear once, under the title, in the sans. It is the only
prose the page writes for itself; a row or a step gets at most one line under
its title. **Nothing is written to fill a slot** — a section with nothing to
say does not render, and a measure that needs a week of history before it
means anything stays silent for that week rather than scoring noise
(`DayNovelty`).

## 2. Turns, not blocks

Every other room is one-directional: the wiki is you reading the record, chat
is you interrogating it, a day article is its account of you written
overnight. Home is the only place both parties are present, so it is built as
**turns of a meeting** rather than as a dashboard. In order, each one a
component:

| Turn | What it is | Where |
|---|---|---|
| speaks | one counted observation, no model involved | `home/DayNovelty.svelte` |
| shows | today from the raw streams, scrubbable down to the rows | `home/DayDeck.svelte` + `home/DayGround.svelte` |
| opens | the work you had in your hands last | the recents list in `HomeView` |
| asks | the one thing only the owner can answer | `home/PlaceAsk.svelte` |
| answers | the one line you choose to keep | the keep card in `HomeView` |

**A rule is the box talking; a card is you answering.** That is the only
decoration the page has, and it is load-bearing — so a new block on this page
has to pick a side before it picks a style.

In the code the distinction is **framed or unframed**, and nothing else. The
box's turns are unframed: a serif line at the page's measure, flush to the
paper, no border and no ground of its own. Your turn is framed: `--color-surface`
inside a 1px `--color-border` at 12px, because it is a surface you write on and
the border is the only affordance the question needs. (No hairline is literally
drawn — "rule" names the turn, not a `<hr>`. Do not add one.)

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
every tab change inside one room. Two measures exist and no more: a gridded one
and a prose one. And when a layout responds to width, **ask the container, not
the viewport** — what runs out of space is the pane, and in split view a pane
is nothing like the window.

**Four page genres**, so a reader knows which grammar applies before styling
anything:

- **The meeting** — Home. Turns, as above. One of a kind.
- **The article** — the wiki's day, person, place, year. A centered title page
  (h1, meta, a 3rem hairline), one 48rem measure, `.section-title` sections
  that hide when empty, and rails either side for contents and notes. It reads;
  it does not converse.
- **The room** — chat, and Getting Started since 2026-09-13. The thread *is*
  the page. Nothing is placed beside it; a picture sits inside the column at
  the measure of the words (`getting-started/RoomCover.svelte`).
- **The tools** — Settings, the lists. `Card` for surfaces,
  `UniversalDataGrid` for rows, the type scale below, and no page furniture of
  their own.

## 3. Sequence and progress

A sequence with state is numbered **on the reading axis** — as headings in the
thread, the way a conversational form numbers each question — never as an
ornament beside it. `getting-started/StepEyebrow.svelte` is the shape:
`1 of 4 · Connect AI`, set as an `<h2>` so the room has the hierarchy of an
authored page rather than a caption strip. The last heading is where you are; a
step settled before it was ever asked still gets its heading, carried by its
settling line.

A step's work happens in the step when it is small (three fields, three Connect
rows, a sign-in) and goes to the page where it permanently lives when it is
large (the interview, the manual). Controls sit on their own line under the
words they belong to, never butted against a sentence
(`getting-started/ui/Choices.svelte`) — one rhythm everywhere in a room.

**Progress is counted in words, not in marks.** Per-step marks — one glyph per
step, inked for done and hollow for later — were tried and read as stray
punctuation: the inked run came out as `— ——`, a typo rather than a measure.
A folio fraction (`1 of 4`, `3 of 7`) says the same thing and cannot be
misread.

Struck, each built first: chips, a progress rule, an accordion, and the
vertical claret-dot stepper that replaced all three. See [Struck](#struck).

## 4. Type

Four faces, the ones `themes.css` declares: `--font-serif` (JJannon),
`--font-serif-ui`, `--font-sans` (Avenir), `--font-mono` (IBM Plex Mono).

`--font-serif-ui` is the same file with its vertical metrics normalized, and
it exists for **serif set beside an icon or inside a fixed-height row**.
JJannon declares ascent/descent 0.740/0.260 — exactly one em, a metric box
clamped to the ink with no leading in it — so centering that box in a row
leaves the letters 0.098em high, which is why a serif spine sat above its icon
while the sans row beneath it looked fine. The override is expressed in em, so
it holds at 13.5px and at 46px alike. Prose keeps `--font-serif`: applying the
correction there would move first-baseline position across the wiki, pages and
Home.

One scale:

| Size | Face | Used for |
|---|---|---|
| 36 | serif | the page title (a dateline on Home) |
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
one there moves page and editor together — which is the whole WYSIWYG
guarantee.

**Two voices to a surface, not five.** The mobile drawer carried five text
stylings and a bordered card, and read as assembled rather than designed; what
ships has a 16px row and a 13px muted caption, with the serif masthead as the
single exception. A surface that needs a third voice usually needs fewer
things on it.

**A section heading outranks the rows under it.** Overview headings were once
11px serif — *smaller than their own body text* — which is why nothing on that
page read as structure.

**Figures are lining and tabular** (`font-variant-numeric: tabular-nums`)
wherever numbers sit in a column or update in place, so `0` and `10` occupy the
same width and a row of cards agrees on a baseline instead of twitching.

Prohibitions, each of which has shipped:

- **The serif is never bold and never italic.** JJannon has no italic cut.
  Hierarchy comes from size and from the weight of the ink, not from shouting.
- **No uppercase serif with tracking.** A settings heading was set 12px
  uppercase letterspaced in the lightest ink on the page, on the theory that it
  is a label you scan past. On an `<h2>`, which this app sets in JJannon, that
  produced faux small caps in a face with no small-cap cut — stretched capitals
  with tracking added to keep them apart. It read as decoration, and **it was
  the loudest, ugliest thing on every settings page precisely because it was
  trying to be quiet.** A heading, set like a heading.
- **No mono outside a chart's own labels and a clock time.** Mono kickers
  ("recents", "map") at 9.5–10.5px were the single strongest tell of the dated
  register.
- **Nothing under 11px in HTML.** A chart's SVG labels are the exemption and
  the only one.
- **No half-pixel sizes** (`12.5`, `13.5`).

## 5. Color

Paper and ink from the theme tokens, never literals in the pane. A chart's own
paint is the exception, and even there the values are token-derived.

**One accent, two meanings, and no third.** In the pane `--color-primary` is
the only color the page carries, and it means either *now* — the deck's live
head and its open clip, the novelty marker — or *pressable* — a link, a save.
Nothing else is colored.

The one exception is the filled action: claret (`--color-secondary`), on the
single `Button variant="primary"` a page is allowed. Claret is never a rule, a
fill, a border for emphasis, or a second button.

**Color never implies a category that isn't real.** The deck draws nine lanes
and separates them by form and weight alone, because a hue on a lane would
assert a kinship the record does not have.

**Semantic color appears only on a state a person can act on.** The deck's
`blocked` lane is drawn at full opacity in the warning hue while `never` and
`silent` both fade, because `blocked` is the one the reader can fix and the
other two are varieties of "nothing to report." Neutral facts keep
`color: inherit`: tinting an applet's ordinary lifecycle text in the warning
hue made every applet's normal state read as a problem and left nothing
distinct for the state that *is* one. Warning is not error, either — a run
that stopped at its budget ceiling did what it was told, and should read as
*held*, not *broken*.

**Say it once.** A condition is stated in one place, in one register. The
attention list used to give every row a red border, a red fill and red text —
severity stated three times, which is how a list of two broken connections
became the loudest thing on a page otherwise made of hairlines and one accent.
The fix: color appears once per row as a small mark, the remedy is the page's
ordinary quiet button, and no warning glyph sits beside a sentence that already
says what is wrong.

**The accent means interactive, never decorative.** An accent tint plus an
accent border is the register of every onboarding widget ever shipped; a
status surface gets a neutral foreground wash and no border. A standing offer
is drawn in the info tokens, not the accent — acting-on-this is the buttons'
register, and a chip that is merely informative must not borrow it.

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

- **Radius is `{0, 6px, 12px, 50%, a pill}` and nothing else.** 12px on a card
  in the pane (`Card.svelte`, since 2026-09-16); a pill on a button; 50% on a
  dot; 6px inside the shell (design.md). The tree still carries 4, 5, 7, 8, 9
  and 10px from before the rule — `tools/design-lint.sh` holds that count as a
  baseline, so do not add to them.
- **No shadows in the pane.** A card is a hairline (`--color-border`) on
  `--color-surface`, and it reads as separate because it can be acted on, never
  as grouping.
- **Elevation is for something that floats over the page, not for something
  that fills it.** The mobile drawer sat on `--surface-elevated` so the two
  planes would read as different materials — but it covers the screen, so on a
  warm theme "elevated" simply read as the page turning beige when the menu
  opened. A full-bleed plane takes the page's own paint and gets its depth from
  the fact that it slides. (Interaction states are a separate matter and come
  from the foreground-mix ramp — see design.md; a surface token is a 3–4% shift
  that reads as nothing happening.)
- **Do not hand-roll a bordered box.** A sweep on 2026-08-31 found 22
  hand-rolled `rounded-lg border border-border` blocks across nine files,
  disagreeing about padding and about whether they carry a ground at all. None
  of those disagreements were decisions; they are what happens when a shape is
  copied rather than named, and they are why the settings surface read as
  assembled rather than designed. `<Card>` is one padded surface, `<Card list>`
  is rows divided by hairlines each padding itself, and `list` drops the
  container padding on purpose — a divided list whose container is also padded
  puts a gutter outside the first divider. Not every bordered box is a `Card`:
  the QR quiet zone is white whatever the theme, because it is part of the
  image rather than chrome.
- **8pt grid.** Gaps of 8 · 12 · 16 · 20 · 24 · 32 · 40 · 48 · 56 · 64. A turn
  opens 48px below the last; the head clears 40px before the first turn. (The
  10px under a title's sentence is the one off-grid survivor, and is baselined
  rather than blessed.)
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
- **Touch targets move into the control, not around it.** Under
  `(pointer: coarse)` a list row zeroes its own padding and the button inside it
  takes that padding instead, so the 44pt floor is cleared by the thing you
  press rather than by the space beside it. An inline prose link is the one
  honest exception — a 44pt box around it reaches into the lines above and
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

**The unintended animation is the jump, and it is the one to hunt.** Three
rules, each of which cost a visible lurch:

- **A placeholder takes the real layout's shape.** Skeleton rows in the table's
  own geometry mean nothing reflows when the data lands — that is the whole
  difference between "loading" and "jumping."
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

Getting Started left the page on 2026-09-13 and is a chat room now, so the
second half of that pair is gone; `HomeView`'s `.host` is what remains. The
rule outlives the example: phase is a thing a component reports, not a thing a
parent remounts it to change.

## 9. The checklist

Before a page ships, count. Every row is meant to be greppable, and the ones
that are already counted are counted by `tools/design-lint.sh` — a ratchet
against a frozen baseline, not a wall, because a check that is red on day one
gets switched off by the end of the week. Escape a line it is wrong about with
`design-ok: <why>` in a comment on or above it.

| | Limit |
|---|---|
| sentences of prose the page writes for itself | 1 |
| filled buttons (`variant="primary"`) | ≤ 1 |
| colored things that mean neither *now* nor *pressable* | 0 |
| `border` + `background` on a block holding no input, button or link | 0 |
| `box-shadow` in the pane | 0 |
| `border-radius` outside `{0, 6px, 12px, 50%, a pill}` | 0 |
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

## Struck

Kept because the reasoning outlives the thing. Do not rebuild any of these
without answering what killed it.

**The spread, and the frontispiece** (2026-09-04 → 2026-09-13). Every chapter
page was to be a spread: the work on the left in one measure, and on the right
a painting set in the page's margin as a 12px card — sticky, pane-tall, with a
line on it, up to three figures, and two ways onward. It was the one framed
object Home and the setup spread shared, which was its entire justification.
Getting Started became a chat room on 2026-09-13; Home shed the painting on
2026-09-08 and kept only the two adjacent-page doors. With one page left there
was no "shared" left, and a single page carrying a framed painting for itself
is a decoration rather than a grammar. The component, its line bank and its
plates directory are all gone from the tree. Three arrangements had already
been struck inside it and stay struck: a painting as a ground behind the text
column, a painting as a band cut by the page, a painting faded into the paper.

**The full-width picture over a room.** The room's first cover was an oil
painting running the pane's full width, which made the page read as two
products stacked — a cinematic band over a column of chat. What ships sits
*inside* the column, at the measure of the words. It is also a mask rather than
a picture: one ink, so the PNG carries only its alpha and the page paints it
with the foreground token, which means no rectangle, no paper color of its own
to clash with the page, and no border or fade needed to stop it looking like a
component — the drawing simply trails off into the page at its edges.

**The vertical stepper.** A rail of 20px dots — filled ink with a check for
done, filled claret with the number for now, hollow for later — with the
current step opening in place. It was itself the survivor of three struck
alternatives (chips, a progress rule, an accordion). It died with the Getting
Started page on 2026-09-13: progress that lives on the reading axis (§3) needs
no rail, and a rail beside a chat thread is an ornament competing with the
thread.

**The mono kicker.** Small-caps-ish mono labels at 9.5–10.5px over sections
("recents", "map"). The single strongest tell of the dated register.

**The 12px uppercase letterspaced serif label.** See §4 — faux small caps in a
face with no small-cap cut, loudest thing on the page while trying to be
quiet.

**The wrapper fade** (`.rv`). A second fade around a page that already staggers
its blocks in. Doubled the ghosting.

## Worked examples

**Home** (`tabs/views/HomeView.svelte`). The dateline as title, the weather and
the clock under it; then the five turns of §2 in order — the novelty line, the
deck with its ground map and the moment it opens when you click it, the
recents, the ask, the keep. The two adjacent-page doors close the page below
the work.

**A day** (`wiki/DayPage.svelte`). A centered title page — h1, byline, a 3rem
hairline — then the autobiography, the dayline, the timeline, the chats and the
sources, each a `.section` that does not render when it is empty. Contents and
notes ride in rails; there is no frontispiece and there is no right-hand card.

**The Getting Started room** (`chat/getting-started/`). A seeded chat whose
steps are derived from the record rather than stored. The cover mask, then the
thread: each step an `<h2>` on the reading axis, its controls on their own line
beneath the words, and the introductions it writes back as the proof.
