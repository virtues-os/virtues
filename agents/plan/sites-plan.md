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
  site in the pane beside it and keeps the keyboard
  (`applets/sites.svelte.ts`, `ToolSideEffects.collectNewSites`). A rebuild
  reloads a frame that is already showing the site. An old chat opens
  nothing, and a phone opens nothing because it has no beside. This is the
  one place the app splits the window without being asked; a page the
  assistant makes still never does.
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

- The sidebar door is renamed "Sites" and shows everything you set up, as a
  grid of cards four across by default, with a list as the other view. A card
  is a preview of the site, its name and when its data last changed. It shows
  no run status.
- A site opens as a tab in the browser layout, like a page or a chat, with
  the chat beside it while it is being built. There is no separate look for
  sites.
- `AppletView` becomes the default page for a site.
- Routes: add the new path and keep `/applet/...` and `/actions/...`
  resolving, as `/actions` does today.

### Slice 3: a site's options hold how it runs

- `AppletDetailView` becomes the site's options, opened from the site's tab
  and from its card: when it runs, what it may spend, the on/off switch, its
  runs, its prompt, and delete.
- A site that has no view gets the server's default view: what it made,
  newest first, each linking to the page or chat it wrote.
- A site that needs you says so in one line on its card and at the top of its
  page ("Sign in to Google again"), and the server status line links to it.
  It is a sentence, not a badge or a count.
- `settings/AppletsPage.svelte` keeps only your server's upkeep jobs. Sync
  jobs move to their source's page.
- One-time reminders are not sites. They stay with
  [reminders-plan.md](reminders-plan.md).

### Slice 4: copy and the manual

Every string in the table above, in the same change: Keys ("what your sites
and scheduled jobs sign in with"), capabilities, devices, shared links, model
slots. Rewrite the glossary entry and the backup page. The glossary entry and
the door's empty state say that a site is private until you share it, because
"site" can read as "on the public web". The door carries no mark of where
anything runs. Settle face or view in
`faces.rs` and AGENTS.md. `tools/check-manual.py` must pass.

### Slice 5: code names (only if D3 says so)

Renaming `app_actions`, the `applet` tool, `applet_*` schemas, the CLI verb
and the folder is a large sweep with migrations and a release-order trap for
the CLI. Do it only if D3 calls for it, as its own plan.

## Decisions

| # | Question | Pick |
|---|---|---|
| D1 | The noun for a made view | **Decided 2026-11-20: "Sites".** "Artifacts" was the other candidate. It is jargon, it covers documents in Claude and so collides with Pages, and it reads as a static leftover when a site keeps tables and refreshes. "Site" can suggest the public web, so the copy says a site is private until you share it. |
| D2 | Where scheduled work lives | **Decided 2026-11-20: in Sites.** Each one is a site, and how it runs is in that site's options. There is no "Runs for you" list in Settings. |
| D3 | Rename in code too? | No. People never see the word, and the sweep costs migrations and a CLI release. Revisit only if the word confuses contributors. |
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
