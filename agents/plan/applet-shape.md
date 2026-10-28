# Applet shape

An applet is a process the box runs, described by one file. The file says
what it runs and when; the box keeps whether it is on and what happened.
Everything built on a box is one: a prompt you start from chat, a scheduled
report, a reminder, a tracker with a view, a sync, and the box's own jobs
such as the day summary.

An applet's folder is an [Agent Skills](https://agentskills.io/specification)
folder: the skill is the know-how, and the applet adds what the box needs to
run it on its own (when it starts, what it runs first, what it keeps, what it
shows, what it may spend).

This plan defines the file, what the box keeps beside it, what an applet may
do, what the assistant writes, what goes, and the order of work.

It builds on [record/applets.md](../record/applets.md), which already makes an
applet "a name plus any subset of flat fields, with behavior inferred from
which fields are set". [applets-refactor-plan.md](applets-refactor-plan.md)
still owns the screens (the list, the applet page, the chat card).
[applets-next-plan.md](applets-next-plan.md) still owns the design of data
triggers; this plan gives them a spelling.

## Decisions

### Made on 2026-10-28

The owner's direction: applets are a simple primitive, which a modest model
can write on the first try. Fewer permissions checks up front are acceptable;
a permission system per applet is not wanted.

- **An applet is a process, in the sense of a systemd unit.** One file says
  what it is. The box holds its on/off state, its memory, a sync's cursor and
  the run history, the way systemd keeps enablement and the journal apart
  from the unit file. Syncs and the box's own jobs are applets.
- **The file is the only source of truth for what an applet is.** The box
  reads it and rebuilds the applet's row from it; nothing else writes those
  fields. Every edit, from chat, the settings screen or an editor, rewrites
  the file and reloads it.
- **A change to a shipped applet is an override file**, like a systemd
  drop-in: a file in the state root that holds only the keys you changed,
  read on top of the shipped one. A shipped fix still reaches every key you
  did not change. A fork, a whole copy, stays for changing the rest.
- **An applet can do what chat can do.** The lines that stay: only `admin`
  has the root shell, deleting data asks, and an unattended run does not send
  messages or drive your Mac's browser, because nobody is there to see it.
  There is no per-applet permission system: no host approvals, no Rule of
  Two check, no lake grant, no login per applet.
- **Decided the same day:** the per-applet connection approval goes with
  the rest (an applet that lists a connection's tools gets them), and the
  unattended-sends line stays.
- **What an applet did is shown, not gated.** Every run records what it
  reached outside the box and what it made, and the run's page shows it. This
  is what the data sovereign asks for.
- **No imported code for now.** The Git import leaves the app until imports
  are wanted. When they return, they come with one consent at import ("This
  applet can read your records and reach the internet"), not a permission
  system.
- **The assistant writes prompt applets**: a prompt, when it runs, and
  optionally a table and a view. It does not write scripts; a prompt uses
  tools at run time, the sandboxed shell among them. Code applets are the
  shipped ones and any a person writes by hand.
- **Setup takes the file's own keys**, with the same names, and a short
  instruction in the tool replaces the 2,568-word guide.
- **The 2026-10-06 auto-allow decision is built**: an applet the assistant
  makes starts on, under a default limit of $1 a day shown on its card. The
  gate that leaves a scheduled applet off until the owner enables it goes.

The full reasoning for what this replaces (the Rule of Two at save, owner
host approvals, a real boundary for imported code, a database login per
applet) is in this file's history at 7d971a580.

### Still standing from 2026-10-25 and 2026-10-26

- The `/` menu is the applet picker, titled "Applets". "Skill" leaves the UI.
- An applet's frontend is a **view**; its code is "Code" and its notes are
  "Memory".
- Deep research and sudo are the shipped `deep-research` and `admin` applets.
  The composer's depth and access controls are gone.
- The folder follows the Agent Skills spec as published, with Virtues' keys
  under `metadata`, folder names in hyphens, and the displayed name derived
  from `name`.
- An applet has three optional stages: a trigger, a run and a prompt. A run
  that returns nothing skips the prompt.
- `invoke` says who may start it by hand; `trigger` says what starts it on
  its own, in plain words with no SQL. SQL lives only in `run`.
- `budget` and `timeout` are one key, `limit`. `delivers` is gone: the box
  records what each run made.
- Every applet may use the sandboxed `shell`. The unsandboxed shell is
  `sudo`, and only `admin` lists it.
- No settings key for code applets until one needs it.

## The Agent Skills spec

Anthropic published the format as an open standard on 2025-12-18
([announcement](https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills)).
It defines a folder named after the skill holding `SKILL.md` (frontmatter
with `name` and `description` required, and `license`, `compatibility`,
`metadata` and `allowed-tools` optional; the body holds the instructions),
plus optional `scripts/`, `references/` and `assets/`. A skill loads in three
stages: name and description up front, the body when it is used, other files
when the body asks for them.

The spec leaves to the host when a skill starts, schedules, state, UI and
packaging. Claude Code adds top-level keys, which its docs say are an error
when the skill is uploaded to the Claude API; OpenAI adds a sidecar file.
Virtues puts its keys under `metadata`, so every applet stays valid against
the spec.

Third-party skills carry risk: Snyk scanned 3,984 public skills in 2026-02
and found 13.4% with a critical issue
([report](https://snyk.io/blog/toxicskills-malicious-ai-agent-skills-clawhub)).
That is why imports wait for their consent.

## How other systems shape this

- **Agent Skills and Claude Code:** a skill is instructions the model loads
  when its description fits. It has no permissions of its own; it runs with
  what the host agent can do. Triggers are separate (slash commands, hooks).
- **ChatGPT scheduled tasks:** a prompt and a schedule, run with the
  assistant's ordinary tools.
- **Val Town:** one function; cron, HTTP or email is how it starts. Secrets
  are environment variables and the network is open.
- **Home Assistant:** trigger, condition and action in a file a person can
  read. This plan's three stages are the same idea.
- **systemd:** the unit file says what runs and when (`ExecStart`, timers,
  path units, `ExecCondition=` to skip quietly); a template unit runs one
  copy per instance; enablement and the journal live outside the file; a
  drop-in overrides a shipped unit's keys.

What they share: the unit is instructions or code plus how it starts, it runs
with the host's own permissions, and secrets are coarse. Fine-grained
permissions appear in multi-tenant products, not personal ones.

## What a run does

```
trigger  →  run (code or SQL)  →  prompt (AI)
```

| Stage | What it is | Cost | What it hands on |
|---|---|---|---|
| Trigger | what started it: you, the assistant, a schedule, a message in its thread, new rows, a webhook | none | its input: your text, the new rows, the payload, or nothing |
| Run | code or SQL. It reads the input, may write rows and state, and returns a result. | no model call | its result. **When the result is empty and a prompt follows, the run is recorded as skipped and the prompt does not run.** |
| Prompt | the model, with the input, the run's result, its memory and its tools | a model call | its reply, and whatever its tools made |

Every stage is optional, and every kind of applet is a subset:

| Applet | Trigger | Run | Prompt |
|---|---|---|---|
| Plaid sync | every hour, one copy per account | a shipped binary | none |
| Weekly review | you, the assistant, Sundays | none | yes |
| No workout in 3 days | every day at 09:00 | SQL that returns a row only when it is true | yes, only then |
| High heart rate | new heart-rate rows | SQL that selects the high readings | yes, only when there are some |
| Water tracker | you, the assistant, its thread | none | yes, with its own table and a view |
| A view | none | none | none: `view.html` and its tables |

## How an applet describes itself

Every key is one clause of a sentence. The chat card and the applet page
render it, so what you see is what runs:

> **Weekly review** looks back over your week. It runs when you ask and every
> Sunday at 6pm. It reads your records, writes a page, and spends at most $1 a
> run.

The run's page adds what it did: "Reached api.weather.gov 3 times. Made 1
page."

## The file

```
weekly-review/
├── SKILL.md        required: frontmatter is the wiring, the body is the prompt
├── view.html       optional: how it looks
├── tables/         optional: 0001_*.sql, the tables it keeps
├── scripts/        optional (spec): code, for shipped and hand-written applets
├── references/     optional (spec): files the prompt reads when it needs them
└── assets/         optional (spec): templates and data files
```

```markdown
---
name: weekly-review
description: Looks back over your past seven days. Use when someone asks how their week went.
allowed-tools: sql_query semantic_search create_page
metadata:
  trigger: every sunday at 18:00
  limit: $1 per run
---
Write the person a review of the seven days before today…
```

### Spec keys

| Key | Required | Values | Use |
|---|---|---|---|
| `name` | yes | 1–64 lowercase letters, digits and single hyphens, not starting or ending with a hyphen; equals the folder | the id, the folder, and the displayed name |
| `description` | yes | 1–1024 characters, saying what it does and when to use it | lists, and how the assistant decides to call it |
| `allowed-tools` | | space-separated tool ids | narrows the tools from chat's ordinary set, to keep a run focused and cheap |
| `license`, `compatibility` | | as the spec says | shown; the box does not read them |

### Virtues keys, under `metadata`

Every value is a string, as the spec requires. Most applets set one to three.

| Key | Clause | Values | Default |
|---|---|---|---|
| `invoke` | who may start it by hand | `chat`, `assistant`, both, or `none` | `chat, assistant` with a prompt, `none` without |
| `trigger` | what starts it on its own | comma-separated triggers, below | none |
| `run` | the deterministic stage | a shipped binary, a script, a SQL file or inline SQL | none |
| `source` | one copy per connected account, and the run gets that account's sign-in | a source id from the catalog, such as `plaid` | none |
| `limit` | the most it may spend and take | `$N per run`, `$N per day`, `N runs per day` or `per hour`, and a duration, comma-separated | $1 per day for an applet the assistant makes |
| `model` | which model slot | `standard`, `deep`, `lite` | `standard` |
| `enabled` | whether it starts on when first read | `true`, `false` | `true` |
| `icon`, `color` | | an `ri:` icon id; a palette name | derived |
| `writes`, `secrets` | for code: the data tables it writes, and the named keys it needs | comma-separated names | none |
| `id` | an older id to keep, which only a converted folder carries | an id differing from the folder's only in hyphens and underscores | the folder's |

When `allowed-tools` is absent the applet gets chat's ordinary tools, minus
those an unattended run does not have (`browse`, `send_imessage`) and minus
`sudo`.

### `trigger`

Plain words separated by commas, in the box's timezone.

| Family | Value |
|---|---|
| Time | `every day at 07:00`, `every weekday at 08:30`, `every monday and thursday at 09:00`, `every month on day 1 at 09:00`, `every hour`, `every 15 minutes`, `cron 0 0 7 * * *` |
| | any of those followed by ` until 2026-12-01` |
| | `on 2026-11-02 at 09:00`, which runs once and archives itself |
| Data | `new <table>`, `30 minutes before <table>`, `15 minutes after <table> ends` (slice 11) |
| Outside | `webhook`, which needs `source` |

Run now and the applet's thread are always available and are not listed.

### `run`

| Form | Example | Runs as |
|---|---|---|
| A shipped binary | `run: plaid_transactions_sync` | the box user |
| A script | `run: scripts/fetch_tides.py` | the sandbox (below) |
| A SQL file or inline SQL | `run: sql select 1 where not exists (…)` | a read-only transaction on the reader login |

Code receives the trigger's input as JSON on stdin and prints
`{"result": …, "config": …}`; the box keeps `config` as the applet's state
for its next run. SQL's result is its rows. An empty result with a prompt
after it skips the prompt.

## The file and the box

| In the file | On the box |
|---|---|
| name, description, prompt | whether it is on |
| when it runs, what it runs first, its limit and model | its memory |
| its tools, its tables, its view | a sync's state (cursor) |
| which account kind it runs as | its run history: inputs, results, what it reached and made, cost |
| | the copies per account, and the sign-ins they use |

The rules:

1. The box reads files at start and whenever one changes, and rebuilds each
   applet's row from its file. That is the only writer of those fields.
2. Every edit rewrites the file, then reloads it. The settings screen, the
   assistant and a person in an editor go through the same path.
3. A change to a shipped applet writes its override file in the state root,
   which holds only the changed keys (and the prompt, when that changed).
   The box reads the shipped file and lays the override on top. Removing the
   override returns the shipped applet.
4. The box's own record of an applet (on/off, memory, state, runs) never
   goes into the file, and the file never holds it.

One rule replaces today's three reconcile branches and the two ways to
customize a shipped applet (shipped starters, which keep your prompt edit,
and fork).

## What an applet may do

- **Tools:** chat's ordinary tools, or the narrower list in `allowed-tools`.
- **Data:** your records, as chat reads them, and its own tables, which it
  writes through the applet login.
- **The network:** prompts use chat's web tools. Code reaches the internet
  through the box's proxy, which allows any public host, refuses the box and
  its network, and records every request on the run.
- **The lake:** code reads it, read-only, and writes its own folder,
  `lake/applets/<name>/`, which has a size limit and is indexed after each
  run.
- **Lines that stay:**
  - only `admin` has `sudo`, the root shell;
  - deleting data asks, as it does in chat;
  - an unattended run has no `send_imessage` and no `browse`;
  - `dispatch_subagents` is deep research's alone, because of what it costs.

Code that did not ship runs in the sandbox (a throwaway user, no route to the
box's API, the box's secrets and data folders hidden, limits on memory, CPU,
time and file size). It costs an author nothing, and the `shell` tool uses
the same sandbox every day.

## What the assistant writes

Setup's parameters are the file's keys, with the same names:

| Parameter | Becomes |
|---|---|
| `name` | `name` and the folder (`Weekly review` → `weekly-review`) |
| `description` | `description` |
| `prompt` | the body |
| `trigger` | `metadata.trigger` |
| `run` | `metadata.run`, `sql …` only |
| `invoke` | `metadata.invoke` |
| `tools` | `allowed-tools` |
| `tables` | `tables/`: the whole schema it wants; the box adds what is missing |
| `view` | `view.html` |
| `limit`, `model` | `metadata.limit`, `metadata.model` |

Calling setup with an existing name updates that applet, and only the keys
sent change. A finding names the key and says how to fix it.

`tables` takes every `CREATE TABLE` the applet wants, every time. The box
compares it with what exists, creates missing tables, adds missing columns,
and never drops one; a column whose type differs is a finding. The assistant
does not number migrations or work out what changed.

The tool's description carries the whole instruction:

> An applet is a short file. Give it a `name` of a few words and a one-sentence
> `description` of what it does for the person and when to use it. Add a
> `prompt` if it should do something: say what to read, what to write and how
> long, in under 300 words, naming only tables you have seen with `sql_query`.
> Add a `trigger` if it should start on its own, such as `every day at 07:00`,
> `every monday at 09:00`, or `on 2026-11-02 at 09:00` for a one-off. Add
> `run: sql …` that returns a row only when it should run. Add `tables` with
> CREATE TABLE statements if it keeps its own records, and send all of them
> each time. Add a `view` for a page. Calling setup with the same name changes
> that applet. If setup returns findings, fix each one and call it again.

Dropped from setup: `guide`, `schedule`, `until`, `condition`, `triggers`,
the `limits` object and its aliases, `agent` (now `prompt`), `face_html`
(now `view`), `schema_sql` (now `tables`), and the separate `edit` action.
`check_only` stays for the CLI.

## Output and where results go

- **Kept:** every run, in the applet's thread on its page, with what it made
  and what it reached.
- **Told:** an unattended run that makes a page or text adds one Inbox item
  that links to the run. A run that only writes rows adds nothing. A failed
  run always adds one. Notifications, when built
  ([notifications-plan.md](notifications-plan.md)), point to the same item.
- **Talked to:** the same thread. A message there starts a run.
- **Answers where it was started.** From `/` or called by the assistant, it
  replies in that chat. Started by a schedule, rows or a webhook, it writes
  into no chat.
- **Runs start clean.** A scheduled run reads its memory and its tables and
  never replays a chat.

No key declares what an applet makes. Tools record what they wrote on the
run (the box already does this for pages), and code reports `wrote` beside
`result`, shown and not trusted for anything else. A prompt that ends with
nothing to say is recorded as skipped and adds no Inbox item.

Nothing here counts streaks, shows overdue alarms or nags; someone in
recovery sees every page.

## What each persona gets

- **Narrative seeker:** "every Sunday, write me how my week went", said in
  chat and written correctly by a cheap model the first time; the result is
  a page in the applet's thread.
- **Quantified self:** the same for a chart each morning, and trackers with
  their own table and view.
- **Tech prosumer:** a readable file in the open format, which they can
  share once imports return.
- **Recovery:** applets stay quiet by default: nothing counts or nags, and a
  run with nothing to say adds nothing.
- **Data sovereign:** every run shows what it reached and made. Losing the
  per-host approval means seeing where data went instead of stopping it
  first; on a box with no imported code, everything that runs was shipped,
  written by the owner, or written by the assistant at the owner's request.

## What goes

Each item is deleted in its slice, not disabled.

**The permission layers** (slice 9):
- `net(host)` entries, the owner's host approvals (`app_applets.net_hosts`,
  `PUT /api/applets/:id/hosts`, the hosts half of `AppletAllowList.svelte`)
  and the egress refused-hosts list. The proxy keeps the address checks and
  the request log.
- The `lake` grant (code reads the lake by default), the shipped-only rule
  on `source`, and the PATCH guard on `allowed_tools` and `net_hosts`.
- The planned Rule of Two check and the planned login per applet.
- The per-applet connection approval (`app_applets.connections`, its
  route and the connections half of `AppletAllowList.svelte`, built in
  261ae171e by the MCP work). An applet that lists a connection's tools
  gets them.
- The Git import in the app (`GitImportModal.svelte`, the import endpoint and
  `applet_git_import.rs`), until imports are wanted.

**The second source of truth** (slice 7):
- PATCH of anything but on/off and memory, `mirror_patch_to_file` and
  `skill::apply_patch`'s field mapping. Edits write the file.
- The three reconcile branches, `SHIPPED_STARTERS` and `agent_shipped` with
  its "a newer version exists" notice, which the override file replaces.
- The enable gate: setup's `crosses_boundary` and `re_gate`, the edit
  action's refusal to enable an assistant-made applet, and the card's Enable
  step.
- The `condition` column and its gate, after a migration moves each value
  into `run_sql`. `until` stays, because `every … until <date>` uses it.

**The old vocabulary** (slice 8):
- Setup's parameters listed above, the `edit` action, and most of
  `applets/AGENTS.md`, which shrinks to the instruction above plus a page of
  examples.
- `skill_md_as_setup_args`: `virtues applet put` sends the folder's
  `SKILL.md` instead of converting it into setup arguments.
- The manifest structs move into `convert.rs`, the one reader of
  `manifest.toml` left, and the catalog's `Template` becomes a plain struct.

**Kept for upgrading boxes:** `convert.rs`, `metadata.id`, and the move of
saved skills, each until every box has run it.

## Order of work

Each slice lands on its own and leaves the box working.

### Built on 2026-10-27 and 2026-10-28

None of this has run on a real box.

- **0. The bugs confirmed on 2026-10-25:** view and model SQL on their own
  logins, sync cursors in their own column, exact folder lookup, edits that
  write the file, schedules that can fire, and cron jobs that follow a DST
  change (with a slot claimed once, so the repeated hour does not run a job
  twice).
- **1. The SKILL.md reader.**
- **2. The sandbox and the proxy**, with `shell` and `sudo` as two tools.
  Slice 9 removes the parts that 2026-10-28 dropped.
- **3. Skills are applets**, started with `/name`.
- **4. Every manifest converts to SKILL.md**, with `run: sql` and the skipped
  run. A conversion is journaled and keeps the folder's files and its id.
- **5. Chat modes are the `admin` and `deep-research` applets.** The owner
  removed Local from the app; the server's Local mode stays behind a dated
  TODO.
- **6. The assistant loads applets** with `use_applet`, which also reads an
  applet's `references/` and `assets/`. The four shipped chat applets are
  `invoke: chat, assistant`.
- **The 2026-10-27 audit's fixes and the 2026-10-28 sweep:** commits
  7d971a580 through 0bd4f4ff3.

### 7. One source of truth

Edits write the file; PATCH takes only on/off and memory. Override files for
shipped applets replace shipped starters. One reconcile rule. The enable gate
goes, and an assistant-made applet starts on under its limit. A migration
moves `condition` into `run_sql` and drops the column (claim the number with
`make migration`).

**Done when:** changing a shipped sync's schedule writes an override and
survives a restart, removing the override returns the shipped schedule, and
no code path writes a behavior field to a row.

Built on 2026-10-28 except on a box. Decided on the way:
- An override lives at `<state root>/overrides/<name>.md` and may change
  `description`, `trigger`, `limit`, `model`, `invoke` and the prompt. An
  empty value removes a key. Anything else is refused, which is what keeps an
  override from changing what a shipped applet runs or may use.
- PATCH writes `enabled` and `memory` to the row and everything else through
  the file, then reconciles. `reset` removes an override. A shipped applet
  takes `agent` now, so the settings screen edits any shipped prompt.
- The three shipped starters' edited prompts become overrides once, at boot,
  from `agent` and `agent_shipped`. A retimed shipped schedule is not carried
  over, because the row cannot tell it from one an older release shipped.
  `agent_shipped` stays until every box has run that step.
- Migration 0072 moves a row's `condition` into `run_sql` and empties the
  column, which stays so `virtues rollback` still works. A later migration
  drops both columns.
- The chat card is a receipt with Turn off, and `applets/AGENTS.md` says an
  applet starts on.

### 8. Setup takes the file's keys

The setup parameters above, the instruction in the tool's description,
`tables` as the whole schema, and `edit` folded into setup. `applets/AGENTS.md`
shrinks; `virtues applet put`, the MCP `applet` tool and the shipped
`new-applet` applet move to the new keys.

**Done when:** the Lite slot, given only the tool's description, makes a
water tracker with a table and a view, a weekly review, and a one-off
reminder, each in one call with no findings.

### 9. Permissions collapse

The deletions under "The permission layers". The proxy allows any public
host and keeps its log, and the run's page shows the hosts reached.

**Done when:** a hand-written script applet reaches a public host with no
approval, cannot reach the box or the LAN, and its run shows the host.

### 10. Output, threads and the Inbox

Every run lands in its applet's thread; nothing posts into a chat it was not
started from; failing applets and runs that made a page reach the Inbox,
which [api/attention.rs](../../virtues-core/src/api/attention.rs) does not
cover today. Code's `wrote` is shown on the run.

**Done when:** a scheduled `weekly-review` adds one Inbox item linking to its
thread, a failed sync adds one, and a `water` run shows the rows it wrote.

### 11. Data triggers

`trigger: new <table>` with `new_rows` for `run`, a `(created_at, id)`
watermark per applet and table, batching, no waking on the applet's own rows
or a source's first backfill, and rate limits on by default, per
[applets-next-plan.md](applets-next-plan.md). `before` and `after … ends`
follow.

### Later: imports and export

Import from a git URL or an archive with one consent at import, and export a
folder as a zip. Skills written for other agents map their tool names
(`WebSearch` to `web_search`, `Bash` to `shell`) and Claude Code's top-level
keys into `invoke`.

## Old to new

The converter's mapping, for boxes upgrading from `manifest.toml`:

| Old | New |
|---|---|
| `agent = """…"""` | the body |
| `command` | `metadata.run`; a script moves under `scripts/` |
| `triggers` and `schedule` | `metadata.trigger` |
| `until = "once"` | `trigger: on <date> at <time>` |
| `condition` with no command | `run: sql select 1 where (…)` |
| `tool` in `triggers` | `invoke: assistant` |
| `default_enabled` | `metadata.enabled` |
| `source = { id }` | `metadata.source` |
| `[config.limits]` | `metadata.limit` |
| `model_slot` | `metadata.model`; an unattended prompt keeps `lite` |
| `face/index.html` | `view.html` |
| `schema/` | `tables/` |
| `forked_from` | `metadata.forked-from` |
| an id with hyphens | `metadata.id` |

## Open decisions

### How many rows `run: sql` hands to the prompt

Proposed: the first 200 rows as a table, then a line with the total count.

## Not in this plan

- Chat's own leak paths: `browser_open` can open any URL through the owner's
  signed-in browser and `send_imessage` can send to any address with a
  5-second undo. They need their own pass.
- Tasks, Local chat, Interview and Getting started.

When this ships, delete this plan, move the shape into
[record/applets.md](../record/applets.md), and write the manual page.
