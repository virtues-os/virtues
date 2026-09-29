# Applets

**Written 2026-09-29.** One primitive for everything that runs on the box's
behalf, and a chat door through which a model can author one without ever
being able to switch it on. Supersedes `agents/plan/applets-overhaul-plan.md`
and `agents/plan/applet-authoring-plan.md` (both deleted); what is still
unbuilt is in [applets-next-plan.md](../plan/applets-next-plan.md). The
contract a model reads is `applets/AGENTS.md`; the builtin guide is
`applets/AUTHORING.md`.

## The model

**A user-space systemd with an AI author.** A sync, a daemon, a dashboard, a
reminder and an AI job are the same `app_applets` row with different fields
set. The model compiles chat intent into flat, individually validated values
(a prompt, a cron, a SQL condition, an HTML face), and trusted code
interprets them. It is not a coding agent. The comp is systemd, whose unit
file *is* flat fields; IFTTT and Shortcuts show personal automation is config.

**Shape is derived, never declared.** `command` makes a subprocess, `agent`
an agent loop, a face alone a view. A declared `runtime` field existed and
was removed: a manifest could declare one thing while its fields meant
another, and the declaration won. **Archetypes are recipes, not types**
(Reflect = schedule + agent; Tracker = tables + face + `message`; View = face
only). They live in AGENTS.md, never in the schema.

**Definition versus state.** What changes when it *runs* lives in Postgres
(memory, rows, runs, `enabled`); what changes when it is *edited* lives in the
folder (prompt, schedule seed, schema migrations). Reconcile is the one-way
apply from folder to row, and the two share no field they could disagree on.
Run state on disk was rejected: it would churn a git lane on every run and
split the "the database is the backup" story. An authored applet is per-box
state, in the state root under `user/<slug>/` with id `applet_user__<slug>`,
never in the shipped `applets/` tree the installer replaces.

### Wake, gate, life

| Axis | Question | Semantics |
|---|---|---|
| **Wake** | what causes a run attempt | cron · manual · tool · api · webhook · message. A closed set. |
| **Gate** | does the attempt proceed | none, or a SQL `condition` over **local state only, never network I/O**. A gate that fetches is a run. Fuzzy judgment belongs in the prompt. |
| **Life** | when is it done | one nullable `until`: absent = forever · `"once"` = after first success · SQL boolean = archive when true after a success. |

Trigger is who wakes you; condition is what you check once awake. Every
applet is a singleton: a wake during a live run records a `skipped` run. A
falsy condition skips *without* a run row, deliberately, or a two-minute poll
buries the history. A `message` bypasses the condition, because a condition
guards scheduled work against repeating and a message is new input.

Catch-up is read from the schedule's shape: daily-or-slower schedules catch up
once, same day only; frequent ones wait for the next tick. **Trap:** a
clock-time condition annihilates catch-up (a 7am applet gated `hour < 8` is
skipped on the morning the box woke at 9:30). Gate on the day, not the hour.

## The invariant

> **No path exists from model output to an enabled, scheduled row without a
> user-surface action.**

The first design broke it three ways: self-enabling edits, check-failed drafts
promoted by reconcile, and deleted applets resurrected from their folders:

- `setup_applet` checks **before anything touches disk**; a failed check
  creates nothing. Re-calling with the same name is the edit path; another
  name that collapses to the same slug gets a suffix, never an overwrite.
- **Gate predicate:** a `schedule` or an `api`/`webhook` trigger materializes
  with `default_enabled = false`. Manual, message-driven and face-only applets
  start enabled: storage on your own box is not a boundary, and a gate on a
  free read-only view is habituation.
- `edit_applet` refuses `enabled: true` on an ai-owned row and force-disables
  one that gains a boundary while enabled. Re-authoring re-gates the same way.
- The gate is an Enable card in the chat; `enabled` is mirrored into the
  manifest so a rebuilt database keeps the choice. `delete_applet` removes the
  folder. Reconcile's `ai` branch never touches `enabled` or `memory`.

## What an applet may do at runtime

An applet run gets an **explicit allowlist**. The first design gave runtime
agents every tool, including the one that creates applets, so an ai-owned
applet could mint scheduled applets past every boundary.

| Verb | Mechanism |
|---|---|
| think, read, recall, search | `think`, `sql_query` (read-only), `semantic_search`, `web_search` (queries only; it cannot fetch a URL) |
| deliver to the owner | the run's result posts to the chat that authored it |
| write its own tables | `sql_write` as `virtues_applet_writer`: DML inside `applet_*` schemas only |
| notes, pages, compute | `update_applet_memory`; `create_page` / `edit_page` / `get_page_content`; `code_interpreter` (jailed) |
| introspect | `list_applets` / `get_applet`, read-only |
| be told something | the `message` wake |

**Closing rule:** a verb not in the table means decompose or decline, never a
prompt that pretends a tool exists.

**Faces** are `face/index.html` in a `sandbox="allow-scripts"` iframe: opaque
origin, strict CSP, and `virtues.query(sql)` as the default-deny
`virtues_face_reader` role behind a short-lived per-applet token. Svelte is
the app; iframe HTML is the applets; the boundary is trust.

**Tables** are one Postgres schema per applet, `applet_<slug>`, so they join
against `data_*`. `schema_sql` is **a numbered, append-only migration**, not a
rewritten file: `CREATE TABLE IF NOT EXISTS` on an existing table silently
adds nothing, so the model would believe a column existed and fail nightly.
The check detects that drift and hands back the `ALTER`.

**The check is the LSP.** The reader of an error is a model in a retry loop,
so every finding names its fix: SQL EXPLAINed read-only under a timeout, DDL
dry-run in a rolled-back transaction, did-you-mean against the live catalog.
"Prompt prose cannot be checked" was **half refuted**: a table name is a
token, so `data_*`/`wiki_*` names in a prompt are checked. Columns are not.

## Limits

**A limit nobody enforces is worse than none**: it reads as protection on the
gate. `timeout` was advertised for months while only `timeout_s` was read, so
the check now rejects any key the runner does not read. Five exist:
`max_llm_cost` (per run, dollars, checked after every model call),
`max_llm_cost_per_day`, `max_runs_per_hour`, `max_runs_per_day`, `timeout_s`.

- Spend is enforced from the gateway's per-call cost, never by a helper code
  could bypass. A crossed ceiling is `budget_exceeded`: not an error, and not
  the success that `until = "once"` archives on.
- "Run now" is exempt from run-count caps, not from spend. Rate caps bound
  automation; refusing the person who pressed the button is a lock.
- Every limit is user-editable. Auto-pause on inactivity was rejected
  (undefined for a headless sync; nothing the owner turned on turns itself
  off), and count caps were demoted to surfaced information.

## Rejected

- **A tagged-enum `spec`.** A seven-arm union is a bespoke DSL, and models are
  fluent in lingua francas and mediocre at bespoke schemas. Flat fields won.
- **A sandboxed-Python hatch.** A foreign runtime for a gap that does not
  exist: predicates go to SQL, judgment to the prompt, code to Rust.
- **SQLite in the folder.** State on disk, a second engine, a data island.
- **Per-type colors in the list.** Shape is derived; nothing honest to color by.
- **Chaining** (`parent_run_id`). Planned as "wire the dead chaining", instead
  dropped in 0011: never written across 190k real runs. If composition
  returns, it returns whole.
- **Reply-to-iterate threads.** Reply-as-input and reply-as-edit collide. The
  `message` wake survived: an applet's composer is for input; edits are chat.

## Appendix: decision log

- **Name: Applet** (2026-07-19), chosen for being unembarrassing in the nav:
  engineers name the mechanism, contemplatives the meaning, laypeople the
  instance, and no word wins all three. The rename reached the code
  (`app_applets`, `app_applet_runs`, `applet_runner`) on 2026-07-29.
- **`description` is the intent source**, the sentence the gate blesses.
  One-way: recompiling fields from an edited sentence was deferred because a
  nondeterministic recompile clobbers hand-tuned fields.
- **Chat is the front door**, no separate builder. **Ensure-semantics**
  ("make sure today's X exists") lets idempotency absorb missed slots and races.
- **`memory` kept** beside the `applet_*` tables; **limits merged into
  `config`**; user and ai applets are **concrete** (one row, one optional
  credential), fan-out stays a system-sync mechanism.
- **`morning_examen` demoted from Rust to a manifest**: the flagship fits in
  flat fields. **Git: fork-on-edit** (`forked_from`); git URLs, no registry.
- **Interrupt at four boundaries only:** granting a credential, enabling a
  schedule or trigger, side effects that send or spend, promoting sandboxed
  code to trusted. Capabilities are derived for display, declared only by
  opaque binaries, for enforcement.
