# One row per folder: what "applet" stays as, and what follows

Status: decided on 2026-11-22. Nothing here is built.

People see three words: task, skill and site
([task-skill-site-plan.md](task-skill-site-plan.md)). On 2026-11-22 the owner
asked for the word "applet" to leave the database and the code, and a first
version of this plan split the one `app_applets` table into a table per
word. An audit the same day found that plan unsafe to start, and the owner
decided to keep one row per folder. This is what that decision settles, the
small work that follows from it, and what is still his to decide.

The inventory the audit made, of everything that depends on the word or on
an id, is kept in
[applet-name-dependencies.md](../record/applet-name-dependencies.md).

## What the owner decided

| Date | Decision | His words |
|---|---|---|
| 2026-11-22 | He wanted the word out of the code and the database. | "I think we should rename for sites & tasks as such and rm applets" |
| 2026-11-22 | If a folder on disk needs a word, it is "package". | "package if so yes, probably" |
| 2026-11-22 | After the audit: one row per folder, and no table per word. | Asked "Keep one row per folder and drop the split into separate tables?", he answered "Yes, one row". |

Three answers he gave earlier that day were to the first version and do not
stand as decisions now: sites and tasks as different tables, the server's
tasks in the task table, and "One cut" over a staged rollout. Each was an
answer about a split that is not happening.

## Why one row

A folder on disk has one `SKILL.md`: one name, one description, one prompt
and one trigger. The server rebuilds its row from that file on every start
(`applet_templates/mod.rs`). A site, a skill and a task made from one folder
would be three copies of one file's facts.

The design record says the same of the table as it is:
[applets.md](../record/applets.md), "a sync, a dashboard and a reminder are
one row with different fields set".

What the split would have cost, in short:

- It did not remove the derivation of what a row is. It moved it into the
  task table, as a test of which columns are filled.
- The server's syncs do not fit the to-do table. A to-do ends, has a due
  date and a place in a list. A sync never ends.
- A person would have seen one thing three times.

## What a row is, said once

The server says what a row is, in one place: `derived_kind` in
`scheduler/applets.rs`, returned as `kind` by the list, the detail and the
PATCH. A view is a site, a program is the server's task, a prompt that
starts itself is the assistant's task, and a prompt nothing starts is a
skill.

Whether a row has a view is a file on disk, so `kind` is worked out when it
is asked for and is not a stored column.

## What follows

Small pieces, none of them a rename.

1. **Delete the web's copy of the rule.** `kindOf` and `startsItself` in
   `apps/web/src/lib/applets/kind.ts` repeat the server's rule for a server
   from before `kind`. The server ships the web bundle, so the app never
   meets one. The app reads `kind` and nothing else.
2. **Merge and run what is waiting.** Pull request 15 carries the last of
   the task, skill and site work. It has not run on a real server.
3. **The simplifications the audit's experience review found**, each its own
   small change:
   - a server task opens on the task page any other task has;
   - a site with no refresh and no prompt has no Options side, and no
     switch, since the switch does nothing to its view;
   - the Skills page and the `/` menu are one list;
   - the delete dialog says what goes with the site: its prompt leaves the
     `/` menu and its refresh stops.

## Open questions

Each is for the owner, with what I would pick.

| # | Question | Pick |
|---|---|---|
| Q1 | With one row per folder, does the code's word still change from "applet" to "package"? Example: the table `app_applets` would become `app_packages`, and the folder `applets/` would become `packages/`. | No. Nobody sees the word, a table rename breaks text saved on servers, and seven things outside the server would have to change in step (the record lists them). |
| Q2 | A repeating task of the assistant's is stored twice: its row, and a row in `app_tasks` that stands for it in your list. Does the second row stay? | Yes, until it has run on a real server. It gives the task a page, a thread, notes and a history. Take it out only if it keeps producing defects. |
| Q3 | Is "Keep this answered" worth designing? On any reply you would say keep it, and get one page for that standing question: the chart on top, the written answers under it by date, and one line saying when and how it runs. Example: a Sunday sleep review whose chart and words are on one page, where today the chart is in Sites and the words are in Tasks. | Yes, as an experience to sketch first. It fits one row per folder: one folder with a view, a prompt and a schedule. |
| Q4 | Two smaller ideas from the review: handing a to-do to a skill ("Plan the Lisbon trip", with Trip planner), and giving a skill a schedule from where you ran it ("Do this every Sunday"). Build either? | The second first. It is one more way to reach what setup already makes. |

## Traps

- **A rename is larger than it looks.** The record counts about 370 files,
  and names the gates, table files and prompts saved on servers that name a
  table, a schema or an id.
- **An id is read for meaning.** Code takes the folder, the schema and
  whether it is yours out of an id's text. A change to how ids are made is a
  change to those readers.
- **`kind` cannot be a stored column** while a view is a file: the two would
  drift whenever a file is added or removed by hand.

## Not in this plan

- Any table per word, and any move of the server's work into `app_tasks`.
- A rename, unless Q1 is answered yes.
- One runner for one-time requests and scheduled work.
