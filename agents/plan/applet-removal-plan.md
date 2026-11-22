# Removing "applet": sites, skills and tasks in storage and code

Status: planned on 2026-11-22. Nothing here is built.

People already see three words: task, skill and site
([task-skill-site-plan.md](task-skill-site-plan.md)). The database, the code,
the folders on disk and the assistant's tool still say "applet". This plan
replaces the one `app_applets` table with a table per word, renames
everything else that carries the old word, and says in what order.

Nothing a person sees changes. The command line and the tool outside agents
call do change, and are listed below.

## What the owner decided

| Date | Decision | His words |
|---|---|---|
| 2026-11-22 | The code and the database stop saying applet. | "I think we should rename for sites & tasks as such and rm applets" |
| 2026-11-22 | Sites and tasks are different things in storage. | "im thinking tasks and sites as being different, rm applets" |
| 2026-11-22 | The server's tasks are in the same table as yours. | "they shoudl be the same right? ... why call them the same if diff schema/dbs" |
| 2026-11-22 | If the folder on disk needs a word, it is "package". | "package if so yes, probably" |
| 2026-11-22 | No old names are kept working beside the new ones. | He picked "One cut" over "Staged, rollback-safe". |

This replaces open question O6 of the task, skill and site plan, whose pick
was to rename nothing.

## What the code has today

Measured on `wave` on 2026-11-22.

- **One table does four jobs.** `app_applets` has 24 columns: what the thing
  is (`name`, `description`, `agent`, `command`, `config`), when it starts
  (`schedule`, `watch`, `watched_through`, `until`, `triggers`, `run_sql`,
  `last_slot_at`, `next_due_at`), whose account it serves (`credential_id`,
  `device_id`, `owner`), and its state (`enabled`, `archived_at`, `state`,
  `memory`, `seen_at`).
- **What a row is gets worked out twice.** `kind_of` in
  `scheduler/applets.rs` and `kindOf` in `apps/web/src/lib/applets/kind.ts`
  each derive site, server task, assistant task or skill from which columns
  are filled.
- **A repeating task of the assistant's is stored twice**: its applet row,
  and a row in `app_tasks` that stands for it (`applet_id`). The two audits
  of 2026-11-22 found most of their defects in the code that keeps the two
  in step (`api/repeating_tasks.rs`).
- **A row changes kind** when it gains a schedule or a view.
- **Two runners.** `applet_runner/` runs applets. `task_runner.rs` runs
  one-time requests to the assistant from `app_tasks`.
- **Four tables hang off it**: `app_applet_runs` (`applet_id`, set null on
  delete), `app_applet_run_outputs` and `app_applet_run_requests` (cascade
  from a run), and `app_applet_schema_migrations` (cascade from an applet).
- **The word is stored in other places**: `app_ai_calls.applet_run_id` and
  its `feature` value `applet`, `app_publications.producer_kind`,
  `app_visits.kind`, one Postgres schema per applet named `applet_<name>`,
  the roles `virtues_applet_writer` and `virtues_face_reader`, and ids of
  the form `applet_<name>` and `applet_user__<name>`.
- **On disk**: the shipped folder `applets/` (46 folders, each a `SKILL.md`),
  the programs' crate `virtues-applets`, the installed
  `/usr/local/share/virtues/applets` and `applets-bin`, the state root
  `/var/lib/virtues/applets`, `.applet-state/` in a checkout, and the
  variables `VIRTUES_APPLETS_DIR`, `VIRTUES_APPLETS_BIN_DIR`,
  `VIRTUES_APPLET_STATE_DIR` and `VIRTUES_APPLET_RUN_ID`. The names from the
  rename before this one are still accepted too: `VIRTUES_ACTIONS_DIR`,
  `VIRTUES_ACTIONS_BIN_DIR` and `/usr/local/share/virtues/actions`.
- **What the assistant and outside agents call**: the tools `applet`,
  `use_applet` and `update_applet_memory`, the command `virtues applet`, and
  the routes under `/api/applets`, `/api/face` and `/face/`.
- **In the web app**: about 124 files, the tab types `applet`, `applets` and
  `applet-view`, and the address forms from before `/site`.
- **In a page**: a site placed in a page is a block whose tag is
  `virtues-applet` and whose `ref` is the applet's id
  (`crates/virtues-document/contract.json`).
- **In a paired collector**: the Mac collector keeps a map of webhook
  targets by id in its own config (`appletIds` in
  `apps/mac-source/Sources/Core/Config.swift`), read from the pairing reply
  and from `GET /api/devices/applet-ids`.

In all, about 370 tracked files and 4,800 lines outside `agents/` and the
migrations.

## The shape

Four tables hold what people have. The folder on disk is a package.

| Table | Holds | Comes from |
|---|---|---|
| `app_sites` | something with a view | an applet with a `view.html` |
| `app_skills` | how something is done: a prompt, its tools, model and limits | an applet's `agent` and `config` |
| `app_tasks` | all work: yours, the assistant's and the server's, one-time or repeating | today's `app_tasks`, plus every applet that has a schedule, a trigger or a program |
| `app_runs` | every run of a task | `app_applet_runs` |
| `app_packages` | one row per folder on disk | new |

### The rules

1. **Which table a row is in says what it is.** Nothing is derived from
   which columns are filled. `kind_of` and `kindOf` are deleted.
2. **A folder makes up to three rows.** A prompt makes a skill. A schedule,
   a trigger or a program makes a task. A `view.html` makes a site.
3. **Nothing changes kind.** A schedule added to a skill makes a task that
   uses the skill. A view added makes a site beside what was there. Removing
   the schedule deletes the task and leaves the skill.
4. **Every prompt is a row in `app_skills`.** A task points at its skill. A
   repeating task set up in a chat gets a skill of its own that is not in
   the `/` menu.
5. **A task says whose it is in one column**, `assignee`: `owner`,
   `assistant` or `server`. A task that came with the server or serves a
   connected account is the server's, whether it runs a program or a skill.
6. **A site has no switch.** Turning one off does nothing to its view today:
   the token route does not read `enabled`. Only a task is on or off.
7. **Your repeating to-dos and scheduled work are two facts.** `recur_every`
   and `recur_unit` say when your next to-do appears. `schedule` says when
   the runner starts work. A check constraint allows the first only on your
   tasks and the second only on the assistant's and the server's.
8. **The reader login cannot see the server's tasks.** Sites and the
   assistant's `sql_query` both read as one restricted Postgres role. A
   row-level policy on `app_tasks` hides `assignee = 'server'` from it, so a
   count of tasks cannot include the syncs even when a query forgets to say
   whose. The server's own list function takes whose as a required argument.

### Where each column goes

`app_skills`: `id`, `package_id`, `name`, `description`, `prompt` (from
`agent`), `tools`, `model_slot` and `limits` (from `config`), `is_in_menu`
and `is_startable_by_assistant` (from the `message` and `tool` triggers),
`memory`, `origin` (`shipped` or `yours`), `created_at`, `updated_at`.

`app_sites`: `id`, `package_id`, `name`, `description`, `origin`,
`created_at`, `updated_at`. Whether it is shared stays in
`app_publications`.

`app_tasks` keeps its columns and gains: `skill_id`, `program` (from
`command`), `gate_sql` (from `run_sql`), `schedule`, `watch`,
`watched_through`, `until`, `starts` (from `triggers`), `last_slot_at`,
`next_due_at`, `is_enabled`, `credential_id`, `device_id`, `state`,
`seen_at`, `package_id`. `title` takes the applet's `name`, `notes` its
`description`, and `archived_at` becomes `status = 'done'` with `closed_at`.
`assignee` gains `server`, and `origin` gains `shipped` and `source`.
`applet_id` is dropped.

`app_runs`, `app_run_outputs`, `app_run_requests`: the three run tables
renamed, with `applet_id` as `task_id`. `app_ai_calls.applet_run_id` becomes
`run_id`.

`app_packages`: `id`, `folder`, `origin`, `tables_schema`, `created_at`.
`app_applet_schema_migrations` becomes `app_package_migrations`. Deleting a
package deletes its site, skill and task.

### Ids, schemas and roles

- Ids are `site_<name>`, `skill_<name>`, `task_<name>` and `pkg_<name>`.
  Whether you made it is the `origin` column, so the `user__` part goes, and
  with it the two-id lookup an address needs today (`readAddress` returns
  `id` and `otherId`).
- A package's own tables move from the schema `applet_<name>` to
  `pkg_<name>`. Prompts and views name those tables with no schema, so their
  text does not change.
- The roles become `virtues_package_writer` and `virtues_reader`. "Face" is
  the earlier word for a view and goes with applet: the routes become
  `/view/<site id>/` and `/api/view/query`.

### How today's rows move

All 46 shipped folders fit with nothing left over:

- 33 programs become server tasks. Five of them have no schedule and run
  when asked.
- 11 prompts become skills: ten in the `/` menu, and `charts`.
- The morning examen and the wiki editor are prompts that start themselves.
  Each becomes a skill and a server task that uses it.

On a server with its own:

- A site becomes a site row. If it also has a prompt (a water log you add a
  glass to), it gets a skill. If it refreshes on a schedule, it gets a task.
- A skill you made becomes a skill row.
- A repeating task keeps the row it has in `app_tasks` today, with its
  thread, notes and history, and that row takes the applet's schedule and
  state. Its prompt becomes a skill. The applet row is not copied.
- A finished one-time task is a task with `status = 'done'`.

## What one cut cannot cover

Three things sit outside the server and cannot change in the same release.
Each needs something to ship one release earlier.

1. **The upgrade is run by the release before it.** The running binary
   unpacks the new release and looks for folders named `applets` and
   `applets-bin` (`cli/upgrade.rs`, `tools/virtues-installer/src/download.rs`).
   A release whose folders are named `packages` would install with no tasks
   at all. So one release teaches the upgrader the new names, and the next
   one uses them.
2. **Paired collectors keep ids.** A Mac or phone paired before the cut posts
   to ids of the form `applet_…` and reads the key `applet_ids`. The server
   keeps a `former_id` on `app_tasks` that the webhook route and the
   device-ids route still answer to, and sends the new key beside the old
   one, until the collectors that read the new key are the oldest still
   supported.
3. **Pages hold the block.** A page's content is a document the database
   cannot rewrite. The document contract gets a `site` block, and the old
   block is read as one and rewritten when the page is next saved
   (`crates/virtues-document/src/migrate.rs` does this for earlier changes).

Two changes break callers on purpose, with no alias:

- `virtues applet …` becomes `virtues task …`, `virtues site …` and
  `virtues skill …`. An agent key's verbs change with it.
- The tool outside agents reach over MCP is renamed with the assistant's.

## The order of work

The work lands on `wave` in pieces, and `wave` keeps merging to `staging`
for every other session's work. So each piece below is whole by itself: its
migration, its code and its tests land together, and nothing is left half
moved between two pieces. No piece keeps an old name working beside a new
one, except the three boundaries above.

0. **Before anything**: pull request 15 is merged and has run on a real
   server, so this does not land on work nobody has seen run.
1. **Prepare the boundaries.** The upgrader accepts `packages` and
   `packages-bin`. The server sends the device-ids reply under both keys.
   The document contract reads a `site` block. This ships in a release by
   itself.
2. **Runs.** Rename the three run tables and `applet_run_id`. Smallest
   piece, and it proves the sweep and the rehearsal.
3. **Skills.** `app_skills` is created and filled from `agent` and `config`.
   The runner and the chat read prompts from it. `use_applet` becomes
   `use_skill`.
4. **Sites.** `app_sites` is created and filled. The view routes, the token
   and the Sites door read it. The page block and `app_publications` point
   at site ids.
5. **Tasks.** The largest piece. Every remaining applet becomes a task row,
   standing rows merge into the row they stood for, the scheduler and the
   runner read `app_tasks`, the row-level policy goes on, and `app_applets`
   and `api/repeating_tasks.rs` are deleted.
6. **Packages and the word.** `app_packages`, the schema and role renames,
   the folders, the crate, the variables, the routes, the tool and the
   command. The two generations of old names are removed. Web tab types and
   the address forms from before `/site` go, after a migration rewrites the
   addresses kept in `app_pins` and `app_thread_items`.

Merging the two runners into one is a piece after this plan.

## The rehearsal

Each piece from 2 on is run first on a copy of a real server's snapshot, the
way slice G of the task, skill and site plan was: the copy loses the
server's network identity before it starts, the relay is off, and the
snapshot itself is never written to. A piece passes when, on the copy:

- the counts match before and after: sites, skills, tasks, runs, run
  outputs, and rows in every package's own tables;
- every repeating task has one row, with the thread, notes and result it
  had;
- every pin and every thread item that pointed at a site, a skill or a task
  opens;
- every page that held a site block draws it;
- a webhook sent to a task's former id is accepted;
- the reader login counts no server task;
- the chat makes a site, a repeating task and a skill, and each lands in its
  table.

## What has to be swept

`sqlx::query` is untyped, so a renamed table or column fails when it runs,
not when it builds (`.claude/rules/columns.md`). For each piece:

- every SQL string and every `try_get` that names what moved;
- the catalog the assistant reads (`crates/virtues-registry/src/sql_catalog.rs`)
  and the reader role's grants (`server/faces.rs`);
- the tool text and its size budgets (`api/chat_mode.rs`);
- `applets/AGENTS.md` and `applets/AUTHORING.md`, which tell the assistant
  what to write;
- the web client's types and the Mac collector's config keys;
- the release workflow, the installer, the Makefile and `.claude/rules/`;
- the sqlx offline cache.

## Open questions

Each is a question for the owner, with what I would pick.

| # | Question | Pick |
|---|---|---|
| Q1 | Does this ship as one release, or one piece at a time as the order above has it? Example: the run tables could be renamed in a release on Monday and the skills table split out in one a week later. | One piece at a time. `wave` merges to `staging` for everyone, so a rename left half done would ship to servers that install prereleases. |
| Q2 | What is the assistant's tool called? Today it is `applet`, with actions list, get, setup, edit, run and delete. One call can make a site, a skill and a task together. | `setup`. A verb leaves the assistant no fourth noun to say to you. |
| Q3 | The programs' folder and crate are `applets/` and `virtues-applets`. Are they `packages/` and `virtues-packages`? | Yes for the folder. `virtues-programs` for the crate, since it holds only the programs the server's tasks run. |
| Q4 | May a collector paired before the cut keep posting to its old ids for good, or is there a date after which it must pair again? | Keep them until every collector still supported reads the new key, then drop `former_id`. |

## Traps

- **A missed filter is a wrong answer about someone's life.** Before rule 8
  is in, any query over `app_tasks` that forgets whose counts the server's
  syncs as yours. The policy goes on in the same piece that moves the rows.
- **The pool's login is not a superuser.** Renaming a role and enabling a
  row-level policy may need the owner of the table or of the role. The
  upgrade runs one step as root before it gives root away
  (`cli/system_repairs.rs`); the rehearsal has to show the renames work
  from there and from a dev checkout.
- **A rollback past any piece loses what the piece moved.** The older
  binary rebuilds shipped rows from its own folders and does not know the
  new tables. This is what "one cut" accepts.
- **`make migration` first.** Each piece claims its number before its SQL is
  written (`.claude/rules/migrations.md`).
- **Server and app are not the same age.** The apps load the web bundle the
  server ships, but the native shells and the collectors are older than the
  server for weeks. Anything they call by name needs boundary 2.
- **The last rename left its aliases behind.** "Actions" became "applets"
  and the old folder and variable names are still accepted. Piece 6 removes
  both generations, and this plan leaves no third.

## Not in this plan

- One runner for one-time requests and scheduled work.
- Any change to what a person sees.
- The manual. No page names an applet today.
