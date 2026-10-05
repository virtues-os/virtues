# Mobile (iOS) UX — what is left

The phone shell is the chat-first drawer (`lib/components/mobile/MobileShell.svelte`,
`MobileDrawer.svelte`): no tab strip, no split, no back stack; every surface is
a root one drawer away. Built and not repeated here: the keyboard bridge and
`--keyboard-inset` (8098c909, bb4a1395), long-press opening the same context
menus (`ContextMenuProvider`, with the OS callout suppressed in `app.css`),
safe-area offsets on every `<Toaster>`, split view blocked on phone
(`window-shell.svelte.ts`), phone lists defaulting to cards with a scrolling
table (`UniversalDataGrid`), Return as newline on phone
(`lib/codemirror/composer.ts`), and an inset-safe focus-mode exit.

## Open

- **Touch-feel batch** (mechanical, one sweep):
  - No `@media (hover: hover)` guard anywhere; ~116 files use `:hover`, so
    hover states stick after a tap. Guard them, or add a `.no-hover` root
    class from the shell, whichever is cheaper.
  - `-webkit-tap-highlight-color: transparent` is set per component (four),
    not globally.
  - `overscroll-behavior: contain` on inner scrollers (chat list, drawer
    lists, modal bodies, menus); today only the drawer and a few canvases
    have it.
  - `touch-action` on the remaining drag surfaces (grid column resize, chip
    drag).
- **Overlay clamps, unverified on phone:** ContextMenu max-height,
  Modal/SearchModal bottom inset, SelectionPopover and RefPreview viewport
  clamping. `CitationPanel` already pads both safe areas.
- **Pages editor posture on phone** (design decision, then build): reading and
  light edits, not full authoring. Collapse the topbar popovers behind one
  overflow; the references rail becomes a bottom sheet; the slash menu and
  @-picker clamp to the viewport above `--keyboard-inset`; pick the selection
  toolbar or the native callout on touch. Slot it into the pages-editor
  roadmap rather than a separate track.

## Decisions still owed

- **Pinch-zoom is disabled** on mobile (`hooks.client.ts` sets
  `maximum-scale=1, user-scalable=no`), and 15px inputs rely on that to avoid
  iOS auto-zoom. Needs a conscious accessibility sign-off; if zoom returns,
  inputs go to 16px first.
- **iPad gets the phone UI** (`__VIRTUES_MOBILE__` is injected for every iOS
  device, no idiom check). Deferred; the desktop layout (sidebar, split) is
  probably the better iPad starting point, and `mobileLayout.isMobile` is
  fixed for the life of a native shell, so rotation and window size would
  need to feed it.
