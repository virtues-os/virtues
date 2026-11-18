# Design system

This file is the Virtues design system: its tokens, materials, shell, page
grammar, primitives and prohibitions. It is the only design document. The
app renders it at `/design`, with the live parts placed in it (see
[How /design works](#how-design-works)).

The app stores one person's mail, money, health and days. It should look
calm and finished, because people have to trust it with that. Jittery
accents, cramped boxes and alignment that is a pixel off make it look
unfinished.

Order of precedence: the owner's instructions, then this file, then taste.
Check every claim here against the code before you build on it. When this
file and the CSS disagree, the CSS is what ships and this file is the bug.
Every number below was read off the tree on 2026-11-17.

### How /design works

`/design` is a page in the app (`DesignView`) that renders this file, read
from the repo at build time. Editing the file is editing the page.

- **A token named in backticks shows its current value beside it** on the
  page, read from the cascade and read again when the theme changes. When
  the text claims one number and the chip shows another, the text is stale.
- **A specimen is a file named `<Name>.specimen.svelte`, kept beside its
  component**
  (`Button.specimen.svelte` next to `Button.svelte`), found by
  `lib/design/specimens.ts` with no list to maintain. A fenced block with
  the language `specimen` and the file's name, and nothing inside, places
  it here. The page names any specimen this file does not place, and any
  block whose specimen does not exist.
- **A new primitive gets a specimen and a block in the same change.** A
  primitive nobody can see gets rebuilt by hand in the next view that needs
  it.
- The sun and moon in the page's toolbar preview Light and Dark without
  saving. Leaving the page puts back the theme you chose.

---

## Who it is for

Every visual decision is made for the five personas in
[personas.md](personas.md). The two primaries set the default.

| Persona | What the design owes them |
|---|---|
| Narrative seeker (primary) | Reading text in the serif at a comfortable measure. Words lead, chrome recedes. |
| Quantified self (primary) | Charts and tables in the reply and on the page, in the same tokens as everything else, with tabular figures. |
| Tech prosumer | Craft at the last few pixels: motion, glass, alignment, nothing a pixel off. |
| Recovery | No streaks, counts, overdue alarms or game mechanics, anywhere. |
| Data sovereign | Nothing on screen marks work as on or off the server. Where data goes is written down in the manual and [privacy-boundary.md](privacy-boundary.md). |

When you report or review a design, say what each persona gets from it,
primaries first.

---

## Themes

**There are two themes and a switch.**

| Setting | Theme | Notes |
|---|---|---|
| Light | Oxford | The default. `DEFAULT_THEME` in `virtues-registry` must match `DEFAULT_THEMES.light` in `lib/utils/theme.ts`. |
| Dark | Asgard | Derived from the same tokens, never ported from a reference. |
| Match system | Oxford or Asgard | Follows `prefers-color-scheme`. |

```specimen Contrast
```

Every token below is defined once in `:root` for Light and overridden once
for Dark, in `apps/web/src/themes.css`. A theme name stored before the cut
lands on Light or Dark by its darkness (`RETIRED_THEMES` in `theme.ts`), so
nobody on a dark theme is dropped onto a white page.

A design is checked in Light and in Dark. Nothing else needs checking.

---

## Tokens

All tokens live in `apps/web/src/themes.css`. `app.css` registers the fonts
and holds the few app-wide rules (the focus reset, the Glass setting, the
selection color).

**Write the `--color-` spelling in a component.** `themes.css` defines raw
values (`--surface`, `--border`, `--secondary`) and bridges them to Tailwind
through `@theme inline` as `--color-surface`, `--color-border`,
`--color-secondary`. Both resolve. The bridged name is the one the rest of
the tree uses and the one a Tailwind utility maps to. Raw names belong
inside `themes.css` only.

### Color

| Role | Token | Light | Use |
|---|---|---|---|
| Desk | `--desk` | `#F5F4F2` | The window's ground, behind the cards. |
| Page | `--page` | `#FFFFFF` | A card: the pane you work in. |
| Ink | `--color-foreground` | `#1A2030` | Text. |
| Muted, subtle | `--color-foreground-muted`, `--color-foreground-subtle` | | Secondary text, captions. Three inks, no others. |
| Hairline | `--color-border` | `#E3E1DD` | A framed block's edge. |
| Accent | `--color-primary` | `#1D4572` | Means *now* or *pressable*. |
| Filled action | `--color-secondary` | `#9A2B2E` | The single primary Button on a page. |
| Status | `--color-success`, `--color-warning`, `--color-error`, `--color-info` | | A state a person can act on. |

```specimen Colors
```

**One accent, with two meanings.** `--color-primary` is the only color a page
carries, and it means *now* (a live marker, the open clip) or *pressable* (a
link, a save). The one exception is the filled action: claret on the single
`Button variant="primary"` a page is allowed. Claret is never a rule, a fill,
a border for emphasis or a second button, and it never marks where work ran.

**Do not use color to suggest a category that does not exist.** A hue on a
lane suggests a relationship the data does not have. Separate lanes by form
and weight.

**Semantic color appears only on a state a person can act on.** A neutral
fact keeps `color: inherit`. A warning is different from an error: a run that
stopped at its budget did what it was told and should look paused, not
broken.

**State a condition once**, in one place, in one style. A row that is broken
gets one small mark, not a red border, a red fill and red text.

**Never hardcode the ink on a filled control.** Use `--color-surface` or
`--color-background`, never `white`: in Dark the accents are light, and white
on light vanishes.

**Color literals stay in `themes.css`.** A chart's own paint is the exception,
and even there the values are read from tokens. The two canvas files
(`wiki/LifelineCanvas.svelte`, `wiki/LifelineMap.svelte`) read tokens with
`getComputedStyle` and keep a literal only as the fallback.

### Grays

The interaction ramp mixes a neutral pigment (`--wash-ink`: black on Light,
white on Dark) into whatever is behind it, so it reads on any ground. Four
steps, no others:

| Step | Token | Mix | Means |
|---|---|---|---|
| Rest | | 0% | nothing |
| Hover | `--hover-bg` | 7% | the pointer is here |
| Selected | `--active-bg` | 12% | where you are |
| Press | `--press-bg` | 16% | held down, only while pressed |

```specimen Ramp
```

**Never use a surface token for an interaction state.** `--surface-elevated`
is a step of about 4% from the page, which is too small to see. That bug
shipped twice: dead hovers across 39 rules, and an active tab drawn white on
white. The scrollbar's thumb is on the same ramp at 22%, with no track.

### Type

Four faces, all self-hosted `woff2` in `apps/web/static/fonts/`. A box runs on
a LAN with no guaranteed egress, and the kiosk is Linux, so no face may be a
CDN or a system font.

| Token | Face | Cuts | For |
|---|---|---|---|
| `--font-serif` | EB Garamond | 400 only | Reading text and titles. |
| `--font-serif-ui` | EB Garamond UI | 400 only | The serif in a row beside an icon or a control. |
| `--font-serif-lead` | EB Garamond Lead | 600, Latin only | The first word of a day's Abstract, and nothing else. |
| `--font-sans` | Avenir | 400, 500, 700 | The chrome: labels, buttons, menus, captions. |
| `--font-mono` | IBM Plex Mono | 400, 500, 600 | Code, and a chart's own labels. |
| `--font-hand` | Nothing You Could Do | 400 | The owner's notes in a day page's margin, and nothing else. |

**The scale has seven steps.** Every font size in the app is one of them.

| Token | Size | Face | Used for |
|---|---|---|---|
| `--text-title` | 36px | serif | A page title. |
| `--text-heading` | 22px | serif | A section title, a step's heading. Also `--md-h2-size`. |
| `--text-lead` | 18px | serif | The box's line: a list row, a card's question, a caption under a chart. |
| `--text-body` | 16px | serif or sans | What you read and what you type. |
| `--text-label` | 14px | sans | A row, a button, a link, a menu item, a field. Also `--chrome-label-font-size`. |
| `--text-small` | 13px | sans | A label, a quiet verb, a note, a date, a chart's readout. |
| `--text-caption` | 12px | sans | A tab, a group heading, a hint. |

```specimen Type
```

A chart's SVG may set its own tick and lane labels below 12px. Nothing else
may. The markdown sizes (the `--md-` tokens in `themes.css`) are the one definition both
the page and the editor read, which keeps the editor WYSIWYG; they take their
values from this scale.

Today about 20 distinct sizes are written as literals in `.svelte` files,
most often 13, 12, 14 and 15px. Moving them onto the scale is open work, and
`design-lint.sh` does not count off-scale sizes yet.

**Weight never signals state.** A selected row changes fill and ink, never
weight: a bolder label is a wider one, and everything beside it moves.

**Two text styles per surface at most**, a title aside. A surface that needs
a third usually has too much on it.

**A section heading is larger than the rows under it.**

**Figures are tabular** (`font-variant-numeric: tabular-nums`) wherever
numbers sit in a column or update in place.

All text uses grayscale antialiasing (`-webkit-font-smoothing: antialiased`
on `html`).

#### The serif has one weight

There is no bold serif and no italic serif. EB Garamond has an italic and we
don't ship it. Ask the serif for `font-weight: 700` and CSS returns the one
cut with no warning. Ask for italic and the browser slants the roman, which
looks like what it is. A serif hierarchy is set with size, ink and space. A
proposal for a second weight is a new font file and a change to this file,
not a CSS change. `design-lint.sh` (`serif-impossible`) counts any bold or
italic asked of `--font-serif`.

`--font-serif-lead` is its own family so that the 600 cut is reached only by
name. Inside `EB Garamond` it would be handed to every `<strong>` and table
header that asks the serif for bold.

#### Two serif tokens for one font

EB Garamond declares ascent and descent of 1.007 and 0.298, which leaves
centered letters visibly low beside an icon. `EB Garamond UI` is the same
file registered with corrected metrics (0.943 and 0.290), so the cap band
lands on the center line. Use `--font-serif-ui` in a row and `--font-serif`
in prose: the correction in prose would move the first baseline across the
wiki and pages. The derivation is the comment above the registration in
`app.css`.

#### In the chrome

The serif is for writing: a page's title, a page or thread in the sidebar,
and a pin of either. A chat is a conversation, so its row stays sans, as do
places and verbs (New chat, Threads). Recent has no glyphs, so the face is
what tells a page from a chat there.

#### The Avenir license is unresolved

Avenir is a commercial Monotype face. EB Garamond and IBM Plex Mono are OFL.
The repo has no license information for the three Avenir files either way,
and only a human can settle it. Until then, don't add weights, don't copy the
files into another product, and don't remove them: swapping the sans is a
design decision.

### Spacing

**A 4pt grid, set mostly in eights.** Gaps between blocks are 8, 12, 16,
20, 24, 32, 40, 48, 56 or 64px; 4px is for the inside of something small,
such as a chip. A turn opens 48px below the last. `design-lint.sh` counts any
padding, margin or gap in px that is not a multiple of 4 (`off-grid`), and
allows two exceptions: 6px and 22px.

**Page widths.** Three, in `themes.css`, set through `Page`'s `maxWidth`:

| Token | Width | For |
|---|---|---|
| `--page-reading` | the reading measure | text and plain lists |
| `--page-detail` | 60rem | a page with a side column |
| `--page-full` | 72rem | grids and list pages |

**A page has one measure, and its padding lives on the outer scroller.**
Center the measure inside that padding, never pad from inside it, or two
pages that claim the same width start their text at different x. When a
layout responds to width, use a container query, not a media query: in a
split the pane runs out of room, not the window.

### Radius

| Token | Size | Used by |
|---|---|---|
| `--radius-xs` | 2px | a drop line's end |
| `--radius-sm` | 4px | a box inside a box, such as a tab's × |
| `--radius-md` | 8px | controls, menus, popovers, rows (`--card-radius`) |
| `--radius-lg` | 10px | the glass controls in the header |
| `--radius-xl` | 12px | a pane, the sidebar's glass, a card in the pane, a home tile |
| `--radius-full` | pill | tabs, the composer, the search field |

```specimen Radius
```

A radius is one of these, 0, or 50% for a dot. Write the token, not the
length.

### Elevation

**One shadow.** `--shadow-float` (`0 4px 16px` at 15%) is on every layer that
floats over a page: menus, popovers, dialogs. Glass carries its own lift
(`--lq-lift`), described under [Glass](#glass). Nothing else casts a shadow:
not a card in the pane, not the chrome.

**Separation goes whitespace, then a step of 3 to 5% lightness, then
elevation.** Stop at the first one that works. Do not reach for a 1px rule.

**Elevation is for things that float over the page.** A plane that fills the
screen, such as the phone's drawer, takes the page's own paint and gets its
depth from sliding.

### Motion

The shell's tokens, always on `--ease-premium`:

| Token | Value | For |
|---|---|---|
| `--motion-hover-in` | 80ms | a hover coming on |
| `--motion-hover-out` | 200ms | going off: instant on, gentle off, so running the pointer down a list leaves a short trail |
| `--motion-press` | 60ms | sinking on press |
| `--motion-release` | 180ms | springing back |
| `--motion-state` | 150ms | a selection changing, a group folding |
| `--motion-enter` | 180ms | a popover or menu appearing |
| `--motion-layout` | 300ms | a pane resizing, a tab opening or closing |

```specimen Motion
```

The sidebar opens in 440ms with its glass on a small spring and closes in
260ms (`data-sidebar-motion` on the root). Leaving is quicker than arriving.

Square controls sink to 94% while pressed. Rows and tabs take the press fill
only, because a wide row that shrinks reads as a wobble.

In the pane, three keyframe animations, each from-only so the resting state
is the stylesheet's:

| Name | Where | Shape |
|---|---|---|
| `arrive` | a page's blocks, staggered 60ms | opacity from 0, 6px up, 500ms |
| `wipe` | a chart's tracks, first draw | a clip from the left, 660ms |
| `pulse` | the now-marker | one slow breath at 3.4s; nothing else pulses |

**Look for layout jumps.** A placeholder takes the real layout's shape. Pin
the height of anything whose contents swap. Don't start in a state you are
about to leave.

**Under `prefers-reduced-motion`, keep the information and drop the
travel.** A CSS media block cannot reach Svelte's JS transitions, so those
zero their own duration. `design-lint.sh` counts animating files with no
answer (`no-reduced-motion`).

---

## Materials

The window has three materials: the desk, paper and glass.

```specimen Materials
```

### Desk

`--desk` is the ground. The window header and the sidebar sit on it. Along
the bottom of the whole window, behind everything, is the painting
(`routes/(app)/+layout.svelte`). It is in full color, and the panes cover
most of it.

### Paper

A pane is a sheet of paper (`--page`) at `--radius-xl`, standing
`--window-air` (12px) in from the window's edges and `--pane-gap` (10px) from
the sidebar and from the other pane in a split. A card separates from the
desk by tone, not a line: the desk is a step darker than the page in both
themes. Home has no paper: its tiles stand on the desk.

**Reading text is always on paper, never on glass.**

### Glass

Glass is for what floats over the desk and the painting: the ∴, the rail and
the open sidebar, the tabs, the header's corner buttons, the home tiles. It is
one recipe, `.lq` in `app.css` with its `--lq-` tokens in `themes.css`. Do
not add a second glass.

- **Clear** (`.lq`): small controls.
- **Regular** (`.lq-regular`): surfaces that hold text, such as the sidebar
  and the home tiles. A flat 62% fill, because a gradient this tall shows
  bands in Chrome.
- **Selected inside glass** is a flat tint, never a second pane of glass. No
  card on a card, no shadow on a shadow.
- The rim is a bright top line, a faint light ring and a soft inner shade.
  The lift is a faint outline, a contact shadow and one wide soft shadow.

**The Glass setting** (Settings, Appearance) is None, Low, Medium or High,
per browser (`lib/stores/glass.svelte.ts`, `data-glass` on the root, set
before first paint in `app.html`). It maps to `--lq-see` (0, 0.5, 1, 1.4), and
every fill thins by its share times `--lq-see`. None also drops the backdrop
filters. `prefers-reduced-transparency` means None.

**A backdrop root turns glass into a flat tint.** Any ancestor with
`clip-path`, `filter`, `mask`, `opacity` below 1 or a `view-transition-name`
stops the blur at that ancestor. When glass looks too transparent, look up
the tree first.

The web can blur but cannot refract: Safari and WKWebView have no SVG
displacement in `backdrop-filter`.

---

## The shell

### Window header

The header row is `--chrome-row-h` (54px). Its controls are `--chrome-tab-h`
(34px) with 10px of air above and below, so the panes start 54px down. In the
Mac app the traffic lights sit on the row's center line (`CENTRE_Y` in
`src-tauri/src/traffic_lights.rs`, 27), so a change to the row height changes
that constant too.

From left to right:

- **The ∴** (`HeaderMark`) opens and closes the sidebar (⌘B). It sits on the
  sidebar's glass. Closed, it is the top of the rail. Open, it widens into
  the sidebar's first row: the ∴ on the column of the rows' icons and
  "Virtues" in the serif where their labels start. Pointing at it turns the
  three dots into a column with a hairline beside them, the sidebar's own
  glyph. It never lights or moves on its own. In the Mac app, where the
  window's buttons take the start of the row, it is a square with no name.
- **The tabs**, one strip per pane, starting at that pane's edge.
- **The corner** (`HeaderCorner`): Search, Sources and Settings as glass
  buttons. Search opens the palette, as ⌘K does. It searches everything, so
  it lives here and not on a page.

### Sidebar

Closed, the sidebar is a floating glass rail `--rail-w` (44px) wide that
fits its icons. Open, it is a full-height panel of regular glass. Both reach
up into the header row. Every row stands 4px in from the glass with 10px of
padding, open or closed, so icons and the ∴ sit 14px in and nothing moves
sideways when it toggles. Closing shrinks the width and the height together.

Rows are `--sidebar-interactive-height` (32px), 2px apart, so a row lands
every 34px. The ∴ row is as tall as a row and stands 16px above the doors,
the same 16px as from the doors to Pinned.

The sidebar is one list (`HomePanel`):

1. **Doors**: New chat, Threads, Pages, Applets, Wiki, Storage
   (`DOORS` in `lib/stores/sidebarDoors.svelte.ts`). Every door opens a page;
   none lists its contents. A door can be reordered and hidden on the desktop
   (drag, Alt and an arrow, or Move up and Move down in its menu), saved on
   the profile as `sidebar_doors`. Customize sidebar lists every door with a
   check beside the shown ones, and Reset to default. The last shown door
   cannot be hidden. The phone reads the same arrangement and cannot change
   it.
2. **Pinned**: anything with a url, in the owner's order. A pin is a tinted
   tile in its own color (16%, 26% on hover and when active) holding its
   mark. Drag a Recent row or a tab into Pinned to pin it; a line shows the
   gap it will land in. A pinned thread takes a dropped row or tab, which
   files it there.
3. **Recent**: the ten newest chats and pages, newest first, then "See all".
   No glyphs: chats are set in the sans and pages in the serif.

Every group folds from its label and the fold is remembered. A group with
nothing in it is not drawn.

**The sidebar is quieter than the pane.** No borders, no fills of its own, no
badges, no rules. Type and spacing make the structure. Nothing in the sidebar
may be more prominent than the page.

**Hover fills the icon.** Under the pointer and on the active row the line
glyph becomes its solid version (`SOLID_GLYPHS` and `fillable` in
`AtlasIcon`) and the ink comes up. There is no hover fill on a sidebar row.

### Tabs

Tabs are pills (`--radius-full`) sized to their labels. The selected tab is
on the same glass as the sidebar, as a flat fill. Tabs you are not on have no
fill until you point at them. In a split, the active tab of the pane you are
not working in dims its ink.

- **Switching tabs is instant.** Fill and ink change in one frame; only the
  hover fades.
- **Opening and closing a tab is one motion run both ways** on
  `--motion-layout`: the space opens and the neighbors glide aside, then the
  tab fades in; closing reverses it. Tabs already open when the window loads
  do not animate.
- **A crowded strip compacts, then scrolls.** The tabs you are not on drop to
  their icons. Past that the strip scrolls, and an edge fades over 32px while
  there are tabs beyond it.
- **A tab carries the sidebar's marks.** A chat that is replying shows the
  working ∴ in place of its icon. A reply that landed in the background puts
  the unread dot on the icon's corner. A title that does not fit fades over
  its last 24px instead of ending in an ellipsis.
- **The window's only tab, when it is an empty new chat, has no close.**
  Closing it would only open another new chat.

### Action toolbar

Every page on the desktop has an action toolbar along its top
(`ActionToolbar`, drawn by `TabContent` above every view, so no view lays out
its own).

- **Left**: back and forward for this tab.
- **Middle**: where the page is (`tabs/crumbs.ts`, or a path a view publishes
  with `paneActions.setCrumbs`), then the pin. A segment that holds things
  opens a menu of them. The last segment is the page: double-click to rename
  it in place, right-click for its actions. The page's title is that segment,
  so a page draws no title row of its own.
- **Right**: the page's status (`usePageStatus`), such as "Saved" or
  "Offline", then what the page lets you do: controls handed over with
  `usePageActions`, then buttons from `paneActions.set`, two inline and the
  rest behind a `···`.

The status says the page's state. It never says where work runs.

The phone has no toolbar. There a page keeps its title and controls at its
top, and the chat keeps its corner button.

### Search

The palette (`SearchModal`) opens centred over the panes on a blurred veil.
On the phone and under reduced transparency the veil has no blur.

### Glyphs

**The icons are Myna** (`@iconify-json/mynaui`, named `mynaui:…`). The shell's
own things keep their names in `AtlasIcon`'s table (`atlas:wiki`,
`atlas:pages`), and each name draws a Myna icon. `scripts/myna-icons.mjs`
bundles only the icons the source names, so write a name out in full: a name
built at runtime is not bundled. Older Remix and Phosphor names are mapped in
`lib/legacyIcons.ts`. Brand marks come from Simple Icons.

**Every icon renders at 1.25px of ink.** `AtlasIcon` derives its stroke from
its size, so a stroke is never typed by hand. Two glyph sizes: 14px beside
12px text, and 16px (`SHELL_GLYPH_PX`) in rows and the header.

**Labels lead; icons support them.** A column of identical full-contrast
icons makes a sidebar look like every other sidebar. Sidebar icons rest at
half opacity (`--sidebar-icon-opacity`) and come up on hover and for the
active row.

The ∴ is the one filled shape in the chrome.

### Alignment

**One left edge.** Every icon in a column starts at the same x, and every
label starts at the same x. Near misses (12, 16, 20px) look careless even
when nobody can say why.

---

## The page

What is drawn inside a pane.

### One sentence per page

Every page has one sentence that states its purpose, written before anything
is drawn. Every object on the page either develops that sentence or supports
it with evidence. Remove anything that does neither.

The sentence may appear once, under the title, in the sans. It is the only
prose the page writes for itself. A row gets at most one line under its
title. **Do not write text to fill a slot.** A section with nothing to say
does not render.

### Framed and unframed blocks

**An unframed block is the box speaking; a framed block is where you
answer.** That is the page's only decoration, and it carries meaning. The
box's words are unframed: serif at the page's measure, flush on the paper.
Your turn is framed: `--color-surface` inside a 1px `--color-border` at
`--radius-xl`, because it is a surface you write on.

**Do not hand-roll a bordered box.** Use `Card`: one padded surface, or
`<Card list>` for rows divided by hairlines, each padding itself. A card marks
something you can act on. Do not use one only to group content.

### Page genres

Decide which grammar applies before styling anything.

- **The article**: the wiki's day, person, place, year. A centered title,
  one reading measure, sections that hide when empty, and margins either
  side for contents and notes. For reading, with no conversation on the
  page.
- **The room**: chat, and the getting-started chat. The thread is the whole
  page. A picture sits inside the column at the measure of the words
  (`chat/getting-started/RoomCover.svelte`).
- **The tools**: Settings and the list pages. `Card` for surfaces,
  `UniversalDataGrid` for rows, `SubNav` for tabs, and no furniture of their
  own.

### Sequence and progress

A sequence with state is numbered on the reading axis, as headings in the
thread (`chat/getting-started/StepEyebrow.svelte`: `1 of 4 · Connect AI`).
Controls sit on their own line under the words they belong to
(`chat/getting-started/ui/Choices.svelte`).

**Show progress as text.** `1 of 4` cannot be misread. A row of per-step marks
rendered as `— ——` and looked like a typo.

### Mounting

A component that computes its own phase is mounted once. A parent must not
remount it to change that phase, or the two chase each other.

---

## Primitives

The parts every page is built from. Each lives in `apps/web/src/lib/` and
has a specimen on `/design`. When a primitive exists, a page uses it: a hand-made
copy is a regression, however small.

"Today" is what the code does now. "Decided" is where it is going, and the
gap is open work.

| Primitive | Job | Today | Decided |
|---|---|---|---|
| `Button` | A labeled action. `primary` is the one action a page is for, at most one; `danger` destroys something and is never also primary; it is red ink and an edge at rest and fills only under the pointer, so it never looks like the primary. | `--radius-md`; 32, 40, 48px. 73 files, beside about 485 hand-styled `<button>`s. | One control height, 34px. |
| `IconButton` | A button with no words. `label` is required and feeds `aria-label` and `title`. | 20, 24, 28, 44px; ghost, secondary, danger. 35 files. | 20px nested, 34px control, 44px touch. |
| `TextAction` | A button with no box: a quiet link in `--color-primary`. `inline` keeps `font: inherit` mid-sentence. | 33 files. | Kept. |
| `Input`, `Textarea` | Text fields with label, help and validation. | 12 and 3 files, beside about 100 raw fields. | Input at 34px. |
| `Switch`, `Choice` | A toggle and a select. | Live under `settings/`. | Promote to `lib/components`. |
| Checkbox | A box you tick. | None: `TaskCheck`, `TaskCard` and `ReviewAsk` each build `role=checkbox`, beside about 10 native ones. | Add one. |
| Segmented control | Two to five exclusive choices in a row. | Six separate builds (ContextViewPanel, DayPage, TimelineScrubber, DisplaySettingsPopover, IconPicker, ThreadsView). | Add one. |
| Chip | A small label or filter. | `Badge`, `LinkChip`, `ProjectChip`, `DataGridFilterChip`, `ref-badge.css`, `timeline/Pill`, and `.chip` in 13 files. | One Chip. |
| `MenuItem`, context menu | A row in a menu. | `MenuItem` in 11 files; the context menu positions itself (`contextMenu/fit.ts`). | One menu on `lib/floating`. |
| `Popover` | Anything anchored to a control. | `lib/floating` in about 24 files; about 15 overlays hand-build their own. | Everything anchored uses it. |
| `Modal` | A dialog. `DialogHost` draws confirm and prompt on it. | 16 files; SearchModal, MediaLightbox and MobileDrawer build their own. | Kept. |
| Tooltip | A name for something whose words are hidden or cut off. | `data-tip` through `TooltipLayer` (29), and the browser's `title=` (185). | `data-tip` only, and only where words are hidden. |
| `Card` | A framed surface. | 5 files. | Kept. |
| List row | One row in a list. | `.row` written about 188 times; `SettingRow`, `LinkRow`. `UniversalDataGrid` for list pages (19 files). | One row primitive; the grid stays for list pages. |
| `SubNav` | In-page tabs, route-driven. | 1 file. | Kept. |
| Section label | The heading of a group of rows. | `Section` in 4 files; `.eyebrow`, `.group-label`, `.section-title`, `.kicker` in 26. | One label. |
| `EmptyState`, `LoadingState`, `ErrorState` | What a page shows when there is nothing yet. | Built on `StateBlock`; used in 1, 3 and 3 files, beside 26 hand-made empty states and the spinner icon placed by hand in 31. | Kept, and used. |
| `Page` | The page frame and its width. | 32 files. | Kept. |

Dead code to delete: `floating/Dropdown`, `floating/Tooltip`,
`floating/composites/Select` and `UniversalPicker` have no users.

### What they look like

```specimen Button
```

```specimen IconButton
```

```specimen TextAction
```

```specimen Input
```

```specimen Switch
```

```specimen MenuItem
```

```specimen Modal
```

```specimen Card
```

```specimen Badge
```

```specimen StateBlock
```

```specimen TooltipLayer
```

**On touch screens, enlarge the control itself.** Under `(pointer: coarse)`
a list row zeroes its own padding and the button inside it takes that
padding, so the 44pt floor is cleared by the thing you press. An inline prose
link is the one exception: a 44pt box around it would swallow taps on the
lines above and below.

---

## Interaction

- **No focus rings**, from the browser or from a component. The global
  `:where(:focus, :focus-visible)` reset is in `app.css`. Focus still moves
  and the keyboard still works.
- **A row that names a destination navigates to it.** A chevron beside it
  expands. Two hit targets.
- **No control that goes where you already are.**
- **Modifier hints wait for a bare accelerator**, never for a key held alone.
  ⌘⇧4 holds ⌘ for as long as it takes to drag a screenshot.
- **Reveal-on-hover has a keyboard path and is visible on touch.** HTML5
  drag events don't fire on touch, so every drag has a keyboard equivalent
  (Alt and an arrow) and a menu equivalent.
- **Tooltips only where words are hidden or cut off.**

### Drag and drop

Every drag uses the same marks, from `themes.css`:

| Token | Value | For |
|---|---|---|
| `--drop-line` | `--color-primary` | where a dragged thing will land between others |
| `--drop-line-width` | 2px | that line, with a 1px radius |
| `--drop-tint` | primary at 15% | a thing or zone that will take the drop |
| `--drag-source-opacity` | 0.4 | the dragged thing's place while it is away |

- **The thing in your hand looks like itself**: the whole pill, with its
  fill. Set the dragged look before the browser takes its snapshot, and dim
  the source after.
- **Only a place that will take the drop lights up.** Elsewhere the cursor
  says no.
- **A drop that worked does not fly home.**
- **Marks fade in on `--motion-hover-in` and out on `--motion-hover-out`.**
  Things that move on a drop slide on `--motion-state`.
- **A drag never leaves state behind.** Escape, a drop outside, and the
  source unmounting all clear the line, the tint and the dimming.

---

## Writing in the interface

UI strings follow the writing rules in the repo's `CLAUDE.md`: plain
English, sentence case, second person, sentences under about 25 words,
hyphens or periods instead of em dashes. An error says what happened and how
to fix it. A button's verb names the place it goes: "Open sources", never an
arrow alone.

---

## Prohibited patterns

Each of these shipped and had to be undone.

1. **No colored strip on a row's leading edge.** At 2px on a 32px row the
   color identified nothing and looked like a stray mark.
2. **No rule where space would do.** See [Elevation](#elevation). There are
   no rules in the sidebar.
3. **Nothing in the chrome recedes.** A recessed sidebar turned its empty
   lower half into a visible object.
4. **No size between the scale's steps**: no `12.5px`, `13.5px`, `15px`,
   `26px`. A value a pixel off its neighbor reads as a mistake.
5. **No monospace as decoration.** A keyboard hint is not code. Mono kickers
   at 9.5 to 10.5px were the clearest sign of the old style.
6. **No uppercase, letterspaced serif.** We ship no small-cap cut, so it is
   stretched capitals. It was the most conspicuous thing on every settings
   page, although it was meant to be quiet.
7. **No color without a meaning.** A compacted tab does not get the accent.
8. **No mark of where work runs**: no cloud marks, no "off your server"
   labels, no lanes. They worried people and nobody used them.
9. **No streaks, counts, overdue alarms or game mechanics.** Someone in
   recovery sees every page.
10. **No wrapper fade** around a page that already staggers its blocks in.
    It doubled the ghosting.

---

## The checklist

Count each row before a page ships. `tools/design-lint.sh` counts the rows
marked with its rule name, as a ratchet against `tools/design-lint.baseline`:
it fails only when a count rises. Escape a single line with `design-ok: <why>`
in a comment on or above it. Never cut a baseline from the shared checkout
(see `CLAUDE.md`).

| | Limit | Lint rule |
|---|---|---|
| prose the page writes for itself | 1 sentence | |
| `Button variant="primary"` | at most 1 | |
| colored things that mean neither *now* nor *pressable* | 0 | |
| `border` and `background` on a block with no input, button or link | 0 | |
| `box-shadow` in the pane | 0 | `shadow` |
| a radius that is not a radius step | 0 | `radius` |
| off-grid px in padding, margin or gap | 0 | `off-grid` |
| `--font-mono` outside code and a chart's SVG | 0 | `mono`, `mono-literal` |
| a font size under 12px outside a chart's SVG | 0 | `tiny-type` (counts under 11px) |
| a half-pixel font size | 0 | `half-px-type` |
| a font size off the scale | 0 | |
| color literals outside `themes.css` and a chart's paint | 0 | `hex-literal`, `rgb-literal` |
| bold or italic asked of the serif | 0 | `serif-impossible` |
| raw `--secondary`, `--border`, `--surface` in a component | 0 | |
| `--surface-elevated` as a hover, active or selected state | 0 | |
| a hand-made copy of a primitive | 0 | |
| distinct font sizes on one list or drawer, its title aside | at most 2 | |
| animating files with no `prefers-reduced-motion` answer | 0 | `no-reduced-motion` |

---

## Removed designs

Kept for the reasoning. Don't rebuild one without answering why it went.

- **Fourteen more themes** (removed 2026-11-17). Everything was designed in
  Oxford and Asgard, and sixteen themes multiplied every check by sixteen.
- **An ink print of the painting as the desk** (2026-11-15). Glass over it
  looked wrong, and with the header row over the print's blank sky it barely
  showed.
- **Flush panes** that met the sidebar edge to edge with a 1px split line.
  Replaced by inset paper cards.
- **A one-row header** with the page's tools in the window corner. The tools
  went back to the action toolbar under each tab.
- **A status capsule** in the header for the server's state. Beside the ∴ it
  read as a second mark.
- **Sidebar hover fills and a pointer-following glow.** The icon filling in
  replaced both.
- **A "∴ Virtues" lockup and New chat beside the ∴.**
- **The home page** (`/home`, 2026-10-06). Home and chat had become one
  place, and the page was a second front door. Today's streams live on the
  Timeline, and a day's account is the wiki's day article.
- **A painting in the page's margin, and a full-width painting over a
  room.** A framed painting on one page is decoration, and a cinematic band
  over a column of chat looked like two products stacked.
- **A vertical stepper**, after chips, a progress rule and an accordion.
  Progress on the reading axis needs no rail.
- **Pins as a shelf of nav rows** that looked like a copy of the doors under
  them, **Recents as a list** of rooms already in the nav, and **threads as a
  sidebar group**. Each became a door or a pin.

---

## Naming

Check whether a word is taken before reusing it.

- **`data_content_bookmark`** owns "bookmark" for ingested saved links. The
  sidebar's kept things are "Pinned" for this reason.
- **`app_project_items.role = 'pin'`** is retrieval scope, unrelated to
  sidebar pins.
- **`tab.pinned`** is tab compaction, unrelated to both.
- **"The desk"** is the window's ground (`--desk`). It is not a section of
  the sidebar.
