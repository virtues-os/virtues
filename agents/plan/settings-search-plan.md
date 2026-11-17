# Settings you search

Status: planned. Decided with the owner on 2026-11-17, after five rounds of
sketches on the "Settings navigation" design canvas (Round 4 shows the states,
Round 5 what this plan builds).

Settings opens on one question, "What do you want to change?", with a search
field under it. Typing brings up the settings themselves, with their controls,
instead of links to pages. The section list down the left of every Settings
page goes away. Sources gets its own button in the window's corner, and SQL and
Terminal leave Settings.

## What the app has today

- One Settings room, `SettingsView.svelte`, rendering a section per route under
  `/virtues/*`. Bare `/virtues` lands on Profile.
- `SectionNav.svelte`, a 208px list of 15 rows in four groups (Account, Server,
  Sources, Developer), drawn inside the pane beside every Settings page. It
  copies the sidebar's row style, so the window shows two sidebars side by
  side. Its rows come from `SETTINGS_MODE` in `lib/sidebar/modes.ts`.
- Settings pages are written by hand. Each page holds its own fields (Profile:
  name, birth date, height, timezone and so on; Assistant: name, how it talks,
  rules, memory, models, spending on requests, suggestions). Nothing lists every
  setting in one place.
- The ⌘K palette (`components/search/SearchList.svelte`, `lib/search/results.ts`)
  searches commands, records and record contents, and has an "Ask" row. Its
  "Change theme" command already changes a setting inside search (`keepOpen`).
- The window corner (`tabs/HeaderCorner.svelte`) holds two glass buttons,
  Search and Settings. Sources has no door of its own: you reach it through
  Settings or ⌘K's "Go to Sources".
- `app_auth_event` records pairing and other auth events. No table records
  setting changes.

## The design

### The landing page

`/virtues` shows the question, the search field (focused, ⌘ , opens it from
anywhere), a "Changed lately" list, and every section in three groups:

- You: Profile, Assistant, Billing
- Your data: Keys, Shared links
- Your server: System, Devices, Search, Applets

"Changed lately" lists changes you made and things that stopped working, newest
first, each with one action: Undo, Open or Sign in. "You set the chat model to
Standard, yesterday, Undo." "Google Calendar stopped arriving. It needs you to
sign in again." It ships with the landing page, not after it.

### Results

Each result is a setting, shown in one of three ways:

- **Control.** The setting's own control in the row: a switch, a choice, a
  short field. Changing it there saves it, as it would on its page.
- **Value.** Something you read, shown with its current value and an Open link:
  storage used, balance, the release a device runs.
- **Action.** Anything that cannot be undone (unpair, start over, turn off a
  link, delete) shows only "Open its page". Search never performs it. The page
  asks first, as it does today.

Each result has a title that makes sense outside its page ("Most one request
can spend", not "Each run, in dollars") and names its section under the title.
Arrow keys move, Tab enters a control, Return opens the page.

### Opening a result

The section's page opens at `/virtues/<section>#<setting>`, scrolls to the
setting and lights it in the highlighter yellow, which fades. The citation
highlight already does this for passages opened from a chat; reuse it. The
search field stays at the top of every section page in a smaller form, with
"1 of 3, next" to step through the other matches without going back.

### When nothing matches

If the query reads like a question or matches nothing, the last row offers
"Ask Iris", which opens a new chat with what was typed. The palette's existing
"Ask" row does this.

### ⌘K

The palette gets a Settings group built from the same list. Simple controls
(a switch, a choice of two or three) work inside the palette; everything else
opens its page. `#settings` scopes the palette to settings, beside the existing
`#chats`, `#pages` and the rest. "Change theme" stays as it is.

### Sources in the corner

The corner holds three glass buttons: Search, Sources, Settings. Sources opens
its own page with Overview, Catalog, Activity, Streams and Query across the top.
The button lights while a Sources page has focus, as Settings does. It carries
no dot or badge, not even when a source stops: that would be an alarm. A stopped
source shows in "Changed lately" and as a sentence on the Sources page.

### SQL and Terminal

SQL becomes the Query tab on the Sources page, beside Streams, since it queries
those streams. Terminal becomes a ⌘K command, "Open a terminal on your server".
Neither is a Settings section.

## Slices

1. **The list of settings.** `lib/settings/index.ts`: one entry per setting,
   about 60, each with an id, a title that makes sense outside its page, the
   words people use for it, its section, its kind (control, value, action), and
   for controls a small component that reads and writes through the calls the
   page already makes. Every settings page marks each entry's block with
   `data-setting="<id>"`. Search for an existing matcher before writing one
   (`commandsMatching` in `results.ts`).
2. **The landing page and results.** `/virtues` renders the question page.
   Matching runs in the browser on the list. Values fetch when their rows
   appear, one request per section. Delete `SectionNav.svelte`, its use in
   `TabContent.svelte`, and the Settings rows of `SETTINGS_MODE`. Add legacy
   route redirects for every removed row.
3. **Opening a result.** The anchor, the scroll, the fading highlight, and the
   small field with "1 of 3" on section pages.
4. **Changed lately.** Server: an append-only record of setting changes, written
   by each settings endpoint, with enough to undo a change. Read pairing from
   `app_auth_event` and stopped sources from their status, without copying them.
   Claim the migration number first (`make migration`).
5. **Sources and the corner.** The Sources button in `HeaderCorner.svelte`, the
   Sources page with its five tabs, SQL moved into Query, the Terminal command
   in ⌘K, redirects from `/virtues/developer/*`.
6. **⌘K.** The Settings group, `#settings`, controls inside the palette, and
   ⌘ , opening Settings with the field focused.

Later: the phone layout, and Iris drafting a change from a sentence (Round 3,
H2 on the canvas). Drafting works only for settings written as text (how it
talks, rules, memory).

## Who it serves

- Narrative seeker: you ask in your own words, and results and "Changed lately"
  are written as sentences.
- Quantified self: values like storage and costs show in results, and Sources is
  one click from anywhere.
- Tech prosumer: ⌘ , and settings that change from ⌘K on any page.
- Recovery: no counts, badges or warnings anywhere in it. "Changed lately" is a
  plain list.
- Data sovereign: searching a device shows everything tied to it in one place.

## Open questions

- How long "Changed lately" reaches back, and whether it shows changes made from
  another device.
- Which settings can be undone from the list. A change of model can; a pairing
  cannot.
- Whether the words people use for each setting are written by hand only, or
  also learned from searches that found nothing.
