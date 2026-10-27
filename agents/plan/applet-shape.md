# Applet shape

The applet becomes the one container for everything built on a box: a prompt
you start from chat, a sync, a tracker with a view, a scheduled report, a
reminder. Skills and chat modes fold into it. An applet's folder is an
[Agent Skills](https://agentskills.io/specification) folder: the skill is the
know-how, and the applet adds what the box needs to run it on its own (when it
starts, what it may reach, what it keeps, what it shows, what it may spend).

This plan defines the folder, every key, what the box works out without a
key, the sandbox and network rules, how outside skills are imported, the
checks, and the order of work.

It builds on [record/applets.md](../record/applets.md), which already makes an
applet "a name plus any subset of flat fields, with behavior inferred from
which fields are set". Skills (2026-09-21) and chat modes were built beside
that container instead of inside it. This plan moves them in.

[applets-refactor-plan.md](applets-refactor-plan.md) still owns the screens
(the list, the applet page, the chat card) and its slice 0 bug fixes, which
come first here too. [applets-next-plan.md](applets-next-plan.md) still owns
the design of data triggers; this plan gives them a spelling.

## Layers

| Layer | What is in it |
|---|---|
| Kernel | Tools, your data, credentials, the source catalog, models, the agent loop, the sandbox and the network proxy, permission enforcement, box upkeep |
| Applets | Everything that runs: deep research, weekly review, a Plaid sync, morning examen, suggested questions |
| Surfaces | Chat, pages, the wiki, projects, tasks, the Inbox, the display |

The kernel stays Rust and stays small. Surfaces are where applets show up.
Tasks stay a separate surface for the owner's own to-dos, and their shape is
decided later.

## Decisions made on 2026-10-25

- The applet is the universal container. A skill is an applet's prompt.
- The composer's `/` menu becomes the applet picker and is titled "Applets".
  "Create new skill" becomes "New applet". "Skill" leaves the UI.
- The applet's frontend is called a **view** everywhere. "Face", "dashboard"
  and "live view" go.
- An applet's code is "Code", its notes are "Memory", and "Needs you" means
  the Inbox only.
- Deep research becomes an applet in `/`, and the depth control is removed.
- The model can call applets itself. `/` is how you pick one yourself.
- Nothing asks before running. The check before deleting data stays on
  `sudo`, because that was a separate decision.
- Interview and Getting started stay kernel code for now.
- Local chat is experimental and out of scope.
- Database roles follow Postgres best practice (see Sandbox and network).

## Decisions made on 2026-10-26

- The applet folder follows the Agent Skills spec as published. Virtues' keys
  go under the spec's `metadata` map.
- `title` goes: the displayed name is derived from `name`.
- Sudo becomes the shipped `admin` applet, and the composer's access control
  goes.
- Folder names use hyphens, as the spec requires.
- The SQL forms of `condition` and `until` are removed. `run` takes their place
  (see What replaces `condition` and `until`).
- An applet has three optional stages: a trigger, a run and a prompt. A run can
  be code or SQL, and when it returns nothing and a prompt follows, the prompt
  is skipped. This is the deterministic check before any model call.
- `when` splits into `invoke` (who may start it by hand) and `trigger` (what
  starts it on its own). Trigger values are plain words with no SQL in them;
  SQL lives only in `run`, so the `where` filter goes.
- `budget` and `timeout` merge into `limit`.
- `delivers` goes, and no `output` key replaces it. The box records what each
  run made.
- `icon` and `color` stay as optional keys. `color` takes a palette name, never
  a hex value.
- Every applet may have a sandboxed `shell`. The unsandboxed shell becomes a
  separate tool, `sudo`, that only shipped applets may list.
- Network is off by default. An applet reaches the hosts it lists as
  `net(host)` entries in `allowed-tools`, through a proxy on the box. The spec's
  permission list holds every permission, and only the owner can add a host to
  an applet that reads their data.
- Code that did not ship with the box runs inside a real boundary: its own
  user, its own database login, and no route to the box's API.
- No settings key for code applets until one needs it. No code applet reads a
  setting today: every key the shipped code applets read from `config` is a
  sync cursor, which this plan moves to the row's state.

## The Agent Skills spec

Anthropic launched Skills on 2025-10-16 and published the format as an open
standard on 2025-12-18
([announcement](https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills)).
The spec has no version numbers, and its only changes since launch are
clarifications. About 46 products read it, including Claude, ChatGPT and
Codex, GitHub Copilot, VS Code, Cursor, Gemini CLI, Windsurf and Zed.

What it defines:

- A folder named after the skill, holding `SKILL.md`, and optionally
  `scripts/`, `references/`, `assets/` and any other files.
- Frontmatter: `name` and `description` are required; `license`,
  `compatibility`, `metadata` (a map of string keys to string values) and
  `allowed-tools` (experimental) are optional.
- The body holds the instructions.
- Loading in three stages: name and description at startup, the body when
  the skill is used, other files when the body asks for them.

What it leaves to the host: when a skill starts (most clients let the model
decide), schedules, state, UI, packaging and signing. No implementation keeps
schedules, triggers or state in `SKILL.md`. Vendors that extend it do so in two
ways. Claude Code adds top-level keys (`user-invocable`,
`disable-model-invocation`, `model`, `effort`, `arguments`), and its docs say
those keys are a hard error when the skill is uploaded to claude.ai or the API.
OpenAI adds a sidecar file, `agents/openai.yaml`. Virtues uses `metadata`,
which keeps every applet valid against the spec and accepted by the Claude
API.

Known problems the design accounts for:

- Third-party skills carry risk. Snyk scanned 3,984 public skills in 2026-02:
  13.4% had a critical issue and 76 were confirmed malicious, nearly all using
  prompt injection
  ([report](https://snyk.io/blog/toxicskills-malicious-ai-agent-skills-clawhub)).
- The spec has no versioning, signing or dependencies. Proposals for each are
  open and not accepted.
- Model-chosen activation sometimes fails to fire, so `chat` stays the
  reliable way to start an applet.

## How an applet describes itself

Every key is one clause of a sentence. The creation card and the applet page
render that sentence, so the thing you approve is the thing that runs:

> **Weekly review** looks back over your week. It runs when you ask and every
> Sunday at 6pm. It reads your records, writes a page, and spends at most $1 a
> run.

When an applet can reach the network, the sentence says so: "It can reach
api.weather.gov." When it reads your records and can reach the network, it
says both in one clause.

## What a run does

```
trigger  →  run (code or SQL)  →  prompt (AI)
```

| Stage | What it is | Cost | What it hands on |
|---|---|---|---|
| Trigger | what started it: you, the assistant, a schedule, new rows, a webhook | none | its input: your text, the new rows, the payload, or nothing |
| Run | code or SQL. It reads the input, may write rows and state, and returns a result. | no model call | its result. **When the result is empty and a prompt follows, the run is recorded as skipped and the prompt does not run.** |
| Prompt | the model, with the input, the run's result, its memory and its tools | a model call | its reply, and whatever its tools made |

Every stage is optional, and every kind of applet is a subset of the three:

| Applet | Trigger | Run | Prompt |
|---|---|---|---|
| Plaid sync | every hour | a shipped binary | none |
| Weekly review | you, the assistant, Sundays | none | yes |
| High heart rate | new heart-rate rows | SQL that selects the high readings | yes, only when there are some |
| No workout in 3 days | every day at 09:00 | SQL that returns a row only when it is true | yes, only then |
| Water tracker | you, the assistant | none | yes, with its own table and a view |
| A view | none | none | none: `view.html` and its tables |

This is the shape of systemd's `ExecCondition=` (a command that runs first and
skips the unit without marking it failed), Home Assistant's trigger, condition
and action, and GitHub Actions' `on`, `if` and steps. The difference is that the
run also prepares the input: the SQL that decides there is something to do
returns the rows the prompt needs, which saves the prompt its first lookups.

## The folder

```
weekly-review/
├── SKILL.md        required: frontmatter is the wiring, the body is the prompt
├── view.html       optional: how it looks
├── tables/         optional: 0001_*.sql, the tables it keeps
├── scripts/        optional (spec): code the applet's shell or the box runs
├── references/     optional (spec): files the prompt reads when it needs them
└── assets/         optional (spec): templates and data files
```

`view.html` and `tables/` are Virtues' own. The spec allows any other files,
so the folder stays valid.

## SKILL.md

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
| `allowed-tools` | | space-separated tool ids and `net(host)` entries | every permission the applet has. In the spec it means "pre-approved"; since nothing asks in Virtues, pre-approved and allowed are the same list. |
| `license` | | a license name or a bundled file | kept for imported and shared applets |
| `compatibility` | | free text, ≤500 characters | shown to people; the box does not read it |

The displayed name is `name` with hyphens as spaces and the first letter
capitalized. A short list of known spellings fixes brand names (iOS, GitHub,
Gmail).

### Virtues keys, under `metadata`

Every value is a string, as the spec requires.

Every key is one idea, no two keys overlap, and each is one clause of the
applet's sentence. Most applets set one to three of them.

| Key | Clause | Values | Default |
|---|---|---|---|
| `invoke` | who may start it by hand | `chat`, `assistant`, both separated by a comma, or `none` | `chat, assistant` with a prompt, `none` without |
| `trigger` | what starts it on its own | comma-separated triggers, below | none |
| `run` | the deterministic stage | a shipped binary, a script, a SQL file or inline SQL, below | none |
| `source` | whose account it uses, one copy per account | a source id from the catalog, such as `plaid` | none |
| `limit` | the most it may spend and take | `$N per run`, `$N per day` and a duration (`10 minutes`, `2 hours`), comma-separated, in any combination | $1 per day for an applet the assistant makes; 5 minutes for code, 8 for a prompt |
| `model` | how hard it thinks | `standard`, `deep` | `standard` |
| `enabled` | whether it starts on | `true`, `false` | `true` |
| `icon` | | an `ri:` icon id | the source's logo for a sync, otherwise a glyph for its kind |
| `color` | | a palette name: `gray`, `red`, `orange`, `yellow`, `green`, `blue`, `purple`, `pink` | derived from `name`, so it stays stable |

`color` is a name so it follows the theme in light and dark; a hex value is
refused. When `allowed-tools` is absent the applet gets the ordinary chat
tools, minus the tools that leave the box (`browse`, `send_imessage`), minus
`sudo`, and with no `net(…)` entries.

### `invoke`

| Value | Input the applet receives |
|---|---|
| `chat` | the text typed after `/name` in the composer |
| `assistant` | the request the assistant writes when it calls the applet |
| `none` | the applet starts only from its triggers, its thread and Run now |

The applet's thread and Run now on its page are always available and are not
listed.

### `trigger`

Triggers are plain words separated by commas. None of them holds SQL, so a
comma always separates two triggers.

| Family | Value | Input the applet receives |
|---|---|---|
| Time | `every day at 07:00`, `every weekday at 08:30`, `every monday at 09:00`, `every monday and thursday at 09:00`, `every month on day 1 at 09:00`, `every hour`, `every 15 minutes`, `every 2 hours` | none |
| | any schedule above followed by ` until 2026-12-01` | none |
| | `on 2026-11-02 at 09:00` | none; archives itself after the run |
| | `cron 0 0 7 * * *` (six fields) | none |
| Data | `new <table>` | the new rows |
| | `30 minutes before <table>` | the row whose `started_at` is coming up |
| | `15 minutes after <table> ends` | the row whose `ended_at` has passed |
| Outside | `webhook` (needs `source`) | the posted payload and the sending device |

Times are in the box's timezone. A schedule and the trigger that fires it are
one value, so an applet with a schedule and no `cron` trigger, which setup
accepts today and the scheduler never registers, cannot be written.

Data tables name a span `started_at` and `ended_at`
(`data_calendar_event`, `data_audio_session`), so `before` and `after … ends`
work on any table that has those columns.

### `run`

| Form | Example | Runs as |
|---|---|---|
| A shipped binary | `run: plaid_transactions_sync` | the box user, because it shipped |
| A script | `run: scripts/fetch_tides.py` | the sandbox |
| A SQL file | `run: scripts/high_readings.sql` | a read-only transaction on the applet's own login (the shared applet login until slice 9) |
| Inline SQL | `run: sql select * from new_rows where bpm > 100` | the same |

- **Input.** Code receives the trigger's input as JSON on stdin, as the runner
  passes it today. SQL sees a data trigger's rows as a table named `new_rows`
  with the triggering table's columns.
- **Result.** SQL's result is its rows. Code prints JSON:
  `{"result": …, "state": …, "wrote": {"data_financial_transaction": 40}}`.
  `state` is kept on the row for the next run. `wrote` is described under
  Output.
- **Empty.** Zero rows, or a missing or empty `result`, is empty. With a prompt
  after it, the run is recorded as skipped and no model is called. Without a
  prompt, an empty result is a normal run.
- **Handed to the prompt.** The result goes into the prompt's input, after the
  trigger's input. SQL rows are rendered as a table with a row cap (Open
  decisions).
- **Checked at save.** SQL is prepared in Postgres, so a wrong table or column
  is refused with Postgres's own message. The login limits what the SQL can
  read; the syntax is not policed.

## What the box derives

| Thing | Comes from |
|---|---|
| Id | `name` |
| Database schema | `applet_` + `name` with hyphens as underscores |
| Displayed name | `name`, with the brand spellings list |
| Origin: shipped, yours, assistant-made, imported | where the folder lives and who saved it, kept on the row. A manifest's `owner` is attacker-controlled, as `is_shipped_applet` in [applet_runner/mod.rs](../../virtues-core/src/applet_runner/mod.rs) notes. |
| Kind label (Ask, Routine, Sync, Tracker, View) | which parts are present. Shown as a chip and never stored. |
| Run now | available unless the applet only starts from a data trigger or `webhook` |
| What each run made | the box's record of the run (Output) |
| Which tables unshipped code may write | its own schema, and the tables its `source` provides in the catalog |
| Where unshipped code writes files | its own lake folder, `lake/applets/<name>/` (The lake) |
| Its thread | every applet with a prompt has one on its page. A message there starts a run with that message as input. |
| Memory | every applet with a prompt keeps notes, editable on its page |
| State | the code's round-trip JSON (sync cursors), kept on the row and never in the file |
| One copy per account or device | `source` |
| One run at a time, one missed daily run caught up | the existing scheduler rules |
| Inbox items | described under Output |
| The sandbox | every shell and all code that did not ship with the box |

The file is the applet and the row is an index of it. Every edit, from chat,
the applet page or the `applet` tool, writes the file and then reconciles.
Today `edit` and PATCH write only the row, and reconcile puts the manifest back
for AI-made applets; under this rule nothing writes the row alone.

## Output and where results go

The rule "chat is not a delivery target" in
[applets-refactor-plan.md](applets-refactor-plan.md) came from three problems
with posting runs into the chat the applet was made in: nobody rereads that
chat, each run replayed the whole chat as context so cost grew with every run,
and the conversation that built the applet filled up with its reports.

The fix separates where a result is kept, how you hear about it, and where you
talk to the applet:

- **Kept:** every run, in the applet's thread on its page, with what it made.
- **Told:** an unattended run that makes a page or text adds one Inbox item
  that links to the run. A run that only writes rows adds nothing. A failed run
  always adds one. Notifications, when built
  ([notifications-plan.md](notifications-plan.md)), point to the same item.
- **Talked to:** the same thread. You reply there.
- **An applet answers where it was started.** Started from `/` or called by the
  assistant, it replies in that chat, because the owner is there, and its
  thread lists the run with a link to that chat. Started by a schedule, new
  rows or a webhook, it writes into no chat.
- **Runs start clean.** A scheduled run never replays a chat. It reads its
  memory and its tables.

### What each run made

No key declares what an applet makes. The box records it on each run, in
`app_applet_run_outputs` extended with a kind, a reference and a count:

- **Through tools.** Everything a prompt makes besides its reply goes through a
  tool that `allowed-tools` already controls: pages, rows, tasks, messages,
  images. The tool records what it wrote on the run. The box already does this
  for pages.
- **Through code.** Code reports what it wrote in `wrote` on stdout. The report
  is shown and never trusted for anything else: data triggers do not read it.
- **The run's page and thread** show it as "made 1 page and 12 rows in
  drinks", and a run that made nothing says so.

Data triggers find new rows with a `(created_at, id)` watermark kept per
applet and table, so a missing or wrong `wrote` cannot hide or invent a wake.
The record is used only to keep an applet from waking on rows it wrote itself.

A prompt that ends with nothing to say is recorded as skipped and adds no Inbox
item, the same as a run whose result is empty. Today an empty reply records
success.

The "must write a page" check that `delivers = page` gave goes. If prompts
start claiming work they did not do, it comes back as a check on the record.

## Sandbox and network

### The problem the rules address

An agent that holds private data, reads text someone else wrote, and can send
something out can be made to leak, and filters do not prevent it: published
defenses were bypassed more than 90% of the time by adaptive attacks. This is
Simon Willison's lethal trifecta
([post](https://simonwillison.net/2025/Jun/16/the-lethal-trifecta/)). Meta's
Rule of Two allows at most two of the three per session and asks for a person
or a fresh context when all three are needed
([post](https://ai.meta.com/blog/practical-ai-agent-security/)).

An audit on 2026-10-26 found all three present, with nothing asking, in
several places on the box:

- Applet runs and background task runs have `sql_query` and `web_search`.
- Code that did not ship with the box has full network, the box's own database
  login, and the loopback API, which treats it as the owner. The comment above
  `build_command` in
  [applet_runner/mod.rs](../../virtues-core/src/applet_runner/mod.rs) says the
  jail "is NOT an authority boundary" and that the trust decision is made once,
  at import.
- A view can leak by navigating its own frame. Neither its CSP nor the iframe
  sandbox stops that.

Chat has the same problem through `browser_open` and `send_imessage`; that is
outside this plan (Not in this plan).

### Two shells

- **`shell`, for any applet.** It is the Python sandbox from
  [api/code.rs](../../virtues-core/src/api/code.rs) running `bash -c` instead
  of `python3 -`: a throwaway user (`DynamicUser`), `PrivateNetwork`, the box's
  data, state root, secrets and Postgres socket hidden, other processes
  invisible, seccomp, and memory, CPU, process, file-size and time limits. It
  adds:
  - the applet's folder, read-only, at `/skill`, so `scripts/` runs as the spec
    expects;
  - a private `/work` for files it makes;
  - the pinned Python environment at `/opt/py`, as today;
  - the proxy socket, only when `allowed-tools` has `net(…)` entries;
  - the Postgres socket logged in as the applet's own role, only once those
    roles exist (slice 9).
- **`sudo`, for shipped applets only.** Today's unsandboxed shell with
  passwordless sudo. `admin` lists it, and the check before deleting data stays
  on it.

`code_interpreter` folds into `shell`: running Python is `python3 script.py`.

### Network

1. **No network by default.** The sandbox has no route out and no DNS
   resolver.
2. **One way out: a proxy in the box**, reached through a Unix socket mounted
   into the sandbox. This is the pattern in Anthropic's sandbox runtime
   ([repo](https://github.com/anthropic-experimental/sandbox-runtime)) and in
   Claude Code ([docs](https://code.claude.com/docs/en/sandboxing)).
3. **The proxy decides, per applet:**
   - it allows only the hosts in the applet's `net(…)` entries;
   - it resolves DNS itself, then refuses the address if it is loopback,
     link-local, a metadata endpoint, a private range (10/8, 172.16/12,
     192.168/16, the owner's home network), or the box itself;
   - it refuses IP literals;
   - it records each request (host, method, bytes) on the run, so the applet's
     page can say "reached api.weather.gov, 3 requests".
4. **Hosts that are refused even when listed:** hosts anyone can write to
   (`github.com`, `gist.github.com`, npm, Google Forms, cloud-storage
   wildcards) and model-provider APIs, which accept uploads under an
   attacker's own key. The 2026-01 Claude Cowork leak went through an
   allowlisted `api.anthropic.com`
   ([post](https://simonwillison.net/2026/Jan/14/claude-cowork-exfiltrates-files)).
   PromptArmor's list is the starting point
   ([post](https://www.promptarmor.com/resources/what-domains-should-i-add-to-my-allowlist)).
5. **Network while installing, none while running.** Dependencies a script
   needs install once, when the applet is saved, from a lockfile with hashes,
   the way [api/code_env.rs](../../virtues-core/src/api/code_env.rs) builds the
   Python environment. Runs are offline unless there are `net(…)` entries.
   Codex cloud separates its setup and agent phases the same way
   ([docs](https://learn.chatgpt.com/docs/cloud/internet-access)).

### The Rule of Two, checked at save

Each tool counts toward the three legs:

| Leg | What counts |
|---|---|
| Your data | `sql_query`, `semantic_search`, `get_page_content`, `get_project_item`, `read_asset`, `lake`, the Postgres socket |
| Outside text | the same data tools, because your messages and email contain other people's words; `web_search`; anything fetched through `net(…)` |
| A way out | `net(…)` entries, `browse`, `send_imessage`, `sudo` |

`web_search` is not a way out: its query goes to one fixed vendor whose logs an
attacker cannot read. The card still shows it.

Any applet that reads your records already has two legs. So:

- An applet the assistant writes or an import brings may not hold the third.
  The check refuses `net(…)` entries, `browse` or `send_imessage` alongside any
  data tool on such an applet.
- **Only the owner can add the third leg**, by adding a host on the applet's
  page. The assistant's `applet` tool cannot. The row records the hosts the
  owner set, and reconcile refuses a file whose `net(…)` entries differ from
  the owner's for any applet that is not shipped. That owner action is the
  person the Rule of Two asks for, without an approval card.
- The applet's sentence shows the combination: "It reads your records and can
  send to hooks.example.com."

### A real boundary for code that did not ship

Applet code (`run`) that did not ship with the box moves from today's jail to
the same sandbox as `shell`, with:

- a throwaway user instead of the box user;
- no loopback route to the box's API, which `PrivateNetwork` gives;
- its own database login (slice 9) that writes its own schema and the tables
  its `source` provides in the catalog, and nothing else;
- credentials as a file only the run can read (systemd `LoadCredential`)
  instead of stdin.

This replaces the position in the `build_command` comment. Shipped code keeps
running as the box user; shipped applets' `VIRTUES_API_KEY` moves from the
environment to a credential file, because the systemd docs say environment
variables "are not suitable for passing secrets".

### The lake

Decided on 2026-10-27.

- **Shipped applets** keep full access: they run as the box user outside the
  sandbox and write through `lake::archive`, as today.
- **Every other applet** gets its own folder, `lake/applets/<name>/`,
  writable inside its sandbox. The rest of the lake is hidden.
- **The runner indexes that folder after each run.** A file written straight
  to disk has no `lake_objects` row, so without this step the content-hash
  dedupe, retention and replay would never see it.
- **Each folder has a size limit.** Direct writes skip the low-disk guard in
  `lake::archive`, so the sandbox enforces the limit where it can, and the
  runner refuses the next run while a folder is over it.
- **Reading the whole lake is a grant**, `lake` in `allowed-tools`, which
  mounts the lake read-only. It counts as your data in the Rule of Two, so an
  applet with it cannot also list `net(…)` unless the owner adds the host.
- The throwaway user changes every run, so the folder uses the same shared
  group and umask as the code workspace, and each run can write what the
  last one left.

### Database roles

View and applet SQL run today as low-privilege roles through `SET LOCAL ROLE`
from the server's own connection
([server/faces.rs](../../virtues-core/src/server/faces.rs)). On 2026-10-25 a
single view query against the dev database ran
`set_config('role', session_user, true)` followed by `query_to_xml(...)` and
read an `app_*` table that a direct read in the same transaction was refused.
`SET ROLE` cannot be a boundary for code that writes arbitrary SQL, because the
session can always return to the role it logged in as. In order:

1. A login role per trust level with its own pool. View and applet SQL connect
   as a login role that has no membership in the server's role.
2. The server's own role owns the schema and is not a superuser.
3. A role per applet, `applet_<name>`, that owns its schema, reads the `data_*`
   and `wiki_*` tables it is granted, and has nothing on `app_*` or on other
   applets' schemas, with its login in the vault.
4. A view reads only its own applet's tables and what that applet is granted.
   That narrows what the frame-navigation leak can carry.
5. The current extras stay: a read-only transaction, `statement_timeout` and a
   row limit.

## Importing outside skills

Any Agent Skills folder becomes an applet without conversion. On import:

- **Default wiring.** A skill with no `metadata` gets the default `invoke:
  chat, assistant`, which is how the spec expects skills to be used.
- **Claude Code and VS Code keys** become Virtues wiring, and the top-level key
  is dropped:
  - `user-invocable: false` removes `chat` from `invoke`;
  - `disable-model-invocation: true` removes `assistant` from `invoke`;
  - `model` maps to `standard` or `deep` where it names a tier.
- **Tool names.** Skills written for other agents list their tools
  (`Read`, `Bash`, `WebSearch`). Names with an equivalent are mapped
  (`WebSearch` to `web_search`, `Bash` to `shell`, `Read` to the folder-reading
  tool); the rest are dropped with a note on the applet's page, and the
  applet falls back to the default tools.
- **Scripts** run in the sandboxed `shell` with no network. An import arrives
  with any `net(…)` entries removed and says which hosts its instructions
  mention; the owner adds them on the page.
- **Where it came from.** The row records the git URL or archive and the
  commit or hash. An update shows the diff before it applies, because the spec
  has no signing or versioning.
- **The Rule of Two applies** as to any applet that is not shipped.
- **Export** is the folder as a zip.

## Checks

The check runs on every save, in setup's dry run, and in reconcile. Each
failure is refused with a sentence:

1. The spec's rules: `name` and `description` lengths and characters, `name`
   matches its folder, `metadata` values are strings.
2. `name` does not clash with a shipped applet.
3. Every `trigger` value parses. A data trigger names an existing table;
   `before` needs `started_at` and `after … ends` needs `ended_at`. `on` and
   `until` dates are in the future.
4. `run` names a shipped binary, a file under `scripts/`, or `sql …`. SQL
   prepares in Postgres. `new_rows` appears only when there is a data trigger.
   Only a shipped applet may name a binary outside its folder.
5. A trigger needs a prompt or `run`. `webhook` needs `source`. `invoke` other
   than `none` needs a prompt.
6. `allowed-tools` lists registry ids and `net(…)` entries. Only a shipped
   applet may list `sudo` or the vault.
7. The Rule of Two, as above. A `net(…)` entry is a host name, not an IP
   literal, and none is on the refused list.
8. `limit` is at most $10 a run. `color` is a palette name.
9. `view.html` is at most 48KB and loads nothing from outside hosts.
10. An unknown `metadata` key is refused with the closest known key.
11. A successful save returns the next three run times with the timezone, so
    the assistant can say when it will run.

## Recipes

These are written examples to copy, and none of them is a type.

A report you ask for and that also runs weekly:

```markdown
---
name: weekly-review
description: Looks back over your past seven days and the next seven. Use when someone asks how their week went.
allowed-tools: sql_query semantic_search create_page
metadata:
  trigger: every sunday at 18:00
---
Write the person a review of the seven days before today…
```

A sync, one copy per connected account:

```markdown
---
name: plaid-transactions
description: Pulls new card and bank transactions.
metadata:
  trigger: every hour
  run: plaid_transactions_sync
  source: plaid
---
```

A tracker with its own table and a view (the folder also holds `view.html` and
`tables/0001_drinks.sql`):

```markdown
---
name: water
description: Tracks glasses of water. Use when someone says they drank water or asks how much they have had.
allowed-tools: sql_query sql_write
---
Log what they drank in the drinks table…
```

A one-time reminder:

```markdown
---
name: dentist
description: Reminds you about the dentist.
metadata:
  invoke: none
  trigger: on 2026-11-02 at 09:00
---
Remind them that their dentist appointment is at 11.
```

Reacting to new rows, and only when some match:

```markdown
---
name: high-heart-rate
description: Notes when your heart rate ran high while you were sitting still.
allowed-tools: sql_query
metadata:
  invoke: none
  trigger: new data_health_heart_rate
  run: sql select * from new_rows where bpm > 100
  limit: $0.50 per day
---
Look at these readings. Leave out any during a logged workout or a walk…
```

A threshold checked every morning, with no model call on most days:

```markdown
---
name: workout-nudge
description: Nudges you when three days pass without a workout.
allowed-tools: sql_query
metadata:
  invoke: none
  trigger: every day at 09:00
  run: scripts/no_recent_workout.sql
---
Write one short line suggesting a workout today. Mention the last one by type.
```

`scripts/no_recent_workout.sql` returns the last workout's row when it ended
more than three days ago, and nothing otherwise. A cooldown is another `not
exists` clause on the applet's own runs, and a time-of-day rule is
`extract(hour …)` in the same query. Neither can cancel a catch-up run, because
the trigger has already fired.

A script that reaches one outside host and reads none of your records:

```markdown
---
name: tide-times
description: Keeps the next week of tide times for one harbor.
allowed-tools: net(tides.example.org)
metadata:
  trigger: every day at 05:00
  run: scripts/fetch_tides.py
---
```

The script writes into the applet's own `tides` table through its own login
(slice 9), and its view reads that table.

Deep research, shipped:

```markdown
---
name: deep-research
description: Investigates your records and the web and writes a cited report.
allowed-tools: think dispatch_subagents sql_query semantic_search web_search shell create_page
metadata:
  invoke: chat
  model: deep
  limit: $10 per run, 25 minutes
---
You are an investigation orchestrator…
```

Full access, shipped:

```markdown
---
name: admin
description: Runs commands on your server. Asks before deleting anything.
allowed-tools: sudo sql_query sql_write
metadata:
  invoke: chat
  limit: $5 per run, 30 minutes
---
```

## How applets combine

- **Through tables.** The rows one applet writes are another's `trigger: new
  …`. This is the 2026-07-20 decision that the database is the message bus.
- **Through the assistant.** Every applet whose `invoke` includes `assistant`
  is offered to the model as a name and description; using one loads its body.
  One chat turn can call several, inside that turn's limit.
- **Through pages.** A view embeds in any page.
- **Through packages.** A git repository of applet folders plus a
  `sources.toml`, as [record/sources-packages.md](../record/sources-packages.md)
  describes.

## Left out on purpose

- **Applets calling applets.** Letting `allowed-tools` name another applet, so
  scheduled runs could call each other, reopens the decision against run
  chaining, and no applet needs it yet. Tables connect them instead.
- **Retries.** A failed run waits for its next trigger and adds an Inbox item.
  Automatic retries hide failures and double the spend.
- **Typed inputs.** The text you type is the input, and the prompt reads it.
- **Settings for code applets.** No code applet reads one today.
- **Dependencies as a key.** A script's packages go in a lockfile beside it
  (`scripts/requirements.txt` with hashes), installed when the applet is saved.
- **Version and author keys.** An imported applet may carry them in `metadata`,
  and the page shows them. The box trusts only the import's own record of the
  URL and commit.

## Old to new

| Today | Becomes |
|---|---|
| `manifest.toml` | `SKILL.md` frontmatter |
| `name` (display name) | `name` (the spec's id), displayed with the brand spellings list |
| `description` | `description` |
| `agent = """…"""` | the body |
| `command` | `metadata.run` |
| `triggers` and `schedule` | `metadata.trigger` |
| `manual` and `message` triggers | always available: Run now and the thread |
| `api` trigger | deleted; it has never fired |
| skill invocation from `/`, assistant calls | `metadata.invoke` |
| `until = "once"` | `trigger: on <date> at <time>` |
| `until` as a date | `trigger: every … until <date>` |
| `condition` as SQL | `run: sql …` that returns nothing when the condition is false |
| `until` as SQL | removed |
| `owner` | derived from origin |
| `default_enabled` | `metadata.enabled` |
| `source = { id }` and `per_credential` | `metadata.source` |
| `id_prefix` | `name` |
| `[config.limits]`, skill `max_steps`, `max_cost_usd`, `max_minutes` | `metadata.limit`; a step limit stays as an internal default |
| `[config.delivers]` | removed; the box records what each run made |
| `writes` | removed; unshipped code writes its own schema and its source's tables |
| `config.model`, `config.model_slot`, skill `model` | `metadata.model` |
| `config` as settings | removed until a code applet needs settings; a prompt applet's settings live in its memory |
| `config` as code state | the row's state, never the file |
| skill `tools` | `allowed-tools` |
| skill `icon` | `metadata.icon` or derived |
| `face/index.html` | `view.html` |
| `schema/NNNN_*.sql` | `tables/NNNN_*.sql` |
| `needs_vault_key` | a kernel list of applet names |
| `forked_from` | row metadata, with the import's source and commit |
| `skills/*/SKILL.md` and `<state root>/skills/` | applet folders under `applets/` and `<state root>/` |
| `save_skill` | the `applet` tool's setup |
| `code_interpreter` | `shell` |
| Sudo mode's `shell` | `sudo` |
| `ChatMode::DeepResearch` | the `deep-research` applet |
| `ChatMode::Sudo` and the access control | the `admin` applet |
| `ChatMode::Chat` | the default when no applet is picked |
| `ChatMode::Skill` | `ChatMode::Applet` |
| `agentMode` on the wire | the picked applet's name |
| the applet jail | the sandbox |

## Order of work

Each slice lands on its own and leaves the box working.

### 0. Fix the bugs confirmed on 2026-10-25

Each was checked against the code on that date.

- View SQL can return to the server's role (Database roles, step 1). A test
  runs the `set_config` query through `run_face_query` and is refused.
- Reconcile overwrites a shipped applet's `config`, where sync cursors live
  (`config = EXCLUDED.config` in the system branch of
  [applet_templates/mod.rs](../../virtues-core/src/applet_templates/mod.rs)).
  Cursors move to their own column. A test: a cursor survives a reconcile.
- `dir_for_applet_id` matches by prefix, and the folder list is unsorted, so
  deleting `daily_2` can remove `daily`'s folder. Match exactly.
- `edit` and PATCH write only the row, and reconcile reverts AI-made applets.
  Edits write the manifest. A test: a retimed applet survives a reconcile.
- Setup accepts a schedule without a `cron` trigger, and the scheduler never
  registers it
  ([tools/applet_setup.rs](../../virtues-core/src/tools/applet_setup.rs)).
  Refuse it.
- Cron jobs drift an hour after a DST change. `new_async_tz` takes a
  `chrono_tz::Tz` but fixes its UTC offset when the job is registered, and
  the sync pass re-registered only retimed jobs
  ([scheduler/mod.rs](../../virtues-core/src/scheduler/mod.rs)). Each job
  now records its offset, and the sync pass re-registers it when the offset
  changes.

### 1. Read applet folders written as SKILL.md

A YAML reader for the frontmatter (the registry's flat parser cannot read the
`metadata` map, quoted values or a multi-line description), the spec's checks,
and the checks above. Reconcile reads both formats while the folders move,
with a dated `TODO` at the `manifest.toml` reader.

**Done when:** a folder with only `SKILL.md` reconciles into a row, every check
has a test that refuses it, and the spec's reference validator accepts every
shipped `SKILL.md`.

The reader refuses what has nothing behind it yet: data triggers (slice 8),
SQL in `run` (slice 8), and `net(…)` entries (slice 2). `allowed-tools`
narrows the tools a run is offered. Editing a `SKILL.md` applet from the app
and forking one are refused until setup writes the format (slice 4).

### 2. The sandbox, the proxy and the boundary

`shell` and `sudo` as two tools; `code_interpreter` folds into `shell`. The
proxy with its host list, address checks and request log. Unshipped `run` code
moves into the sandbox with its own user and no loopback route. The Rule of Two
check, and the owner-only host field on the applet page.

**Done when:** a test script in the sandbox cannot reach the box's API, the
LAN, a refused host or an unlisted host, and can reach a listed host; the
request appears on the run; and the `applet` tool cannot add a host to an
applet that lists `sql_query`.

The agent loop also refuses a call to a tool the run was not offered. Today
it runs whatever tool the model names, so `allowed-tools`, like the applet
allowlist before it, only decides what is offered.

### 3. Move skills into applets

The five shipped skills become applet folders with `invoke: chat`. `skills/`,
the registry's compiled-in list and `<state root>/skills/` go. The `/` menu
lists applets whose `invoke` includes `chat` and is titled "Applets"
([SkillsPanel.svelte](../../apps/web/src/lib/components/chat/SkillsPanel.svelte)).
`save_skill` folds into the `applet` tool.

**Done when:** `/weekly-review` runs from its applet folder, and no code reads
`skills/`.

### 4. Convert every manifest

Every `manifest.toml` becomes `SKILL.md`, prompts move out of TOML, and folders
take hyphenated names. Ids, run rows and schema names move in one migration
(claim the number with `make migration`). `run: sql` and the skipped run land
here, because converted conditions need them. Existing `until = "once"` rows
become `on <date>`. An existing `condition` becomes `run: sql select 1 where
(<condition>)` when the applet has no `command`; with a `command`, it moves
into the prompt as its first step. The `manifest.toml` reader and its `TODO`
are deleted.

**Done when:** no `manifest.toml` remains, an upgraded box keeps every applet's
runs, memory, state and tables, and a converted condition that is false records
a skipped run with no model call.

### 5. Remove chat modes

`deep-research` and `admin` ship as applets. The composer loses the depth and
access controls
([agentModes.ts](../../apps/web/src/lib/config/agentModes.ts)). `ChatMode`
keeps the default, `Applet`, the two rooms, and Local untouched.

**Done when:** nothing in the composer says depth or access, and a deep
research turn and an admin turn run from their applet folders.

### 6. Let the assistant call applets

Applets whose `invoke` includes `assistant` are offered to the model by name
and description; calling one loads its body (the spec's second stage). A tool
reads files from the applet's own folder (`references/`, `assets/`), the third
stage.

**Done when:** a chat that mentions drinking water calls `water` without `/`,
and an applet's prompt can read a file from its `references/`.

### 7. Output, threads and the Inbox

Runs start clean, every run lands in its applet's thread, nothing posts into a
chat it was not started from, and the Inbox rule under Output is built.
`app_applet_run_outputs` records rows per table, tasks, messages and images
alongside pages, and code's `wrote` report is shown on the run. Failing applets
reach the Inbox, which
[api/attention.rs](../../virtues-core/src/api/attention.rs) does not cover
today.

**Done when:** a scheduled `weekly-review` adds one Inbox item linking to its
thread, a failed sync adds one Inbox item, and a `water` run shows the rows it
wrote.

### 8. Data triggers

`run: sql …` and SQL files under `scripts/`, prepared in Postgres at save and
run read-only on the applet login, with an empty result skipping the prompt.
`trigger: new <table>` with `new_rows` for `run`, a `(created_at, id)`
watermark per applet and table, batching, a short delay, no waking on the
applet's own rows, one hop per pass, no waking on a source's first backfill,
and rate limits on by default, per [applets-next-plan.md](applets-next-plan.md).
`before` and `after … ends` follow, reading `started_at` and `ended_at`.

### 9. A role per applet

Database roles, step 3, and the Postgres socket in the sandbox logged in as
that role. `run: sql` moves from the shared applet login to the applet's own.
Due before importing skills from outside sources is offered.

### 10. Import and export

Import from a git URL or an archive with the rules under Importing outside
skills; export as a zip. Git import of applet folders exists today and moves
onto these rules.

## What replaces `condition` and `until`

Decided on 2026-10-26: remove both SQL forms. No shipped applet uses either,
and the dev database has none. The model's guide
([applets/AGENTS.md](../../applets/AGENTS.md)) and the `applet` tool's schema
both teach them, so applets the assistant made on real boxes may use them; the
guide makes `until = "once"` mandatory for one-off reminders. What covers each
use:

- `until = "once"`: `trigger: on <date>`, which also removes the guide's
  warning that a one-off written as a yearly cron fires every year if `until`
  is forgotten.
- `until` as a date: `trigger: every … until <date>`.
- `until` as SQL ("stop after 30 logged workouts"): nothing. It is rare enough
  to leave out.
- `condition` waking on new data: `trigger: new <table>` with `run: sql …`
  over `new_rows`, without polling.
- `condition` for a threshold ("no workout in 3 days"): `run: sql …` that
  returns a row only when it is true. No model call on the days it is false.
- A cooldown: a `not exists` clause on the applet's own runs in the same SQL.
- `condition` on the time of day: `extract(hour …)` in the SQL. The guide calls
  the old form a trap because it cancels catch-up runs; in `run` the trigger
  has already fired, so a catch-up run is recorded as skipped instead of lost.

## Open decisions

### How many rows `run: sql` hands to the prompt

Proposed: the first 200 rows as a table, then a line with the total count. The
prompt can query for more with `sql_query`.

### `enabled`

Only shipped applets that start off use it. If shipped applets become a
gallery you add from, the key goes.

## Not in this plan

- **Chat's own leak paths.** In chat, `sql_query` reads messages and email
  (other people's words), `browser_open` can GET any URL through the owner's
  signed-in browser, and `send_imessage` can send to any number or address with
  only a 5-second undo. Nothing asks. Limiting sends to known contacts unless
  the owner typed the address, and stopping the browser from opening URLs built
  from the owner's data, need their own security pass.
- Tasks, Local chat, Interview and Getting started.

When this ships, delete this plan, move the shape into
[record/applets.md](../record/applets.md), and write the manual page.
