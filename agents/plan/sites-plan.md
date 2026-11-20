# Sites and the background: splitting what people see of applets

People see one word, "applet", for three different things. This plan gives
the thing people actually look at, a page the assistant made for them over
their own data, its own name and door. The other two kinds become a line next
to what they make, or a list in Settings. The runtime underneath stays one
primitive.

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
2. **Things that run for you.** A scheduled prompt with no view, such as a
   weekly review or a morning examen. People see it where its output lands, as
   one line: "Runs every Sunday at 18:00 · Turn off". It is listed in Settings,
   not in the sidebar.
3. **Your server's own jobs.** Syncs, ingest, the index and backups
   (`owner = system`). They are hidden by default and listed in Settings with
   their runs, so anyone can see what runs and when.

The word "applet" leaves every surface a person sees. It stays in code, the
CLI and the `applets/` folder unless D3 says otherwise.

## Personas

- **Narrative seeker (primary).** One less term to learn. A morning examen is
  "your morning examen", with a line saying when it runs, not an applet.
- **Quantified self (primary).** Gains the most. A chart they liked in a reply
  becomes a site that keeps its tables and refreshes. Today that path exists
  but nothing in chat shows the view.
- **Tech prosumer.** A made thing with a link is what gets shown off. Shared
  links already support an applet's view (`SharedLinksView.svelte`), and
  [publishing-plan.md](publishing-plan.md) has your server host it.
- **Recovery.** Neutral, with one guard: AGENTS.md must tell the model that a
  tracker site shows the record, not streaks, counts against a goal or
  "days since". The run dots on the Applets list (`DayDot.svelte`) move to
  Settings with the rest of the run history and never appear on a site.
- **Data sovereign.** Every system job stays visible in Settings with its runs,
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

### Slice 1: show the site in chat

This is the change that makes the idea real, and it helps even if the rename
waits.

- When `setup` returns an applet with a view, the chat receipt renders the
  view inline (the same sandboxed frame as `document/applet.ts`) with "Open"
  and "Add to a page". Schedule, cost and the switch move under it in one line.
- AGENTS.md and `APPLET_ACTIONS`: when the request is to see or track
  something, make a view, with tables if it keeps data. A view with no prompt
  and no trigger is a valid site (dot-cloud is one already). Add the recovery
  guard here.
- Fix first, from [applets-refactor-plan.md](applets-refactor-plan.md): the
  CSP case (a view that loads a CDN is blank, so the model must be told
  external hosts are blocked), the PATCH reply that drops `has_face`, and the
  face token that never refreshes. A blank frame inline in chat is worse than
  the receipt it replaces.

### Slice 2: the door shows sites

- The sidebar door is renamed "Sites" and lists only applets with a
  view, as tiles or rows showing the view's title and when its data last
  changed. It shows no run status.
- `AppletView` becomes the default page for a site. `AppletDetailView` becomes
  its "Settings" or "How it works" panel.
- Routes: add the new path and keep `/applet/...` and `/actions/...`
  resolving, as `/actions` does today.

### Slice 3: everything else moves to Settings

- `settings/AppletsPage.svelte` becomes the list of things that run for you
  (D2 names it), grouped as yours and your server's, with system jobs folded
  shut. The "needs you" strip and run dots live here.
- Server status reads "Your weekly review needs you" and links to Settings.
- Where a prompt-only applet writes (a Page, a chat, a note), show its "Runs
  every…" line there. If that surface has no slot for it, the Settings list is
  enough for this slice.

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
| D2 | The name for scheduled work in Settings | "Runs for you" as the Settings heading, with no noun for a single item; each one goes by its own name. |
| D3 | Rename in code too? | No. People never see the word, and the sweep costs migrations and a CLI release. Revisit only if the word confuses contributors. |
| D4 | Does a site without a view still exist as a "site"? | No. No view means it's scheduled work, by definition. Adding a view later moves it into the door. |
| D5 | Reopening "Applet" as FINAL (decided 2026-07-19) | The owner reopened it on 2026-11-20 by asking for this plan. This plan changes only what people see. |

## Not in this plan

- The runtime, the file format, triggers, limits and the sandbox stay as
  [applet-shape.md](applet-shape.md) and
  [applets-next-plan.md](applets-next-plan.md) describe them.
- Hosting a site on the open web belongs to
  [publishing-plan.md](publishing-plan.md).
- No gallery or sharing between people.
