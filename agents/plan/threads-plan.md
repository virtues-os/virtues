# Threads and task homes

**Status:** open. The rules below were decided by the owner in November 2026.
Nothing on this page is built except where it says so. Delete this plan when
the Threads door ships; what survives is a record and a manual page.

The designs live on two Claude Design canvases:

- [Threads and home, final](https://claude.ai/artifact/NgapkHQNfjVP1fuzLNkvwn):
  the Threads door (Overview, Tasks, Threads, Insights) and home with its
  notes line. This is the copy to build from.
- [Day tranches](https://claude.ai/artifact/U4SsKkv6nseE8D9iWDi4JU), rounds 5
  to 8: how the week-by-thread chart was reached, with the owner's comments.

All names in this plan and on both canvases are invented.

## Decided

Each item is the owner's call, with the date and his words where they matter.

- **The unit is a thread** (2026-11-17). "I actually like this as the mockup
  for the 'threads' (which is the new name for the old 'projects')." Migration
  0080 renames the tables and its header states the definition: something
  happening in the person's life that they are following, which holds tasks,
  records, pages, chats and files, and points to the subjects it involves.
- **The Threads door puts tasks above the chart of threads** (2026-11-17). "In
  threads at the top is tasks from the threads, and then this chart of the
  threads is underneath." His reason: "you can see how much work you've done or
  is coming up for a thread very visually with the amount of dots/nodes."
- **A thread is Active or Backlog while open** (2026-11-16), and ends finished
  or dropped (0078, built).
- **Two kinds of link** (2026-11-01): parent and child, and involves. No
  stored links between peers.
- **Every task gets a home; not every record does** (2026-11-17). In his
  words: "A task is something you mean to do, so it always belongs somewhere.
  Most records are ambient: a song played, a step count, a payment for lunch.
  Forcing each one into a thread would make filing noisy and often wrong.
  Records join a thread when they matter to it." He agreed the rule as written
  in [The rule for task homes](#the-rule-for-task-homes), including things that
  are abstract areas he names.
- **There is no values object** (2026-11-17). He asked "maybe just skip
  values?" because values lead back to domains of life, which were ruled out on
  2026-10-28. Something he cares about that goes on is a thing he names or a
  habit thread. A thread's why stays a sentence on that thread.
- **The new chat carries insights** (2026-11-17). Home shows the one or two
  notes that matter now; the Threads door keeps every note on what it is about.
- **A thread is a folder first** (2026-11-17). He asked for "a 'magnet off'
  style of primitive to be and act more as a 'folder' to which you can group
  chats and pages manually", "less about auto thread resolution", and answered
  "Yes, add it" to "Only what you add" as the default. Each thread has one
  setting, What goes in: "Only what you add" (the default) or "Also what your
  server files", under which records it is sure of are added and the rest
  become filing questions. Tasks keep their homes either way.

## What the app has today

Checked against the working tree on 2026-11-17. Migration 0080 and the Rust
rename were uncommitted that day.

- `app_threads` (0080, from `app_projects`): `name`, `icon`, `accent_color`,
  `current_status` with `current_status_at`, `instructions`, `archived_at`,
  `outcome` (`finished` or `dropped`) and `thing_id` (0078). There is no column
  for Active or Backlog.
- `app_thread_items`: an item is a `url` with a `role` (`library`, `pin`,
  `manuscript`) and `added_by` (`user` or `magnet`). Items arrive only by hand
  today: `add_thread_item` in `api/threads.rs` inserts them with role
  `library`. `app_threads.auto_add_materials` (default false) is still in the
  schema, and no code reads it.
- Adding to a thread by hand already works under the old name. Dropping a
  recent row or a tab on a pinned project's row in the sidebar files it there
  (`HomePanel.svelte`, `fileIntoProject`); a tab's menu offers the projects
  (`projectMenuItems` in `utils/projectActions.ts`); a page has "Add to
  project…" (`PageToolbar.svelte`), and a chat has "Move to project".
- `app_tasks`: `thread_id` or `thing_id`, never both
  (`app_tasks_one_parent_check`, 0078). There is no column for a person, place
  or organization, and no way to link a task to a second thread or subject. One
  `source_url`, `due_on`/`due_at`, recurrence (0078), `assignee`, and `status`
  (`open`, `active`, `done`, `dismissed`).
- Subjects: `wiki_people`, `wiki_places`, `wiki_orgs`, `wiki_things`.
- Server: `virtues-core/src/api/threads.rs` holds the thread functions. The
  HTTP routes keep their old names (`/api/projects…` in
  `server/api/library.rs`), as do the JSON fields. A dated TODO there adds
  `/api/threads` with the old paths kept as an alias. Tasks are
  `server/api/tasks.rs`.
- Web: `ProjectsListView.svelte`, `ProjectDetailView.svelte`,
  `components/projects/` and `components/tasks/`. The owner said on 2026-11-16
  that this page "needs a HUGE overhaul", so this plan replaces it.
- Home: `HomeNotes.svelte` shows one note at a time, with its action, "Not
  now" and "Next". Notes come from `GET /api/attention` (conditions the server
  observes, which leave when the condition clears), unread chats, and the
  suggested question.
- Not built, and needed by parts of the design: a model that files records
  into threads, a voicemail collector, calendar write access
  ([google-write-access-plan.md](google-write-access-plan.md)), notifications
  that send ([reminders-plan.md](reminders-plan.md)), and reminders by place.

## The rule for task homes

- **One home.** A task ends up with one home: the thread it moves forward, or,
  when no thread fits, the person, place, organization or thing it concerns.
  A thing can be an abstract area the owner names, such as Health. There is no
  preset list of areas.
- **Also about.** A task can also be about any number of other threads and
  subjects. It shows on each of them, and its time counts once, in its home.
- **Capture never waits.** A task can be saved with no home. Your server then
  proposes one, the most specific active thread first, then a subject, and the
  owner can move it.
- **Records join a thread only when they belong.** A model files each record
  by one closed question, which thread or none (decided 2026-10-28). The rest
  stay "Not in a thread", and the week chart shows that time as such.
- **New threads come from evidence.** When several homeless or subject-homed
  tasks gather, your server may propose a thread, built only from tasks and
  records the owner made or received, never from conversation topics alone.
  A suggestion once named a goal the owner did not have, built from topics in
  model-written day pages; this guard is for that failure class.
- **No virtues or vices.** Your server never names a value for the owner and
  never pins one on a single task.

## The design

From the final canvas. Sizes, grays, timings and corners come from
[design.md](../build/design.md)'s scale. All text uses the theme's two darkest
inks, which reach 7:1; marks that carry meaning reach 4.5:1 on the page and on
the desk tone.

### The Threads door

- **Views.** Overview, Tasks, Threads and Insights across the top. Filter and
  Display are icon buttons on the right. No view shows a count.
- **Overview.** "Tasks" with its next five and an "All tasks" link, then
  "Active threads" with the week. "New task" and "New thread" sit at the right
  of their headings.
- **Tasks.** Grouped as Today, Tomorrow, Later this week and No date. A row has
  a ring to finish it, the title, a navy dot when your server noticed
  something, a reminder (by time, or "When you get home"), the icons of what it
  is also about, the home's icon and name, and the due day.
- **Threads.** Active, Backlog and Finished as filters inside the view. Active
  shows the week; the others are rows with the thread's line and when it was
  last touched.
- **Insights.** Every note on a task, a thread, or records that are in no
  thread yet. One sentence, what it is about, when it matters, one action and
  "Not now". A note leaves once its moment has passed.
- **Preview column.** A full-height column on the right, in the desk tone,
  that follows hover and stays when the page scrolls. A row click pins it,
  arrow keys and J/K move it, Escape clears it, and a pager steps through
  items.
  - A thread shows its icon and name, where it stands (drafted by your server
    and corrected by the owner), any note, the thread as a line of records
    from the day it began to its last planned step, its properties (status,
    this week's time, started, next task, what is ahead, who it is with) and
    "Tasks here", which includes tasks that live elsewhere and are also about
    it.
  - A task shows why it exists, any note, the reminder choice, its home and
    what it is also about, the due day, the record it came from, and "Mark
    done", "Change date" and "Move".

### The week of threads

- One row per active thread, then habits, then "Not in a thread", each with
  the thread's chosen icon in its color (`icon` and `accent_color` exist).
- One dot is fifteen minutes. A solid dot is time spent. A ring is time ahead,
  from the calendar or a dated task. A gray dot is recorded time in no thread.
- Today has a dotted outline of round dots. Week arrows sit in the chart's
  top-left corner.
- No totals in the rows, because the dots show the amount. The preview shows
  "6 h this week, 6 h 30 m ahead" for anyone who wants the number.
- A quiet day stays blank. Habits show time, never days in a row.

### Adding to a thread

What exists today stays, renamed: drop a chat, page, tab or recent row on a
pinned thread in the sidebar, or choose "Add to thread…" from a tab's, chat's
or page's menu and pick the thread. The thread page adds a third way: its
Materials list takes a drop, as does the row of a thread in the Threads door.
Each follows design.md's drag rules: only a place that takes the drop lights,
and a drop that worked stays where it landed.

### The detail pages

Each opens from a row and has the same frame: the path ("Threads / Chapter
four"), the main column, and a side column of properties.

- **A thread's page.** Its icon and name, where it stands in one sentence
  ("Drafted by your server" with Edit), any note with "Why this?", its tasks,
  and the thread as a vertical line: dates on the left, a typed node for each
  record, solid down to a dotted Today mark, dashed after it, undated tasks
  last. The side column holds status, started, the thing it sits under, this
  week in small dots, What goes in, and Materials: the chats, pages and files
  added to it, with Add and a place to drop them. "Finish the thread" and "Drop
  it" end it.
- **A task's page.** Its ring and title, why it exists in one sentence, any
  note, "Where it came from" (the source record, with a voicemail's player and
  transcript), the owner's notes, and its history from `app_task_events`, each
  line signed by who did it. The side column holds the home with Move, what it
  is also about with Add, due, repeats, who it is for, and the reminder.
- **An insight's page.** What it is about, the sentence as its title, when it
  leaves, and its actions. "Why your server raised it" states the rule in one
  line and draws the evidence, such as the two overlapping calendar entries on
  an afternoon's timeline. Then what acting changes, its history, and in the
  side column how it was found ("a rule that compares your calendar entries; a
  model wrote the sentence") and "Fewer like this".

### Home

The notes line keeps today's behaviour (one note at a time, its action, "Not
now", "Next") and gains two things: a row of its own under the message box,
since an insight sentence is too long for the tiles' header line, and a link
back to what the note is about, with that thread's or subject's icon.

## Slices

Migration numbers are claimed with `make migration NAME=…` when a slice
starts. Read `.claude/rules/migrations.md` first.

1. **Homes in the schema.** Add `person_id`, `place_id` and `org_id` to
   `app_tasks`, each `REFERENCES … ON DELETE SET NULL`, and widen the
   one-parent check to all five parents. Add `app_task_links (task_id, kind,
   ref_id)` for "also about", with `kind` in thread, person, place, org, thing.
   Add `status` to `app_threads` (`active` or `backlog`, default `active`).
   What goes in is stored in `auto_add_materials`, whose default of false
   already means "Only what you add".
   The migration header says what a rollback to an older release breaks.
2. **Homes in the API.** Tasks take and return their home and links. A
   thread's detail returns its own tasks and the tasks that are also about it.
   `/api/threads` arrives with thread field names, and `/api/projects` stays as
   an alias until released phone apps no longer call it.
3. **The Threads door.** Replace `ProjectsListView` and `ProjectDetailView`
   with the four views, the preview column, and the thread, task and insight
   pages. Move `TaskList` onto the new row. Statuses come from slice 1. Rename
   "Add to project…" and "Move to project" to "Add to thread…" in the tab,
   chat and page menus, and let Materials and thread rows take a drop.
4. **The week.** One endpoint returns, per thread and per day, minutes done and
   minutes ahead, plus the day's time in no thread. The first version counts
   only what can be counted without a model: calendar events and dated tasks
   placed in a thread by the owner, and records with a duration added to a
   thread by hand. Filed records join when filing exists.
5. **Home proposals.** For a task with no home, your server proposes one. For a
   cluster of tasks under one subject, it proposes a thread with "Make it a
   thread". Both follow the evidence rule above.
6. **Notes on tasks and threads.** The first rule needs no model to find: a
   clash between two calendar entries in different threads, or a dated task
   with no time set aside before its date. A model writes the sentence only.
   Notes reach the Insights view, the item's preview and the home notes line,
   and leave when their condition clears.
7. **Later, each behind its own plan.** Reminders by time and by arriving home
   (reminders-plan, and iOS region monitoring); moving one occurrence of a
   recurring event (google-write-access-plan); the voicemail collector; filing
   records into threads by the closed question.

## Open decisions

Each has my pick. None is decided.

1. **How a task names its home.** One column per kind of parent, or a generic
   `(home_kind, home_id)`. Pick: one column per kind, because the foreign keys
   then keep a deleted subject from leaving a broken home.
2. **Where proposals live.** As a new kind of suggestion in `app_suggestions`,
   or as attention items. Pick: attention items, since a proposal is a
   condition your server observes and should leave on its own when the owner
   files the task.
3. **Where notes live.** A table of their own, or attention items that carry
   the task or thread they are about. Pick: attention items with a subject, so
   home already reads them.
4. **The word for a message conversation.** "Thread" now names the unit. Pick:
   call a message conversation a "conversation" in the interface and keep
   `thread_id` on messages as it is.
5. **When the old routes go.** Pick: keep `/api/projects` until no released
   phone app calls it, following
   [version-compat-plan.md](version-compat-plan.md).
6. **The name of the What goes in column.** Keep `auto_add_materials` or
   rename it with the next change to `app_threads`. Pick: rename it to
   `files_records` in slice 1, since the old name describes the magnet that was
   removed.

## How to test

The owner's evals, in invented versions, each expressed with the design:

| Eval | How the design holds it |
|---|---|
| A voicemail asking for a call back | A task whose source is the voicemail, at home on the organization's page, also about a money thread when it mentions a balance. A note before the office closes. Needs the voicemail collector. |
| Shut the dog away before a repair visit | A task under the repair thread, also about the dog (a thing), with a reminder by time or on getting home. |
| Move one occurrence of a weekly evening | A task under the habit thread that holds the series. Its action moves this week only. Needs calendar write. |
| Call someone after a good call with a candidate | A task from the call, at home in a hiring thread, also about the person. |
| A clash between two calendar entries | A note on the thread with one action that moves one of them. |
| Loose records that agree | "Make it a thread" in Insights and in the preview of "Not in a thread". |

The six evals of 2026-11-13 still apply; the notes rules in slice 6 cover the
calendar clash and the dated task with no time, and the rest need the
collectors and filing listed in slice 7.

## What each persona gets

- **Narrative seeker:** where each thread stands in one sentence, and a thread
  read as a line from its start to its next step.
- **Quantified self:** the week in dots, real minutes in the preview, and every
  task traced to the record it came from.
- **Tech prosumer:** a fast keyboard page with a preview column, chosen icons,
  and threads that form from the owner's own records.
- **Recovery:** no counts, no red, nothing overdue, habits shown as time and
  never as days in a row, and no value pinned on anything by the server.
- **Data sovereign:** each task names the record it came from, and each note
  is about something the owner can open. Nothing is marked as on or off the
  server, per CLAUDE.md.
