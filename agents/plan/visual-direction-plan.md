# Visual direction plan

Status: **Open, decisions needed.** Nothing here is built.

On 2026-11-13 the owner locked a visual direction after a long run of design
sketches. The system and one clickable prototype are on the "Final" page of the
Glass explorations canvas in Claude Design
(https://claude.ai/artifact/ESUoWtiDTBP2mTTLTLtPrn). This plan moves that
prototype into the app. It lists what already exists, the decisions still
open, and an order of work.

## Who this is for

Every change below is checked against the five personas in
[build/personas.md](../build/personas.md).

- **Narrative seeker:** the notes line on home, serif replies, and pages that
  the chat keeps up to date.
- **Quantified self:** one numeric tile on home, charts and date cards in
  replies.
- **Tech prosumer:** the painting returning as ink in page headers, and a split
  view where the chat and the page work on each other.
- **Recovery:** nothing on home counts, ranks or warns. The notes line shows
  one item at a time and any item can be dismissed.
- **Data sovereign:** a status capsule that says when work is running and when
  work leaves the server.

## The locked system

- **Frame:** today's app layout. The ∴ button, back and forward, the tab
  group, a corner control at top right, the icon rail, and white sheets on the
  desk.
- **Color:** the Oxford theme tokens as they are in `themes.css`. Desk
  `#F4F4F4`, sheet `#FFFFFF`, hairline `--border` `#E1E1E1`, ink
  `--foreground`, navy `--primary` for actions and citation tags, claret
  `--secondary` used once per screen, and `--highlight` (the yellow
  highlighter) for a cited passage.
- **Type:** EB Garamond (`--font-serif`) for titles, replies and page text.
  Avenir for the interface. Small spaced capitals only for running labels. The
  serif is never bold or italic.
- **Surfaces:** content sits on solid sheets with a hairline, on a plain
  desk. Glass (`--glass-bg`, `--glass-blur`) is only for controls that
  float over the desk or over content. No glass or color behind reading
  text, no saturation boost, shallow shadows. The panes sit inset on the
  desk as one rounded sheet, 10px from the window's right and bottom edges
  (decided 2026-11-15). Split view divides the sheet with one line.
- **The image:** one scene. In full color it is home's place. On pages,
  including a page in the split view, it appears as a one-ink print of the
  same scene in the page's header, never behind reading text. Chat has no
  print (decided 2026-11-13): the newest reply lands at the message box, so
  a print there sits under text. The color returns when you go home.
- **Kept behavior:** the notes line under the home headline (replacing
  suggested prompts), the status capsule, citations that open the page beside
  the chat and mark the passage, Add to page, and the chat editing the open
  page with undo.

## What exists today

Checked against the code on 2026-11-13.

| Prototype piece | Today |
|---|---|
| Split view | `window-shell.svelte.ts` holds two panes per window; `SplitContainer.svelte` draws them. |
| Sidebar lists | `HomePanel.svelte` has the doors, Pinned and Recent, with drag and keyboard order. |
| Theme and serif | Oxford is the default theme; `--font-serif` is EB Garamond. |
| Citations | Citations are refs (see the refs work in `lib/refs/`). |
| A home producer | The hourly `suggested_questions` applet writes one question for the empty composer. |
| Notifications | No notification object exists. A design for one was archived on 2026-11-13 ([archive/notifications-plan.md](../archive/notifications-plan.md)). |

Most of the shell files this plan touches are being edited by other agents as
of 2026-11-13: `UnifiedSidebar.svelte`, `HomePanel.svelte`,
`WindowHeader.svelte`, `WindowTabBar.svelte`, `SplitContainer.svelte` and
`ChatView.svelte`. Work here waits for those changes to land or is agreed with
whoever holds them.

## Open decisions

Each has a recommended pick. None is decided until the owner says so.

### Structure

1. **Rail or full sidebar by default.** Decided 2026-11-13: closing the
   sidebar narrows it to its glyphs. It is the same `HomePanel` rows: the
   doors, each pin's own icon or emoji in its color, and Settings stay where
   they are, and the words fade. It opens only by click, with the header
   toggle (⌘B). No hover open and no edge peek.
2. **The top-right corner.** Decided 2026-11-13: the status capsule sits
   there and is the one place for your server's state. It shows offline,
   a reply being written (off the server), something that needs you (the
   dot takes the accent), or a quiet dot, and clicking it opens the
   "Needs you" list the ∴ used to open. The ∴ is now the sidebar toggle; the
   separate toggle is gone. On hover its three dots line up as a
   column (the list) and a hairline draws down beside them (the panel's
   edge), with no dot crossing another. While the sidebar is shown, the
   space between them tints. Open:
   Standard moves into the message box chip row.
3. **A split in the tab bar.** Pick: keep window-shell's two panes in one
   window and show both pane tabs as active. No new tab type. Each page keeps
   its bar under its tab, joined to it (decided 2026-11-15, after a one-row
   header was tried and turned down). A chat's bar names the project it is
   filed in, or nothing, since its tab already says the chat's name (decided
   2026-11-16).
4. **What home is.** Pick: the empty state of a new tab, as New chat is today.

### Home

5. **What feeds the notes line.** Replies, broken syncs, insights, new applets.
   There is no notification object to read from. Pick: one small table of
   home notes that a few producers write (the suggestions applet, sync health,
   new replies), with dismiss and act. One item shows at a time. Suggested
   prompts in the composer are removed.
6. **Home tiles.** Pick: three fixed tiles for v1: the first pin, Continue,
   and one numeric tile. No editing in v1. Each tile says what is in it: a
   line from the page or the last reply, and for sleep the night's length
   against the rest of the week (built 2026-11-16).
7. **The headline.** Pick: a static "Friday afternoon", as in the v3 sketch.
### The image

8. **Where paintings come from.** Pick: a bundled set of about eight for v1,
   each with an ink print made at build time. Painting a scene for each day
   with the image slot can come later.
9. **Which pages get an ink header.** Pick: projects and day pages opt in.
    Other pages keep a plain header.
10. **Themes.** There are sixteen. Pick: the image system works under any
    theme, and the ink print is recolored to the theme's `--foreground`.
11. **Dark.** Pick: derive an Oxford night from the token roles. Open: pair it
    with an existing dark theme or add one.

### Chat and page

12. **How precisely citations point.** Marking a passage needs anchors in the
    page document (Yjs and Tiptap, see
    [document-contract-plan.md](document-contract-plan.md)). Pick: block-level
    anchors first, marking the whole block; sentence-level later.
13. **Add to page.** Pick: append a block under the page's Notes heading, or at
    the end when there is none, with a link back to the chat and undo.
14. **The chat editing pages.** Pick: edits to the page open beside the chat
    apply with a visible change and undo; edits to any other page ask first.

### Status

15. **What drives the status capsule.** Today it reads the two sources the
    app has: chat activity (replies running here or on the box, always
    written off the server) and reachability, and it replaces the sidebar's
    offline pill. Pick for later: add activity and telemetry events,
    filtered to work the person would care about now. A model call that
    leaves the server always shows.

### Type

16. **Serif replies.** Pick: EB Garamond at 19.5px for assistant replies,
    through the shared stylesheet in
    [prose-consolidation-plan.md](prose-consolidation-plan.md).

### Mobile

17. **Not designed.** The phone layout needs its own sketch round before any
    slice that changes the shell.

## Order of work

Each slice replaces the old way in the same change. Feature flags only for a
slice in progress, removed when it lands.

1. **Tokens:** hairline sheets, the highlighter for cited passages, claret
   once. No layout change.
2. **Shell:** the click-only rail with pin icons; the status
   capsule in the corner; Standard into the composer.
3. **Home:** the headline, the notes line on its own small table, three
   tiles, the painting.
4. **Image system:** the bundled paintings, build-time ink prints, the color to
   ink change, opt-in page headers.
5. **Chat and page:** block anchors, citations that open the page beside and
   mark the block, Add to page, page edits with undo.
6. **Dark:** Oxford night from the token roles.
7. **Mobile:** after its own design round.

## When this plan is done

Delete it when the slices ship. What stays is a record in `agents/record/`
and the manual pages for anything a person runs.
