# Page grammar

[design.md](design.md) governs the **shell** — the desk, the chrome, the
sidebar. This governs the **page**: everything rendered in the pane. Where the
two touch (the 8pt grid, the interaction ramp), design.md wins; on radius this
file wins, and says why below.

This file exists because Getting Started was redesigned eight times on
2026-09-04 and every review was taste against taste. The first grammar,
written mid-way, described a printed book page — hairlines, oldstyle numerals,
a shoulder column, 6px cards — and the user called every build of it "an old
Windows XP thing." What finally landed, and what this file now describes, is
the **spread**: a painting with words on it, and the work beside it, in a
current app's register. The lesson, so it is not re-learned: the reader
responds to atmosphere, depth and real controls, not to bare pages; "academic"
means the serif, the paintings and the lines — never small gray type and
rules.

Precedence, always: the user's words, then this file, then taste.

---

## 1. One sentence per page

Every page has a sentence, written down before anything is drawn. Every object
on the page either **advances** it or **proves** it. An object that does
neither is cut, however handsome.

- Getting Started: *Your server is reading your life, and tomorrow morning it
  writes the first page.* The stepper advances it; the census on the painting
  proves it. Source cards went to Sources and the lifeline to the interview
  by this test.
- Home: *Today, while it is still happening.* The deck advances it; today's
  count on the painting proves it.

The sentence may appear once, under the title, in the sans. It is the only
prose the page writes; a step or a row gets at most one line under its title.

## 2. The spread

Every chapter page is a **spread**: the work on the left in one measure, and
on the right the **frontispiece** — a painting as a card set in the page's
margin. Chapter pages are Home, Getting Started, a day, a place, a person.
Pages that are lists or tools (Sources, Settings, the wiki index, chat) keep
the shell's `Page` and have no frontispiece.

```
┌──────────────────────────────────┬──────────────────────────┐
│ title              (serif 36)    │  ┌────────────────────┐  │
│ one line           (sans 15)     │  │                    │  │
│                                  │  │   the painting     │  │
│ the work                         │  │                    │  │
│   stepper · deck · list · card   │  │  the line (26)     │  │
│                                  │  │  figures  (28)     │  │
│                                  │  │  since · links     │  │
│                                  │  └────────────────────┘  │
└──────────────────────────────────┴──────────────────────────┘
```

**The frontispiece** is one component, `home/Frontispiece.svelte`, and is
the only framed object the app's pages share. A painting for the moment, one
line on it, up to three numbers, one quiet line, up to two ways onward — all
in white over the painting's dusk. It is a 12px card with the page's margin
on three sides, sticky and pane-tall so its words stay in view on a long page,
and it becomes a header above the work below 900px. It is never a ground
behind the text column, never a band cut by the page, never faded into paper;
those were built and struck.

What goes on it, by page: Getting Started shows the step's painting, the
step's banked line and the record's census. Home shows the hour's painting,
yesterday's own sentence (the record quoting itself; the day's banked line
when it has none), today's steps, screen time and messages, and the two
adjacent day pages.

**The paintings** are oil, in color, portrait, one palette (warm gold and
cool teal, morning light), and ship in `static/plates` — drawn once through
the box's own image slot — until the plate job draws them from the record
(today's sky over the home place, the first place the record can name). One
ink-engraving register for small marginal plates is still allowed where a
picture must invert for the dark themes; it is not in use on any page today.
No words on a painting except the frontispiece's own.

**The lines** come from the bank in [voice.md](voice.md), unattributed on the
page; `home/lines.ts` carries the app's copy and rotates one per day. A line
the record wrote itself (a day's epigraph) always outranks a banked one.

**The work** sits in `.work`: padding 56 / 56 / 48 / 64, one measure of about
40em for prose and lists, full width for a chart. Nothing in it paints its own
ground; the pane's surface is the page's.

## 3. The stepper

A sequence with state renders as a **vertical stepper**, never as chips, a
progress rule, or an accordion — all three were built and struck. Every step
in walking order; a rail on the left carrying a 20px dot: filled ink with a
check for done, filled claret with the number for now, hollow with the number
for later; a hairline joining them. The current step opens in place: its
title one size up, its one paragraph, its button, its skip. Done steps keep
their way back as a quiet verb at the right edge (`Read again`, `Change`,
`Add a source`). Later steps sit muted with their one line.

A step's work happens in the step, not on another page, when it is small (a
card of three fields, three Connect rows, a sign-in); it goes to the page
where it permanently lives when it is large (the interview, the manual).

## 4. Type

Three faces, the ones `themes.css` declares: `--font-serif` (JJannon),
`--font-sans` (Avenir), `--font-mono` (IBM Plex Mono). One scale:

| Size | Face | Used for |
|---|---|---|
| 36 | serif | the page title (a dateline on Home) |
| 26 · 28 | serif | the frontispiece's line · its figures |
| 22 | serif | the open step's title; a card's question |
| 18 | serif | step titles, list rows, the novelty line, the ask |
| 15 | sans | the sentence under the title; a paragraph |
| 14 | sans | buttons, links, list bodies |
| 13 | sans | labels, quiet verbs, notes, dates |
| 12 | sans / mono | times and captions in white on the painting; mono only for a clock time |
| 11 | sans, 500 | the number inside a stepper dot |

Prohibitions, each of which has shipped: the serif is never bold and never
italic (JJannon has no italic); no uppercase serif with tracking; **no mono
outside a chart's own axis and a clock time** — mono kickers ("recents",
"map") at 9.5–10.5px were the single strongest tell of the dated register;
nothing under 11px; no half-pixel sizes (`12.5`, `13.5`).

## 5. Color

Paper and ink from the theme tokens, never literals in the pane (the
frontispiece's white and its dusk gradient are the one exception, because
they sit on a painting, not on the theme).

**One accent, one meaning.** Claret (`--secondary`) means *you are here; do
this now*: the current step's dot and eyebrow, the one filled button on the
page, the now-marker on a wire. Never a rule, a fill, a border for emphasis,
or a second button. Navy (`--primary`) is the ink of anything else pressable.
Semantic color is separate and appears only on a state a person acts on.

## 6. Radius, spacing, surfaces

- **12px** on cards and the frontispiece; **pills** on buttons; **circles**
  on stepper dots; **6px** only inside the shell (design.md). Nothing else.
- **No shadows in the pane.** A card is a hairline (`--border`) on
  `--surface`; it reads as separate because it can be acted on (the keep, a
  step's gate), never as grouping.
- **8pt grid.** Gaps of 8 · 12 · 16 · 20 · 24 · 32 · 40 · 48 · 56 · 64.
  Section spacing is `margin-top: 48px`; a title's sentence sits 10px below
  it; the stepper starts 40px below the sentence.
- Buttons: 40px tall, `padding: 0 22px`, filled claret with white 14px/500
  sans for the one action; a quiet navy 14px link for its skip. Verbs name
  the place: `Start the interview`, `Open sources`, never an arrow alone.

## 7. Motion

Three animations in the pane, all **from-only** keyframes so the resting
state is the stylesheet's:

| Name | Where | Shape |
|---|---|---|
| `arrive` | the work's blocks, staggered 60ms | opacity from 0, translateY from 6px, 500ms |
| `front-in` | the painting | opacity from 0, 800ms; its text follows at 200ms |
| `pulse` | the deck's now-marker | a slow breath, and nothing else pulses |

One page, one load sequence. Wrapping a spread in a second fade (the old
`.rv`) doubled the ghosting and is gone. Under `prefers-reduced-motion`,
keep the information and drop the travel.

## 8. Mounting

A page that computes its own phase is **mounted once**. Getting Started is
what tells Home whether Home exists; it lives outside the `Page` shell in a
`.host` that is `display: none` when settled, never unmounted. Re-creating it
on a phase change — by switching a parent's props on that phase — made the
two chase each other at twelve instances a second. Both files carry the
record; do not undo it.

## 9. The checklist

Before a chapter page ships, count.

| | Limit |
|---|---|
| sentences of prose the page writes | 1 |
| framed objects | the frontispiece, plus cards that can be acted on |
| claret objects | the current step's dot, its eyebrow, its one button |
| paintings | 1, with the frontispiece's words only |
| mono outside a chart or a clock | 0 |
| sizes under 11px, or with a half | 0 |
| shadows | 0 |
| animations | 3, from-only |
| radii | 12, pills, circles |

## Worked examples

**Getting Started** (`home/GettingStarted.svelte`). Title, the sentence, the
seven-step stepper with the current step open in place; the frontispiece
with the step's painting, its banked line and the census. The hidden door
bottom-right skips everything.

**Home** (`tabs/views/HomeView.svelte`). The dateline as title, the weather
and the clock under it; the novelty line; the deck with its map; the recents;
the ask; the keep. The frontispiece with the hour's painting, yesterday's
sentence, today's count, and the two adjacent pages.

**Next in the pass:** a day page (its own painting, its epigraph, its
numbers), a place, a person; then the list pages and Settings brought onto
the type scale and radius without a frontispiece; chat last.
