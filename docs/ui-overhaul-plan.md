# UI Overhaul Plan

Nineteen layout/settings/design items, triaged against the codebase. Dated
2026-07-27. Items are numbered as originally raised (10, 11, 13 were never
sent).

Verdicts: **ship** (agreed, planned), **spike** (needs a proof before
committing), **defer** (agreed in principle, not now), **drop**.

---

## Findings that change the shape of the work

Five things the code says that the item list assumed otherwise:

1. **Bookmarks are already built.** `app_pins` (url/label/icon/sort_order),
   full CRUD + `PUT /api/pins/reorder` in `virtues-core/src/api/pins.rs`,
   rendered by `sidebar/PinnedSection.svelte`. Item 8's bookmark half is
   placement and display work, not a feature build. Recents is the real work.

2. **⌘K has no backend at all.** `sidebar/SearchModal.svelte` makes zero
   network calls — it filters the `chatSessions` / `pages` / `notebooks`
   stores client-side. And there is no local-search HTTP route: `/api/search/web`
   is Exa (external). The hybrid engine is only reachable as an agent tool
   (`tools/semantic_search.rs`). It *is* already constructed on server state
   (`server/mod.rs:77`), so this is route wiring, not IR work.

3. **Two updaters, opposite doctrines.** The Mac app self-updates every 6h off
   `mac-latest`. The box does not auto-update by explicit design
   (`cli/upgrade.rs:10`) and requires root (`cli/upgrade.rs:84`). Neither
   persists a channel.

4. **CI deliberately blocks edge auto-update.** `release-mac.yml:251` gates
   `latest.json` generation on `IS_EDGE == 'false'` — "Edge never produces
   these, so the updater can't pull a rolling test build." Mac edge updates
   require reversing this.

5. **125 unique theme tokens across 36 `data-theme` blocks.** Larger than it
   looks; token collapse is justified. Do it *after* the theme count drops so
   the audit covers 16 themes, not 17.

---

## Batch A — surface fixes

Small, self-contained, individually shippable. No design decisions outstanding.

### 6 — Sidebar hover CLS · ship

`sidebar/UnifiedSidebar.svelte:225-248`: `.sidebar-collapsed` sets
`overflow: visible` so its hover zone extends past its zero width, and
`.sidebar-collapsed:hover` expands. That zone sits at the same x as the
toolbar's `.sidebar-toggle`, so travelling to the button trips the peek first.

Narrow the hover zone; add ~150ms intent delay before expanding; suppress the
peek while the pointer is over the toggle button.

### 17 — Toolbar icons have no hover state · ship

`tabs/WindowTabBar.svelte`: `.sidebar-toggle`, `.split-toggle`,
`.merge-toggle`, `.nav-btn`, `.new-tab-btn` share a rule block with no
`:hover`. Consolidate into one `.toolbar-btn` class carrying
hover / active / disabled / focus-visible. Ship with item 16 — same file.

### 16a — Tab close button · ship

`tabs/WindowTabBar.svelte:645-678`. Currently 16px, `opacity: 0` until hover,
and on hover turns **red** (`--error-subtle` bg, `--error` fg) plus tints the
whole tab red. Red should mean destructive-with-consequence; closing a tab is
neither, and it's the source of the "weird colour".

Neutral hover (foreground ~8%), hit target to 18-20px, keep reveal-on-hover.

### 15 — Toast position · ship

`routes/(app)/+layout.svelte:270` is `position="top-center"`. Move to
bottom-right on desktop; **keep top-center on mobile** — platform convention,
and the notch safe-area offset is already wired there.

Check for collision with the chat input; may need a conditional offset when a
chat pane holds focus.

### 14 — Collapse `Page.maxWidth` · ship

`components/Page.svelte` offers five. Actual usage: `prose` ×6, `wide` ×6,
`full` ×2, `narrow` ×1 — two values already cover 12 of 15 call sites.

Collapse to **prose** (`max-w-3xl`) and **wide** (`max-w-6xl`). Map
narrow→prose, full→wide.

**Do not touch Pages' own width.** `stores/pageDisplay.svelte.ts`
(`WidthMode: small|medium|full`) is a *user reading preference*;
`Page.maxWidth` is a *chrome constant chosen by the developer*. Different
things. Merging them takes a setting away from the user.

### 7 — Sidebar waterfall · ship

Already half-built: `WorkspaceHeader.svelte` has `animationDelay`,
`--stagger-delay`, `.animate-row`. Extend the stagger to the remaining
sections.

Two constraints: fire only on first mount and on expand (not on every reactive
update), and respect `prefers-reduced-motion`.

### 3 — Boot animation · ship, folded into 7

No splash screen — a splash is added latency wearing a costume. The ∴ resolves
in at the masthead, the sidebar waterfalls from it, pane skeletons fill. The
animation *is* the app assembling.

The one real fix here is the Tauri cold-start white flash: put a themed
background colour on the HTML shell. That is not an animation task.

### 2 — ⌘+/− text size · ship (mostly by doing nothing)

On desktop, native webview zoom already does this correctly for ⌘+ / ⌘− / ⌘0,
and it scales everything. In the browser, the browser owns it.

So the actual work is **making the layout survive zoom** — fixed-px heights in
the tab bar and sidebar will break first. Audit and relativise those.

Build a custom `--app-zoom` rem scale *only* if the size should persist
per-account and sync across devices. That is a product decision, not a
shortcut binding, and it is not currently justified.

---

## Batch B — window chrome

Shares one thesis: **every pane toolbar is breadcrumb/title left, view actions
right.** Items 4, 5 and 16b are facets of that.

### 4 — Sidebar masthead · ship

`sidebar/WorkspaceHeader.svelte` today is one full-width button: ∴ left, a
`⌘K` kbd chip right. It reads as chrome rather than as an item — it's a label
pretending to be a control, and it's the only row in the sidebar carrying a
keyboard hint.

Three affordances, one row, height aligned to the pane toolbar so the left and
right columns share a baseline:

- **Left:** ∴ = Home. Then **remove Home from the nav list below** — it's
  redundant, and it gives the mark a job.
- **Right:** two icon buttons — search (⌘K) and new chat.
- **Drop the kbd chip.** It moves into the tooltip.

Bookmarks do not go here; they're item 8's section. Three controls is the
ceiling before this becomes a toolbar.

### 5 — Global action bar · ship, as a slot contract

Not a new bar. Today, view actions live in three unrelated places: the
`actions` snippet on `Page.svelte`→`PageHeading`, the datagrid's own toolbar,
and Pages' Aa popover + bottom bar.

Define one slot: views publish their actions into the pane toolbar via a
store, so actions always appear in the same physical place. Pages then stops
being special — it just fills the slot.

**Open:** the toolbar's right side already holds split/merge/new-tab. Needs an
overflow rule for narrow split panes before implementation.

### 4b — Settings as a sidebar mode · ship

**Decided.** Entering settings is a *sidebar mode*, not a pane takeover. The
regular sidebar items translate out to the left as the settings items come in;
you leave settings by exiting it in the sidebar, not by closing a tab.

This dissolves the split-pane conflict that made the takeover option
attractive: the mode belongs to the sidebar and is entered and left
deliberately, so it is never derived from which tab happens to hold focus. Two
panes can show whatever they want while the sidebar is in settings mode.

Consequences:

- `SettingsView.svelte`'s horizontal `SubNav` (a primary row, plus a second
  underline row for Developer) is replaced by sidebar rows. Developer's
  sub-nav becomes nesting rather than a second underline — which is the actual
  problem being solved; two stacked underline rows is the smell that says the
  nav outgrew its container.
- The mode needs an explicit exit row at the top of the settings sidebar
  (`← Virtues` or similar), since there's no other way out.
- Reuse item 7's stagger for the transition — same easing, opposite direction.
- The pane toolbar gains the breadcrumb (item 5's left slot).

**Open:** when a settings section is clicked, does it replace the focused
pane's tab, open a dedicated settings tab, or reuse one pinned settings tab?
And does the sidebar stay in settings mode if the user switches to an
unrelated tab? Recommend: one reused settings tab, and the mode persists until
explicitly exited — that's what "you have to exit settings in the sidebar"
implies.

### 16b — Full-height tabs · ship

Tabs become full-height rather than floating pills. They read as containers,
and the toolbar gains a single baseline. Depends on 16a and 17 landing first.

### 16c — Dia-style swoop · spike

Masked corners interacting with split panes, where each pane has its own tab
bar and can get very narrow. Not a CSS afternoon. Prove it at minimum pane
width before committing.

### 12 — ⌘1/⌘2 pane focus · ship

**Decided: ⌘1/⌘2 = pane focus.** There are only ever two panes (left/right in
`tabs/SplitContainer.svelte`), so that's the complete set; ⌘3-9 stay free.

- Tab cycling moves to `⌘⇧[` / `⌘⇧]` — browser convention, no collision.
- **When not split, ⌘2 creates the split and focuses it.** The shortcut
  teaches the feature.
- Hold-⌘ reveals pane labels with a flip-up animation, gated on a ~400ms hold
  so it doesn't flash on every ⌘S.

Depends on item 9's registry landing first.

---

## Batch C — infrastructure

Strict order: 9 → 19 → 18. Item 18 is meaningless before 19.

### 9 — Shortcut registry · ship

~40 files carry `keydown` handlers; every global is inline in
`sidebar/UnifiedSidebar.svelte:43-70` (⌘⇧T, ⌘⇧N, ⌘N, ⌘S, ⌘K, ⌘W).

Extract a registry: declarative bindings, one listener, scope-aware
(modal/editor/global). Unlocks a shortcuts cheat-sheet and user rebinding for
free, and is a hard prerequisite for 12 and 18.

**OS-global hotkey — scope down.** Double-tap-⌘ is not registerable as a
normal global hotkey; Raycast and Spotlight implement it with an
accessibility-level event tap. That means prompting for Accessibility
permission on an appliance holding someone's entire life — a bad trade, and
⌘⌘ is frequently already taken by Raycast on the same machines.

Ship a real chord (`⌥Space` or `⌘⇧Space`) via the Tauri global-shortcut
plugin, rebindable from the registry. Revisit the event tap only on demand.

### 19 — ⌘K on the IR stack · ship

This is a build, not a hookup (see finding 2).

1. **New HTTP route** exposing the existing `SemanticSearchEngine` from
   `server/mod.rs:77`. Name it distinctly — `/api/search/local` or similar —
   `/api/search/web` is already Exa.
2. **Latency budget, decided now: no reranker in the keystroke path.** Palette
   queries use BM25 + dense fusion only, target <50ms. Rerank only when the
   user commits to a full search. This matches the recall/rerank split already
   identified as the keystone refactor in the IR notes.
3. **Debounced client** in `SearchModal.svelte`, results grouped as commands /
   navigation / content hits.
4. **Generalise the theme-picker mode into a command registry.** It's
   currently a bespoke second mode inside the modal; it's really just a
   command. Same registry as item 9.

### 18 — Tab vs Enter in ⌘K · ship

Enter = navigate to best match. Tab = hand the query to the agent.

Tab inside a modal normally means focus-next, so this needs `preventDefault`
plus a **persistent footer hint** (`↵ open · ⇥ ask`) or the affordance is
invisible. If the hint doesn't land in testing, fall back to ⌘Enter.

---

## Batch D — product

### 1 — Update channel · ship

Full plan in the section below; it spans two repos and CI.

### 8 — Bookmarks & Recents · ship

**Bookmarks — mostly built.** `app_pins` + `pins.rs` + `PinnedSection.svelte`
already do CRUD, ordering, and reorder. Remaining:

- **Rename pins → bookmarks throughout.** `app_pins` → `app_bookmarks`,
  `pinned_at` → `bookmarked_at`, `api/pins.rs` → `api/bookmarks.rs`,
  `stores/pins.svelte.ts`, `PinnedSection.svelte` → `BookmarksSection.svelte`,
  and the `Pin` type in `api/client.ts`. Roughly nine frontend files plus the
  Rust module, `server/mod.rs` routes, and a rename migration. Do it as part of
  this item rather than as a standalone chore — the same files are already
  being opened, and `app_bookmarks` + `app_history` reads as a pair in a way
  that `app_pins` + `app_history` does not.
- **Do not touch `app_notebook_items.role = 'pin'`.** That's an unrelated
  concept — nav-only shortcut vs `library` (retrievable material) — and it is
  load-bearing for retrieval scope resolution (migrations 0032, 0056). Same
  word, different meaning. `notebooks.pinned` is likewise separate.
- **Route rename needs a deprecation window.** `/api/pins` → `/api/bookmarks`
  is client-visible, and the Tauri app bundles its own SPA
  (`tauri.conf.json: frontendDist: "ui"`), so a Mac or iOS app older than the
  box would 404. Keep `/api/pins` as an alias for one release, then drop it.
- Move the section **above Home** in the sidebar.
- Wire drag-reorder in the UI to the existing reorder endpoint.
- Add the icons-vs-full-width display modes (Arc-style).

**Recents — the real work.** Requirements as specified: full visit history
natively (not just modified), a hover-revealed `···` for filtering, and
saveable filters.

- **New `app_history` table + endpoint.** Exactly what it sounds like: every
  in-app navigation, recorded by url. Naming matches the existing `app_pins`,
  and the two are siblings — pins are the routes you chose to keep, history is
  the routes you've been. Columns: `url`, `label`, `icon`, `kind`,
  `visited_at`. Server-side rather than `localStorage`, because "full history"
  and saved filters both have to survive a device wipe. This is a migration,
  not a sidebar component.
- **Capture point:** `windowShellStore` route navigation (not just tab-opens —
  navigating within a tab is still a visit).
- **Append-only log, rolled up on read.** Store every visit; collapse to
  latest-per-url when rendering the sidebar. Keeps the stated "full history,
  not just modified" property while stopping one page visited fifty times from
  filling the list. An upsert-with-counter schema would be cheaper to read but
  discards the sequence, which is the thing that makes it a history.
- **Retention policy required** — append-only needs a bound. A rolling window
  (90 days) or a row cap, pruned on write.
- **Exclusion rule required** — transient and modal routes shouldn't be
  recorded. Needs an explicit allowlist of what counts as a visit.
- **Resolve labels at read time** from the url, falling back to the stored
  snapshot label. Otherwise a renamed page shows its old title in history, and
  a deleted one shows nothing.
- **Clearable.** A complete record of everything the owner has looked at, on
  an appliance holding their whole life, needs a visible clear/pause
  affordance. Not optional.
- **Reconcile with `HistoryView.svelte`**, which today is chat-only
  (`listChats`). Recents should supersede it: one history, sidebar shows the
  recent slice, "See all" opens the full grid. Otherwise there are two
  histories with different scopes and users will notice.
- **Filter dimensions:** type (chat/page/notebook/asset), time window, notebook
  scope.
- **A saved filter is a bookmark.** A bookmark is either a pinned *thing* or a
  pinned *query* — that unifies both halves of this item instead of building
  two systems. `app_pins` stores a `url`, so a saved filter is expressible as
  a pinned route today.
- **Accessibility:** the hover-revealed `···` must be keyboard-reachable and
  permanently visible on touch, or the filter is unreachable on iPad/iPhone.

### 20 — Themes · ship

**Count.** 17 in the `Theme` union (`lib/utils/theme.ts:10`); dropping
`gatsby` lands on exactly 16.

**Migration is mandatory.** Theme persists in the profile row *and*
localStorage, and `isValidTheme` currently falls back silently to `pemberley`.
Deleting `gatsby` would yank those users to an unrelated theme with no
explanation. Add an explicit remap to its nearest surviving sibling on load.

**Default light theme.** Add a neutral pure-white theme and make it both the
fallback and the new-user default in place of `pemberley`. Note this is
two-sided: the real default lives in `virtues-registry` (Rust), delivered via
`/api/assistant-profile` — the TS `FALLBACK_THEME` is only flash-prevention.

**Token collapse.** 125 unique tokens × 36 blocks. Audit usage and collapse —
but *after* the theme count drops, so the audit covers 16 themes. Don't target
Linear's 4; that's their number, not ours.

---

## Item 1 in detail — update channel

### Current state

| | auto-check | channel | privilege |
|---|---|---|---|
| Mac app | every 6h, `mac-latest` | none, hardcoded | user |
| Box | none, by doctrine | `--pre` flag, persists nothing | root |

The box's post-update toast already exists (`routes/(app)/+layout.svelte:113`,
keyed on `BUILD_COMMIT` vs sessionStorage). What's missing is *update
available*. `virtues upgrade --check` exists in the CLI but nothing exposes it
over HTTP.

### Phase 1 — channel primitive on the box

Store: a one-line file in the state root, `/var/lib/virtues/channel`, holding
`stable` | `prerelease`.

Not the DB: `virtues upgrade` must work when the DB is unhealthy — that's half
of why you'd be upgrading. Not `/etc/virtues/env`: the server can't write it
without root, and `upgrade.rs:575` already has to hand-load that file because
sudo doesn't inherit it.

Resolution precedence: `--version` > `--pre` > stored channel > stable. `--pre`
stays a one-off override, so nothing existing changes behaviour.

Files: `cli/upgrade.rs` (`run()` target-tag resolution), `cli/mod.rs` (a
`virtues channel` verb).

### Phase 2 — box update API

**Refactor first.** `fetch_latest_tag`, `fetch_latest_prerelease`,
`list_releases`, `is_linux_tag` are private in `upgrade.rs`. Lift into
`cli/releases.rs` so CLI and API share one implementation — otherwise the
release-listing quirks (mac-tag filtering, draft/prerelease handling) drift.

New `api/updates.rs`:

- `GET /api/system/update` → `{current, channel, available: {tag, notes}|null}`
- `PUT /api/system/update/channel`
- `POST /api/system/update/apply`

**Privilege — decided.** The server runs as `virtues`; upgrade needs root. A
narrow sudoers grant for exactly `/usr/local/bin/virtues upgrade`, installed by
the installer. There's precedent for both halves — `upgrade.rs:321` already
removes a stale setup grant, so the install/cleanup pattern exists.

Constraints on the grant, since this is standing root-adjacent surface on an
appliance: the grant covers that one binary path and that one subcommand, takes
no user-supplied arguments (channel comes from the state-root file, not the
request), and is removed on uninstall.

### Phase 3 — Settings → Box

Version, channel selector ("Main (recommended)" / nightly), check-for-updates,
and the download/install button. Reuse the sonner toast; mirror the Mac app's
ambient language — notify once per version, never force.

One addition the Mac app doesn't need: **a confirm naming the blast radius.**
A box restart drops every connected client, not just the one clicking.

### Phase 4 — Mac app channel

`tauri-plugin-updater` endpoints are static in `tauri.conf.json` but
overridable at runtime via `updater_builder().endpoints(...)`, so the app picks
its manifest URL from a stored pref.

**Requires the CI change:** publish `latest.json` for `mac-edge`, currently
blocked at `release-mac.yml:251`. The original concern was that the updater
must not *accidentally* pull a rolling build — an explicit channel selector is
exactly that opt-in, so the guard's purpose is preserved by the selector.

Files: `apps/web/src-tauri/src/main.rs:547`, `tauri.conf.json:36`,
`.github/workflows/release-mac.yml`.

### Deferred within item 1

- **Auto-check on the box.** Would mean periodic outbound GitHub calls from the
  appliance. Recommend on-demand plus on-open of Settings → Box; no background
  poll.
- **Channel downgrade.** Switching prerelease→stable while ahead of stable hits
  the downgrade guard and silently does nothing until stable catches up. Needs
  either an explicit "you are ahead of stable" state or a forced reinstall.

---

## Deferred / dropped

- **16c Dia swoop** — spike first.
- **9's ⌘⌘ event tap** — dropped for v1; chord instead.
- **2's custom rem-scale zoom** — dropped unless cross-device persistence is
  wanted.
- **3's splash screen** — dropped; folded into 7.

---

## Resolved

- **Settings is a sidebar mode**, not a pane takeover — see 4b. Supersedes the
  "flat nav in a tab" arrangement for navigation, but keeps settings in the
  pane system.
- **Update-apply uses a narrow sudoers grant** for exactly
  `/usr/local/bin/virtues upgrade`.
- **CI will publish `latest.json` for `mac-edge`**; the channel selector is the
  opt-in the original guard was protecting.
- **Recents is `app_history`** — a global in-app navigation log, sibling to
  `app_pins`.
- **Item 2 is native webview zoom**, no custom rem scale. The work is
  layout-hardening against zoom.

## Open questions

1. **Settings section click target** — replace the focused tab, dedicated tab,
   or one reused settings tab? (Recommend: one reused tab, mode persists until
   explicitly exited.)
2. **Item 5's toolbar overflow rule** at narrow split-pane widths.
3. **`app_history` retention** — rolling window or row cap, and what counts as
   a visit (the exclusion allowlist).

---

## Suggested order

**A** (6, 17, 16a, 15, 14, 7, 3, 2) — parallel-safe, no blockers.
**C** (9 → 19 → 18) — start early, longest pole, gates B's item 12.
**B** (4, 5, 16b, 12) — after 9.
**D** (1, 8, 20) — independent of the rest; 1 and 8 both have backend legs.
