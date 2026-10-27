# Design constraints

The shell should look **calm and finished**, because people have to trust it.
This appliance stores one person's mail, money, health and days. Jittery
accents, cramped boxes and near-miss alignment make it look *unfinished*, and
nobody wants an unfinished product storing their whole life.

This file exists because the same mistakes kept recurring across sessions.
Without written constraints, generated UI drifts toward the average SaaS
dashboard.

---

## Sidebar and pane

**The sidebar (called "the desk") stays quiet; the pane (called "the page")
holds the content.**

- The sidebar is quiet: no borders, no fills, no badges, and no colour of its
  own. Type and spacing create the structure.
- The pane can use borders, fills and structure, because that is where the
  work happens.
- **Nothing in the sidebar may be more prominent than the pane.** This one rule
  prevents most of the past mistakes: bordered search fields, saturated
  modifier badges, coloured strips on pinned rows.

---

## The shell

The window has two materials. **The desk** (`--desk`) is the ground: the
header row and the sidebar sit on it and paint nothing of their own. **The
cards** are the work, raised off it in the page's color (`--page`): one per
pane, a desk gap (8px, `--pane-inset`) from the window's edges and from each
other. In split view the gap between the two cards is the resize handle.
Every card keeps its rounded corners, and the one thing that joins a card
is its active tab, which is the top of the card (see the active tab below).
Opening the sidebar or a split only moves cards. There is no rail.

The sidebar is one list, and it never changes: the doors (Setup while it
is not done, New chat, Pages, Tasks, Projects, Applets, Wiki), Pinned and
Recent, with Settings at the foot. Every door opens a page. A page that
has sections carries their list itself: the Wiki's overview has an index,
every Settings page has Settings' section list down its left inside the
card (`SectionNav`, drawn by TabContent), and Setup's tab has its steps
across its top.

- **The sidebar has no title.** Its doors say what it holds.
- **Pages, Tasks, Projects, Applets and the Wiki are doors**, not places of
  their own in the chrome. None of them lists its contents in the sidebar.
- **Settings stands at the foot**, apart from the places above it: you do
  something to the server and leave.
- **The doors are the owner's to arrange, not to add to.** They can be
  reordered and hidden on the desktop sidebar (the web app and the Mac
  app), and that is saved on the profile (`sidebar_doors`). The phone's
  drawer reads the same arrangement and shows the same Pinned list, but
  cannot change either: it has no drag, Alt-arrows or door menu. What a
  person wants one click away goes in Pinned. Doors and pins stay two groups: hiding a door hides
  a place that still exists, and unpinning removes the person's own entry,
  so one list would make the same gesture mean two things.
- Routes did not move.

The phone's drawer (`MobileDrawer`) follows the same order with what a
phone needs around it: Search and Continue setup first, then the doors as
arranged, then All chats and Settings as fixed rows, then Pinned and
Recents, which scroll. A door with a `+` always shows it, since a phone has
no hover.

The Home panel, top to bottom:

1. **Doors**: Setup (while it is not done), then New chat, Pages, Tasks,
   Projects, Applets, Wiki in their shipped order. Pages, Tasks and
   Projects carry a `+` on hover for a new one. Search has no door: it is
   the ⌘K palette. A door moves the way a pin does: by drag, by Alt and an
   arrow, or by Move up and Move down in its menu. Hide in its menu hides
   it. Customize sidebar, in every door's menu and on the space between the
   doors, lists every door with a check beside the shown ones, and Reset to
   default. The last shown door cannot be hidden, so that menu is always
   reachable. Setup is not arranged: it shows until setup is done.
2. **Pinned.** A chat, a project, an applet, a page, or any other url can be
   pinned, from the thing's own pin control or menu, or by dragging a Recent
   row or a pane's tab into Pinned, where a line shows the gap it will land
   in. While something is dragged and nothing is pinned yet, Pinned shows a
   "Drop here to pin" row. A pin shows the mark
   picked for it, else a solid dot in its color. Pins stay in the order the
   person puts them: drag a pin, press Alt with an arrow on a focused pin, or
   use Move up and Move down in its menu. A line in the gap shows where a
   dragged pin will land. A pinned project takes a dropped row or tab, which
   files it there; while a pin is being dragged, the middle of a pinned
   project's row files it and the edges move it.
3. **Recent**: the ten newest chats and pages together, newest first, then
   "See all", which opens All chats. The group's head filters it to chats or
   pages, and its label says which. Chats are set in the sans and pages in
   the serif, with no glyphs.

Every group folds from its label, and the fold is remembered. A group with
nothing in it is not drawn: no empty-state line, because the doors above
already say how to make the first one.

Hovering a chat row or a pinned row reveals inline controls (pin, more).
Both need the keyboard path and the touch fallback the interaction rules
below require.

**Pinned replaces the Desk.** "Desk" is retired as a name: the verb is pin and
unpin, the section is Pinned. "The desk" in [Sidebar and pane](#sidebar-and-pane)
is a name for the sidebar chrome as a whole, not a section of it.

---

## Prohibited patterns

Every one of these was shipped in this codebase and had to be undone, so they
are written as prohibitions.

1. **No coloured left-edge strips.** A 2–4px accent bar on the leading edge of a
   row is one of the most common signs of machine-made UI. It was tried on
   pinned rows; at 2px on a 32px row, three pixels from a 16px icon, the colour
   identified nothing and looked like a stray mark.
2. **Separation goes whitespace → 3–5% lightness → elevation, and you stop at
   the first one that works.** Do not reach for a 1px rule. There are no
   hairline separators in the sidebar.
3. **Nothing in the chrome recedes.** Recessing the sidebar (200px wide,
   full height, lower half empty) turned the emptiness into a visible object;
   *the empty space was not noticeable while the sidebar matched the page*.
   Recessing only the tab strip left a dark band beside a light panel. The
   tabs are browser tabs and still need no strip of their own: they stand on
   the desk, which is already the tone step between the chrome and a card.
   The sidebar sits on `--surface-elevated`, a 4% step, and that is the only
   shift in the shell.
4. **Sizes come from [the scale](#the-scale).** No value between its steps:
   no `12.5px`, `13.5px`, `26px`, `30px`, `34px`. A value that is one or two
   pixels off a neighbour reads as a mistake even when nobody can say why.
5. **No monospace as decoration.** IBM Plex Mono is for code and data. A
   keyboard hint is not code. A shortcut label is not code.
6. **No uppercase + serif + wide tracking.** It looks deliberate but does
   nothing useful. It appeared on a *back button* at 11px/0.14em, which is the
   opposite of what an exit should be.
7. **One left edge.** Every icon in the sidebar starts at the same x; every
   label starts at the same x. Near-misses (12 / 16 / 20px) look sloppy even
   when the reader can't say why.
8. **Non-semantic colour is prohibited.** Use colour only when it carries a
   meaning. A merely-compacted tab does not get the theme accent.

---

## The scale

The shell takes every size, line, gray, type step, corner and timing from
this one scale. It exists because the shell was built in passes that each
kept a consistent rule of their own, and side by side they disagreed by a
pixel or two everywhere: five box heights, two icon weights, five grays,
half-pixel type. Before adding a value, find its step here; if none fits,
change the scale here first.

### Heights

| Step | Size | Used by |
|---|---|---|
| Chrome row | 40px (`--chrome-row-h`) | the window header |
| Control | 28px (`--chrome-tab-h`, `--sidebar-interactive-height`) | every clickable box in a row: tabs, the +, the header's controls, the ∴, sidebar rows |
| Nested control | 20px | a control inside a control: a tab's ×, a row's inline + and ⋯ |

**Everything in the window header shares one centre line**, 20px down the
40px row: the ∴, the controls, the tabs, the + and a view's actions. They
are all 28px controls, 6px from the row's top and bottom.

### Lines and glyphs

**The shell's icons are Atlas**, the hand-drawn set in `AtlasIcon`. Phosphor
and Fluent were tried in its place and both read wrong beside Avenir and the
serif, so the shell keeps its own drawings.

**Every icon renders at 1.25px of ink, at every size.** `AtlasIcon` derives
its stroke from its size (1.25 × 16 ÷ size), so a stroke is never typed by
hand. Two glyph sizes: 14px beside 12px text (a tab's icon and ×, inbox
rows), and 16px (`SHELL_GLYPH_PX`) in rows and the header.

Glyphs sit on doors, pins and projects. The row slot, the 16px before such
a row's name, holds a line glyph in ink. A pin or a project tints its glyph
with its own color. An emoji the user picked stays, because it is their
content. Recent carries no glyphs: it is a list of titles, and the face
tells a page from a chat there.

The ∴ is the one filled shape in the chrome, and its lit dot is the one
accent color there.

### Grays

The interaction ramp mixes the theme's ink (`--wash-ink`) into whatever is
behind it, so it reads in all sixteen themes. Four steps, no others:

| Step | Token | Mix | Means |
|---|---|---|---|
| Rest | — | 0% | nothing |
| Hover | `--hover-bg` | 7% | the pointer is here |
| Selected | `--active-bg` | 12% | where you are: the rail's room, a sidebar row, the active tab of an unfocused pane |
| Press | `--press-bg` | 16% | held down, only for the duration of the press |

**The active tab is marked by fill and ink**: the page's color (`--page`)
on the desk, the same step that separates a card, and full ink. It is the
top of its card, as in a browser: it takes the card's edge on three sides,
paints over the card's top edge, and its bottom corners flare out into that
edge. A first tab on the card's left edge squares off the card's corner
instead. Tabs you are not on have no fill until you point at them. In split
view each pane's active tab joins its own card, and the unfocused one dims
its ink.

### Tone, not lines

**Cards separate from the desk by tone.** In every theme, light or dark, the
desk is a step darker than the page, so a card always reads as a sheet lying
on the ground, never as a hole in it.
Where a theme's step is large enough, a card has no border
(`--card-edge: transparent`); a line only stands in where the tone cannot.

| Theme | Desk | Page | Card edge |
|---|---|---|---|
| Pemberley | `#F0F0F2`, a cool light gray | `#FFFFFF` | none |
| Oxford (Virtues Light) | `#ECEAE5`, a warm gray | `#FFFFFF` | none |
| Asgard (Virtues Dark) | `#0A0A0A` | `#161616` | none |
| the other twelve | each theme's own `--surface-elevated` tint, darkened to about 7 L* below the page (light) or 5 (dark) | `--page` | none |
| Borghese | `--surface-elevated` | pure black | `--border`: nothing can sit below black, so its cards keep a line |

Text uses three inks: foreground, muted and subtle.

### Type

See [Type](#type).

### Corners and elevation

| Step | Radius | Used by |
|---|---|---|
| Box | 8px (`--card-radius`, `--radius-md`) | cards, controls, tabs, rows, popovers |
| Nested | 4px (`--radius-sm`) | a box inside a box, such as a tab's × |
| Pill | `--radius-full` | the composer |

Floating layers (popovers, menus, modals) carry the one shadow. The chrome
never does.

### Motion

Seven tokens in `themes.css`, always on `--ease-premium`:

| Token | Value | For |
|---|---|---|
| `--motion-hover-in` | 80ms | a hover's fill and ink coming on |
| `--motion-hover-out` | 200ms | going off: instant on, gentle off, so running the pointer along a row leaves a short trail rather than a strobe |
| `--motion-press` | 60ms | sinking on press |
| `--motion-release` | 180ms | springing back |
| `--motion-state` | 150ms | a selection changing, a group folding |
| `--motion-enter` | 180ms | a popover or a menu appearing |
| `--motion-layout` | 300ms | the sidebar opening, a pane resizing, the sidebar glyph's tint, a tab opening or closing |

Square controls sink to 94% while pressed. Rows and tabs take the press fill
only, because a wide row that shrank would read as a wobble. The ∴ dot's
single swell when it lights is the one motion with a meaning of its own, and
it does not repeat.

### Drag and drop

Every drag in the app uses the same few marks, from tokens in `themes.css`:

| Token | Value | For |
|---|---|---|
| `--drop-line` | `--color-primary` | where a dragged thing will land between others: a pin gap, a gap between blocks |
| `--drop-line-width` | 2px | that line's thickness, with a 1px radius |
| `--drop-tint` | primary at 15% | a thing or a zone that will take the drop: a pane, a project row, a file zone, and the slot a tab opens in a strip |
| `--drag-source-opacity` | 0.4 | the dragged thing's place while it is away |

- **The thing in your hand looks like itself.** A dragged row or tab is the
  whole pill, with the page fill, and never a bare label or a faded copy.
  Set the dragged look before the browser takes its snapshot, and dim the
  source only after it.
- **Only a place that will take the drop lights up.** A file zone checks for
  files, and a pane does not take its own tab. Anywhere else the cursor says
  no: the app-wide `dragover` sets `none` unless a zone accepted the drag.
- **A drop that worked does not fly home.** The dragged copy goes where it
  landed, or fades there.
- **Marks fade in on `--motion-hover-in` and out on `--motion-hover-out`.**
  Things that move on a drop slide on `--motion-state`. Under reduced motion
  the marks stay and the travel goes.
- **A drag never leaves state behind.** Escape, a drop outside, and the
  source unmounting all clear the line, the tint and the dimming.

### Alignment

**Each group in the window header starts at the column below it.** The ∴'s
glyph starts on the sidebar's icon line, level with the rows' icons, and the
window's controls follow it, 4px apart, so every glyph has 16px of air on
each side. In the Mac app the title bar is an overlay: the traffic lights sit
in the header row on its centre line, the close button as far in from the
left edge as it is down from the top, and the ∴ follows the zoom button with
the same 16px of air. In full screen macOS hides the lights and the ∴ moves
back to the sidebar's icon line. The row's empty space moves the window. Each pane's tabs start at that pane's edge.
With the sidebar closed the controls keep their place, and the left pane's
tabs start just after them. 

---

## Tokens

Defined in `apps/web/src/themes.css`. Sixteen themes; every value below is set
per theme where it varies. The scale above says which step each one is.

| Token | Value | Notes |
|---|---|---|
| `--chrome-row-h` | 40px | The window header AND the panel's title row. One number. |
| `--chrome-tab-h` | 28px | A tab, and every other control in a row. |
| `--pane-inset` | 8px | The desk gap: around the cards and between any two of them. |
| `--sidebar-interactive-height` | 28px | Sidebar rows. |
| `--sidebar-padding-left-base` | 12px | Row inset. The panel body adds 4px. |
| `--sidebar-icon-opacity` | 0.5 | 0.4 for children, 1.0 on hover/active. |
| `--hover-bg` / `--active-bg` / `--press-bg` | ink mix 7% / 12% / 16% | The interaction ramp. |

**Never use a surface token for an interaction state.** `--surface-elevated` is
#F5F5F5 against a #FFFFFF parent — a 4% shift that is too small to see.
Hovers, selections and active states come from the foreground-mix ramp, which is
defined against the text colour and therefore legible in all sixteen themes.
This bug shipped twice: dead hovers across 39 rules, and an active tab rendered
`#FFFFFF` on `#FFFFFF`.

---

## Type

| Role | Face | Size | Weight |
|---|---|---|---|
| Tab, group heading, meta | Avenir | 12px | 400 (headings muted) |
| Row (every panel) | Avenir | 13px | 400 |
| Writing in the sidebar (a page, a project, a pin of either) | EB Garamond | 14px | 400 |
| Panel title | EB Garamond | 16px | 400 |
| Body / prose | EB Garamond | — | the page, not the chrome |

**Weight never signals state.** A selected row or the active tab changes
fill and ink, never weight: a bolder label is a wider one, and everything
beside it moves.

**In the chrome the serif is for writing**: a room's title, a page, a
project, and a pin of either. A chat is a conversation, so its row stays
sans, as do places and verbs (New chat, Pages, Tasks). Because Recent has
no glyphs, the face is what tells a page from a chat there. The serif's
smaller x-height is why its rows are a step larger (14px against 13px), on
the scale rather than at a half pixel.

The ∴ is drawn, not typed, and it stands alone in the window header without
a wordmark.

**Labels are primary; icons support them.** Eight identical 16px icons at full
contrast in a column make a sidebar look like every other sidebar; the labels
should do the work, as they do in a table of contents.

---

## Interaction

- **Tabs are browser tabs on their card.** Sized to their labels, one
  card radius apart, so the active tab's flared corner fills the gap and
  never reaches a neighbour's hover pill. The + sits the same distance
  after the last tab. The first tab starts on the card's left edge. The
  active tab is the top of its card (see [Grays](#grays)); the others have
  no fill and light as a pill when hovered or pressed. In split view the
  active tab of the pane you are not working in has muted ink. Joined tabs
  without flares made a card look half floating sheet and half tab foot.
  The flared corners are what make the tab and the card read as one
  surface.
- **Switching tabs is instant.** The fill, edge, corners and ink change in
  one frame. Only the hover pill fades.
- **Opening and closing a tab is one motion run both ways**, on
  `--motion-layout`, in two beats that overlap a little. Opening: the space
  opens and the tabs to its right and the + glide aside, then the tab rises
  one radius out of the card and fades in. Closing: the tab sinks toward
  the card and fades, then the space closes and its neighbours glide in.
  Nothing is squeezed while it can be seen. Tabs already open when the
  window loads do not animate.
- **A crowded strip compacts, then scrolls.** The tabs you are not on drop
  to their icons. The tab you are on keeps its full width. Past that the
  strip scrolls, and an edge fades over 32px while there are tabs past it.
  Switching to a tab scrolls it into view clear of the fades.
- **A tab carries the sidebar's marks.** A chat that is replying shows the
  working ∴ in place of its icon. A reply that landed while the tab was in
  the background puts the unread dot on the icon's corner. A title that
  does not fit fades out over its last 24px instead of ending in an
  ellipsis.
- **A view's actions sit at the end of its tab strip** (`paneActions`), the
  chat's ⋮ included, not in a corner of the page. The phone has no strip, so
  there the chat keeps its corner button.
- **A row that names a destination navigates to it.** The chevron expands.
  Two hit targets. A row that only toggles means "Projects" cannot take you to
  Projects.
- **No control that goes where you already are.** The `···` overflow is only
  rendered when the row itself cannot reach the index.
- **Modifier hints gate on a BARE accelerator**, never on hold-duration alone.
  ⌘⇧4 holds ⌘ for as long as it takes to drag a screenshot marquee — which is
  why the pane badge kept appearing in screenshots of the app.
- **Reveal-on-hover must have a keyboard path and be visible on touch.**
  HTML5 drag events don't fire on touch at all, so any drag affordance needs a
  keyboard equivalent (⌥↑/⌥↓) and a context-menu equivalent.
- Respect `prefers-reduced-motion` — but keep the *information*, drop only the
  travel.

---

## Removed, and why

- **Pins as a shelf of nav rows (the Desk).** Two visual treatments were
  tried (plain rows, then colored ribbons) and neither solved the actual
  problem: a pinned "Pages" and a nav "Pages" render identically, so the
  section looked like a duplicate of the nav directly beneath it. The `app_pins`
  table, its API and the reorder endpoint stayed through that. Pinning came
  back as **Pinned** at the top of the Home panel (see The shell), reached
  by a pin control on the thing itself or by a tab's menu, which can pin any
  route. A pin wears its own mark, or a dot in its color, so a pinned room
  does not read as a copy of its door.
- **Recents as a sidebar-wide list.** Six of twelve rows were destinations
  already in the nav; the sidebar was the largest contributor to its own
  history list. Recent returned at the foot of the Home panel, where it
  lists chats and pages only, so no row in it can be a room.
- **Projects as a group in the sidebar.** Five projects, "Show more" and a
  hover card on each row. Projects became a door like Tasks: the list is
  the Projects page, and a project someone wants close is pinned.
- **Today.** The Timeline is the live view of today; `/day` only exists
  after the nightly run.

## Naming

Check whether a word is already taken before reusing it. Three live examples:

- **`data_content_bookmark`** owns "bookmark" for *ingested* saved links (GitHub
  stars, browser bookmarks). The sidebar's kept routes stayed "Pinned" for this
  reason.
- **`app_project_items.role = 'pin'`** is retrieval scope, unrelated to sidebar
  pins.
- **`tab.pinned`** is tab *compaction*, unrelated to both.
