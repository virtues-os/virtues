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
| Dark | Asgard | Near-black and near neutral, with bright main text, two cool grays below it and quiet hairlines. The glass and the painting stay. Derived from the same tokens, never ported from a reference. |
| Match system | Oxford or Asgard | Follows `prefers-color-scheme`. |

```specimen Contrast
```

The tokens below live in `apps/web/src/themes.css`. A BASE `:root` block holds
the values every theme starts from; Oxford's block applies to `:root` as well,
so Light is the default, and Asgard's blocks override them for Dark. A theme name stored before the cut
lands on Light or Dark by its darkness (`RETIRED_THEMES` in `theme.ts`), so
nobody on a dark theme is dropped onto a white page.

A design is checked in Light and in Dark. Nothing else needs checking.

---

## Tokens

All tokens live in `apps/web/src/themes.css`. `app.css` registers the fonts
and holds the few app-wide rules (the focus reset, the Glass setting, the
selection color).

**Write the `--color-` spelling in a component.** `themes.css` defines raw
values (`--surface`, `--border`, `--primary`) and bridges them to Tailwind
through `@theme inline` as `--color-surface`, `--color-border`,
`--color-primary`. Both resolve. The bridged name is the one the rest of
the tree uses and the one a Tailwind utility maps to. Raw names belong
inside `themes.css` only.

### Color

| Role | Token | Use |
|---|---|---|
| Desk | `--desk` | The window's ground, behind the cards. |
| Page | `--page` | A card: the pane you work in. |
| Ink | `--color-foreground` | Text. |
| Muted, subtle | `--color-foreground-muted`, `--color-foreground-subtle` | Secondary text, captions. Three inks, no others. |
| Hairline | `--color-border` | A framed block's edge. Decoration, held to no contrast minimum. |
| Field edge | `--color-border-field` | A text field's edge at rest (Input, Textarea, SearchField). It shows where a field is, so it holds 3:1 against the page in both themes. |
| Accent | `--color-primary` | Means *now* or *pressable*, and fills the page's one primary Button. The person picks it (see [Accent](#accent)). |
| Status | `--color-success`, `--color-warning`, `--color-error`, `--color-info` | A state a person can act on. |

```specimen Colors
```

**One accent, with two meanings.** `--color-primary` is the only color a page
carries, and it means *now* (a live marker, today's bar, the open clip) or
*pressable* (a link, a save, the one filled `Button variant="primary"` a page
is allowed). It is never a rule, a fill for decoration, a border for emphasis
or a second filled button, and it never marks where work ran.

### Accent

The person picks the accent in Settings, Appearance, from seven swatches: Navy
(the default), Blue, Indigo, Purple, Pink, Teal and Graphite (`ACCENTS` in
`lib/utils/theme.ts`), or chooses their own color from the last swatch,
which opens the system color picker. It is saved on the profile beside the
theme (`ui_preferences.accent`, a swatch id or a `#rrggbb`), cached for the
pre-paint in `app.html`, and set as `data-accent` on `<html>`.

- **Each swatch is tuned twice** in `themes.css`, as `--accent-light` and
  `--accent-dark`, so links and the filled button's label clear AA on the
  page in both themes. Light's `--primary` reads `--accent-light` and Dark's
  reads `--accent-dark`. Hover, press and the tint are mixed from it, so a
  swatch is one value per theme.
- **A chosen color is fitted to both themes.** `tuneAccent` keeps its hue
  and saturation and moves its lightness: darker until it clears 4.6:1 on
  Light's page, lighter until it clears 7:1 on Dark's, where the swatches
  sit. A color that already clears is kept. Both tunings are set on
  `<html>` as `--accent-light` and `--accent-dark` with
  `data-accent="custom"`.
- **No red, green or amber swatch.** Those mean danger, success and warning,
  and an accent in one of them would make a link read as a state. A person
  may still choose one of their own.
- Every swatch is a plain `[data-accent]` rule as well, so an element can
  show a swatch other than the chosen one: the picker's dots and the
  specimen below do this.

```specimen Accents
```

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
and even there the values are read from tokens. Canvas code, which cannot
resolve `var()`, reads tokens with `getComputedStyle` and keeps a literal only
as the fallback.

### Grays

Every gray in Light is neutral: the grounds (`--background`,
`--surface-elevated`, `--desk`), the borders and the washes have no hue of
their own, so the only color on a page is its ink and its accent. The inks
keep their navy.

Every gray wash mixes one pigment, `--wash-ink`, into whatever is behind it,
so it reads on any ground. On Light the pigment is a neutral near-black, so a
wash is a plain gray like the grounds and lines. A pigment with a hue tints
everything it touches: a warm one comes out beige on the white sheet. On Dark
it is white.

**A tinted block at rest uses `--fill-bg`** (6% on Light, 3% on Dark): a sent
message, an open card, a chip, a key cap. It is a wash, so it shows on any
ground, where the surface token would vanish on a ground of its own gray.
`--surface-elevated` is for a pane's own ground. On the sheet the two come
out within a step of each other.

The interaction ramp sits above the fill. Four steps, no others:

| Step | Token | Mix, Light / Dark | Means |
|---|---|---|---|
| Rest | | 0% | nothing |
| Hover | `--hover-bg` | 4% / 5% | the pointer is here |
| Selected | `--active-bg` | 7% / 9% | where you are |
| Press | `--press-bg` | 10% / 12% | held down, only while pressed |

```specimen Ramp
```

The steps on Light are light on purpose, so a chosen row stays quiet beside
its text. Hover is lighter than the fill, so a block that rests on `--fill-bg`
must not change to `--hover-bg` under the pointer: it would get lighter.

**Never use a surface token for an interaction state.** `--surface-elevated`
is one fixed color, so on a ground of the same gray it is no step at all,
where a wash is a step off whatever is behind it. That bug shipped twice:
dead hovers across 39 rules, and an active tab drawn white on white. **Never
use an interaction step for a block at rest** either: a block filled with
`--hover-bg` sits next to one filled with `--fill-bg` as a second gray. The scrollbar's thumb is on the same ramp at 28% on Light and 22% on
Dark, with no track.

### Type

Four faces, all self-hosted `woff2` in `apps/web/static/fonts/`. A box runs on
a LAN with no guaranteed egress, and the kiosk is Linux, so no face may be a
CDN or a system font.

| Token | Face | Cuts | For |
|---|---|---|---|
| `--font-serif` | EB Garamond | 400 only | Reading text and titles. |
| `--font-serif-ui` | EB Garamond UI | 400 only | The serif in a row beside an icon or a control. |
| `--font-serif-lead` | EB Garamond Lead | 600, Latin only | The first word of a day's Abstract, and bold in a chat reply set in the essay layout. Nothing else. |
| `--font-sans` | Avenir | 400, 500, 700 | The chrome: labels, buttons, menus, captions. |
| `--font-mono` | IBM Plex Mono | 400, 500, 600 | Code, and a chart's own labels. |
| `--font-hand` | Nothing You Could Do | 400 | The owner's notes in a day page's margin, and nothing else. |

**The scale has seven steps.** Every font size in the app is one of them.

| Token | Face | Used for |
|---|---|---|
| `--text-title` | serif | A page title. |
| `--text-heading` | serif | A section title, a step's heading. Also `--md-h2-size`. |
| `--text-lead` | serif | The box's line: a list row, a card's question, a caption under a chart. |
| `--text-body` | serif or sans | What you read and what you type. |
| `--text-label` | sans | A row, a button, a link, a menu item, a field. Also `--chrome-label-font-size`. |
| `--text-small` | sans | A label, a quiet verb, a note, a date, a chart's readout. |
| `--text-caption` | sans | A tab, a group heading, a hint. |

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
header that asks the serif for bold. The essay layout's reply bold names it
on purpose: a reply's bold lead-ins are how it is scanned, and in the regular
cut they read as plain text. The owner chose this on 2026-11-21.

#### Two serif tokens for one font

EB Garamond declares ascent and descent of 1.007 and 0.298, which leaves
centered letters visibly low beside an icon. `EB Garamond UI` is the same
file registered with corrected metrics (0.943 and 0.290), so the cap band
lands on the center line. Use `--font-serif-ui` in a row and `--font-serif`
in prose: the correction in prose would move the first baseline across the
wiki and pages. The derivation is the comment above the registration in
`app.css`.

#### In the chrome

The chrome is set in the sans. The one serif in it is "Virtues" beside the ∴
when the sidebar is open. Reading text and page titles are serif on the
paper.

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

**Page widths.** Three, in `themes.css`. A view declares the one it is
designed for with `Page`'s `maxWidth` or, when it lays out its own column,
`usePageWidth` (`stores/pageWidth.svelte.ts`), and sets the column's
`max-width` from the result. The person reading can change it from the width
button in the page bar. The choice is kept per kind of page on that device.

| Width | Token | Size | For |
|---|---|---|---|
| Reading | `--page-reading` | the reading measure | text and plain lists |
| Medium | `--page-medium` | 60rem | a page with a side column |
| Full | `--page-full` | the whole pane | grids, tables and list pages |

Settings is the one room with its own Reading column: `--page-settings`
(42.5rem), wide enough for a row's label, help and control side by side.
`SettingsPage` and the landing pass it to `usePageWidth` as `reading`.

Never cap a page column with a literal or a token directly: the page bar
can't change a width it doesn't know about. A measure inside the column (a
paragraph at 60ch, a dialog) is not the page's width and stays as it is.

**A page has one measure, and its padding lives on the outer scroller.**
Center the measure inside that padding, never pad from inside it, or two
pages that claim the same width start their text at different x. When a
layout responds to width, use a container query, not a media query: in a
split the pane runs out of room, not the window.

### Controls

Every box you click is one of three heights, from `themes.css`:

| Token | For |
|---|---|
| `--control-h` | every control: a Button, an IconButton, a field, a select, a tab, the header's glass buttons |
| `--control-nested-h` | a control inside a card, a row or a field: a tab's ×, a row's ⋯, Copy code |
| `--control-touch-h` | a control on a touch screen |

Under `(pointer: coarse)` a Button, a field and a select grow to 44px
themselves. A field's text grows to 16px there too, because iOS zooms the
page into any field set smaller. An IconButton chooses its size
(`size="nested"`, `size="touch"`), since a nested one stays small and takes a
44px hit area around it instead. Sidebar rows are 32px with 2px between
them, so a row lands every 34px.

```specimen Controls
```

### Radius

| Token | Used by |
|---|---|
| `--radius-xs` | a drop line's end |
| `--radius-sm` | a box inside a box, such as a tab's × |
| `--radius-md` | controls, menus, popovers, rows (`--card-radius`) |
| `--radius-lg` | a tab, the header's glass controls, a text field, the search field |
| `--radius-xl` | a pane, the sidebar's glass, a card in the pane, a home tile |
| `--radius-full` | a pill: a badge, a chip, a toggle's track |

The composer is the one exception today: `ChatInput.svelte` gives it its own
16px radius, which is off this scale.

```specimen Radius
```

A radius is one of these, 0, or 50% for a dot. Write the token, not the
length.

### Elevation

**One shadow.** `--shadow-float` is on every layer that floats over a page:
menus, popovers, dialogs. Glass carries its own lift (`--lq-lift`), described
under [Glass](#glass). Nothing else casts a shadow: not a card in the pane, not
the chrome. Two layers still break this today: `Modal` draws its own deeper
shadow, and the search palette adds a second one to `--shadow-float`.

**Separation goes whitespace, then a step of 3 to 5% lightness, then
elevation.** Stop at the first one that works. Do not reach for a 1px rule.

**Elevation is for things that float over the page.** A plane that fills the
screen, such as the phone's drawer, takes the page's own paint and gets its
depth from sliding.

### Stacking

**Anything that floats over a page or the app takes a step from the ladder.**
The ladder is in `themes.css`, low to high:

| Step | For |
|---|---|
| `--z-sticky` | a page's sticky bar, breadcrumbs, outline |
| `--z-dropdown` | dropdowns, menus, a field's list of matches |
| `--z-popover` | a ref preview, a selection popover, a hover note |
| `--z-page` | a full-screen page on the phone, and a page opened from it one above |
| `--z-overlay` | split panes, side panels, the tab bar, the floating layer |
| `--z-modal` | dialogs, search |
| `--z-lightbox` | fullscreen media |
| `--z-context-menu` | the right-click menu, and `--z-context-submenu` one above it |
| `--z-tooltip` | tooltips and the label that follows a drag |

A neighbor of a step is a `calc()` from it. A scrim under its popover is
`calc(var(--z-popover) - 1)`, and a floating menu that opens over another is
`calc(var(--z-overlay) + 1)`.

**Order inside one component is a small plain number.** A placeholder over
its field, a marker over its line and a table's own pinned header only have
to beat their siblings. Do not give them a step from the ladder: a table
header at `--z-sticky` would tie with the page's sticky bar.

The setup flow numbers its own fixed layers below `--z-overlay`. The ladder
has no step for them.

### Motion

The shell's tokens, always on `--ease-premium`:

| Token | For |
|---|---|
| `--motion-hover-in` | a hover coming on |
| `--motion-hover-out` | going off: instant on, gentle off, so running the pointer down a list leaves a short trail |
| `--motion-press` | sinking on press |
| `--motion-release` | springing back |
| `--motion-state` | a selection changing, a group folding |
| `--motion-enter` | a popover or menu appearing |
| `--motion-exit` | it leaving, on `--ease-exit`, which speeds up as it goes |
| `--motion-layout` | a pane resizing, a tab opening or closing |

```specimen Motion
```

The sidebar opens in 440ms with its glass on a small spring and closes in
260ms (`data-sidebar-motion` on the root). Leaving is quicker than arriving.

A pressed control sinks a little: an IconButton to 94%, the ∴ and the
header's corner buttons to 92%, a tab to 97%. Rows take the press fill only,
because a wide row that shrinks reads as a wobble.

**A page arrives rather than cutting** (`lib/tabs/arrive.ts`): when a tab's
page changes it rises a few pixels as it fades up, quickly enough that
opening still feels instant. A page opened from search comes up further, on a
soft spring. Home and a new chat keep still, because a fading ancestor cuts
glass off from what it frosts.

**Only work loops.** A loading browser page and a page the AI is writing
pulse until they finish. Nothing loops for decoration.

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
(`routes/(app)/+layout.svelte`). It is in full color in Light; Dark dims it
through `--window-scene-filter`. The panes cover most of it.

### Paper

A pane is a sheet of paper (`--page`) at `--radius-xl`, standing
`--window-air` in from the window's edges and `--pane-gap` from
the sidebar and from the other pane in a split. A card separates from the
desk by tone, not a line: the desk is a step darker than the page in both
themes. Home has no paper: its tiles stand on the desk.

**Reading text is always on paper, never on glass.**

### Glass

Glass is for what floats over the desk and the painting: the ∴, the rail and
the open sidebar, the tabs, the header's corner buttons, the home tiles. It is
one recipe, `.lq` in `app.css` with its `--lq-` tokens in `themes.css`. Do
not add a second glass.

- **One density and one lift.** The sidebar, the selected tab, the corner
  buttons, the ∴ and the home tiles are the same glass. It is dense enough
  that a label stays readable over the painting, and a flat fill, because a
  gradient as tall as the sidebar shows bands in Chrome. Dark's is denser
  than Light's.
- **Selected inside glass** is a flat tint, never a second pane of glass. No
  card on a card, no shadow on a shadow.
- The rim (`--lq-rim`) and the lift (`--lq-lift`) are set per theme in
  `themes.css`; Dark's are plainer than Light's.

**The Glass setting** (Settings, Appearance) is None, Low, Medium or High,
per browser (`lib/stores/glass.svelte.ts`, `data-glass` on the root, set
before first paint in `app.html`). Every fill thins by its share times
`--lq-see`, which is 1 at Medium; Low and High set it lower and higher in
`app.css`. None swaps the fills for the plain surface and drops the backdrop
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

The header row is `--chrome-row-h` tall. Its controls are `--control-h`, with
the rest of the row split as air above and below them, so the panes start
one header row down. In the
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

Closed, the sidebar is a floating glass rail `--rail-w` wide that
fits its icons. Open, it is a full-height panel of regular glass. Both reach
up into the header row. Every row stands 4px in from the glass with 10px of
padding, open or closed, so icons and the ∴ sit 14px in and nothing moves
sideways when it toggles. Closing shrinks the width and the height together.

Rows are `--sidebar-interactive-height`, 2px apart, so a row lands every
`--control-h`. The ∴ is a control (`--chrome-tab-h`), 2px taller than a row,
and stands 16px above the doors, the same 16px as from the doors to Pinned.

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
3. **Recent**: chats, pages, threads and applets you opened or changed,
   newest first, with pinned things left out (`lib/sidebar/recents.svelte.ts`).
   It shows as many as fit, up to ten, then "See all". A filter in the
   group's heading narrows it to one kind and is remembered.

Every group folds from its label and the fold is remembered. A group with
nothing in it is not drawn.

**The sidebar is quieter than the pane.** No borders, no fills of its own, no
badges, no rules. Type and spacing make the structure. Nothing in the sidebar
may be more prominent than the page.

**Hover fills the icon.** Under the pointer and on the active row the line
glyph becomes its solid version (`SOLID_GLYPHS` and `fillable` in
`AtlasIcon`) and the ink comes up. There is no hover fill on a sidebar row.

### Tabs

Every tab is the same width whatever its title (`WindowTabBar`), at
`--radius-lg`, so the strip reads as a row of equal slots and a tab does not
jump when its chat names itself. The selected tab is on the sidebar's glass
(`.lq`). Tabs you are not on have no fill until you point at them.
Pointing at a tab opens its hover card (`TabHoverCard`) with the full title
and its actions. In a split, the active tab of the pane you are not working
in dims its ink.

- **Switching tabs is instant.** Fill and ink change in one frame; only the
  hover fades.
- **Opening and closing a tab is one motion run both ways** on
  `--motion-layout`: the space opens and the neighbors glide aside, then the
  tab fades in; closing reverses it. Tabs already open when the window loads
  do not animate.
- **A crowded strip shrinks its tabs together**, down to their icons, then
  scrolls.
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
- **Right**: the page's status (`usePageStatus`), such as "Saving",
  "Offline" or "Couldn't save" (a saved page shows nothing), then what the
  page lets you do: controls handed over with
  `usePageActions`, then buttons from `paneActions.set`, two inline and the
  rest behind a `···`.

The status says the page's state, and for an applet which model slot runs
it. It never marks work as on or off your server.

The phone has no toolbar. There a page keeps its title and controls at its
top, and the chat keeps its corner button.

### Search

The palette (`SearchModal`) opens centred on the window, near the top, on a
blurred veil.
On the phone and under reduced transparency the veil has no blur.

### Glyphs

**The icons are Myna** (`@iconify-json/mynaui`, named `mynaui:…`). The shell's
own things keep their names in `AtlasIcon`'s table (`atlas:wiki`,
`atlas:pages`), and each name draws a Myna icon. `scripts/myna-icons.mjs`
bundles only the icons the source names, so write a name out in full: a name
built at runtime is not bundled. The icons named by code the first paint
loads come with it; the rest load just after it, and one drawn sooner holds
its space, empty, until it arrives (`Icon.svelte`). Older Remix and Phosphor names are mapped in
`lib/legacyIcons.ts`. Brand marks come from Simple Icons. The icons a person
picks for a page, chat, pin or thread come from Lucide (`lib/lucide.ts`,
`IconPicker`).

**Line icons render at 1.25px of ink.** `AtlasIcon` derives its stroke from
its size. Two glyph sizes: 14px beside 12px text, and 16px (`SHELL_GLYPH_PX`)
in rows and the header. A pin's mark is the exception: `PinGlyph` draws it at
12px with a 1.5px stroke, so it holds up inside its tinted tile.

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
should have a specimen on `/design` (`Popover`, `Page`, `SubNav` and
`Section` do not yet). When a primitive exists, a page uses it: a hand-made
copy is a regression, however small.

"Today" is what the code does now. "Decided" is where it is going, and the
gap is open work.

| Primitive | Job | Today | Decided |
|---|---|---|---|
| `Button` | A labeled action. `primary` is the one action a page is for, at most one; `primary` fills with the accent; `danger` destroys something and is never also primary; it is red ink and an edge at rest and fills only under the pointer, so it never looks like the primary. | `--radius-md`; 34px, 44px on touch. 73 files, beside about 485 hand-styled `<button>`s. | Kept. |
| `IconButton` | A button with no words. `label` is required and feeds `aria-label` and `title`. | `nested` 20px, `control` 34px (the default), `touch` 44px; ghost, secondary, danger. 35 files. | Kept. |
| `TextAction` | A button with no box: a quiet link in `--color-primary`. `inline` keeps `font: inherit` mid-sentence. | 33 files. | Kept. |
| `Input`, `Textarea` | Text fields with label, help and validation. | 34px, 44px on touch; 14px text, 16px on touch. 12 and 3 files, beside about 100 raw fields. | Kept; the raw fields move onto them. |
| `Switch`, `Select` | A toggle and a choice among named options. | `Select` (`lib/components`) is 34px on the field edge and opens a menu of `MenuItem` rows on `lib/floating`, with keyboard, typeahead and groups; `variant="bare"` sits in a sentence or an underlined form. No native `<select>` is left: it draws the operating system's menu, which ignores the theme. `Switch` lives under `settings/`. | Promote `Switch` to `lib/components`. |
| Checkbox | A box you tick. | None: `TaskCheck` and `TaskCard` each build `role=checkbox`, beside about 10 native ones. | Add one. |
| Segmented control | Two to five exclusive choices in a row. | Five separate builds (ContextViewPanel, DayPage, DisplaySettingsPopover, IconPicker, ThreadsView). | Add one. |
| Chip | A small label or filter. | `Badge`, `LinkChip`, `ProjectChip`, `DataGridFilterChip`, `ref-badge.css`, and `.chip` in 13 files. | One Chip. |
| `MenuItem`, context menu | A row in a menu. | `MenuItem` in 11 files; the context menu positions itself (`contextMenu/fit.ts`). | One menu on `lib/floating`. |
| `Popover` | Anything anchored to a control. | `lib/floating` in about 24 files; about 15 overlays hand-build their own. | Everything anchored uses it. |
| `Modal` | A dialog. `DialogHost` draws confirm and prompt on it. | 16 files; SearchModal, MediaLightbox and MobileDrawer build their own. Its close is a hand-made 28px button, and it draws its own shadow and uses raw `--surface` and `--border`. | Close on `IconButton`, `--shadow-float`, `--color-` tokens. |
| Tooltip | A name for something whose words are hidden or cut off. | `data-tip` through `TooltipLayer` (29), and the browser's `title=` (185). | `data-tip` only, and only where words are hidden. |
| `Card` | A framed surface. | 5 files. | Kept. |
| List row | One row in a list. | `.row` written about 188 times; `SettingRow`, `LinkRow`. `UniversalDataGrid` for list pages (19 files). | One row primitive; the grid stays for list pages. |
| `SubNav` | In-page tabs, route-driven. | 1 file. | Kept. |
| Section label | The heading of a group of rows. | `Section` in 4 files; `.eyebrow`, `.group-label`, `.section-title`, `.kicker` in 26. | One label. |
| `EmptyState`, `LoadingState`, `ErrorState` | What a page shows when there is nothing yet. | Built on `StateBlock`; used in 1, 3 and 3 files, beside 26 hand-made empty states and the spinner icon placed by hand in 31. | Kept, and used. |
| `Page` | The page frame and its width. | About 20 files. | Kept. |

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

| Token | For |
|---|---|
| `--drop-line` | where a dragged thing will land between others |
| `--drop-line-width` | that line, with a 1px radius |
| `--drop-tint` | a thing or zone that will take the drop |
| `--drag-source-opacity` | the dragged thing's place while it is away |

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
| raw `--primary`, `--border`, `--surface` in a component | 0 | |
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
- **A "∴ Virtues" logo lockup in the header row, and New chat beside the
  ∴.** The ∴ widening into the sidebar's first row, with "Virtues" on the
  labels' line, replaced both.
- **The home page** (`/home`, 2026-10-06). Home and chat had become one
  place, and the page was a second front door. A day's account is the
  wiki's day article.
- **The Timeline page** (`/timeline`, 2026-11-20). A full-pane map of one day
  with a list beside it, lanes under it and questions about places. It read
  as too much on one page. Today's route is the Inbox's Map view, and the
  old address opens it.
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
