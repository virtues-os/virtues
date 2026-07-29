# Mobile (iOS) UX plan — pragmatic next steps

Status: **planned** (2026-07-13, from the full mobile sweep). Decisions baked in:
**keep** the desktop tab strip on phone; **block** split view on phone; keyboard
work gets a verification spike before real implementation.

Shipped already (same day): edge-to-edge + safe-area theming
(`viewport-fit=cover`), themed root background, status-bar text follows theme
darkness via the `set_appearance` bridge (`src-tauri/src/lib.rs`).

Context: the mobile shell (tab bar / More sheet / device screen / onboarding)
is purpose-built and good. Inside it, content views are desktop components at
390px. The work below fixes the highest-pain gaps without re-architecting.

---

## Phase 1 — Keyboard (spike first, then implement)

The single worst issue: the chat composer is bottom-docked inside a
`position:fixed` shell, and iOS overlays the keyboard on top of it
(`ChatView.svelte` `.chat-input-wrapper`). No `visualViewport` handling exists
anywhere.

**Verified facts (don't re-litigate):**
- WKWebView does NOT shrink `window.innerHeight` on keyboard open; the keyboard
  just covers the bottom.
- `visualViewport` is NOT reliable inside Tauri's WKWebView — open bug
  (tauri#10631) where it fails to account for keyboard height. Safari-blog
  advice does not transfer.
- iOS focuses-input → WKWebView auto-scrolls the WHOLE webview (tauri#9368,
  #9907), dragging fixed chrome offscreen. Community fix:
  `scrollView.contentInsetAdjustmentBehavior = .never`
  (`tauri-plugin-ios-webview-insets` packages exactly this).
- iOS 26 quirk: `visualViewport` height/offsetTop can fail to revert after
  keyboard dismissal — belt-and-suspenders reset needed on `keyboardWillHide`.

**1a. Spike (on-device, ~half a day):** bundled test page logging
`innerHeight` / `visualViewport.height+offsetTop` / focus-scroll behavior on
the real iPhone across focus/dismiss/rotate. Establishes which signals are
trustworthy in OUR wry version before any code.

**1b. Implementation (shape, adjust to spike):**
- Native keyboard bridge = source of truth: observe
  `keyboardWillChangeFrame` notifications, emit keyboard height to JS
  (Tauri event). Natural home: a tiny addition alongside the `set_appearance`
  bridge or a micro-plugin.
- JS sets `--keyboard-inset` on `:root`; the chat composer (and the shell's
  bottom reserve) consume it: `bottom: max(var(--keyboard-inset), env(safe-area-inset-bottom))`.
  Tab bar hides (or sits under) while the keyboard is up.
- Adopt the `contentInsetAdjustmentBehavior = .never` fix so iOS stops
  auto-scrolling the webview on focus.
- `visualViewport` listener only as a cross-check + dismissal-reset fallback.
- Test matrix: chat composer, page title textarea, CodeMirror body, slash
  menu / @-picker near the keyboard, external keyboard attached (inset 0).

## Phase 2 — Touch reachability (long-press → existing context menus)

Row actions (rename/delete/open…) live only in `oncontextmenu` menus
(PagesView, DriveView, NotebookDetailView) — unreachable on touch.
- Wire long-press (pointerdown + ~450ms, cancel on move/scroll) in ONE place —
  `ContextMenuProvider` — dispatching the same menu open. All three views
  inherit it.
- `-webkit-touch-callout: none` + `user-select: none` on rows that own a menu
  so the OS callout doesn't fight ours.
- Menus themselves: clamp to safe-area (see Phase 3), min 44px rows on touch.

## Phase 3 — Overlay + safe-area quick wins (small, visible)

- Toasts: all three `<Toaster position="top-center">` mounts need a top offset
  of `max(12px, env(safe-area-inset-top))` — currently clipped by the Dynamic
  Island.
- Clamp/inset pass over: CitationPanel (top+bottom), ContextMenu max-height,
  Modal/SearchModal bottom, focus-exit button top, SelectionPopover/RefPreview
  viewport clamping.
- Block split on phone: guard `enableSplit()` + the `?right=` deep-link branch
  in `window-shell.svelte.ts` (`handleDeepLink`) behind `!mobileLayout.isMobile`
  — a `?right=` deep link on phone opens the right route as a normal tab
  instead. (Tab strip STAYS — decided; later polish: bigger close targets.)

## Phase 4 — Lists: one grid decision fixes every list view

`UniversalDataGrid` already has card mode + `hideOnMobile`. Two changes inside
the grid, inherited by Pages/Notebooks/History/Drive/Person/Actions-history:
- Phone default = card view (table remains a user choice).
- `.table-view` gets an `overflow-x: auto` wrapper (today overflow is CLIPPED
  by the fixed shell — unreadable, unscrollable).
- While there: bump compact 11px text to ≥12px on touch, row tap targets ≥40px.

## Phase 5 — Touch-feel batch (mechanical, one sweep)

- `@media (hover: hover)` guard for sticky hover states (or a `.no-hover`
  root class from the shell; ~100 files use `:hover`, pick the cheap route).
- `-webkit-tap-highlight-color: transparent` globally (currently tab bar only).
- `overscroll-behavior: contain` on inner scrollers (chat list, sidebar lists,
  modal bodies, menus).
- `touch-action` on drag surfaces (grid resize, chip drag, split divider —
  though split is now blocked on phone).
- Composer: on mobile, Enter = newline, send button sends (soft keyboards have
  no Shift+Enter). Desktop unchanged.

## Phase 6 — Back model (design once, small build)

The tab router already keeps history; iOS users get nothing. Minimal viable:
edge-swipe / back affordance pops `windowShellStore` tab history (popstate
integration already exists — surface it). Explicitly NOT a full nav-stack
rewrite. Design question: does back cross tab-bar sections or stay within one?

## Phase 7 — Pages editor mobile posture (design decision, then build)

Decide the phone posture: reading + light edits, not full authoring.
- Topbar: collapse the ~8 popover buttons behind one overflow on phone; TOC
  keeps the drawer (matches the Library-site direction in the pages roadmap).
- References rail (fixed 300px, sits BESIDE the editor): bottom-sheet or
  full-width panel on phone instead.
- Slash menu / @-picker (280–300px caret popovers): clamp to viewport,
  keyboard-aware (needs Phase 1's `--keyboard-inset`).
- Selection toolbar vs native iOS callout: pick one (likely defer to native on
  touch).
Slot this into the existing pages-editor roadmap rather than a separate track.

---

## Loose ends (each needs a one-line decision, not a project)

1. **Unpair/sign-out routing**: More-sheet sign-out does `goto("/pair")`
   (SvelteKit route); the native connect shell is `mobile-pair.html`, chosen
   only at app launch (`lib.rs`). After unpair on the phone, the app should
   relaunch into the native pair shell — verify `location.reload()` actually
   lands there when unpaired.
2. **Onboarding re-open is a dead affordance**: `openOnboarding()` has no
   caller though comments promise re-open from This Device. Add a "Set up
   collectors" row there, or delete the claim.
3. **Naming**: one surface is called "Settings" (file docs), "More" (tab), and
   "You" (spec). Pick one ("More" is shipped) and align comments/spec.
4. **Pinch-zoom is disabled** (`maximum-scale=1`) and 15px inputs depend on it
   (auto-zoom suppression). Conscious accessibility sign-off required — if
   zoom ever returns, inputs must go to 16px first.
5. **iPad = phone UI** (`__VIRTUES_MOBILE__` injected for all iOS, no idiom
   check). Explicitly deferred; note that desktop layout (sidebar + split) is
   arguably the better iPad starting point when we get there.
6. **Hardcoded theme colors** (violates the no-hardcoded-colors rule):
   AppletDetailView amber/red banners, InlineCitation red/green, TrashView
   emerald button. Batch into any Phase 3/5 pass.
7. **isMobile is sticky** (module-load evaluation; no reaction to
   resize/rotation in shell, `onboardingOpen` computed once pre-pairing).
   Harmless today; will matter for iPad/landscape work.
8. **Focus mode on mobile**: `body.focus-mode` is reachable globally but its
   exit button isn't inset-safe and the feature is keyboard-first — either
   inset-fix or suppress on phone.

## Suggested order & sizing

| Phase | Size | Depends on |
|---|---|---|
| 1a keyboard spike | ~½ day | device on hand |
| 3 overlays + split-block | small | — |
| 2 long-press | small-med | — |
| 1b keyboard impl | medium | 1a |
| 4 grid defaults | small-med | — |
| 5 touch-feel batch | medium (mechanical) | — |
| 6 back model | medium | design call |
| 7 editor posture | large (design+build) | 1b, pages roadmap |

3 and 2 are good "while the spike bakes" work. Everything through Phase 5 is
codifiable without new design; 6 and 7 need a decision first.
