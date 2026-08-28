# Design constraints

The shell's job is to **earn trust through calm**. This is an appliance holding
one person's mail, money, health and days. Twitchy accents, cramped boxes and
near-miss alignment don't just look unpolished — they read as *unfinished*, and
unfinished is not what anyone wants holding their entire life.

This file exists because the same mistakes kept recurring across sessions.
Written-down constraints are the only thing that stops a generative process
producing the statistical average of every SaaS dashboard.

---

## The organising idea

**The sidebar is the desk. The pane is the page.**

- The desk is quiet. Chrome on it whispers: no borders, no fills, no badges,
  and no colour of its own. Type and spacing do the work.
- The page is crisp. Borders, fills and structure are allowed here, because
  this is where the work is.
- **Nothing on the desk may out-shout the page.** That one rule kills most of
  what has gone wrong: bordered search fields, saturated modifier badges,
  coloured strips on pinned rows.

---

## Anti-slop rules

Every one of these was violated in this codebase and had to be undone. They are
listed as prohibitions because that is how they failed.

1. **No coloured left-edge strips.** A 2–4px accent bar on the leading edge of a
   row is among the most reliable tells of machine-made UI. It was tried on
   pinned rows; at 2px on a 32px row, three pixels from a 16px icon, colour
   isn't identity, it's lint.
2. **Separation goes whitespace → 3–5% lightness → elevation, and you stop at
   the first one that reads.** Do not reach for a 1px rule. There are no
   hairline separators in the sidebar.
3. **Nothing in the chrome recedes.** A recessed tab strip was tried, on the
   reasoning that the browser tab model needs one — and it does, which is why
   the browser tab model was abandoned rather than the rule bent. Recessing the
   sidebar (200px wide, full height, lower half empty) turned the emptiness
   into a visible object; *the void was invisible while the sidebar matched the
   page*. Recessing only the strip left a dark band beside a light panel for no
   gain once tabs became pills. The sidebar sits on `--surface-elevated`, a 4%
   step, and that is the only shift in the shell.
4. **8pt grid. No off-grid values.** No `12.5px`, `13px`, `26px`, `30px`,
   `34px`. Radii are the one exception (6px, matching Tailwind).
5. **No monospace as decoration.** IBM Plex Mono is for code and data. A
   keyboard hint is not code. A shortcut label is not code.
6. **No uppercase + serif + wide tracking.** It signals "considered" while doing
   no work. It appeared on a *back button* at 11px/0.14em, which is the
   opposite of what an exit should be.
7. **One left edge.** Every icon in the sidebar starts at the same x; every
   label starts at the same x. Near-misses (12 / 16 / 20px) read as sloppiness
   the eye can feel but not name.
8. **Non-semantic colour is prohibited.** Colour means something or it isn't
   used. A merely-compacted tab does not get the theme accent.

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
#F5F5F5 against a #FFFFFF parent — a 4% shift that reads as nothing happening.
Hovers, selections and active states come from the foreground-mix ramp, which is
defined against the text colour and therefore legible in all sixteen themes.
This bug shipped twice: dead hovers across 39 rules, and an active tab rendered
`#FFFFFF` on `#FFFFFF`.

---

## Type

| | face | size | weight |
|---|---|---|---|
| Mark (`∴ virtues`) | JJannon | 13px (nav size) | regular, lowercase |
| Destinations | Avenir | 13px | 500 |
| Children | Avenir | 12px | 400, muted |
| Body / prose | JJannon | — | the page, not the chrome |

**The serif appears in the chrome exactly once**, in the mark. Serif navigation
was considered and declined; concentrating the typographic identity in one
deliberate place is more disciplined than spreading it across eight rows.

Lowercase `virtues`, not `Virtues` — a capital V is too much weight for
something meant to sit quietly. And at nav size, not larger: a wordmark bigger
than everything around it is a logo demanding attention, and this one has no
job beyond saying whose desk this is. The serif and the ∴ carry the identity;
scale would only make it loud.

**Icons assist, labels lead.** Eight identical 16px icons at full contrast in a
column is what makes a sidebar look like every other sidebar; the words should
carry a contents page.

---

## Interaction

- **Tabs are pills.** Centred in the strip, rounded on all four corners, filled
  from the interaction ramp when active. Two alternatives were built and
  rejected: the full-height container and the bottom-anchored browser tab both
  require a recessed strip, and a recessed strip is not worth what it costs the
  rest of the window.
- **A row that names a destination navigates to it.** The chevron expands.
  Two hit targets. A row that only toggles means "Notebooks" cannot take you to
  Notebooks.
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

- **Pins / favourites.** Two visual treatments were tried (plain rows, then
  coloured ribbons) and neither solved the actual problem: a pinned "Pages" and
  a nav "Pages" render identically, so the section read as a duplicate of the
  nav directly beneath it. The `app_pins` table, its API and the reorder
  endpoint all remain — the UI is gone, not the data. If it returns it should
  be a different SHAPE from a nav row, not a tinted one.
- **Recents.** Six of twelve rows were destinations already in the nav; the
  sidebar was the largest contributor to its own history list.
- **Today.** `/home` is the live view of today; `/day` only exists after the
  nightly run.

## Naming

Check whether a word is already taken before reusing it. Three live examples:

- **`data_content_bookmark`** owns "bookmark" for *ingested* saved links (GitHub
  stars, browser bookmarks). The sidebar's kept routes stayed "Pinned" for this
  reason.
- **`app_notebook_items.role = 'pin'`** is retrieval scope, unrelated to sidebar
  pins.
- **`tab.pinned`** is tab *compaction*, unrelated to both.
