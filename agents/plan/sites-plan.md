# Sites and the background: splitting what people see of applets

People see one word, "applet", for three different things. This plan gives
the thing people actually look at, a page the assistant made for them over
their own data, its own name and door. Anything else a person set up to run
becomes a site as well, and your server's own jobs show with their source or
under your server's status. The runtime underneath stays one primitive.

The noun is "site", decided by the owner on 2026-11-20 (D1).

## Why change it

An applet today is "a process the box runs, described by one file"
([applet-shape.md](applet-shape.md)). That covers everything with a trigger,
a prompt, tables, a view, or any subset of them. It is a good runtime, but
people can't picture it.

Counted in `applets/` on 2026-11-20 (39 shipped applets; each Rust job has a
`SKILL.md` folder and a source folder, so the directory count is higher).
Applets a person made in chat live in the state root and are not counted.

| Kind | How many | Examples | What a person thinks it is |
|---|---|---|---|
| Rust job (`run: <binary>`) | 26 | google-gmail-sync, embedding-index, backup-volumes | nothing; they never asked for it |
| Prompt with no trigger, started from chat | 11 | morning-brief, meeting-prep, deep-research | "something the assistant knows how to do" |
| Prompt on a schedule | 1 | morning-examen | "something it does for me" |
| View with no prompt | 1 | dot-cloud | "a thing it made for me" |

The 11 chat prompts are skills: the assistant loads them, or you pick one from
the `/` menu. They already have a home there and need no door or Settings row.

The view is the case people can point at and the one ChatGPT Sites and Claude
artifacts have made familiar: you ask in chat, a page appears, you keep it.
Here it is the rarest kind and the last key in the authoring table.

## What people see after this

1. **Sites.** A page the assistant made with you, over your data. It can have
   its own tables and can refresh on a schedule. It appears in the chat that
   made it, opens full page, can be embedded in a Page, pinned and shared.
   It gets the sidebar door that Applets has today.
2. **Anything you set up to run is a site too.** A weekly review or a morning
   examen has a page that shows what it made, newest first. When the assistant
   writes no view for it, the server gives it a plain one. Its schedule, cost,
   on/off switch and run history are in that site's own options, not in
   Settings. The owner chose this on 2026-11-20 so that there is one place for
   everything you made.
3. **Your server's own jobs.** Syncs and ingest (`owner = system`) show with
   the source they belong to. The index, backups and other upkeep show in
   Settings under your server's status, folded shut, with their runs.

The word "applet" leaves every surface a person sees. It stays in code, the
CLI and the `applets/` folder unless D3 says otherwise.

## Personas

- **Narrative seeker (primary).** One less term to learn. A morning examen is
  a site that holds each morning's examen, and its options say when it runs.
- **Quantified self (primary).** Gains the most. A chart they liked in a reply
  becomes a site that keeps its tables and refreshes. Today that path exists
  but nothing in chat shows the view.
- **Tech prosumer.** A made thing with a link is what gets shown off. Shared
  links already support an applet's view (`SharedLinksView.svelte`), and
  [publishing-plan.md](publishing-plan.md) has your server host it.
- **Recovery.** Neutral, with one guard: AGENTS.md must tell the model that a
  tracker site shows the record, not streaks, counts against a goal or
  "days since". The run dots on the Applets list (`DayDot.svelte`) move into
  a site's options with its run history and never appear on a card or a view.
- **Data sovereign.** Every system job stays visible with its runs, a sync on
  its source's page and upkeep in Settings,
  and the view sandbox (`server/faces.rs`) is unchanged.

## What exists today

| Surface | File | What it says or does |
|---|---|---|
| Sidebar door and list | `tabs/registry.ts` (`applets`), `AppletsView.svelte` | "Applets", "New applet", "Manage applets", run status per row |
| An applet's page | `AppletDetailView.svelte` | settings, prompt, runs, "Run this applet", "Delete applet" |
| An applet's view, full page | `AppletView.svelte`, route `/applet/<id>/view` | the sandboxed iframe |
| View in a Page | `document/applet.ts` | an embedded block, 420px default |
| Chat receipt | `AppletProposalCard` in `ChatView.svelte` | name, schedule, cost per day, an on/off switch; **not the view** |
| Settings list | `settings/AppletsPage.svelte` | Finished, troubleshooting, re-read folders |
| Server status | `settings/serverStatus.ts`, `applets/standing.ts` | "Some of your applets need you." |
| Keys, capabilities, devices | `KeysView.svelte`, `CapabilitiesSettings.svelte`, `DeviceView.svelte`, `DataSettings.svelte` | "What your applets sign in with", "Make and change applets" |
| Shared links | `SharedLinksView.svelte` | "An applet, shared" |
| Model slots | `modelSlots.svelte.ts`, `ByoPage.svelte` | "Titles, summaries, and applets" |
| Manual | `docs/understand/glossary.md`, `docs/operate/backup-and-restore.md` | the glossary entry and backup mentions |
| Authoring | `applets/AGENTS.md`, `APPLET_ACTIONS` in `virtues-registry/src/tools.rs` | `view` is an optional key; the assistant is not told when to add one |

The view also has two names in code: `faces.rs` documents
`face/index.html`, and AGENTS.md documents `view.html`. Slice 4 settles this
on one name.

## Slices

### Slice 0: decide the words

D1 is decided: "Sites". D2 and D3 below have picks and wait on the owner.

### Slice 1: the chat builds the site beside it

Built on wave on 2026-11-20, except the last line of wiring. Not yet run
against a server that reports `has_view`.

- When `setup` makes or rebuilds an applet with a view, the chat opens the
  site the way it opens a page the assistant made: beside the chat when the
  window is already split, otherwise as a new tab next to it. It never splits
  the window on its own (the owner kept that rule on 2026-11-20), and the
  chat keeps the keyboard (`applets/sites.svelte.ts`,
  `ToolSideEffects.collectNewSites`). A rebuild reloads a frame that is
  already showing the site. An old chat opens nothing, and so does a phone.
- The chat keeps one row for the site: its name, when it refreshes, what
  that costs, and the switch (`AppletProposalCard.svelte`).
- `setup` reports `has_view` and refuses a view that names another host,
  naming the host. `APPLET_ACTIONS` and AGENTS.md say when to make a view,
  that it is a whole page, and that it shows what was recorded with no
  streaks or goals.
- A frame renews its token before the hour is up (`FaceFrame.svelte`).
- Left: the two lines in `ChatView.svelte` that call the above are written
  but not committed, because that file holds another change in progress.
  "Add to a page" and "Share" on the site's pane are not built. A frame
  still stops working when the server restarts, since tokens are kept in
  memory. The PATCH reply already carries `has_face`.

### Slice 2: the door shows sites

Built on wave on 2026-11-20, except the door's name on the rail and the tab.

- The door's page is titled "Sites" and shows everything you set up as a
  grid of cards, four across on a desktop window, with a list as the other
  view (`AppletsView.svelte`). A card is a live preview of the site scaled
  to the card, its name and one line: when it last refreshed, or what it
  needs from you (`SiteCard.svelte`). A site with no view shows its
  description until slice 3 gives it a default view. It shows no run status.
- A card or row opens the site's view as a tab, like a page or a chat. There
  is no separate look for sites.
- Left: the label "Applets" on the sidebar door and on the tab
  (`sidebarDoors.svelte.ts`, `tabs/registry.ts`) and a `/sites` path. Both
  files held another change in progress when this was built. `/applet/...`
  and `/actions/...` keep resolving. "Refreshed" is the last successful run,
  because nothing records when a site's own tables last changed.

### Slice 3: a site's options hold how it runs

Built on wave on 2026-11-20 and seen in the dev app, except the last two
items under Left.

- A site's page has two sides in one tab, with a "Site / Options" switch in
  its toolbar (`SiteSwitch.svelte`). Switching replaces the tab's place in
  history, so Back leaves the site.
- Site: the view, full page (`AppletView.svelte`). A site with no view shows
  what it made, newest first, which `AppletDetailView.svelte` already drew:
  the pages it wrote, or its results.
- Options: `AppletDetailView` with `?panel=info`, one column in the page.
  When it runs, its next run and its cost are the first rows, then its run
  history and its technical details. The panel that opened beside the
  results is gone (the owner's call on 2026-11-20), and so is the copy of
  the view that page drew at a fixed height. A link to a site's bare address
  goes to its view.
- What a site needs is one sentence above its view with the button that
  fixes it, the same sentence its card shows.
- Settings lists your server's own jobs and what needs you. For what you set
  up, it points to Sites.
- Left: sync jobs still show in the Settings list and not on their source's
  page. The Settings page is still titled "Applets". One-time reminders are
  not sites; they stay with [reminders-plan.md](reminders-plan.md).

### Slice 4: copy and the manual

Built on wave on 2026-11-20.

- The words: what you made is a **site**, what your server runs for itself is
  a **job** (the owner's word, 2026-11-20: on Linux each one is a scheduled
  process that starts, works and exits, which is a job and not a daemon), and
  a prompt you start from chat is a slash command. Where a string covers more
  than one of them it names none ("Nothing uses it yet").
- The Settings page is titled "Jobs". Keys, capabilities, devices, shared
  links, model slots, the server status line, the chat's tool lines, the page
  editor's site block, search and recents say "site" or "job".
- The manual: the glossary defines Site and Job in place of Applet, and the
  backup page says "sites". The two planned pages are renamed in the manifest.
- `applets/AGENTS.md` opens with the three words, and the applet tool tells
  the assistant to say "site" to the person.
- The share sheet drew its "stays on your server" line twice; it draws once.
- Left, each in a file that held another change in progress: the tab and door
  labels (`tabs/registry.ts`, `sidebarDoors.svelte.ts`, `MobileDrawer.svelte`),
  "Wiki Editor applet" in `wiki/ThingPage.svelte`, one error string in
  `api/client.ts`, the assistant's own prompt in `virtues-registry`'s
  `assistant.rs`, and `face` against `view` in `server/faces.rs`. Server
  error messages still say "applet".

### Slice 5: the addresses

Built on wave on 2026-11-20 and seen in the dev app. The tab registry's part
is in 30abf3ef4, with another session's change to the same file.

- `/sites` is the door. `/site/<name>` is a site and `/job/<name>` is one of
  your server's jobs, each with `/options` for its other side. `/virtues/jobs`
  is the Jobs page. The name is the server's id without its prefix
  (`applets/routes.ts`).
- Every earlier address still opens and takes today's form: `/applets`,
  `/applet/<id>`, `/applet/<id>/view`, `?panel=info`, `/actions`, and
  `/virtues/applets`. They are in pins, saved tabs and links the assistant
  wrote.
- One component draws a site's page now. `AppletView.svelte` is gone, and
  `AppletDetailView` draws the view full page when the site has one.
- The tab says "Site" or "Job", the crumb says "Sites", and the switch on a
  job's page reads "Job / Options".
- Left: a shipped applet with a view (the dot cloud) is addressed as a job,
  because the address is chosen from the id alone.

### Slice 6: names nobody sees

Not started, and each needs a decision (D8, D9).

- Web code: components, stores and file names (111 files).
- Server routes (`/api/applets/...`, `/face/...`) and the assistant's tool
  names: add the new names and keep the old ones, because installed Mac and
  iPhone apps call the old routes and saved chats hold the old tool names.
- Server code, the repo's `applets/` folder, the CLI verb.
- The database (`app_applets` and five more tables), stored ids
  (`applet_user__...`) and the per-site Postgres schemas. This is the only
  part that can lose data or break a rollback.

## Decisions

| # | Question | Pick |
|---|---|---|
| D1 | The noun for a made view | **Decided 2026-11-20: "Sites".** "Artifacts" was the other candidate. It is jargon, it covers documents in Claude and so collides with Pages, and it reads as a static leftover when a site keeps tables and refreshes. "Site" can suggest the public web, so the copy says a site is private until you share it. |
| D7 | The name for what your server runs for itself | **Decided 2026-11-20: "Jobs".** "Upkeep" was offered and refused. "Daemons" is wrong, because nothing stays running between runs. |
| D2 | Where scheduled work lives | **Decided 2026-11-20: in Sites.** Each one is a site, and how it runs is in that site's options. There is no "Runs for you" list in Settings. |
| D3 | Rename in code too? | Reopened by the owner on 2026-11-20 ("it still says applets in the route and page and copy and everywhere, probably db too"). The addresses are done in slice 5. The rest is slice 6 and waits on D8 and D9. |
| D8 | What the code calls the one thing that is a site or a job | Open. One table holds both, so it cannot be `app_sites`. Split it into two tables, give it a new neutral name, or keep `applet` where nobody sees it. |
| D9 | Do stored ids and schemas change? | Open. They are in pins, page embeds and SQL that sites and chats wrote, on servers people use. Pick: leave them. |
| D4 | Is something with no view a site? | Yes, since D2. It gets the server's default view of what it made. |
| D5 | Reopening "Applet" as FINAL (decided 2026-07-19) | The owner reopened it on 2026-11-20 by asking for this plan. This plan changes only what people see. |
| D6 | Is "agent" a word people see? | **Decided 2026-11-20: no.** The owner wants fewer words, so people are not overwhelmed. A person can still set up work that stays on from chat (it wakes on new data, a message or a time). It is a site like any other, with its own options. Sites is the only new noun. |

## Not in this plan

- The runtime, the file format, triggers, limits and the sandbox stay as
  [applet-shape.md](applet-shape.md) and
  [applets-next-plan.md](applets-next-plan.md) describe them.
- Hosting a site on the open web belongs to
  [publishing-plan.md](publishing-plan.md).
- No gallery or sharing between people.
