# Authoring applets

The contract for any agent — the in-box chat assistant, or an external tool
working in this repo — that creates or edits applets. An applet is a small
thing that runs for the user: a scheduled task, a one-off reminder, a
self-ending monitor, a tracker with its own tables, a dashboard face.

Chat-authored applets are created with the `applet` tool's `setup` action
(never by writing SQL rows). It validates first, writes `user/<name>/SKILL.md` (with
`view.html` and `tables/` when it has them),
and reconciles — a `check_failed` result lists findings; fix them and call
again, nothing was created. Re-calling with the same name **updates** that
applet: that is the edit path. So check `applet` with `list` before creating one —
a name already in use overwrites that applet instead of adding a new one.

## The fields

| Field | Language | Notes |
|---|---|---|
| `name` (req) | text | becomes the folder, lowercase words joined by `-` ("Calorie tracker" → `calorie-tracker`); the same name is the same applet. Tables live in schema `applet_<slug>`, the name with `_` (`applet_calorie_tracker`) |
| `description` (req) | one sentence | **the user reads this.** It is the line under the applet's name in the list and the headline on its page — what the applet does FOR them, in their words, not how it works. "Keeps your bank balances current", not "Sync accounts via Plaid cursor" |
| `agent` | prompt | **optional** — only for applets that DO something each run. Runs self-contained: no chat history, opens with the kickoff turn "Run your action instruction now." Omit it entirely for a face-only dashboard |
| `schedule` | 6-field cron | seconds first, **box-local timezone**. `0 0 6 * * *` = daily 6am |
| `triggers` | list | `cron` `manual` `tool` `message`. `tool` lets you start it with `run`; Run now and `message` are always there for an applet with a prompt |
| `condition` | SQL boolean | checked before each run, on local data only. False records a skipped run and calls no model |
| `until` | lifecycle | omit = forever · `"once"` = runs at the schedule's next time, then finishes |
| `schema_sql` | **one migration** | **only** schema `applet_<slug>`. First call creates; later calls submit *only what changed* — see below |
| `face_html` | complete index.html | sandboxed iframe; include `<link rel="stylesheet" href="virtues.css">` + `<script src="virtues.js"></script>`; read data with `await virtues.query(sql)` (read-only); 48KB max. A face meant to be published is the exception: see below |
| `limits` | object | protective ceilings — see below. Only enforced keys are accepted |

## A dashboard has no agent

An applet needs **either** an `agent` (something to do each run) **or** a
`face_html` (something to show). A dashboard is face-only: the face queries the
data itself, so there is nothing for a prompt to do.

Do not write a placeholder prompt that says it has nothing to do. One exists on
a real box — *"This applet is a face-only dashboard… If run, do nothing and
report that this is a display-only dashboard"* — and it means "Run now" spends
a model call to say nothing. **Omit the field.** The check accepts that; a
face-only applet with no `agent` is a complete, valid applet.

## Tables: `schema_sql` is a migration, not a schema

Each `setup` call's `schema_sql` is **one numbered, append-only
migration**, applied once and never re-run. They accumulate at
`user/<name>/tables/NNNN_*.sql`, and a fresh box replays them in order.

- **Creating**: `CREATE SCHEMA IF NOT EXISTS applet_<slug>;` then your
  `CREATE TABLE`s. This is version 1.
- **Changing**: submit **only the change** — `ALTER TABLE applet_<slug>.entries
  ADD COLUMN protein_g INTEGER;`. That becomes version 2.
- **Not changing the tables**: resubmit the identical DDL, or omit
  `schema_sql`. Identical text is recognized as already applied and nothing
  is appended or re-run.

**Why it cannot be one rewritten file.** `CREATE TABLE IF NOT EXISTS` on a
table that already exists does *nothing* — it does not add your new column,
and it does not complain. The apply succeeds, so you would believe the column
is there and write a prompt that uses it. Every `sql_write` naming it then
fails at runtime, nightly, forever. The check now catches this specific case
and hands you the exact `ALTER TABLE` to write instead; when you see that
finding, do not re-send the `CREATE` with the column added.

Row-level work (`INSERT`/`UPDATE`/`DELETE`) is not this. That happens at
runtime through `sql_write`, needs no migration, and is how a tracker records
what the user tells it.

## Applets you can talk to

Add `message` to `triggers` and the applet gets a composer on its page. What
the user types becomes the run's opening turn — the prompt sees their words
instead of the synthetic "Run your action instruction now." — and the reply is
the run's result. The exchange lives on the run, so the run log **is** the
conversation.

This is the front door for anything the user feeds rather than schedules:

- **A tracker.** `schema_sql` for the table, `message` to log into it, a face
  to see it. "I had eggs and toast" → the prompt parses it and `sql_write`s a
  row. No schedule needed at all.
- **A capture inbox.** Send it something; it files it.
- **Anything you would otherwise have declined** with "there is no way for the
  user to give it input." Check this list before declining.

Three rules:

- **List `message` and nothing else** when a message is the only thing that
  makes the applet do something. The defaults add `manual` and `tool`, and on
  a tracker those wake it with the synthetic "Run your action instruction now"
  — a model call that can only report it has nothing to do, which is the no-op
  prompt this file tells you not to write. Same reasoning as a dashboard
  having no agent.
- **A `condition` does not gate a message.** Conditions gate polls. Someone who
  just pressed send is not a poll, and a clock gate would silently swallow what
  they wrote. Same reason manual "Run now" is exempt from rate caps.
- **The composer is for input, not edits.** "I had eggs" goes to the applet;
  "make it weekly" is an edit and belongs in chat with you. Do not write a
  prompt that tries to reconfigure the applet from its own messages — it
  cannot, and the `edit` action refuses it.

Still not authorable: **Personas.** A message wake is inbound from the *user*,
not from a third party, and there is no send-as-me channel. Draft-mode stands.

## Applets the owner starts with `/name`

Set `invoke` to `["chat"]` and the applet is a prompt the owner runs by typing
`/` and its name in the composer. It runs as a turn of that chat, so its reply
lands where they asked; it is not a headless run. Add `"assistant"` and you can
start it with the `run` action too.

- `agent` is the prompt the turn runs with: what to read, what to write, and
  in what shape, addressed to the assistant.
- `tools` lists what the turn may call, and nothing else: `sql_query`,
  `semantic_search`, `web_search`, `get_project_item`, `create_page`,
  `create_task`, `update_task`, `shell`, `applet`.
- `model` is `deep` only for slow research that is worth the cost.
- `limits` takes `max_llm_cost` (per run), `max_llm_cost_per_day` and
  `timeout_s`.
- The name becomes lowercase words joined by `-`: "Garden log" is
  `/garden-log`. The result says it as `run_it_with`.

`invoke` goes with any other field: an applet can run on a schedule and be
started with `/name` too.

## Limits

Five keys, and only these five — anything else fails the check. A limit that
is stored but never read looks like protection on the gate and is not, which
is the whole reason the check rejects unknown keys instead of ignoring them.

| Key | Unit | Enforced |
|---|---|---|
| `max_llm_cost` | **dollars** (`0.25` = 25¢) | mid-run: spend is summed from the gateway's authoritative per-call cost after every model call; crossing the line stops the loop and records `budget_exceeded` |
| `max_llm_cost_per_day` | **dollars** | before the run: rolling 24h of this applet's spend |
| `max_runs_per_day` | whole runs (`max_runs` means this) | before the run, rolling 24h |
| `max_runs_per_hour` | whole runs | before the run, rolling hour |
| `timeout_s` | seconds | wall clock on the subprocess phase |

Notes that change what you should write:

- **Money is dollars, not micros.** `max_llm_cost = 1` is one dollar.
- **Manual "Run now" is exempt from the run-count caps, and only those.** Rate
  caps exist to bound automation; refusing the person who just pressed the
  button is a limit behaving as a lock. Spend ceilings bind everyone — the
  wallet does not care who pressed it.
- **Skipped runs don't count** toward `max_runs_*`. A false `condition` on a
  two-minute poll would otherwise exhaust a daily cap before lunch.
- **`budget_exceeded` is not an error.** It means a ceiling the owner set was
  reached, so it stays out of the needs-attention strip and never counts as
  the success that `until = "once"` archives on.
- **Put a spend ceiling on anything scheduled that calls a model.** An applet
  that wakes hourly and reasons each time is the shape that quietly spends;
  `max_llm_cost` is the difference between a cap and a hope.

## What the applet can do at runtime

Its prompt may rely on exactly this set — nothing else exists:

| Verb | Tool |
|---|---|
| think / plan | `think` |
| read the box's data | `sql_query` (read-only) |
| semantic recall | `semantic_search` |
| search the public web | `web_search` — **queries only; it cannot fetch a URL or feed** |
| **deliver to the user** | the run's result message posts to the chat that authored it |
| write its own tables | `sql_write` — DML inside `applet_*` schemas only (PG-enforced) |
| keep notes across runs | `update_applet_memory` |
| write durable pages | `create_page` / `edit_page` / `get_page_content` (block pages: ops on the data-ids `get_page_content` returns; `append` needs no read) |
| compute | `shell` (bash and Python in the sandbox, no network) |
| introspect applets | `applet` with `list` or `get` (read-only) |

| **be told something** | add `message` to `triggers` — the user's text becomes the run's opening turn |

**The closing rule: if the ask needs a verb not on this list — send an email,
fetch a URL, react to a message from someone else, act on another service —
decompose it into what IS possible, or decline honestly and offer the nearest
real alternative. Never write a prompt that pretends a tool exists.**

## Doctrine

- **Catalog-check-first.** Before writing any SQL or any prompt that names a
  table, confirm it exists: `sql_query` over `information_schema.tables`
  (`data_*`, `wiki_*`), then a `LIMIT 3` sample for the columns.
  **`data_*` and `wiki_*` table names in your prompt ARE now checked** — a
  name that is not on this box fails the check with a did-you-mean. Columns
  are not, and neither is anything you claim in prose, so name columns from a
  real sample rather than from memory. An imaginary column in a prompt still
  fails softly, nightly, forever.
- **Honest downgrades.** No data source → say so; offer to connect one, or a
  manual tracker (`schema_sql` + `message` so the user can log into it).
  Feed/URL watching → needs a fetch verb that doesn't exist yet: decline, or
  offer a `web_search` approximation labeled as such.
- **The gate.** Applets with a `schedule` are
  created **disabled**, and you cannot enable them — do not try. An **Enable
  card appears in the chat** right after your tool call, showing what it does,
  when it runs, what it may touch and roughly what it costs; one tap turns it
  on. So say what you built and that it is waiting for them — **do not send
  them to the applet page to find a toggle.** Everything else (manual-only,
  message-driven, face-only) crosses no boundary and is already on; say so
  rather than implying an approval that is not needed.
- **Ensure-semantics** for data-product applets (summaries, digests,
  rollups): phrase the prompt as "make sure today's X exists — check first,
  create if missing." Idempotency absorbs missed slots, retries, and races.
- **Date-anchored one-offs** ("remind me on the 25th"): a 6-field cron for
  that day and time, and `until: "once"`, which pins it to the next time the
  cron fires and finishes the applet after it runs.
- **Catch-up is automatic, and only for daily-or-slower schedules.** A box
  asleep at 7am runs the 7am applet once when it wakes, provided it is still
  the same day; a 15-minute sync that missed a tick just waits for the next
  one. At most one slot is ever caught up — a box off for a week does not
  replay seven mornings on Monday. You do not declare this; it is read from
  the schedule's shape.
- **Time-of-day gates** (`condition` on the clock) can annihilate catch-up,
  and this is the trap: a schedule of `0 0 7 * * *` with
  `condition = "extract(hour from now()) < 8"` looks careful and is not. The
  box wakes at 9:30, the catch-up fires, the gate says no, and the applet is
  skipped on exactly the day it mattered. **Widen the window rather
  than tightening the cron** — gate on the day, not the hour
  (`NOT EXISTS (a successful run today)` beats `hour < 8`).
- **Repeat-fire / cooldown.** A threshold condition ("no workout in 3 days")
  stays true for the whole lapse and fires daily. Until a cooldown field
  exists, gate on your own runs:
  `... AND NOT EXISTS (SELECT 1 FROM app_applet_runs r WHERE r.applet_id = '<id>' AND r.status = 'success' AND r.started_at > now() - interval '1 day')`
- **Two cadences = two applets** (an archiver that runs half-hourly and a
  Sunday digest are siblings, `<name>_archiver` / `<name>_digest`), each
  restating its shared definitions in its own prompt.
- **Personas** ("reply to Mom as me") are **not yet authorable**, for the
  reasons above. Decline and offer draft-mode: a scheduled applet that drafts
  replies as pages for the user to send.

## Exemplars (real folders, kept current)

- `applets/morning-examen/` — the canonical Reflect: schedule, agent and a
  spend ceiling, nothing else.
- `applets/dot-cloud/` — a face-only applet (`view.html`): WebGL
  canvas, `virtues.query` over the `data_*` ontology, graceful degradation
  (renders its star field even with zero rows). The screen's default face.

## For builtin (Rust) applet development

See [AUTHORING.md](./AUTHORING.md) — the subprocess contract, the `SKILL.md`
reference for applets that run code, and the reconcile field-ownership rules.
