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
Every card keeps all four rounded corners, and nothing joins: opening the
sidebar or a split only moves cards. There is no rail.

The sidebar shows one room's list at a
time, Home's unless you have stepped into another room. Defined in
`apps/web/src/lib/sidebar/rooms.ts`.

| Room | Stepped into from | Its list |
|---|---|---|
| **Home** | the ground; every other room steps back to it | doors, Pinned, Projects, Recents |
| **Wiki** | the Wiki door in Home | Time (Today, Timeline, Days, Years, Chapters), Subjects, Drive |
| **Settings** | the Settings row at the sidebar's foot | Account, Server, Sources, Developer |
| **Setup** | a Setup door at the top of Home, only while a step is not done | the steps |

- **Stepping into a room** opens its page and swaps the sidebar to the
  room's list. Its head is a back arrow to Home and the room's name. The
  Wiki's and Settings' lists are those rooms' only navigation; their pages
  carry none of their own.
- **Home has no title.** It is the ground, and its doors say what it holds.
  Only a room you stepped into is named, behind its way back.
- **Applets, Pages, Tasks and the Wiki are doors in Home**, not places of
  their own in the chrome. A project is the room a chat lives in, so
  projects are listed in Home too.
- **Settings stands at the foot**, apart from the places above it: you do
  something to the server and leave.
- Routes did not move.

The Home panel, top to bottom:

1. **Doors**: Setup (while it is not done), New chat, Pages, Tasks,
   Applets, Wiki. The verb, then the places. Pages and Tasks carry a `+` on
   hover for a new one, the same shape as the Projects label. Search has no
   door: it is the ⌘K palette.
2. **Pinned.** A chat, a project, an applet or a page can be pinned.
3. **Projects.** Five, then "Show more"; the label opens the full list. A
   project cannot contain a project.
4. **Today, Recent**: the ten newest chats and pages, then "See all", which
   opens All chats.

Every group folds from its label, and the fold is remembered. A group with
nothing in it is not drawn: no empty-state line, because the doors above
already say how to make the first one.

Hovering a project row opens a hover card: the name, its counts, a pin
control, and two doors, "Open project" and "New chat here". Hovering a chat
row or a pinned row reveals inline controls (pin, more). Both need the
keyboard path and the touch fallback the interaction rules below require.

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
3. **Nothing in the chrome recedes.** A recessed tab strip was tried, on the
   reasoning that the browser tab model needs one — and it does, which is why
   the browser tab model was abandoned rather than the rule bent. Recessing the
   sidebar (200px wide, full height, lower half empty) turned the emptiness
   into a visible object; *the empty space was not noticeable while the sidebar
   matched the page*. Recessing only the strip left a dark band beside a light panel for no
   gain once tabs became pills. The sidebar sits on `--surface-elevated`, a 4%
   step, and that is the only shift in the shell.
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
| Chrome row | 40px (`--chrome-row-h`) | the window header, a room's title row in the sidebar |
| Control | 28px (`--chrome-tab-h`, `--sidebar-interactive-height`) | every clickable box in a row: tabs, the +, the header's controls, the ∴, sidebar rows |
| Nested control | 20px | a control inside a control: a tab's ×, a row's inline + and ⋯ |

**Everything in the window header shares one centre line**, 20px down the
40px row: the ∴, the controls, the tabs, the + and a view's actions. They
are all 28px controls, 6px from the row's top and bottom.

### Lines and glyphs

**Every icon renders at 1.25px of ink, at every size.** `AtlasIcon` derives
its stroke from its size (1.25 × 16 ÷ size), so a stroke is never typed by
hand. Three glyph sizes: 14px beside 12px text (a tab's icon and ×, inbox
rows), 16px in rows and the header, 20px on the rail.

The row slot, the 16px before a row's name, always holds a line glyph in
ink. A pin or a project tints its glyph with its own color. An emoji the
user picked stays, because it is their content.

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
on the desk, the same step that separates a card, and full ink. It sits
above its card, not joined to it.

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
| Box | 6px (`--card-radius`) | cards, controls, tabs, rows, popovers |
| Nested | 4px | a box inside a box, such as a tab's × |
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
| `--motion-enter` | 180ms | a popover, a menu or a new tab appearing |
| `--motion-layout` | 300ms | the sidebar opening, a pane resizing, the sidebar glyph's tint |

Square controls sink to 94% while pressed. Rows and tabs take the press fill
only, because a wide row that shrank would read as a wobble. The ∴ dot's
single swell when it lights is the one motion with a meaning of its own, and
it does not repeat.

### Alignment

**Each group in the window header starts at the column below it.** The ∴'s
glyph starts on the sidebar's icon line, level with the rows' icons, and the
window's controls follow it. In the Mac app the title bar is an overlay: the
traffic lights sit in the header row on its centre line, and the ∴ and the
controls follow them after 12pt, as in a native Mac app. The row's empty
space moves the window. Each pane's tabs start at that pane's edge.
With the sidebar closed the controls keep their place, and the left pane's
tabs start just after them. Inside a room, the panel head's back arrow sits
on the same icon line and the room's name on the rows' label line.

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
| Rail label | Avenir | 11px | 500 |
| Tab, group heading, meta | Avenir | 12px | 400 (headings muted) |
| Row (every panel) | Avenir | 13px | 400 |
| Your things (Pinned, Projects, Recents) | EB Garamond | 14px | 400 |
| Panel title | EB Garamond | 16px | 400 |
| Body / prose | EB Garamond | — | the page, not the chrome |

**Weight never signals state.** A selected row or the active tab changes
fill and ink, never weight: a bolder label is a wider one, and everything
beside it moves.

**In the chrome the serif is the voice of your things**: a room's title and
every titled row (a pin, a project, a chat, a page). Places and verbs (New
chat, Pages, Tasks) stay sans. So the sidebar changes texture once, where
the doors end and your things begin. The serif's smaller x-height is why its
rows are a step larger (14px against 13px), on the scale rather than at a
half pixel.

The ∴ is drawn, not typed, and it stands alone in the window header without
a wordmark.

**Labels are primary; icons support them.** Eight identical 16px icons at full
contrast in a column make a sidebar look like every other sidebar; the labels
should do the work, as they do in a table of contents.

---

## Interaction

- **Tabs are pills above the card.** Sized to their labels, so the gaps
  between them are even, with the + one gap after the last. The first tab
  starts on the card's left edge. The active tab takes the page's fill (see
  [Grays](#grays)); the others have none until hovered. In split view the
  active tab of the pane you are not working in has muted ink. Tabs joined
  to their card (a browser's model) were tried and taken out: on cards that
  float on a desk they made each card half a floating sheet and half the
  foot of a tab.
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
  by a pin control on the thing itself rather than a tab menu: what gets
  pinned is a chat, a project, an applet or a page, never a room, which is
  the different shape the rule asked for.
- **Recents as a sidebar-wide list.** Six of twelve rows were destinations
  already in the nav; the sidebar was the largest contributor to its own
  history list. Recents returned at the foot of the Home panel, where it
  lists chats only, so no row in it can be a room.
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
