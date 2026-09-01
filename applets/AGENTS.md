# Authoring applets

The contract for any agent — the in-box chat assistant, or an external tool
working in this repo — that creates or edits applets. An applet is a small
thing that runs for the user: a scheduled task, a one-off reminder, a
self-ending monitor, a tracker with its own tables, a dashboard face.

Chat-authored applets are created with the `setup_applet` tool (never by
writing SQL rows). It validates first, materializes `applets/user/<slug>/`,
and reconciles — a `check_failed` result lists findings; fix them and call
again, nothing was created. Re-calling with the same name **updates** that
applet: that is the edit path.

## The fields

| Field | Language | Notes |
|---|---|---|
| `name` (req) | text | slug = lowercased name with `_` ("Calorie Tracker" → `calorie_tracker`); tables live in schema `applet_<slug>` |
| `description` (req) | one sentence | **the user reads this.** It is the line under the applet's name in the list and the headline on its page — what the applet does FOR them, in their words, not how it works. "Keeps your bank balances current", not "Sync accounts via Plaid cursor" |
| `agent` | prompt | **optional** — only for applets that DO something each run. Runs self-contained: no chat history, opens with the kickoff turn "Run your action instruction now." Omit it entirely for a face-only dashboard |
| `schedule` | 6-field cron | seconds first, **box-local timezone**. `0 0 6 * * *` = daily 6am |
| `triggers` | list | `cron` `manual` `tool` `api` `webhook` `message`; defaults follow `schedule`. Add `message` to let the user talk to it |
| `condition` | SQL boolean | run gate — local data only, never network |
| `until` | lifecycle | omit = forever · `"once"` = archive after first success · SQL boolean = archive when true after a success |
| `schema_sql` | **one migration** | **only** schema `applet_<slug>`. First call creates; later calls submit *only what changed* — see below |
| `face_html` | complete index.html | sandboxed iframe; include `<link rel="stylesheet" href="virtues.css">` + `<script src="virtues.js"></script>`; read data with `await virtues.query(sql)` (read-only); 48KB max |
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

Each `setup_applet` call's `schema_sql` is **one numbered, append-only
migration**, applied once and never re-run. They accumulate at
`applets/<slug>/schema/NNNN_*.sql`, and a fresh box replays them in order.

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
  cannot, and `edit_applet` refuses it.

Still not authorable: **Personas.** A message wake is inbound from the *user*,
not from a third party, and there is no send-as-me channel. Draft-mode stands.

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
- **Skipped runs don't count** toward `max_runs_*`. A falsy `condition` on a
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
| write durable pages | `create_page` / `edit_page` / `get_page_content` |
| compute | `code_interpreter` (jailed, no network) |
| introspect applets | `list_applets` / `get_applet` (read-only) |

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
- **The gate.** Applets with a `schedule` or `api`/`webhook` trigger are
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
- **Date-anchored one-offs** ("remind me on the 25th"): nearest future
  occurrence, 6-field cron, and `until = "once"` — mandatory, or it fires
  yearly forever.
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
  silently skipped on exactly the day it mattered. **Widen the window rather
  than tightening the cron** — gate on the day, not the hour
  (`NOT EXISTS (a successful run today)` beats `hour < 8`).
- **Repeat-fire / cooldown.** A threshold condition ("no workout in 3 days")
  stays true for the whole lapse and fires daily. Until a cooldown field
  exists, gate on your own runs:
  `... AND NOT EXISTS (SELECT 1 FROM app_applet_runs r WHERE r.action_id = '<id>' AND r.status = 'success' AND r.started_at > now() - interval '1 day')`
- **Two cadences = two applets** (an archiver that runs half-hourly and a
  Sunday digest are siblings, `<name>_archiver` / `<name>_digest`), each
  restating its shared definitions in its own prompt.
- **Personas** ("reply to Mom as me") are **not yet authorable** — no inbound
  message wake, no send-as-me channel. Decline and offer draft-mode: a
  scheduled applet that drafts replies as pages for the user to send.

## Exemplars (real folders, kept current)

- `applets/morning_examen/` — the canonical Reflect: schedule + agent, four
  flat fields, nothing else.
- `applets/dot_cloud/` — a face-only applet (`face/index.html`): WebGL
  canvas, `virtues.query` over the `data_*` ontology, graceful degradation
  (renders its star field even with zero rows). The screen's default face.

## For builtin (Rust) applet development

See [AUTHORING.md](./AUTHORING.md) — the subprocess contract, manifest
reference for `command` applets, and the reconcile field-ownership rules.
