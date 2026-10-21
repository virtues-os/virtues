# Tasks: one list for the owner and their assistant

**Status:** Slices 1 and 2 built on `wave` 2026-10-20, not yet on
`staging`. Left: slice 3, and the drag-to-reorder question below.

Tasks is a plain to-do list in Home, between Pages and Applets. The owner
and the assistant work from the same list. Think of a light version of
Linear for one person and their agent: a title, an optional due date, a
checkbox, an optional project, and a short history of who did what.

This plan covers that core and nothing else. Picking up tasks from the
record (overheard intentions, emails waiting for a reply), syncing from other
task apps, and reminders by time or place are later work. They are sketched
at the end only so that the core leaves room for them.

## What stays out

The test for every feature: would someone who uses Apple Reminders miss it?
If not, it stays out.

No priorities, labels, estimates, cycles, boards or subtasks. No statuses
beyond the four below. Assignees are the owner and the assistant, and
nobody else. Anything that needs more structure goes in the notes field.

There are no badges, counts or nagging, which carries forward the glossary's
line on commitments. The sidebar entry never shows a number. An overdue task
sits under Overdue and nothing gets louder. A task can notify only when its
owner gave it a time or a place, which is the reminders slice and not this
one.

## The object

A task is one sentence with a few facts attached.

| Fact | Values | Notes |
|---|---|---|
| **Title** | one line | the only required field |
| **Notes** | markdown, optional | where anything bigger goes |
| **Assignee** | `owner` · `assistant` | the owner is the default |
| **Status** | `open` · `active` · `done` · `dismissed` | `active` means the assistant is working on it now. The owner's own tasks are open or done; the list shows no in-progress control for them |
| **Due** | none · a date · a date and time | a date is enough most of the time |
| **Project** | none · one project | the existing Projects (`app_projects`) |
| **Origin** | `owner` · `chat` · `assistant` | who created it: typed into the list, asked for in chat, or added by the assistant on its own |
| **Source** | optional app URL | `/page/page_…`, `/chat/chat_…`: the URL convention pins and refs use, so it opens anywhere a ref opens |

**The three ways a task ends,** taken from the glossary's commitment:
*done*, *dismissed* (abandoned, or never a real task) and *superseded*. In
the core, superseded is simply dismissed with a note. It gets its own value
only if the record-capture slice needs one.

### History

Each task keeps a short, append-only log of what happened to it: created,
edited, status changed, a note added. Each entry is signed `owner` or
`assistant`. This is how the owner sees what the assistant did ("Drafted
the reply. It's in Pages under *Lease renewal*.") without reading a chat
transcript. It is the one Linear idea worth keeping. Comment threads,
mentions and reactions are not.

## Data

The schema is migration `0057_add_tasks.sql`: `app_tasks` (text ids,
`task_…`) and the append-only `app_task_events`. `due_on` is always set when
`due_at` is, and `closed_at` is set exactly when the status is done or
dismissed. Both rules are enforced by CHECK constraints.

There is no delete. A task leaves the list as done or dismissed, and both
stay visible under Done. That keeps one way to remove a task, and the
record-capture slice will later use dismissed tasks as examples of what not
to pick up again. A deleted project leaves its tasks with no project
(`ON DELETE SET NULL`); it does not take them with it.

Register both tables in `crates/virtues-registry/src/sql_catalog.rs` so the
assistant can read them through `sql_query`.

## API

| Route | Does |
|---|---|
| `GET /tasks?status=&assignee=&project_id=` | the list (open and active by default) |
| `POST /tasks` | create |
| `PATCH /tasks/:id` | edit any field, including status; writes an event |
| `POST /tasks/:id/notes` | append a note event |
| `GET /tasks/:id/events` | the history |

Every write goes through one function in core that updates the row and
appends the event in a single transaction. The HTTP handler and the chat
tools both call that function, so nothing writes to `app_tasks` directly.

## Surface

### Sidebar

- Add a **Tasks** entry to the Home list in `HomePanel.svelte`, between
  Pages and Applets, with a `tasks` glyph in `AtlasIcon.svelte`. Like
  Pages, it gets a hover "+" for quick add. ⌘⇧E (`task.new`; ⌘⇧T is the temporary chat's) opens the
  list with the add row focused.
- **Remove the Search entry from that list.** ⌘K, the summon chord and the
  palette are not affected. Move the "All chats via search" comment
  (`HomePanel.svelte:34`) to point at ⌘K.
- In the mobile drawer (`MobileDrawer.svelte`), add a Tasks row. Keep the
  search pill, because the phone has no ⌘K.
- Add a `tasks` tab type in `tabs/types.ts` and `tabs/registry.ts`,
  including `orderedTypes`.

### The list

The list follows Apple Reminders, not a data grid.

- An **add row** at the top. Typing and pressing Return creates a task.
  The row accepts a plain-language date ("fri", "tomorrow 5pm"), parsed on
  the client and shown as a chip before the task is saved.
- **Groups:** Overdue · Today · Upcoming · No date. Done is collapsed at
  the bottom.
- **Filter tabs** in a `SubNav`: **Mine** · **Assistant** · **All**, plus a
  project picker.
- **Row:** checkbox, title, due chip, project chip, an assistant mark when
  the assignee is the assistant, and a source chip when there is a source.
  `active` shows as a quiet in-progress dot. A row never has more than
  that.
- **Actions:** click the checkbox to complete, × to dismiss (on hover;
  always drawn on a phone), and click a row to open the detail.

### Detail

The detail opens in place under its row, on desktop and phone alike. It holds the
editable title and notes, the facts as inline pickers, and the history
underneath. Two actions:

- **Hand to assistant.** Sets the assignee to `assistant` and opens a new
  chat seeded with the task. It always opens a new chat, even when a chat
  created the task, because an old chat brings unrelated context and the
  history already links every chat. That chat changes the task only through the task
  tools, so everything it does lands in the history with its `chat_id`.
- **Take back.** Sets the assignee to `owner`.

## The assistant

### Tools

Add two native tools in `virtues-registry/src/tools.rs`, routed through
`tools/executor.rs` the same way `update_memory` is:

- **`create_task`** takes title, notes, assignee, due and project. It
  accepts a list, so "make tasks for X, Y and Z" is one call.
- **`update_task`** takes an id and any of: edit fields, set status, or
  add a note.

Reading the list goes through `sql_query` against the catalog entry. That
keeps the tool count at two.

Neither tool goes in `PERMISSION_REQUIRED`. Tasks are low-stakes, the
history shows every change, and the owner can undo any of them.

Regenerate `apps/web/src/lib/ai/fixtures/tool-ids.json` and add entries in
`toolPresentation.ts`. A task tool call renders in chat as the task row
itself, with a working checkbox.

### When the assistant creates a task

The assistant creates a task in two cases:

- **Asked:** the owner says "add a task…" in chat. Origin is `chat`.
- **Its own:** the assistant commits to doing something later ("I'll check
  whether the reply came in tomorrow"). Origin is `assistant`, and the
  assignee is `assistant`.

It does not create tasks from things the owner merely mentions. That is
the record-capture slice, and it needs its own threshold and its own
dismissal loop.

### Working an assistant task

In the core, the assistant works a task only inside a chat: one the owner
opened with **Hand to assistant**, or any chat where the owner asks it to.
It marks the task `active` while working, adds a note with the result,
then marks it `done`. If it cannot finish, it marks the task `open`
again with a note saying what is blocking it.

Running assistant tasks in the background when they come due, through the
applet runner's agent phase, is a later slice. The tools and the history
work the same either way, so nothing in the core changes when that slice
lands.

### The brief

The chat prompt gets a single line with the number of open tasks due today
or overdue, so "what's on my plate" doesn't need a query to start. The
line never lists the tasks themselves, so the prompt prefix stays
cacheable.

## Glossary

`agents/build/glossary.md` currently reads "no task manager (no badges,
counts, or nagging…)". Tasks is a task manager, so rewrite the line in the
same change: tasks are how commitments become visible, and the parenthetical
rule stays. Leaving the old line in place would put a doc in conflict with a
feature that ships.

## Slices

1. **Core list** (built). The tables, the API,
   the Tasks view and detail, the sidebar change, and adding and editing
   by hand. Useful alone.
2. **Assistant** (built). `create_task` and `update_task`, Hand to assistant, the
   history pane, the brief line, and the glossary edit.
3. **Background work** (not started). Assistant tasks that come due run headless through
   the applet runner, under the same spending cap as applets.

Each slice is one PR to `staging`.

## Later: designed for, not built

Each of these adds columns or a producer. None of them changes the object.

- **Reminders.** A task with a *when* (a time, or arriving at or leaving a
  saved place) is a reminder. There is no separate reminder object. Time
  and place can fire as local notifications on the phone (iOS region
  monitoring allows 20 regions), which avoids waiting on the APNs signer in
  [reminders-plan.md](reminders-plan.md). That plan's idea of "a reminder
  is an applet row" should be revisited when this lands.
- **Synced sources.** Apple Reminders through EventKit (the iOS collector
  already sees them and skips them), Google Tasks, and possibly checkboxes
  inside pages. Sync is one-way, keyed by `(source, external_id)`, under
  three rules:
  1. Closed at the source means closed here.
  2. Closed here stays closed, and we never write back.
  3. The source owns the title and date until the owner edits them here.
- **Capture from the record.** Tasks heard in transcripts, read in
  messages, or arriving as emails that need a reply. These come in as
  *tentative*: shown lighter, made permanent with one tap, and quietly
  lapsing at their horizon if nothing confirms them. A second mention
  confirms a task, and a completion phrase ("did the laundry") closes it.
  An email task closes when the owner replies in its thread. This slice
  adds a `confidence` column and a `lapses_on` date, and adds `heard`,
  `synced` and `email` to `origin`.

## Open

- **Drag to reorder** is not built. Groups sort by date, then time, then
  `sort_order`, so a drag could only change the order of tasks due the same
  day or of undated ones, and a dragged task would jump back whenever its
  date sorted it elsewhere. Build it only if manual order is wanted, and
  then for the No date group alone. The column is already there.

## Decided

Settled 2026-10-20:

- `active` appears only on assistant tasks.
- Hand to assistant always opens a new chat.
- When the owner finishes an assistant-created task, the assistant is not
  told. The history records it.
- The mobile drawer keeps its search pill. Only the desktop Home list
  loses Search.
- The glossary line is rewritten, not deleted, and keeps "no badges,
  counts, or nagging".
- Quick add is ⌘⇧E, because ⌘⇧T is already New temporary chat.
- A deleted project leaves its tasks with no project.
