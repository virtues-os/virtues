# Settings on one page

Status: planned. Decided with the owner on 2026-11-17, over seven rounds of
sketches on the "Settings navigation" design canvas. Round 7 shows what this
plan builds; Round 4 holds the search states it keeps.

Settings becomes one scrolling page of the settings people actually change, in
six sections, with a search field at the top. Typing filters the page in place.
Return hands the text to the assistant, which drafts the change for you to
apply. Anything that is a list, a chart, a table or a form opens its own page
from a row. The section list beside every Settings page goes away, Sources gets
a button in the window's corner, and SQL and Terminal leave Settings.

## What the app has today

- One Settings room, `SettingsView.svelte`, rendering a section per route under
  `/virtues/*`. Bare `/virtues` lands on Profile.
- `SectionNav.svelte`, a 208px list of 15 rows in four groups, drawn inside the
  pane beside every Settings page in the sidebar's own row style, so the window
  shows two sidebars side by side. Its rows come from `SETTINGS_MODE` in
  `lib/sidebar/modes.ts`.
- An audit on 2026-11-17 counted about 60 blocks across the Settings pages: 18
  settings, about 21 readings, about 10 one-off actions, 8 to 10 tools and 3
  explainers. System has one setting in eleven blocks; Search has none.
- The ⌘K palette (`components/search/SearchList.svelte`, `lib/search/results.ts`)
  searches commands, records and record contents, and has an "Ask" row. Its
  "Change theme" command already changes a setting inside search (`keepOpen`).
- The window corner (`tabs/HeaderCorner.svelte`) holds Search and Settings.
  Sources has no door of its own.
- `app_auth_event` records pairing and other auth events. No table records
  setting changes.
- The assistant's name is a setting (default "Ari", `setup.svelte.ts`), and
  about 15 files put it in their copy.

### What the audit found wrong

- The "Chat sky" switch (`SuggestionsSettings.svelte`) saves a preference that
  nothing reads; `ChatSky.svelte` is rendered nowhere.
- "Suggested questions" can be switched on Assistant and again on Applets.
- Models are picked two ways: Assistant's selects and the catalog's "Use for"
  buttons.
- Spending limits sit on Assistant while balance and top-up sit on Billing. The
  bring-your-own-AI key sits on Billing while every other key sits on Keys.
- Keys' "Sign-ins" list repeats Sources.
- Raw JSON, process ids, commit hashes and raw `source_id`s show to everyone.
  The update channels are named after git branches ("Main", "Staging").
- One page says "Revoke" and another "Unpair"; some pages say "box".
- Stale comments in `SystemInfoView.svelte` (Backup "above" Storage, links to
  Settings routes that now redirect) and `modes.ts` (a `/virtues/system/network`
  route that does not exist).
- Profile asks for ethnicity, which nothing is known to use.

## The page

`/virtues` is one page, 720px wide, in this order:

1. The title, the search field ("Find a setting, or say what you want changed",
   ⌘ , from anywhere), and the six section names.
2. **Changed lately**: changes you made, changes the assistant made at your
   request, and things that stopped working, newest first, each with one action
   (Undo, Open, Sign in). It ships with the page.
3. Six sections, each a serif heading, one sentence, and rows:

| Section | Rows |
|---|---|
| You | Name; what the assistant calls you; birthday; work; where you live (sets the time zone); units |
| Assistant (titled with its name) | Name; how it talks to you, with a live preview of a sample reply; subjects it never raises; remember things from chats; what it remembers (opens); what it can do: search the web, read pages you link, make applets, write into your pages; model for chat; model for background work; every model (opens); suggested questions |
| Your data | What the assistant may read (opens); shared links (opens); keys (opens); backup (opens); keep recordings; export everything |
| Appearance | Light or dark; theme; reading font; text size; reduce motion |
| Billing | Subscription (Stripe); balance; auto top-up; most a request can spend in a day; most one request can spend; bring your own AI (opens); what AI has cost (opens) |
| Server and devices | Updates (check now); install updates on their own; which updates ("Released", "Early, may break"); reach it away from home; search runs on (setup guide); how your server is doing (opens); each device (opens); pair a device |

4. **Start over** alone at the bottom, then the founder's letter and "Run the
   introduction again" as quiet links.

Rows are 58px with a hairline between them: the label and one line of help on
the left, the control on the right. Nothing carries a count, a badge or a dot.

### Rows that open a page

A setting moves off the main page when it holds a list that grows, a chart or a
table, or more than about three controls. Those rows show "Open" and push a page
with a back link ("Settings, Billing"), a title, one sentence, then the
content. Back returns to the same scroll position and flashes the row once.

- What the assistant remembers: the notes list.
- Every model: the catalog, which becomes the only other way to pick a model,
  writing the same two choices as the selects.
- What the assistant may read: one switch per source. Turning one off hides it
  from the assistant; the server keeps collecting it. Cycle is off unless you
  turn it on.
- Backup: drive, schedule, last copy, restore.
- Bring your own AI: the endpoint, key and model fields. The key is stored in
  the one `credentials` table with every other key.
- What AI has cost: two weeks by day, cost by kind of work, every call.
- How your server is doing: vitals, storage, the last day. Process lists and raw
  telemetry sit behind its own Detail switch.
- Each device: one page per device, saying "Unpair" everywhere.
- Keys and Shared links: their grids, as today.

### Where you are on the page

As you scroll, the title folds into the bar, following the scroll position, and
the six section names stay beside a small search field with an underline under
the section you are reading (Round 6, option 1). Clicking a name scrolls there;
its heading gets one sweep of the highlighter yellow, which fades.

## Search

- **Each keystroke filters the page.** Matching runs in the browser on a list of
  every setting, by title, section and the words people use for it, and
  tolerates typos. Rows without a match collapse (about 180ms), sections with
  none fold shut and grey their name in the bar, and matched words take the
  highlighter. Settings inside the opened pages match too and are listed under
  their row. Esc shows everything.
- **↑ and ↓** move through matching rows; Return on a row enters its control or
  opens its page.
- **Return with no row picked** asks the assistant. A hint under the field says
  so before you press it. The assistant drafts one or more changes as a card at
  the top of the page: each setting, its old and new value, with Apply, Edit
  first and Not this. Applying writes each change to Changed lately as "You
  changed X with <name>", with Undo. The assistant only drafts settings whose
  kind is a control; for an action (unpair, start over, delete, turn off a link)
  it offers to open the page, and the page asks first.
- **A question** ("how much room do my photos take") shows the value if a
  setting holds it, or offers to ask in a new chat.

## Outside Settings

- The corner holds Search, Sources and Settings. Sources opens its own page with
  Overview, Catalog, Activity, Streams and Query. The button lights while a
  Sources page has focus and never carries a dot or badge.
- SQL becomes the Query tab on Sources. Terminal becomes a ⌘K command, "Open a
  terminal on your server".
- The applet list and its runs move to the Applets door.
- ⌘K gets a Settings group from the same list and a `#settings` scope; simple
  controls work in the palette.

## The assistant's name

The name stays a setting. Copy uses it only where the assistant is the actor in
a sentence ("Ari drafted two changes", "What Ari remembers"), and "your
assistant" where no name was chosen. It is never a door or button label ("New
chat" stays), and there is no greeting or avatar.

## Removed

The Chat sky switch, the second Suggested questions switch, Keys' Sign-ins,
the ethnicity field (height and weight move to the Health source if anything
uses them), developer-only content outside the server page's Detail switch,
the branch-named channels, "Revoke", "box" in copy, the "measured here" and "as
your server heard it" labels, and the stale comments listed above.

## Slices

1. **The list of settings.** `lib/settings/index.ts`: one entry per setting,
   with an id, a title that reads on its own, the words people use for it, its
   section, the page it opens if any, its kind (control, value, action), and for
   controls a component that reads and writes through the calls the pages make
   today. Search for an existing matcher before writing one (`commandsMatching`
   in `results.ts`).
2. **The page.** `/virtues` renders the six sections from the list, with rows
   that open pages. Delete `SectionNav.svelte`, its use in `TabContent.svelte`,
   the Settings rows of `SETTINGS_MODE`, and the removals above. Move spending
   limits into Billing. Redirect every old `/virtues/*` route to its section
   anchor.
3. **Filter and the bar.** Filtering in place, the folding title, the section
   names and the scroll highlight.
4. **Changed lately.** An append-only record of setting changes written by each
   settings endpoint, with enough to undo a change. Pairing comes from
   `app_auth_event` and stopped sources from their status, without copying them.
   Claim the migration number first (`make migration`).
5. **Return asks the assistant.** The drafting card, the rule that it only
   drafts controls, Apply and Undo through Changed lately.
6. **Sources and the corner.** The Sources button, its page with five tabs, SQL
   as Query, the Terminal command, redirects from `/virtues/developer/*`, the
   applet list moved to Applets.
7. **New settings.** Each needs its own work behind it and can land one at a
   time: units, what the assistant can do, what it may read (server-enforced,
   as cycle data is today), keep recordings, export everything, backup as a
   page, reading font, text size, reduce motion, the live style preview.
8. **⌘K.** The Settings group, `#settings`, controls in the palette.

Later: the phone, which takes the same single page.

## Who it serves

- Narrative seeker: one page that reads top to bottom in sentences, a live
  preview of how the assistant will sound, and changes asked for in plain words.
- Quantified self: what AI has cost and how the server is doing are one row
  away, with real charts; Sources has its own door.
- Tech prosumer: filter as you type, ⌘ , drafted changes, settings in ⌘K.
- Recovery: no counts, badges or warnings; Changed lately is a plain list.
- Data sovereign: what the assistant may read, source by source; export; keys,
  shared links and every device on one page.

## Open questions

- Whether "subjects it never raises" moves to its own page once it holds more
  than a few rules.
- How far back Changed lately reaches, and whether it shows changes made from
  another device.
- Which changes can be undone from Changed lately. A model change can; a pairing
  cannot.
- What "keep recordings" removes and when, and what "export everything" writes.
