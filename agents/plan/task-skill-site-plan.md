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

None of this is on `staging`, and none has run against a real server.

| Piece | Commits | What it does |
|---|---|---|
| The chat opens the site it built | `9b7484f24`, `c8aaee671` | Beside the chat when the window is split, otherwise a new tab. Setup reports `has_view` and refuses a view that loads from another host. |
| The Sites door | `d917b38b1`, `76ec793ad`, `4683d4780` | Cards four across with a live preview, a list as the other view, New site as the first tile. |
| A site's page | `4edbd9b18`, `bf6f6055c` | One component. A "Site / Options" switch in the toolbar, options in the page's own column. |
| Copy and the manual | `ed0551d0d`, `f06f50577` | "Applet" gone from about 40 files, the glossary and the backup page. |
| Addresses | `df399386d`, `30abf3ef4`, `bf6f6055c` | `/sites`, `/site/<name>` and `/options`. Earlier addresses still open. |
| The server's own work | `354920107` | Grouped by purpose, one row per account, rows open in place (`JobOpen.svelte`). |
| The three words | slice A, 2026-11-21 | "Server tasks", "skill", and the addresses `/virtues/server-tasks` and `/server-task/<name>`. |

### What the built parts now say wrong

- **"Job", "Jobs" and "slash command"** were everywhere the server's work or a
  chat prompt was named. Slice A fixed these.
- **The Sites door lists things with no view.** A repeating prompt you set up
  shows there as a card with its description. Under the three words it is the
  assistant's task and belongs in Tasks.
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

- A `kind` function in Rust applies the rule above and the applets list and
  detail routes return it as a new JSON field. Installed apps ignore a field
  they do not know.
- The web app uses it in place of its three separate checks: the id prefix in
  `applets/routes.ts`, `isYours` and `startsItself` in `applets/standing.ts`,
  and `appletsStore.mine`.
- An address is chosen from the kind. That places the dot cloud, which ships
  with the server and has a view, as a site.
- No migration and no stored column. A stored kind can disagree with the
  fields it summarizes, which is why an earlier `runtime` field was removed
  ([record/applets.md](../record/applets.md)).

### Slice C: your server's tasks

- The page keeps the layout it was built with, and slice A gave it its name
  and addresses.
- With the `kind` function, the skills folded under "Runs only when asked"
  leave the page, and `chat-import`, the one server task nothing starts,
  stays.
- Whether an account's row opens in place or links to its source's page is
  open (O2).

### Slice D: the assistant's tasks that repeat

This is the one new thing to build. The four studies of 2026-11-21 agreed on
its shape: show it as a task, and keep the applet and its runner underneath.

- **The applet row stays, and the applet runner keeps running it.** Tasks
  cannot start on a clock time, on new data or behind a check, and they have
  no memory between runs, no tables of their own and no limit of their own.
  Applets have all of these.
- **One task row stands for the whole repeating task.** A new result replaces
  the unread one. It is never one row per run, or a daily task ignored for a
  week becomes seven rows.
- **Opening it clears it.** There is no Accept on a repeating task. Accept
  stays on a one-time request.
- **The row links to its applet** (O3). Its `next_run_at` stays empty for
  good, so the request worker never claims it (see Traps).
- **Its result is the run's summary and the page it wrote.** Earlier results
  are listed on the task's page from the run's recorded outputs, so every past
  review stays in one place in order.
- **Its home is a thread, a thing, a person, a place or an organization,**
  like any task, and it shows on each. A site as a home is a later addition.
- **"Every Sunday at 18:00" comes from the applet's trigger.** The task's own
  repeat fields stay empty. They mean "so many days after you finish it",
  which is right for your own tasks and wrong here.
- **It spends under the applet's limit on the applet's model,** which is the
  cheaper one unless the applet says otherwise. It does not go through the
  request worker's single queue or its server-wide limits.
- **A run with nothing to say changes nothing.** The row keeps its last
  result.
- **A failed run says what happened in a sentence with its fix.**
- **The task's page holds how it runs:** when, what it may spend, the switch,
  its runs and what it was asked to do. These are the options a site has now,
  drawn on the task.
- **The assistant makes one with the applet tool's setup,** and the server
  opens the task row. The chat shows a task row. A repeating task made with
  `create_task` is later.
- **For applets people already made,** the server opens the task row the
  first time it reads the folder, once per applet.
- **The assistant's prompt stops receiving counts.** `tasks_line` in
  `api/circumstances.rs` gives it "{n} overdue" and "Results waiting for them:
  {n}", so it can say a number aloud (O5).

### Slice E: Sites holds only sites

- The door lists rows whose kind is site. A repeating task leaves it for
  Tasks, and its `/site/<name>` address opens its task.
- A site that refreshes keeps its schedule in its own Options.
- The New site sheet asks what you want to see or keep track of.

### Slice F: finish the site

- "Add to a page" and "Share" on a site's page.
- A site left open survives a server restart. Access tokens are kept in memory
  today.
- Failed-run reasons are written as plain sentences. Rows show raw text such
  as "failed to spawn action command" today.

### Slice G: run it on a real server, then the pull request

Three things have never run end to end: the assistant building a site in a
chat and the site opening, a card with real content in its preview, and the
server's tasks with a connected account. After that, one pull request to
`staging` with whatever else on `wave` is ready.

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

Three decisions of 2026-11-20 are replaced by these: "Jobs" as the name for
the server's work, "everything you set up is a site", and "something with no
view is a site".

### Open

| # | Question | Pick |
|---|---|---|
| O2 | Does an account's row open in place, or link to its source's page? | In place, as built. |
| O3 | How a repeating task's row names its applet | One nullable column on `app_tasks`, so the task list reads it in SQL. |
| O4 | Does a repeating task's result need Accept? | No. Opening it clears it. |
| O5 | What the assistant is told about waiting results | That some are waiting, with no number. |
| O6 | Does the code, the database or a stored id change name? | No. Nobody sees them, and the database is the one part that can lose data or break a rollback. |
| O7 | A softer word than "task" for one the server suggests, such as the morning examen | Not picked. A daily reflection listed as a task can read as an obligation. |

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
