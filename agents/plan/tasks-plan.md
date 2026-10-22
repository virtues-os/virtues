# Tasks: your to-dos and requests to the assistant

**Status:** Rewritten 2026-10-21. The list, the two task tools and Hand to
assistant are built on `wave` (`20c0f35f5`, `a7a2df843`). Slice 1, requests
running in the background, is built on `wave` and not yet on `staging`.
Slice 2 rebuilds the page around what requests produce.

## Why the first version fell short

The first version shipped a list shaped like Apple Reminders, with an
assignee column for the assistant. On a running box it has these problems:

- **Assistant tasks never run.** Background runs were left for a later slice,
  so a task assigned to the assistant does nothing until the owner opens a
  chat about it. Hand to assistant opens a new chat tab with the task id
  typed into the owner's own message.
- **The result has nowhere to go.** When the assistant finishes, the answer is
  a history line or a chat message. The person who asked "how did my sleep
  track caffeine last month" wants that answer as prose or a chart, and has to
  go looking for it.
- **The assistant's work is hidden by default.** The page opens on **Mine**,
  which hides every assistant task.
- **The detail is a form.** Boxed inputs, the browser's date and time pickers,
  a `<select>` for project, and the title drawn twice.
- **Every fact is a grey pill,** and an overdue task turns its header and its
  pill red. That breaks the rule that nothing gets louder.
- **Tasks crowd the sidebar.** Recent blends tasks with chats and pages, so a
  handful of tasks pushes the owner's pages out of it.
- **Nothing says where the work ran,** on the owner's server or at a model
  provider.

## What the page is for

One page holds two kinds of item:

- **To-dos** are the owner's own. They stay as light as Reminders: type a
  line, press Return, tick it off.
- **Requests** are things the owner asked the assistant to do. A request is a
  question with an answer. The assistant works on it in the background and
  brings back a **result**: a few paragraphs, a chart from the owner's data,
  or a page it drafted.

A request moves through a short lifecycle, shown on the page:

| State | Status value | Means |
|---|---|---|
| Queued | `open` | waiting to run, now or at its time |
| Working | `active` | a run is in progress |
| Ready | `ready` | the result is waiting for the owner |
| Done | `done` | the owner accepted it |
| Dismissed | `dismissed` | the owner dropped it |

**Ready** is the new state, and it is what the page is for. The assistant
never marks its own request done. It marks it ready, and the owner closes it
by accepting the result. Ready also covers "I need something from you": the
result says what is missing, and the owner answers by replying.

To-dos keep `open`, `done` and `dismissed`. Their rows have no Working or
Ready state.

### What each persona gets

| Persona | What the page gives them |
|---|---|
| Narrative seeker | results written as prose, on the request itself |
| Quantified self | requests that come back as charts from their own data |
| Tech prosumer | ask, close the laptop, come back to finished work |
| Recovery | no red, no overdue alarm, no counts, no streaks |
| Data sovereign | every request says which model did the work and who paid |

## What stays out

The test for every feature is still: would someone who uses Apple Reminders
miss it? Requests are the one exception, and they are the reason the page
exists.

No priorities, labels, estimates, boards or subtasks. Assignees are the owner
and the assistant, and nobody else. No badges, counts or nagging: the sidebar
entry never shows a number, and an overdue to-do never turns red.

## The object

The facts from the first version stay as they are: title, notes, assignee,
status, due, project, origin, source, and the signed, append-only history in
`app_task_events`. This version adds:

| Fact | Values | Notes |
|---|---|---|
| **Status** | adds `ready` | only on requests |
| **Result** | markdown, optional | set when a run finishes. Rendered with the chat's prose and chart blocks |
| **Result URL** | optional app URL | a page or applet the run made, using the ref URL convention |
| **Chat** | optional chat id | the request's own work chat, created by its first run and reused by every later run |
| **Next run at** | timestamp, optional | when the worker may pick the request up. Null means it never runs on its own |
| **Attempts** | integer | failed runs in a row. Reset by a successful run |

A request is a task with `assignee = 'assistant'`. There is no separate
table.

### When a request runs

- A request with a date and time (`due_at`) runs at that time. "Check
  tomorrow at 9 whether the reply came in" means exactly that.
- A request with only a date, or with no date, runs as soon as it is queued.
  The date says when the owner wants the answer, not when to start.
- Handing a to-do to the assistant queues it.
- Replying to a ready request appends the reply as a note, sets the status
  back to `open`, and queues it again in the same chat.

## Data

One migration, claimed with `make migration NAME=task_requests`:

- Add `ready` to the `status` CHECK. `closed_at` stays null for `ready`,
  because a ready request is not closed.
- Add `result text`, `result_url text`, `chat_id text REFERENCES
  app_chats(id) ON DELETE SET NULL`, `next_run_at timestamptz` and
  `attempts integer NOT NULL DEFAULT 0`.
- Add a partial index for the worker:
  `(next_run_at) WHERE assignee = 'assistant' AND status = 'open' AND next_run_at IS NOT NULL`.

**Trap: existing assistant tasks must not all start at once.** The migration
leaves `next_run_at` null on every existing row. Code sets `next_run_at` only on
new requests and on hand-overs and replies after the migration. Without
this, every assistant task on a box starts a model run, and charges, the
minute the upgrade lands.

The migration also adds `app_ai_calls.chat_id` back (0011 had dropped it
because nothing wrote it). Chat turns and headless runs now record the chat
for every call, and each request has one chat, so a request's cost is the
sum over its chat. The same rows carry `model` and
`route` (`wallet` or `byo`), which is where the page's "where it ran" line
comes from.

Register the new columns in `sql_catalog.rs` with the rest of `app_tasks`.

## Slice 1: requests run in the background

### One headless agent loop

`agent::headless::run_headless` runs one pass of the loop from a
`HeadlessJob`: chat, model, prompt, tools, a spend ceiling and the AI-call
attribution. It loads the chat, posts the person's message and the reply,
and records every call. `agent::applet_runner` builds a job from an applet
row; `crate::task_runner` builds one from a request.

### The worker

`crate::task_runner`, started next to the applet cron scheduler, checks
once a minute, and straight after each run, for requests where `status = 'open'`, `assignee = 'assistant'`
and `next_run_at <= now()`. It runs one request at a time.

For each request:

1. Set `active`, signed `assistant`, in one transaction with the event, the
   same way every task write goes through `api::tasks`.
2. Create the request's chat on its first run. The opening message carries
   the title and notes. A later run opens with the owner's notes since the
   last run. The chat belongs to the request, so `list_chats` leaves it out.
3. Run the loop on the Standard slot, because the result is prose the owner
   reads, with the request tool set (`get_tools_for_request`: read, search,
   compute, pages and the two task tools, without `sql_write`). The tool
   context carries the chat id and the task id, so every tool call lands in
   the history signed `assistant`, and the permission gate lets it through
   as it does an applet run.
4. On success: store the final assistant message as `result`, set `ready`,
   reset `attempts` and clear `next_run_at`. The assistant can set
   `result_url` through `update_task` when it made a page.
5. On a model or tool failure: increment `attempts`, keep `open`, and set
   `next_run_at` with a backoff. After three failures, set `ready` with a
   result that says what failed and what the owner can do.
6. When the per-run ceiling stops the run: set `ready` with what it had,
   plus a line naming the limit and asking the owner to reply to continue.
   A ceiling is not a failure, the same distinction `AppletRunStatus` draws.
7. When the daily ceiling is reached, nothing starts. Requests stay queued
   until the day's spend falls under it.

A request left `active` with a work chat when the server stops goes back in
the queue on the next start. A request the owner dismisses or takes back
during a run keeps their change; the run's answer becomes a note.

**A request cannot start another request.** Tasks created during a run are
created for the owner, and `update_task` from a run cannot assign a task to
the assistant. Without this guard, a run that creates a request can keep
the worker busy, and spending, indefinitely.

### Limits

Request runs have a ceiling per run and a ceiling per day across all
requests, counted on wallet spend (BYO calls report no cost). They are
constants in `task_runner.rs` for now, $0.50 a run and $2 a day, and become
owner settings in Settings with slice 2.

### Changes to the tools and the chat side

- `update_task` gains `ready` with `result`, and `result_url`. Only the
  assistant can set `ready`, and it can never set `done` on a request.
- **Hand to assistant** no longer opens a chat. It sets the assignee,
  queues the request, and the row shows Queued at once.
- A chat that creates a request with `create_task` queues it the same way.
  The chat's task card shows the request's state and links to the page.
- The chat prompt's tasks line (`tasks_line` in `api/circumstances.rs`)
  adds ready requests to its counts, so "what came back?" needs no query.

### What slice 1 does not change

The page keeps its current layout. A request row shows Queued, Working or
Ready, and the detail shows the result with Reply and Accept. The list
reloads only when its tab comes forward, so a request that finishes while
the page is open still reads Queued until then. Slice 2 has to refresh it.

## Slice 2: the page

### Layout

One page, top to bottom, with no Mine / Assistant / All tabs:

1. **Ready.** Each ready request shows its title and its result in the
   chat's prose style. Charts and code blocks render as they do in chat. A
   result URL shows as a ref card. Two actions: **Accept**, which sets
   done, and **Reply**, which opens an inline composer under the result and
   queues the request again.
2. **Working.** Queued and active requests. Active ones show a quiet
   progress mark. Each shows one line on where the work runs: the model's
   display name, and "on your account" for BYO or "through Virtues" for the
   wallet. A BYO model at a loopback or LAN address reads "on your server".
3. **To-dos.** Dated to-dos first, in date order, then undated ones. The
   date is muted text at the end of the line ("Tomorrow", "Fri", "2 days
   ago"). No group headers until the list is long enough to need them,
   and no red.
4. **Done,** folded. It holds accepted requests, finished to-dos and
   dismissed items, newest first.

The add row stays at the top. A line typed there is a to-do. Pressing
⌘Return instead of Return makes it a request.

### Rows

- To-do row: checkbox, title, muted date, and the project name in muted
  text when there is one.
- Request row: the Virtues mark in place of the checkbox, title, and a
  status word (Queued, Working, Ready) in muted text.
- No pills anywhere on the page.

### Detail, in place

Clicking a row opens it under itself, as now, but with no form fields:

- The title edits in place in the row.
- Notes use the composer's ProseMirror editor.
- Due and project are text buttons ("Tomorrow · Kitchen remodel") that open
  small popovers. The date popover takes the same plain-language input as
  the add row (`tasks/due.ts`), with a calendar under it.
- History is a quiet timeline under the notes. For a request, it includes
  each run, its cost, and a link to the work chat for anyone who wants the
  full transcript.

### Keyboard

j and k move between rows. x completes a to-do or accepts a ready request.
e edits the title. ⌘Return on a to-do hands it to the assistant. Escape
closes the detail.

### Sidebar

Tasks leave Recent. `recents.svelte.ts` stops blending tasks, and the
`?task=` links stay for anything that already points at one. The Tasks entry
in Home stays, with no count.

### Mobile

The same order on the phone. Accept and Reply sit under the result, always
visible.

## Later: designed for, not built

None of these changes the object; each adds columns or a producer.

- **Reminders.** A to-do with a time or a place notifies on the phone. iOS
  region monitoring allows 20 regions, so place reminders can fire as local
  notifications without the APNs signer in
  [reminders-plan.md](reminders-plan.md).
- **Synced sources.** Apple Reminders through EventKit, Google Tasks, and
  possibly checkboxes in pages. One-way, keyed by `(source, external_id)`:
  closed at the source means closed here, closed here stays closed, and the
  source owns the title and date until the owner edits them here.
- **Capture from the record.** To-dos heard in transcripts or read in
  messages arrive as tentative rows and lapse if nothing confirms them. This
  adds a `confidence` column and a `lapses_on` date.
- **Recurring requests.** "Every Sunday, summarize my week" is an applet
  today. If owners start making requests for this, a request with a
  schedule could replace the applet, but not before the two are compared.

## Open

- **Default spending ceilings.** The code uses $0.50 a run and $2 a day
  until the Settings control exists. Confirm or change both.
- **Whether Ready gets a sidebar mark.** A count is ruled out. A plain dot on
  the Tasks entry while something is ready might be acceptable. Decide after
  using slice 1.
- **Drag to reorder** is still not built. If it is, it applies to undated
  to-dos only, since dates decide the rest of the order. `sort_order` already
  exists.

## Decided

Settled 2026-10-20 and still in force:

- Hand to assistant never reuses an old chat. In this version the request
  gets its own work chat instead of a new chat tab.
- When the owner finishes a to-do the assistant created, the assistant is
  not told. The history records it.
- The mobile drawer keeps its search pill. Only the desktop Home list lost
  Search.
- Quick add is ⌘⇧E, because ⌘⇧T is New temporary chat.
- A deleted project leaves its tasks with no project.
- There is no delete. A task leaves the list as done or dismissed.

Settled 2026-10-21:

- Requests run in the background, and the assistant ends a run at `ready`,
  never `done`.
- Background runs come before the page rebuild.
- The Mine / Assistant / All tabs go away.
