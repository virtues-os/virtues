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
- **Adding to a thread is manual or automatic** (2026-11-17). He asked for "a
  'magnet off' style of primitive to be and act more as a 'folder' to which you
  can group chats and pages manually", "less about auto thread resolution",
  answered "Yes, add it" to manual as the default, and then put it in one line:
  "its more 'manual or auto' for adding nodes, thats really what the 'folder vs
  auto thread' is". Each thread has one setting, Adding:
  - **Manual** (the default): only what the owner adds becomes a node: chats,
    pages, files, records and tasks. Your server neither adds to the thread nor
    asks about it.
  - **Automatic**: your server also adds records and tasks it is sure belong,
    and asks about the ones it is not sure of as filing questions.
- **Task homes, proposals and notes** (2026-11-17). He took my three picks
  ("Yes, use your picks"):
  - A task names its home with one column per kind of parent (`thread_id`,
    `person_id`, `place_id`, `org_id`, `thing_id`), so a foreign key keeps a
    deleted subject from leaving a broken home.
  - Home and thread proposals are attention items, so a proposal leaves on
    its own when the owner files the task.
  - Notes on tasks and threads are attention items with a subject, so home
    already reads them.
- **Build and break: the old project names go** (2026-11-17). "rm old
  cruft/buildnbreak. dont wory about migrating projects", "rm old api projects
  routes and things. feel free to rename schema. rename messages conversation
  from thread." So nothing keeps the project names alive for older clients: the
  `/api/projects` routes, the `project_id` and `projectId` JSON fields, the
  `project` trash kind and the `/project/` urls all become thread names, and a
  message or email conversation is a conversation (`thread_id` on
  `data_communication_message` and `data_communication_email` becomes
  `conversation_id`).

## What the app has today

Checked against the working tree on 2026-11-17, after migration 0082
(`threads_cleanup_and_conversations`).

- `app_threads` (0080, from `app_projects`): `name`, `icon`, `accent_color`,
  `current_status` with `current_status_at`, `instructions`, `archived_at`,
  `outcome` (`finished` or `dropped`), `thing_id` (0078), and `is_backlog`
  (0083), false for Active.
- `app_thread_items`: an item is a `url` with a `role` (`library`, `pin`,
  `manuscript`) and `added_by` (`user` or `magnet`). Items arrive only by hand
  today: `add_thread_item` in `api/threads.rs` inserts them with role
  `library`. `app_threads.is_adding_automatic` (default false, so Manual) is in
  the schema, and no code reads it yet.
- Adding to a thread by hand already works under the old name. Dropping a
  recent row or a tab on a pinned project's row in the sidebar files it there
  (`HomePanel.svelte`, `fileIntoProject`); a tab's menu offers the projects
  (`projectMenuItems` in `utils/projectActions.ts`); a page has "Add to
  project…" (`PageToolbar.svelte`), and a chat has "Move to project".
- `app_tasks` (0083): one home at most among `thread_id`, `person_id`,
  `place_id`, `org_id` and `thing_id` (`app_tasks_one_home_check`), each a
  foreign key that clears when its subject is deleted. `app_task_links` holds
  what else a task is about, one target column per row, deleted with its
  target. `api/tasks.rs` takes and returns both (`also_about`), the task tools
  take them, and a thread's detail returns `tasks` and `tasks_also_about`. The
  web client's types do not have these fields yet. One
  `source_url`, `due_on`/`due_at`, recurrence (0078), `assignee`, and `status`
  (`open`, `active`, `done`, `dismissed`).
- Subjects: `wiki_people`, `wiki_places`, `wiki_orgs`, `wiki_things`.
- Server: `virtues-core/src/api/threads.rs` holds the thread functions, served
  at `/api/threads…` (`server/api/library.rs`) with `thread_id` and `threadId`
  JSON fields, the `thread` ref, trash and visit kinds, and `/thread/{id}`
  urls. Nothing answers at `/api/projects`. Tasks are `server/api/tasks.rs`.
- Messages and emails carry `conversation_id`, not `thread_id`, so "thread"
  means only the thing you follow.
- Web: the Threads door is `ThreadsView.svelte` at `/threads` (Tasks and
  Threads views, Active, Backlog and Finished, the preview column, rows that
  take a drop) and a thread's page is `ThreadView.svelte` at `/thread/{id}`
  (where it stands, tasks and tasks also about it, the thread as a line, and
  a side column with status, Adding, who it is with and Materials). `/tasks`
  opens the door's Tasks view. `projectStore`, `projectActions`,
  `ProjectGlyph` and the `Project*` types keep their names until the files
  that import them are free of other sessions' work (a dated TODO in
  `tabs/registry.ts`). The owner said on 2026-11-16
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
  week in small dots, Adding (Manual or Automatic), and Materials: the chats, pages and files
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

1. **Homes in the schema.** Built in 0083: the three new home columns, the
   one-home check, `app_task_links` with one foreign key per kind of target,
   and `app_threads.is_backlog`. 0082 renamed `auto_add_materials` to
   `is_adding_automatic` and the message and email `thread_id` to
   `conversation_id`.
2. **Homes in the API.** Built on the server: tasks take and return their
   home and `also_about`, a new home replaces the old one, the list filters by
   each kind of home, and a thread's detail returns its own open tasks and the
   ones also about it. Threads return and take `is_backlog` and
   `is_adding_automatic`. The `/api/threads` rename is done, with no alias.
   Left for slice 3: the web client's types and a person, place, org or
   thing page showing its tasks.
3. **The Threads door.** Parts 1 and 2 are built: the door with its Tasks
   and Threads views, the preview column, the thread page, and the task page
   at `/task/{id}`. A task row names its home and shows the icons of what it
   is also about, grouped Today, Tomorrow, Later this week, Later and No
   date; a past day stays under Today and is never marked late. Its home
   moves to a thread, thing, person, place or organization, and "Also about"
   takes threads and subjects. New task is a dialog (title, notes, home,
   due, who it is for, also about) from every place that adds one, and the
   door's Tasks view previews a task in the preview column; the inline add
   row and the row that opened in place are gone. Left: person, place and organization pages
   listing their tasks (those pages held other sessions' uncommitted work on
   2026-11-17), the Overview with the week once slice 4 exists, and Insights
   after slice 6.
   The menus already say "Add to thread…" and "Move to thread", and
   Materials and thread rows take a drop.
4. **The week.** Built: `GET /api/threads/week` (`api/thread_week.rs`) and
   the door's Overview (the next five tasks, then the active threads as dots)
   and the Threads view's Active filter. A task is one dot; spans of twelve
   hours or more are left out. Habits are not in it yet.
   One endpoint returns, per thread and per day, minutes done and
   minutes ahead, plus the day's time in no thread. The first version counts
   only what can be counted without a model: calendar events and dated tasks
   placed in a thread by the owner, and records with a duration added to a
   thread by hand. Filed records join when filing exists.
5. **Home proposals.** For a task with no home, your server proposes one. For a
   cluster of tasks under one subject, it proposes a thread with "Make it a
   thread". Both follow the evidence rule above.
6. **Notes on tasks and threads.** Built (`api/notes.rs`, migration 0084):
   two rules, a clash between calendar events filed in different threads in
   the coming week, and a task in a thread due within three days with no
   calendar time set aside for that thread before then. Notes are worked out
   on each read, so they leave once their condition clears; "Not now" is
   kept per condition. They show in the door's Insights view, in a thread's
   or task's preview, as a mark on their rows, and on home's notes line. The
   sentence is fixed wording; a model writing it is still to do.
   An insight's page is built (`InsightView.svelte`, `GET /api/notes/:key`):
   what it is about, the sentence, when it leaves, its action and "Not now",
   then "Why your server raised it" with the rule in one line and the
   evidence (the overlapping entries on a timeline, or the days up to a due
   date with nothing set aside), and how it was found in the side column.
   "Why this?" in the door's previews opens it. It also says what would
   clear the note (the calendar is read-only, so a clash names the change
   for the owner to make), its history from `app_note_events`, and "Fewer
   like this", which mutes that kind of note for its threads
   (`app_note_mutes`, migration 0086).
   The first rule needs no model to find: a
   clash between two calendar entries in different threads, or a dated task
   with no time set aside before its date. A model writes the sentence only.
   Notes reach the Insights view, the item's preview and the home notes line,
   and leave when their condition clears.
7. **Later, each behind its own plan.** Reminders by time and by arriving home
   (reminders-plan, and iOS region monitoring); moving one occurrence of a
   recurring event (google-write-access-plan); the voicemail collector; filing
   records into threads by the closed question.

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
