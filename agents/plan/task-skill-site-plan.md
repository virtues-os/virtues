# Task, skill, site: the three words people see

Everything the server runs or shows used to be called an applet. People now
see three words: a **task** is work that gets done, a **skill** is how a task
is done, and a **site** is something that exists at an address. The owner
chose these on 2026-11-21. "Applet" stays as the code's word for the one
runtime underneath, and nothing on screen says it.

This plan replaces `sites-plan.md`. Slices 1 to 5 of that plan and the Jobs
page are built on `wave`, under words that were right for one day. This plan
says what the three words mean, what is built, what the built parts now say
wrong, and what is left.

## The three words

| Word | What it is | Examples | Where you meet it |
|---|---|---|---|
| **Task** | Work that gets done. It has a doer. | "Book the flights", "Write my week in review every Sunday", the Gmail sync | Tasks and the Inbox for yours and the assistant's. Settings and the source pages for your server's. |
| **Skill** | Saved instructions for doing a task the same way each time. | Meeting prep, deep research, trip planner | The `/` menu in chat. The assistant also loads one on its own. |
| **Site** | Something that exists at an address. You open it and look at it or use it. | A sleep chart, a water log, a whole app | The Sites door, a tab, a block in a page, a shared link. |

### A task has a doer

| Whose task | What it is underneath | Where it shows | How its state is said |
|---|---|---|---|
| Yours | a row in `app_tasks` | Tasks, the Inbox, its thread | open, done |
| The assistant's, once | a row in `app_tasks` worked by the request worker | Tasks, the Inbox, its thread | working, ready, done |
| The assistant's, repeating | an applet with a prompt and a trigger, shown through one task row | Tasks, the Inbox, its thread | its newest result, read or unread |
| Your server's | an applet that runs a program | Settings under Your server, and its source's page | "Ran 4 minutes ago" |

Four rules keep one word from meaning too much:

1. Your server's tasks never appear in your Tasks list or the Inbox. There are
   24 of them and none is ever finished.
2. Nothing counts tasks or calls one overdue. A late run is said as what
   happened.
3. Your server's tasks "run". Yours get "done". The state words stay apart.
4. A task shows in Tasks only when its result is something to read. A task
   that refreshes a site's data stays inside that site's Options, because the
   site is the result.

### A site has four levels

A person calls all four a site. They differ underneath.

| Level | What it has | State |
|---|---|---|
| 1 | a page over your data | built |
| 2 | a page with its own tables | built |
| 3 | a page whose data a task keeps current | built |
| 4 | a whole app with its own back end that stays running on a port | not built |

Level 4 needs a process with no time limit and a restart rule, a port the
server assigns and serves behind the site's own address, memory and CPU caps,
and its own origin, because the assistant wrote the code and it must not be
able to act as the owner. Build it when someone asks for a site that cannot
work any other way.

### A skill is how a task is done

A skill has no door. The `/` menu starts one now, in the chat you are in. The
assistant loads one when a request calls for it. One shipped skill, `charts`,
is loaded only by the assistant and is not in the menu, so "in the menu" is a
property of a skill and every skill does not have it.

## How the server tells them apart

Nothing stored says what a row is, and nothing needs to. One rule reads the
fields the row already has, in this order:

1. It has a view: a **site**.
2. It runs a program: your **server's task**.
3. It has a prompt and starts itself (a time, or new data): the
   **assistant's task**.
4. It has a prompt and nothing starts it: a **skill**.

Run against the 37 shipped applets on 2026-11-21, the rule places every one:

| Kind | Count | Examples |
|---|---|---|
| Your server's task | 24 | 23 start themselves (syncs, the search index, backups, transcription, the day summary, the wiki editor). `chat-import` runs only when asked. |
| Skill | 11 | 10 in the `/` menu, and `charts` |
| Site | 1 | `dot-cloud` |
| The assistant's task | 1 | `morning-examen` |

The day summary and the wiki editor run a program and then a prompt. The rule
makes them your server's tasks, which is how a person meets them.

## What is built on wave

None of this is on `staging`. It ran on a copy of a real server's snapshot on
2026-11-21 (slice G), not on a real server.

| Piece | Commits | What it does |
|---|---|---|
| The chat opens the site it built | `9b7484f24`, `c8aaee671` | Beside the chat when the window is split, otherwise a new tab. Setup reports `has_view` and refuses a view that loads from another host. |
| The Sites door | `d917b38b1`, `76ec793ad`, `4683d4780` | Cards four across with a live preview, a list as the other view, New site as the first tile. |
| A site's page | `4edbd9b18`, `bf6f6055c` | One component. A "Site / Options" switch in the toolbar, options in the page's own column. |
| Copy and the manual | `ed0551d0d`, `f06f50577` | "Applet" gone from about 40 files, the glossary and the backup page. |
| Addresses | `df399386d`, `30abf3ef4`, `bf6f6055c` | `/sites`, `/site/<name>` and `/options`. Earlier addresses still open. |
| The server's own work | `354920107` | Grouped by purpose, one row per account, rows open in place (`JobOpen.svelte`). |
| The three words | `f126783e1` | "Server tasks", "skill", and the addresses `/virtues/server-tasks` and `/server-task/<name>`. |
| What a row is | `3dee2a53f` | `kind` from the server, addresses and labels chosen from it, and the PATCH reply fixed. |
| A task that repeats | slice D, 2026-11-21 | One task row per repeating applet, its newest result on it, cleared by opening it. |
| The site, finished | slice F, 2026-11-21 | Failed runs said in sentences, a view that outlives a server restart, "Add to a new page". |
| What the run found | `2497c81c1`, `665f89641` | A site's script is a module and setup refuses one that cannot run; the open site's token check has a time limit. |

### What the built parts now say wrong

- **"Job", "Jobs" and "slash command"** were everywhere the server's work or a
  chat prompt was named. Slice A fixed these.
- **The Sites door lists things with no view.** A repeating prompt you set up
  shows there as a card with its description. Under the three words it is the
  assistant's task and belongs in Tasks. It stays until slice D.
- **The chat said "Setting up a site" for every setup, and the authoring
  guide and the applet tool told the assistant to say "site" for everything.**
  Slice A fixed these.

## What is left

### Slice A: the words

Built on wave on 2026-11-21 and seen in the dev app.

- The page that lists your server's tasks is titled "Server tasks" (the
  owner's pick) at `/virtues/server-tasks`, and one of them opens at
  `/server-task/<name>`, which cannot collide with `/task/<id>`. Its switch
  reads "Task / Options". `/virtues/jobs`, `/job/<name>` and the addresses
  before them still open and take today's form.
- "Job" is gone from the app's strings and "slash command" is "skill". Where
  a string covers a site and a task it names both ("What your sites and tasks
  sign in with"), and where it covers anything that came with the server it
  names none.
- The chat's line for a setup says what is being made when the call's own
  keys say so: "Setting up a site", "a task" or "a skill"
  (`applets/words.ts`). A call that changes one key of something made earlier
  says "Setting it up". "In this chat" labels what was made the same way.
- The glossary's Task entry covers your tasks, requests and your server's
  tasks. Skill is new, and Job is gone. It does not yet say the assistant's
  tasks can repeat, because that is slice D.
- `applets/AGENTS.md` opens with the three words, and the applet tool's setup
  line tells the assistant which word to use. That line was 47 characters
  over the tool budget, so the view guidance beside it was shortened.
- Left: the page still folds the server's skills under "Runs only when
  asked", and the Sites door still lists a repeating task as a site. Both
  wait for the `kind` function (slice B). Server error messages and the Mac
  app's six files still say "applet".

### Slice B: one function that says what a row is

Built on wave on 2026-11-21. The web side was seen in the dev app against a
server from before the field, so what ran there was the app's own copy of the
rule. The server's side is covered by its test and has not answered a request.

- `kind_of` in `scheduler/applets.rs` applies the rule and returns `site`,
  `server_task`, `assistant_task` or `skill`. The applets list, the detail and
  the PATCH reply carry it as `kind`. The third step of the rule checks only
  that it starts itself, so a row with a check and no prompt still gets a kind.
- The app reads it through `kindOf` (`applets/kind.ts`), which applies the
  same rule to the parts when a server is too old to say. The two copies have
  the same test table.
- An address starts with the word for the kind: `/site/<name>`,
  `/skill/<name>`, `/server-task/<name>`. The name drops whether you made it,
  so an address can mean two ids, and the page asks the list which exists
  before it asks the server. A page that opens under the wrong word corrects
  its own address once it has loaded. The dot cloud opens at
  `/site/dot_cloud`.
- The switch on a page reads "Site", "Task" or "Skill" from the kind.
- The Server tasks page lists rows whose kind is the server's task. The skills
  that came with the server sit in their own fold, "Skills", and
  "Runs only when asked" holds `chat-import` alone.
- **A bug this fixed.** The PATCH reply was the bare row, without `has_face`.
  The app replaces the applet it holds with that reply, so turning a site off
  and on from its Options drew it as a list of runs until the page reloaded.
  The GET and the PATCH now answer with one builder.
- Two things are held where they were, each with a dated note in the code,
  until slice D gives them a home. A task of the assistant's that you set up
  still shows in the Sites door at a `/site` address, and one that came with
  the server (the morning examen) still shows on the Server tasks page.
- A skill you made still showed in the Sites door until the Skills page was
  built (slice E).

### Slice C: your server's tasks

- The page has its name and addresses (slice A) and lists by kind (slice B).
- Whether an account's row opens in place or links to its source's page is
  open (O2).
- The account group and the "Reaches" line have never been seen with a
  connected account.

### Slice D: the assistant's tasks that repeat

The first cut is built on wave (2026-11-21). The server side is covered by
tests that run the migration. It was also run once against a scratch database
with an invented task: the migration applied, the row opened, a result set by
hand showed under "Ready for you", and opening it cleared it. No real run has
filed a result, and it has not run on a server people use.

What it is: the applet row stays and the applet runner keeps running it.
Tasks cannot start on a clock time, on new data or behind a check, and they
have no memory between runs, no tables of their own and no limit of their
own. Applets have all of these. One row in `app_tasks` stands for the whole
repeating task.

Built:

- **The link.** Migration 0098 adds `app_tasks.applet_id`, one row per
  applet, deleted with its applet.
- **Which applets have a row** (`api/repeating_tasks.rs`). One whose kind is
  the assistant's task, that has not finished, and that someone set up. One
  that came with the server has no row: the morning examen is on from the
  first day, and a row for it would put a task nobody asked for in every
  task list. It stays on the Server tasks page, which the owner confirmed. The row opens at setup,
  in the thread of the chat that made it, and when the server starts, for
  any that lack one.
- **The worker never runs it.** `is_queued` is false for the row, its
  `next_run_at` stays empty, and the claim and the restart requeue both check
  `applet_id IS NULL`.
- **Each result replaces the last.** A run that ends with something to say
  files its summary and the page it wrote on the row, which waits as
  `ready`. A quiet run and a failed run change nothing on the row.
- **Opening it clears it.** `POST /api/tasks/:id/seen` puts it back to
  `open` and keeps the result. There is no Accept. The owner cannot take the
  task or mark it done; it is turned off or deleted from its options.
- **The applet's own limit and model apply,** because the applet runner runs
  it.
- **The task list.** A new result sits under "Ready for you" as "New".
  Otherwise the row sits under "With the assistant" and says when it runs,
  that it is off, or what went wrong. Its page shows the newest result, that
  line, the fix when there is one, and Options, then the pages its earlier
  runs wrote under "Earlier". It has no box to tick and no due date.
- **It leaves Sites.** An applet with a row is not listed in the Sites door
  or on the Server tasks page, and opening it anywhere opens its task.
- **The assistant's prompt is not told how many results wait,** and a
  repeating task is not counted as a request assigned to it.

Left:

- **How it runs is not on the task's page.** Options opens the applet's
  options beside it, at an address that still starts with `/site` or
  `/server-task`.
- **Earlier results that were not pages.** The task's page lists the pages
  its earlier runs wrote. A run that only said something leaves its words in
  the run history under Options.
- **The chat's row** for a new one is still the receipt it had. Its Open
  goes to the task.
- **A site as a task's home,** and making one with `create_task`.
- **An applet that stops being a task** (it gains a view) keeps its row.
- **The overdue and due-today counts** are still in the assistant's prompt
  line. They are about your own tasks and belong to
  [tasks-plan.md](tasks-plan.md).
- **`sql_query`'s catalog** does not list `applet_id`.

### Slice E: Sites holds only sites

The small parts are built on wave (2026-11-21) and seen in a scratch app.

- A repeating task left the door in slice D. Its old `/site/<name>` address,
  in a pin or a link from before it had a task, now opens its task.
- The New site sheet asks what you want to see or keep track of, and its four
  examples are sites: a sleep chart, a water log, a reading list and a trip
  budget. The examples that were tasks are gone.
- A site that refreshes keeps its schedule in its own Options.
- Skills left the door and the Server tasks page. They are listed on a
  Skills page under the assistant's settings (`SkillsPage.svelte`, at
  `/virtues/assistant/skills`): the ones you made, then the ones that came
  with your server. A row opens the skill's page, where it is turned off,
  changed or deleted. What it can do links to it.
- The door lists your sites and nothing else (`inSitesDoor` in
  `applets/kind.ts`). A task of yours stays in it only on a server from
  before task rows.
- Left: the Skills page is not on the Settings landing page or in Settings
  search, because `settings/index.ts` held another change in progress. The
  shipped skill that writes skills is still named "New applet"; renaming its
  folder changes its id and needs a migration to drop the old row. Nothing
  in the app suggests a repeating task any more; it is asked for in a chat
  until Tasks can make one (slice D, Left).

### Slice F: finish the site

Built on wave (2026-11-21) and seen in a scratch app, in the web app only.

- **A failed run says why in a sentence.** `plainReason` in
  `applets/palette.ts` matches the runner's own error strings and the usual
  HTTP and OS errors to ten sentences, such as "Its program couldn't start",
  "It took too long and was stopped" and "A service it uses asked it to slow
  down". Rows on Server tasks, a site's card, a site's page and a repeating
  task's page all read it through `failureReason`. An error it doesn't know
  keeps its first line, since the server writes some for a person ("Add
  OPENAI_API_KEY in Settings, under Keys"). The run log still shows the
  error's own text.
- **A site left open survives a server restart.** The server keeps view
  tokens in memory and forgets them when it restarts. `FaceFrame` asks once a
  minute while the window is showing, and each time the window comes back,
  whether its token still stands (`applets/viewToken.ts`), and loads the view
  again with a new token when the server answers 401. A view that is already
  showing stays up while the server is away and asks for a token again every
  30 seconds. A card's preview in the Sites door does not check.
- **"Add to a new page"** is in a site's More menu, above Share. It makes a
  page named after the site that starts with the site's block and opens it in
  a new tab. A page that already exists takes a site from its own Site block.
- Left: the reload after a restart loses whatever the view had on screen that
  it had not saved, as the hourly renewal already does. A view could tell the
  app the moment one of its queries is refused, which needs a line in
  `virtues.js` (`server/faces.rs`); that file held another change in
  progress. The frame's reload was seen as a new token and a query that
  answered with it. The built-in browser used for the check refuses to draw a
  sandboxed frame, so the redrawn view itself was not seen.

### Slice G: run it on a real server, then the pull request

The run happened on 2026-11-21 on the owner's Mac. The pull request to
`staging` is not opened.

**How it was run.** Committed `wave` was built in a scratch clone, so nothing
another agent had open was in it. The server ran on a copy of the Mac's
snapshot of a real server, never on the snapshot itself. Before the first
start, the copy lost the server's network identity (its key, its relay
config and its door key), and the relay was off, so the copy could not
answer as the real server. It refused requests that did not come from the
Mac itself. Saved sign-ins stayed unreadable there, because the real
server's key was not on the Mac.

**What ran end to end.**

- The 47 migrations from the snapshot's schema to `wave` applied cleanly.
- Server tasks listed the real accounts and devices, one row each.
- The assistant built a site in a chat, the approval card showed, and the
  site opened in a new tab next to the chat with its card in the Sites door.
- The assistant set up a repeating task. A run started by hand went through
  the runner and filed its result on the task. Tasks showed it under "Ready
  for you" as "New", opening it showed the result and cleared it, and the
  request worker had nothing to claim. The owner's earlier repeating tasks
  each had their row.
- "Add to a new page" made a page holding the site.
- A site left open got a new token by itself after a real restart.

**What it found, both fixed on `wave`.**

- The first site the assistant built opened empty while the chat said it was
  up. Its script awaited at the top of a plain `<script>`, which is a syntax
  error, and the guidance had invited it. The setup line and
  `applets/AGENTS.md` now say the script is `<script type="module">`, and
  setup refuses a plain script that awaits with no `async` in it. Asked to
  fix the site, the assistant rewrote it as a module, and it drew its chart
  from the real rows.
- A token check sent as the server went down never got an answer, and the
  frame made no check after it. The check now has a ten-second limit.

**What it did not show.**

- A site drawing inside the app's own frame, or in a card. The built-in
  browser used for the run refuses the resources of a sandboxed frame. The
  site's page was watched drawing through a test page that ran the same
  document against the same data route, and the server's answer to a
  sandboxed frame's preflight was read with `curl`.
- The server's tasks running. The snapshot arrives with every task off, and
  its sign-ins cannot be read on the Mac.
- An upgrade on a real server, with its tasks on.

**Seen and left.**

- When several accounts of one source carry the same stored name, their rows
  on Server tasks read alike.
- After the person allows a setup, the assistant starts the turn again,
  which took over a minute for the site.
- The chat's card for a new repeating task is still the earlier receipt
  (slice D, Left).

## Decisions

In the owner's words where he said them.

| Date | Decision | His words |
|---|---|---|
| 2026-11-20 | A view the assistant made is a site. "Artifacts" lost. | "Sites it is" |
| 2026-11-20 | "Agent" is not a word people see. | "Id very much not want 'agent' to be yet another word herein" |
| 2026-11-20 | A new site never splits the window on its own. | He kept the existing rule when asked. |
| 2026-11-20 | A site's options are part of its page. | "a part of the main page itself and not a sidebar on the right" |
| 2026-11-20 | The Sites door opens on cards with a preview. | "a card view with like 4 cols and then a preview of the site in the card" |
| 2026-11-21 | New site is a tile in the grid. | "new site feels weird as button in the top toolbar of the page" |
| 2026-11-21 | Three words. | "Got it: task, skill, site." |
| 2026-11-21 | "Jobs" and "applets" go. | "I like the idea of rm jobs and applets. And everything is either a task or a site." |
| 2026-11-21 | A whole app on a port is a site. | "Full apps that stay running on ports are just sites, lovely." |
| 2026-11-21 | Syncs and upkeep are tasks. | "the 22 syncs and upkeep jobs -- I would label those tasks for sure" |
| 2026-11-21 | Your tasks and the assistant's sit side by side. | "I like the idea a bit, human tasks vs ai tasks." |
| 2026-11-21 | The page that lists your server's tasks is "Server tasks". | He picked it from four names. |
| 2026-11-21 | A task that came with the server, such as the morning examen, shows on Server tasks and never in your own list. | "th server task list yes not the human one" |
| 2026-11-21 | Skills are managed on a Skills list under the assistant's settings. | "Add Skills list" |

Three decisions of 2026-11-20 are replaced by these: "Jobs" as the name for
the server's work, "everything you set up is a site", and "something with no
view is a site".

### Open

| # | Question | Pick |
|---|---|---|
| O2 | Does an account's row open in place, or link to its source's page? | In place, as built. |
| O6 | Does the code, the database or a stored id change name? | No. Nobody sees them, and the database is the one part that can lose data or break a rollback. |
| O9 | Does a site that came with the server show in the Sites door? | No, as built. The door lists what you set up. The dot cloud opens by its address or in a page. |

## Personas

- **Narrative seeker (primary).** Two things to learn and one recipe word. A
  written review arrives as a task on the thread it concerns, and its earlier
  ones stay listed in order on its page.
- **Quantified self (primary).** Sites becomes charts and trackers only. A
  chart from a reply becomes a site that keeps its tables and refreshes.
- **Tech prosumer.** "Give the assistant a task that repeats" and "a site can
  be a whole app" are the two features worth showing.
- **Recovery.** The risk is in this plan's main slice. A repeating task must
  stay one row, nothing may count what is waiting, and a missed run is said as
  what happened. Sites show what was recorded, with no streak, no count
  toward a goal and no "days since".
- **Data sovereign.** Every one of the server's tasks is listed with when it
  ran, what it did and the hosts it reached. Nothing marks work as on or off
  the server.

## Traps

- **The request worker claims any assistant task that is open and due.** Its
  query is `assignee = 'assistant' AND status = 'open' AND next_run_at <=
  now()` (`api/tasks.rs`, `claim_next_request`). A repeating task's row must
  keep `next_run_at` empty, or the worker runs it a second time as a one-time
  request on the dearer model.
- **Do not move applet rows into `app_tasks`.** The server rewrites shipped
  applets from their files when it starts, so moved rows return, and a
  rollback to an older release would find the prompts gone.
- **A repeating task assigned to the assistant is refused today** in three
  places in `api/tasks.rs`. Slice D does not lift that. It leaves the task's
  repeat fields empty and reads the schedule from the applet.
- **A release from before slice D does not know the row.** After a rollback
  it shows as a request that has not started, with Take it back and Dismiss.
  Neither loses anything: the applet still runs, and the next release reads
  the row again.
- **The tool definitions have a size budget** that a test enforces
  (`tool_definitions_stay_inside_their_budgets`). New wording in the applet
  tool has to fit.
- **A new column on `app_tasks` needs a claimed migration number** (`make
  migration`), and every query that names the table's columns needs a sweep,
  because `sqlx::query` is untyped.

## Not in this plan

- The applet runtime, its file format, triggers, limits and sandbox stay as
  [applet-shape.md](applet-shape.md) and
  [applets-next-plan.md](applets-next-plan.md) describe them.
- Your own tasks and one-time requests stay as
  [tasks-plan.md](tasks-plan.md) describes them. Its "Recurring requests"
  item is slice D here.
- One-time reminders stay with [reminders-plan.md](reminders-plan.md).
- Hosting a site on the open web is [publishing-plan.md](publishing-plan.md).
- Renaming code, routes, tool names, tables or stored ids (O6).
