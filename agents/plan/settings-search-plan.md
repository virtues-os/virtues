# Settings: a landing page, then a page per section

Status: being built. Decided with the owner on 2026-11-17, over eight rounds of
sketches on the "Settings navigation" design canvas. Round 8 shows the layout
this plan builds (it replaced Round 7's single scrolling page); Round 7 holds
the categories, the draft card and the assistant's page, and Round 4 the
search states.

Settings opens on a landing page: a question, the search field, what needs
you, Changed lately, and a link to each page. Each section is a page of its
own. Typing lists the matching settings from every page; Return hands the text
to the assistant, which drafts the change for you to apply. Anything that is a list, a chart, a table or a form opens its own page
from a row. The section list beside every Settings page goes away, Sources gets
a button in the window's corner, and SQL and Terminal leave Settings.

## What the app had before this plan

As of 2026-11-17, before slice 1. Kept for the reasons behind the changes;
the pages above and below describe what replaced it.

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

## The pages

`/virtues` is the landing page, 720px wide: the question "What do you want to
change?", the search field, **Needs you** (what wants an action now) and
**Changed lately** (changes you made, changes the assistant made at your
request, and pairings, newest first), each list showing its first four with
Show all, then three groups of links (You, Your data, Your server), and Start
over with the introduction at the foot.

Each section is a page with a link back to Settings, a serif title, one
sentence and rows: Profile at `/virtues/profile`, the assistant at
`/virtues/assistant`, Appearance, Billing, and Updates and network at
`/virtues/server`. Your data has no page; its links go to What the assistant
remembers, Keys and Shared links. The sections and their rows:

| Page | Rows ("planned" where not built yet) |
|---|---|
| Profile | Your name; what the assistant calls you; birthday; work; where you work; where you live (the time zone); units (planned) |
| The assistant (titled with its name) | Its name; how it talks to you, with a sample reply on request; what it can do: search the web, use your browser, make and change applets, write in your pages; subjects it never raises; models; suggested questions; what it remembers (opens) |
| Appearance | Theme; glass; reading font; text size; reduce motion |
| Billing | Subscription (Stripe); balance; auto top-up; the two spending limits; bring your own AI (opens); what AI has cost (opens) |
| Updates and network | Updates, with install updates on their own and which updates ("Released", "Early, may break"); reach your server away from home |

The landing page's links: You (Profile, the assistant, Appearance, Billing);
Your data (what the assistant remembers, Keys, Shared links, Backup; planned:
what the assistant may read, keep recordings, export everything); Your server
(Updates and network, Devices, How it's doing, Search).

Rows are 56px with a hairline between them: the label and one line of help on
the left, the control on the right. Nothing carries a count, a badge or a dot.

### Rows that open a page

A setting moves off the main page when it holds a list that grows, a chart or a
table, or more than about three controls. Those rows show "Open" and open the
page in the same tab, so Back returns. The page bar's crumbs (Settings / Your
data / Backup) lead back to the landing page.

- What the assistant remembers: the notes list.
- Every model: the catalog, which becomes the only other way to pick a model,
  writing the same two choices as the selects.
- What the assistant may read (planned): one switch per source. Turning one off
  hides it from the assistant; the server keeps collecting it. Cycle is off
  unless you turn it on.
- Backup: how old the newest backup is, each drive, and the commands that set it
  up and restore (built; no buttons, because each step runs on the server).
- Bring your own AI: the endpoint, key and model fields. The key is stored in
  the one `credentials` table with every other key.
- What AI has cost: two weeks by day, cost by kind of work, every call.
- How your server is doing: vitals, storage, the last day. Process lists and raw
  telemetry sit behind its own Detail switch.
- Each device: one page per device, saying "Unpair" everywhere.
- Keys and Shared links: their grids, as today.

## Search

- **Each keystroke lists the matching settings** from every page, in place of
  the rest of the landing page, grouped by the page each lives on. Matching runs
  in the browser on a list of every setting, by title and the words people use
  for it: a name that starts with what you typed, a word in it that does, or a
  name that contains it. It does not correct typos. Esc clears the field.
- **↑ and ↓** pick a result; Return opens its page, scrolled to the row, which
  lights once.
- **Return with no row picked** asks the assistant. A hint under the field says
  so before you press it. The assistant drafts one or more changes as a card at
  the top of the page: each setting, its old and new value, with Apply, Edit
  first and Not this. Applying records each change in Changed lately as the
  assistant's ("…, with <name>"), with Undo. The assistant drafts only text and
  money settings (`DRAFT_FIELDS`: names, work, employer, time zone, its own name
  and style notes, the two spending limits); for anything else the card says it
  didn't find a setting and offers to ask in a chat.
- **A question** ("how much room do my photos take") showing the value a
  setting holds is planned; today it gets the same "ask in a chat" card.

## Outside Settings

- The corner holds Search, Sources and Settings. Sources opens its own page with
  Overview, Catalog, Activity, Streams and Query. The button lights while a
  Sources page has focus and never carries a dot or badge.
- SQL becomes the Query tab on Sources. Terminal becomes a ⌘K command, "Open a
  terminal on your server".
- The applet list and its runs move to the Applets door (planned; the list is
  still at `/virtues/applets`).
- ⌘K gets a Settings group from the same list and a `#settings` scope; simple
  controls work in the palette.

## The assistant's name

The name stays a setting. It titles the assistant's own page and its link on
the landing page, and copy uses it where the assistant is the actor in a
sentence ("Ari drafted two changes", "What Ari remembers"), with "your
assistant" where no name was chosen. It never labels a button that starts
something ("New chat" stays), and there is no greeting or avatar.

## Removed

The Chat sky switch, the second Suggested questions switch, Keys' Sign-ins,
the ethnicity field (height and weight move to the Health source if anything
uses them), developer-only content outside the server page's Detail switch,
the branch-named channels, "Revoke", "box" in copy, the "measured here" and "as
your server heard it" labels, and the stale comments listed above. Still to go:
"Revoke" in the credential and share views, and the "measured here" and "as
your server heard it" labels on the device page.

## Slices

1. **The list of settings.** `lib/settings/index.ts`: one entry per setting,
   with an id, a title that reads on its own, the words people use for it, its
   section, the page it opens if any, its kind (control, value, action), and for
   controls a component that reads and writes through the calls the pages make
   today. Search for an existing matcher before writing one (`commandsMatching`
   in `results.ts`). Built, without the per-setting control components: rows
   are drawn by each page's own components.
2. **The page.** `/virtues` renders the six sections from the list, with rows
   that open pages. Delete `SectionNav.svelte`, its use in `TabContent.svelte`,
   the Settings rows of `SETTINGS_MODE`, and the removals above. Move spending
   limits into Billing. Redirect every old `/virtues/*` route to its section
   anchor. Built, then reshaped by Round 8 into the landing page and a page per
   section; old routes redirect to those pages. The removals are partly done
   (see Removed).
3. **Filter and the bar.** Filtering in place, the folding title, the section
   names and the scroll highlight. Replaced by the landing page and its search
   results (Round 8): see "The pages".
4. **Changed lately.** An append-only record of setting changes written by each
   settings endpoint, with enough to undo a change. Pairing comes from
   `app_auth_event` and stopped sources from their status, without copying them.
   Claim the migration number first (`make migration`). Built (migration 0081,
   `api/settings_changes.rs`); Undo refuses a change that a later one
   replaced. Stopped sources show under Needs you, from `api::attention`.
5. **Return asks the assistant.** The drafting card, the rule that it only
   drafts controls, Apply and Undo through Changed lately. Built, drafting
   text and money settings only (see Search).
6. **Sources and the corner.** The Sources button, its page with five tabs, SQL
   as Query, the Terminal command, redirects from `/virtues/developer/*`, the
   applet list moved to Applets. Built, except the applet list, which is still
   in Settings.
7. **New settings.** Each needs its own work behind it and can land one at a
   time. Built: reading font and text size (the page display settings a
   page's Aa menu also sets), reduce motion (`stores/motion.svelte.ts`), and
   what the assistant can do (`api/capabilities.rs`: the chat leaves an off
   tool out and the tool runner refuses it). No tool reads a linked page, so
   that switch is "use your browser" instead. Also built: the live style
   preview (`api/style_preview.rs`: the chat's own character lines and the
   notes being typed, on the chat model, about an invented week) and Backup
   as a page (`/virtues/backup`: status, drives, and the commands that set it
   up and restore; System links there). Left: units, what it may read
   (server-enforced, as cycle data is today), keep recordings, export
   everything.
8. **⌘K.** The Settings group, `#settings`, controls in the palette. Built:
   typing in ⌘K lists up to three matching settings under Settings, `#settings`
   lists them all, Return opens a setting's page at its row, and Theme and
   Reduce motion change in the palette itself. ⌘ , opens Settings.

Later: the phone, which takes the same landing page and section pages.

## Who it serves

- Narrative seeker: a landing page that opens on a plain question, short pages
  that read top to bottom in sentences, a live preview of how the assistant will
  sound, and changes asked for in plain words.
- Quantified self: what AI has cost and how the server is doing are one row
  away, with real charts; Sources has its own door.
- Tech prosumer: results as you type, drafted changes, settings in ⌘K.
- Recovery: no counts, badges or warnings; Changed lately is a plain list.
- Data sovereign: what the assistant can do and may read; export; keys,
  shared links and every device a link away from the landing page.

## Open questions

- Whether "subjects it never raises" moves to its own page once it holds more
  than a few rules.
- How far back Changed lately reaches, and whether it shows changes made from
  another device.
- Which changes can be undone from Changed lately. A model change can; a pairing
  cannot.
- What "keep recordings" removes and when, and what "export everything" writes.
