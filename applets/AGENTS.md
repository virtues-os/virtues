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
| `description` (req) | one sentence | the user's intent — the applet's headline |
| `agent` (req) | prompt | runs self-contained: no chat history, opens with the kickoff turn "Run your action instruction now." |
| `schedule` | 6-field cron | seconds first, **box-local timezone**. `0 0 6 * * *` = daily 6am |
| `triggers` | list | `cron` `manual` `tool` `api` `webhook`; defaults follow `schedule` |
| `condition` | SQL boolean | run gate — local data only, never network |
| `until` | lifecycle | omit = forever · `"once"` = archive after first success · SQL boolean = archive when true after a success |
| `schema_sql` | idempotent DDL | **only** schema `applet_<slug>`; start with `CREATE SCHEMA IF NOT EXISTS applet_<slug>;` |
| `face_html` | complete index.html | sandboxed iframe; include `<link rel="stylesheet" href="virtues.css">` + `<script src="virtues.js"></script>`; read data with `await virtues.query(sql)` (read-only); 48KB max |
| `limits` | object | `{max_llm_cost, timeout, max_runs}` — protective defaults, user-editable |

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

**The closing rule: if the ask needs a verb not on this list — send an email,
fetch a URL, react to an incoming message, act on another service — decompose
it into what IS possible, or decline honestly and offer the nearest real
alternative. Never write a prompt that pretends a tool exists.**

## Doctrine

- **Catalog-check-first.** Before writing any SQL or any prompt that names a
  table, confirm it exists: `sql_query` over `information_schema.tables`
  (`data_*`, `wiki_*`), then a `LIMIT 3` sample for the columns. The check
  validates `condition`/`until`/`schema_sql` mechanically — **prose is not
  machine-checked; you are the check.** An imaginary table in a prompt fails
  softly, nightly, forever.
- **Honest downgrades.** No data source → say so; offer to connect one, or a
  manual tracker (`schema_sql` + the user logging via chat with `sql_write`).
  Feed/URL watching → needs a fetch verb that doesn't exist yet: decline, or
  offer a `web_search` approximation labeled as such.
- **The gate.** Applets with a `schedule` or `api`/`webhook` trigger are
  created **disabled**. Tell the user to review and enable on the applet
  page. You cannot enable them — do not try.
- **Ensure-semantics** for data-product applets (summaries, digests,
  rollups): phrase the prompt as "make sure today's X exists — check first,
  create if missing." Idempotency absorbs missed slots, retries, and races.
- **Date-anchored one-offs** ("remind me on the 25th"): nearest future
  occurrence, 6-field cron, and `until = "once"` — mandatory, or it fires
  yearly forever.
- **Time-of-day gates** (`condition` on the clock): remember the schedule
  and the gate must agree — a box that wakes late can miss a narrow window.
  Widen the window rather than tightening the cron.
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
- `applets/hello_world/` — a face (`face/index.html`): virtues.css theming,
  `virtues.query`, graceful degradation when tables are absent.

## For builtin (Rust) applet development

See [AUTHORING.md](./AUTHORING.md) — the subprocess contract, manifest
reference for `command` applets, and the reconcile field-ownership rules.
