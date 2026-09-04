# Page grammar

[design.md](design.md) governs the **shell** — the desk, the chrome, the
sidebar. This governs the **page**: everything rendered in the pane. Where the
two touch (radii, the 8pt grid, the interaction ramp), design.md wins.

This file exists because Getting Started was redesigned four times on
2026-09-04 and every review was taste against taste. Each version was either
stripped or cluttered, and the reason was never the objects on the page. It was
that no page had a sentence, and no page had an anatomy, so every screen was
designed from scratch and every radius, accent and paragraph got re-argued.
A grammar is what stops that. It is small enough to hold in your head and
strict enough that a page mostly designs itself.

The lineage, named so nobody has to rediscover it: the English scholarly
book — running heads, folios, shoulder notes, plates tipped in at chapter
heads — under Apple's restraint: one action per screen, large type, one system
everywhere. Not Tufte: his furniture *is* the content. Here the furniture
recedes and the person's record is the content.

---

## 1. One sentence per page

Every page has a sentence, written down before anything is drawn. Every object
on the page either **advances** that sentence or **proves** it. An object that
does neither is cut, however handsome.

Getting Started's sentence: *Your server is reading your life, and tomorrow
morning it writes the first page.* The steps advance it. The census proves it.
Source cards belong in Sources; the lifeline is a promise, not proof, and lives
at the head of the interview until chapters exist. That single test settled a
week of argument.

The sentence may appear on the page **once**, as the running head's line, and
it is the only prose the page gets. Explanatory paragraphs in lists are
prohibited; a row gets at most one line under its title.

## 2. Page anatomy

Every page in the pane has the same four parts, in this order. A page may omit
a part; it may not reorder them or add a fifth.

```
┌──────────────────────────────────────────────────────────────┐
│ plate                (chapter heads only; cut by the page)    │
├──────────────────────────────────────────────────────────────┤
│ running head         title · one line              ┃ folio   │
├────────────────────────────────────────────────────┨ margin  │
│ body                 one measure, ~64ch            ┃ column  │
│                      lists, prose, ledgers         ┃ verbs,  │
│                                                    ┃ dates,  │
│                                                    ┃ counts  │
└──────────────────────────────────────────────────────────────┘
```

**Plate.** A picture, at the head of a *chapter* only: Home, a day, a place, a
person, the interview. Never inside a list, never in a card, never as a ground
behind text. It is cut by the page — a hard bottom edge the body may overlap —
not faded into it. §4 says what a plate is.

**Running head.** The title in the serif; beneath it the page's one sentence in
the sans; at the right edge, in the margin column, the folio — the one number
that says where you are (`3 of 5`, a date, a count). No rule under it. The
claret rule under a heading was tried and read as a progress bar wearing a
costume; the folio *is* the progress.

**Body.** One reading measure, about 64 characters, left-aligned to the running
head's title. Lists, prose, ledgers. Whitespace separates first; a hairline
(`--border-subtle`) separates list rows and nothing else; a border and a fill
are spent only on an object that must read as a separate thing (a card with its
own state), never as decoration.

**Margin column.** The right edge of the measure is a column, not a place
buttons drift to. It holds **marginalia**: the quiet facts and verbs that
belong to a row but are not its title — the way back (`Read again`, `Change`,
`Add a source`), a date, a count, a state (`Tomorrow morning`). One item per
row, set in the quietest style on the page (§5, role *margin*). The one row
that asks for action carries its verb as a pill here, and that is the only
control in the column. Right edges align; a marginalia item never wraps.

**Colophon.** A page may end with an epigraph: one quotation from the bank in
[voice.md](voice.md), set in the serif at the foot of the body, its attribution
in the margin role. It is not the page's sentence and does not count against
§1, because it is quoted, not written. **One at a time,** chosen per day, never
carouselled and never animated: the reader who wants another comes back
tomorrow. A page that has a colophon has nothing below it.

## 3. Cards

A card is an object with its own state, not a way to group things. It earns a
border and a fill because it can be *done*, *connected*, *empty*. Four cards in
a grid that are really four paragraphs are four paragraphs.

- Radius: **6px**, per design.md. Nothing at 2, 8, 12, 14, or 18.
- Border `--border`, fill `--surface`, no shadow at rest. A card that overlaps
  a plate may carry one soft shadow, since it is physically on top of
  something; that is the only shadow in the pane.
- Inside a card the anatomy repeats at small scale: a head (serif name, margin
  state at the right), one line, then a ledger or a list. Never a paragraph.

## 4. Plates

A plate is a picture **drawn from the record** — the first place it can name,
today's sky over the home place, the person's own hand — never stock, never a
diagram of a product construct. The line diagrams of "streams flowing into a
box" were the previous plates; they read as artsy because they were abstract.
A real subject drawn well beats a clever abstraction every time.

Two inks, and only two:

| Plate | Where | Treatment | Why |
|---|---|---|---|
| **Chapter plate** | the head of Home, a day, a place, a person | an oil sky or landscape, in color, cut by the page | the one bold thing on the page |
| **Marginal plate** | small: a wiki thumbnail, an empty state, a colophon | a line engraving, **one ink**, shipped as an alpha mask and painted with `--foreground` | inverts for the dark themes with no second asset |

Rules: one plate per page. No words on a plate. A caption, if any, is set in
the mono in the margin column: number, name, place (`plate 1 · Wren Yard ·
Chicago`), nothing witty. Glosses like "two names, thirty seconds" were tried
and read as ad copy in museum-label costume.

The reference implementation is the Getting Started specimen on `wave`
(2026-09-04, uncommitted at the time of writing): `RecordPlate.svelte` paints a
mask through `mask-image` with the foreground token; the mask was drawn once
through the box's own image slot and thresholded to two tones. The production
shape is a nightly job that draws plates into the asset store — the deleted
day-illustration job, brought back with a fixed style.

## 5. Type

Three faces, four roles, one scale. Faces are the ones `apps/web/src/themes.css`
already declares: `--font-serif` (JJannon, Lora fallback), `--font-sans`
(Avenir), `--font-mono` (IBM Plex Mono).

| Role | Face | Sizes | Used for |
|---|---|---|---|
| **Title** | serif, regular | 32 · 24 · 20 | page titles, card names, list-row titles, numerals |
| **Line** | sans, 400 | 15 · 13 | the one sentence under a title, the one line under a row |
| **Margin** | sans, 400, `--foreground-subtle` | 13 · 12 | marginalia: the way back, a state, a date |
| **Measure** | mono, 400 | 12 · 11 | numbers and only numbers: the folio, counts, dates in a ledger, the CLI's vocabulary (`✓ · ⚠`) |

A page uses at most **four sizes** from this scale. The sweep on 2026-09-04
found `12.5`, `13.5`, `11.5`, `10.5` and `9.5px` scattered through the pane;
those are off the scale and are not to be reused.

Prohibitions, each of which has shipped here:

- **The serif is never bold and never italic.** JJannon has no italic cut;
  hierarchy comes from size and ink, never weight.
- **No uppercase serif with wide tracking** (design.md §6).
- **The mono is for measured things only.** A label is not code; a heading is
  not data. "no. 05 / in your own words / chapters · beliefs · days" was mono
  used as a costume.
- **The sans never headlines.** If a heading wants to be sans it is a label,
  and labels are set in the margin role.

## 6. Color

Paper and ink, from the theme tokens, never literals: `--background`,
`--surface`, `--foreground`, `--foreground-muted`, `--foreground-subtle`,
`--border`, `--border-subtle`.

**One accent, one meaning.** Claret (`--secondary`) means *you are here; do
this now*. The open step's numeral and title, the one verb pill, the
now-marker on a wire. It is never a rule, a fill, a badge, or a border for
emphasis. If two things on a page are claret, one of them is wrong.

Navy (`--primary`) is the ink of anything pressable that is not the one
action: links, the interaction ramp, focus.

Semantic color (`--success`, `--warning`, `--error`) is separate from the
accent and appears only on a state that a person would act on: `✓ Connected`
in green is right; a green check on a finished step is decoration, and a
finished step's check is ink.

No hardcoded badge colors for non-semantic values (see the feedback rule of
the same name). No color on the desk (design.md).

## 7. Spacing, alignment, radius

- **8pt grid.** 4 is the half-step. 8 · 12 · 16 · 24 · 40 · 64 are the only
  gaps. Off-grid paddings (`17px`, `26px`, `34px`) were in three of four
  Getting Started drafts; they are the sloppiness the eye feels and cannot
  name.
- **One left edge** for the body: title, lines, list rows, ledger labels all
  start at the same x. **One right edge** for the margin column.
- **Radius:** 6px on cards; pills (`999px`) on buttons and tabs. Nothing else.
- **Rules:** `--border-subtle`, 1px, between list rows only. No rule under a
  heading, no rule at the foot of a card, no double rules.

## 8. Motion

Forty-seven named keyframes across forty-eight files were counted on
2026-09-04. The pane is allowed **three**:

| Name | When | Shape |
|---|---|---|
| `arrive` | a page or plate mounts | opacity from 0, translateY from 4px, 240ms |
| `disclose` | a row or card reveals its body | height/opacity, 200ms |
| `now` | the now-marker on a wire | a slow pulse, and nothing else pulses |

All keyframes are **from-only** (the house rule since the airlock pass): they
declare where motion starts, never where it ends, so the resting state is the
stylesheet's and the animation cannot leave a property behind. Under
`prefers-reduced-motion`, keep the information and drop the travel. Hover is a
color change on the interaction ramp, never movement.

## 9. Words

[voice.md](voice.md) owns the voice. The grammar adds a budget:

- The running head's line is the page's only sentence.
- A list row gets a title and at most one line. A card gets a name, a state,
  and at most one line.
- Verbs name the place: `Start the interview`, `Add a source`, `Read again`.
  Never an arrow alone, never `Continue`, never `Go`.
- Empty is shown, not explained: a dash in the ledger, not "no data yet".

## 10. The checklist

Before a page ships, count. Every number here is measurable in the DOM.

| | Limit |
|---|---|
| sentences of prose | 1 |
| type sizes | 4 |
| faces | 3, in their roles |
| claret objects | 1 meaning (numeral + title + verb of the *same* row count as one) |
| plates | 1, at the head, no words on it |
| keyframes | 3, from-only |
| radii | 6px and pills |
| rules | between list rows only |
| shadows | 1, and only if something overlaps the plate |
| off-grid values | 0 |

## Worked example: Getting Started

Sentence: *Your server is reading your life, and tomorrow morning it writes
the first page.*

1. **Plate.** An oil sky, cut by the page. No words.
2. **Running head.** `Getting started` · the sentence · folio `3 of 5`.
3. **Body.** Five rows, in walking order: the founder's letter, introductions,
   connect your world, in your own words, your first day. Done rows recede to
   `--foreground-subtle` with an ink `✓`; the open row is claret; later rows
   are ink. Each row: title, at most one line.
4. **Margin column.** `Read again` · `Change` · `Add a source` · the pill
   `Start the interview` · `Tomorrow morning`.
5. **Proof.** One ledger under the list, set in the measure role: the census.
   Not four cards. Sources have a page; the lifeline has the interview.
6. **Colophon.** One epigraph from the bank, today's, and the page ends.

What was cut, and by which rule: the hero on the picture (§4, no words on a
plate); the rule under the heading (§2); the source cards and the lifeline card
(§1, neither advances nor proves); the dates and "Change" links on every row
(§2, one item per row); the explanatory paragraph under the open step (§9);
`13.5px` and `26px` (§5, §7).
