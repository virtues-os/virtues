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

A rail of rooms, and one panel beside it that the selected room fills. The
rail stays fixed; the panel's contents change with the selected room. Defined in
`apps/web/src/lib/sidebar/rooms.ts`.

The rail, top to bottom, in three gap-separated groups:

| Group | Room | Route |
|---|---|---|
| ground | **Home** | `/chat` |
| library | **Wiki** | `/wiki` |
| library | **Drive** | `/storage` |
| utility | **Sources** | `/sources` |
| utility | **Developer** | `/virtues/developer/sql` |
| utility | **Settings** | `/virtues/you` |

- **Home is the only room in the `ground` group.** It sits alone above the first gap and at the top
  of the rail. It was called Chats until its panel held pages, projects and
  applets too; a user reads the panel's title as the name of the place, so
  the room is named for the place, not one of its contents. Its tile opens
  a new chat; there is no separate home page.
- **Drive, not Files.** The room kept its glyph and its `/storage` route; the
  label names the place rather than its contents.
- **Applets is not a room, and neither is Pages.** Each is a door at the top
  of the Home panel; `/applets` and `/page` are unchanged. Projects likewise:
  a project is the room a chat lives in, so projects are listed in the Home
  panel rather than behind a rail door of their own.
- Routes did not move. Labels and grouping did.

The Home panel, top to bottom:

1. **Doors**: New chat, Pages, Applets, Search. The verb, the places, and
   last the door that leaves the panel: Search opens the ⌘K palette. Pages opens the list and carries a `+` on
   hover for a new page, the same shape as the Projects label.
2. **Pinned.** A chat, a project, an applet or a page can be pinned.
3. **Projects.** Five, then "Show more"; the label opens the full list. A
   project cannot contain a project.
4. **Today, Yesterday, Earlier**: the recent chats, grouped by when.

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
4. **8pt grid. No off-grid values.** No `12.5px`, `13px`, `26px`, `30px`,
   `34px`. Radii are the one exception (6px, matching Tailwind).
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

## Tokens

Defined in `apps/web/src/themes.css`. Sixteen themes; every value below is set
per theme where it varies.

| Token | Value | Notes |
|---|---|---|
| `--chrome-row-h` | 40px | Sidebar masthead AND pane toolbar. One number. |
| tab height | 28px | Centred pill inside the 40px strip. |
| `--pane-inset` | 12px | The pane card's margin. The sidebar matches it. |
| `--sidebar-interactive-height` | 32px | Destinations. |
| `--sidebar-child-height` | 24px | Expanded children. |
| `--sidebar-padding-left-base` | 12px | Row inset. `.workspace-nav` adds 8px. |
| `--sidebar-icon-opacity` | 0.5 | 0.4 for children, 1.0 on hover/active. |
| `--hover-bg` / `--active-bg` | fg mix 7% / 12% | The interaction ramp. |

**Never use a surface token for an interaction state.** `--surface-elevated` is
#F5F5F5 against a #FFFFFF parent — a 4% shift that is too small to see.
Hovers, selections and active states come from the foreground-mix ramp, which is
defined against the text colour and therefore legible in all sixteen themes.
This bug shipped twice: dead hovers across 39 rules, and an active tab rendered
`#FFFFFF` on `#FFFFFF`.

---

## Type

| | face | size | weight |
|---|---|---|---|
| Mark (`∴ virtues`) | EB Garamond | 13px (nav size) | regular, lowercase |
| Destinations | Avenir | 13px | 500 |
| Children | Avenir | 12px | 400, muted |
| Body / prose | EB Garamond | — | the page, not the chrome |

**The serif appears in the chrome exactly once**, in the mark. Serif navigation
was considered and rejected; the serif is used in one place instead of across
eight rows.

Lowercase `virtues`, not `Virtues` — a capital V adds too much weight to
something meant to sit quietly. And at nav size, not larger: a wordmark bigger
than everything around it draws attention, and its only job is to identify the
app. The serif and the ∴ identify it; a larger size would only make it louder.

**Labels are primary; icons support them.** Eight identical 16px icons at full
contrast in a column make a sidebar look like every other sidebar; the labels
should do the work, as they do in a table of contents.

---

## Interaction

- **Tabs are pills.** Centred in the strip, rounded on all four corners, filled
  from the interaction ramp when active. Two alternatives were built and
  rejected: the full-height container and the bottom-anchored browser tab both
  require a recessed strip, and a recessed strip is not worth what it costs the
  rest of the window.
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
